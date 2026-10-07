//! `mulch validate` — schema validation over every live record.

use mulch::schema::{plain_detail_lines, validate_message};
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// The legacy-outcome warning text (reference validate.ts).
const LEGACY_OUTCOME_WARNING: &str =
    "Legacy \"outcome\" field (singular); run `mulch doctor --fix` to migrate to \"outcomes[]\"";

/// One validation finding (`domain:line` addressed).
#[derive(Clone)]
struct Finding {
    domain:  String,
    line:    usize,
    message: String,
}

/// Runs `validate`: summary on stdout, details on stderr, JSON envelope
/// on stdout (reference channel quirk), exit 1 on any error.
pub(super) fn run(opts: &GlobalOpts) -> Result<(), Failure> {
    let store = crate::commands::open_store("validate", false)?;

    // Findings in encounter order: the reference prints each line's
    // error or warning inline, interleaved by line number.
    let mut stream: Vec<(bool, Finding)> = Vec::new();
    let mut total_records = 0;
    for domain in store.domains() {
        // The one lenient reader (lib seam): parse outcomes are
        // findings, physical line numbers preserved.
        for finding in store.read_lenient(&domain) {
            total_records += 1;
            match finding {
                mulch::LenientLine::Malformed { line } => stream.push((false, Finding {
                    domain: domain.clone(),
                    line,
                    message: "Invalid JSON: failed to parse".into(),
                })),
                mulch::LenientLine::Record { line, record } => {
                    let unknown = matches!(
                        mulch::schema::verdict(&record),
                        mulch::schema::Verdict::Unknown(_)
                    );
                    if unknown && opts.allow_unknown_types {
                        continue;
                    }
                    // Legacy singular outcome: a warning, and it
                    // REPLACES the schema check for that record
                    // (reference else-if).
                    let legacy = record.as_object().is_some_and(|object| {
                        object.contains_key("outcome") && !object.contains_key("outcomes")
                    });
                    if legacy {
                        stream.push((true, Finding {
                            domain: domain.clone(),
                            line,
                            message: LEGACY_OUTCOME_WARNING.into(),
                        }));
                        continue;
                    }
                    if let Some(message) = validate_message(&record) {
                        stream.push((false, Finding {
                            domain: domain.clone(),
                            line,
                            message,
                        }));
                    }
                }
            }
        }
    }

    let findings: Vec<Finding> = stream
        .iter()
        .filter(|(warning, _)| !warning)
        .map(|(_, finding)| finding.clone())
        .collect();
    let warnings: Vec<Finding> = stream
        .iter()
        .filter(|(warning, _)| *warning)
        .map(|(_, finding)| finding.clone())
        .collect();
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
        fields.insert(
            "totalWarnings".into(),
            Value::from(u64::try_from(warnings.len()).unwrap_or(0)),
        );
        fields.insert("errors".into(), Value::Array(errors));
        let warnings_json: Vec<Value> = warnings
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
        fields.insert("warnings".into(), Value::Array(warnings_json));
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
        // The warning suffix only shows when NO errors were found
        // (reference else-if chain).
        let summary = if total_errors > 0 {
            format!("{total_records} records validated, {total_errors} errors found")
        } else if warnings.is_empty() {
            format!("{total_records} records validated, 0 errors found")
        } else {
            format!(
                "{total_records} records validated, 0 errors found, {} warning(s)",
                warnings.len()
            )
        };
        print_line(false, &summary);
        for (is_warning, finding) in &stream {
            if *is_warning {
                #[allow(clippy::print_stderr, reason = "warnings render on stderr")]
                {
                    eprintln!("{}:{} - {}", finding.domain, finding.line, finding.message);
                }
                continue;
            }
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
