//! `mulch validate` — schema validation over every live record.

use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::schema::schema_error;
use crate::commands::{NO_CONFIG_MESSAGE, NO_STORE_MESSAGE, StoreLocation, domain_file, locate};
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
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("validate", format!("resolving cwd: {source}")))?;
    let store = match locate(&cwd) {
        Ok(StoreLocation::Missing) => {
            let mut failure = Failure::handled("validate", NO_STORE_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Ok(StoreLocation::NoConfig) => {
            let mut failure = Failure::handled("validate", NO_CONFIG_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Ok(StoreLocation::Open(store)) => store,
        Err(source) => {
            return Err(Failure::handled(
                "validate",
                crate::output::chain_message(&source),
            ));
        }
    };

    let mut findings = Vec::new();
    let mut total_records = 0;
    for domain in store.domains() {
        let text = std::fs::read_to_string(domain_file(&store.root, &domain)).unwrap_or_default();
        for (index, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
            let line_no = index + 1;
            total_records += 1;
            match serde_json::from_str::<Value>(line) {
                Err(_) => findings.push(Finding {
                    domain:  domain.clone(),
                    line:    line_no,
                    message: "Invalid JSON: failed to parse".into(),
                }),
                Ok(record) => {
                    if let Some(message) = schema_error(&record) {
                        findings.push(Finding {
                            domain: domain.clone(),
                            line: line_no,
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
        print_line(
            opts.quiet,
            &format!("{total_records} records validated, {total_errors} errors found"),
        );
        #[allow(clippy::print_stderr, reason = "error details render on stderr")]
        for finding in &findings {
            eprintln!("{}:{} - {}", finding.domain, finding.line, finding.message);
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
