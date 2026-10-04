//! `mulch validate` — schema validation over every live record.

use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::schema::{plain_detail_lines, validate_message};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// One validation finding (`domain:line` addressed).
struct Finding {
    domain:  String,
    line:    usize,
    message: String,
}

/// Runs `validate`: summary on stdout, details on stderr, JSON envelope
/// on stdout (reference channel quirk), exit 1 on any error.
pub(super) fn run(opts: &GlobalOpts) -> Result<(), Failure> {
    let store = crate::commands::open_store("validate", false)?;

    let mut findings = Vec::new();
    let mut total_records = 0;
    for domain in store.domains() {
        // The one lenient reader (lib seam): parse outcomes are
        // findings, physical line numbers preserved.
        for finding in store.read_lenient(&domain) {
            total_records += 1;
            match finding {
                mulch::LenientLine::Malformed { line } => findings.push(Finding {
                    domain: domain.clone(),
                    line,
                    message: "Invalid JSON: failed to parse".into(),
                }),
                mulch::LenientLine::Record { line, record } => {
                    let unknown = matches!(
                        crate::commands::schema::verdict(&record),
                        crate::commands::schema::Verdict::Unknown(_)
                    );
                    if unknown && opts.allow_unknown_types {
                        continue;
                    }
                    if let Some(message) = validate_message(&record) {
                        findings.push(Finding {
                            domain: domain.clone(),
                            line,
                            message,
                        });
                    }
                }
            }
        }
    }

    let total_errors = findings.len();
    if opts.json {
        let errors: Vec<Value> = findings
            .iter()
            .map(|f| {
                let mut item = Map::new();
                item.insert("domain".into(), Value::String(f.domain.clone()));
                item.insert(
                    "line".into(),
                    Value::from(u64::try_from(f.line).unwrap_or(0)),
                );
                item.insert("message".into(), Value::String(f.message.clone()));
                Value::Object(item)
            })
            .collect();
        let mut fields = Map::new();
        fields.insert("valid".into(), Value::Bool(findings.is_empty()));
        fields.insert(
            "totalRecords".into(),
            Value::from(u64::try_from(total_records).unwrap_or(0)),
        );
        fields.insert(
            "totalErrors".into(),
            Value::from(u64::try_from(total_errors).unwrap_or(0)),
        );
        fields.insert("totalWarnings".into(), Value::from(0));
        fields.insert("errors".into(), Value::Array(errors));
        fields.insert("warnings".into(), Value::Array(Vec::new()));
        let envelope = if findings.is_empty() {
            success_envelope("validate", fields)
        } else {
            let mut body = Map::new();
            body.insert("success".into(), Value::Bool(false));
            body.insert("command".into(), Value::String("validate".into()));
            body.extend(fields);
            Value::Object(body)
        };
        print_json(&envelope, false);
    } else {
        // The reference prints the validate summary even under --quiet.
        print_line(
            false,
            &format!("{total_records} records validated, {total_errors} errors found"),
        );
        #[allow(clippy::print_stderr, reason = "error details render on stderr")]
        for finding in &findings {
            let prefix = format!("{}:{} - ", finding.domain, finding.line);
            let lines = plain_detail_lines(&finding.message);
            #[allow(clippy::print_stderr, reason = "error details render on stderr")]
            for (index, detail) in lines.iter().enumerate() {
                if index == 0 {
                    eprintln!("{prefix}{detail}");
                } else {
                    eprintln!("{detail}");
                }
            }
        }
    }

    if findings.is_empty() {
        Ok(())
    } else {
        // Plain mode already printed summary + details; the failure
        // carries only the exit code (the JSON envelope printed above
        // came from the command itself).
        let mut failure = Failure::handled("validate", "");
        failure.rendered = true;
        Err(failure)
    }
}
