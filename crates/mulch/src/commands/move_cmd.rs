//! `mulch move` — move a record between domains.
//!
//! Reference semantics: the source file is rewritten through the format
//! core (surviving records re-serialized compactly), the target file
//! gets the moved record APPENDED compactly (existing bytes preserved,
//! insertion order — not sorted). `move` validates the record (schema,
//! archived status, target `allowed_types` and `required_fields`);
//! `delete` does not.

use std::path::PathBuf;

use mulch::{record_summary, resolve_record_id};
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `move`.
pub(super) fn run(
    opts: &GlobalOpts,
    source: &str,
    id: &str,
    target: &str,
    dry_run: bool,
    force: bool,
) -> Result<(), Failure> {
    let store = crate::commands::open_store("move", true)?;

    // Same-domain check comes FIRST (reference order).
    if source == target {
        return Err(Failure::handled_on_stderr(
            "move",
            "Error: Source and target domain are the same — nothing to move.",
        ));
    }
    let domains = store.domains();
    for domain in [source, target] {
        if !domains.iter().any(|d| d == domain) {
            return Err(crate::commands::unknown_domain_failure(
                "move",
                domain,
                &domains,
                opts.json,
                crate::commands::DomainFailure::Move,
            ));
        }
    }

    let source_file = store.domain_path(source);
    let target_file = store.domain_path(target);
    let lines = store
        .read_records(source, opts.allow_unknown_types)
        .map_err(|source_err| {
            Failure::handled_on_stderr("move", crate::commands::render_core_error(&source_err))
        })?;
    let index = resolve_record_id(&lines, id)
        .map_err(|error| crate::commands::resolve_failure("move", error))?;
    let record = lines[index].record.clone();
    let kind = lines[index].record_type();
    let record_id = lines[index].id().map(str::to_string);

    // Archived records must be restored first (reference).
    let archived = record.get("status").and_then(Value::as_str) == Some("archived");
    if archived {
        let shown = record_id.clone().unwrap_or_else(|| id.to_string());
        return Err(Failure::handled_on_stderr(
            "move",
            format!("Error: Record {shown} is archived. Run `ml restore {shown}` first."),
        ));
    }

    // move validates what delete does not.
    if let mulch::schema::FullVerdict::Invalid { subs, .. } = mulch::schema::full_verdict(&record) {
        return Err(Failure::handled_on_stderr(
            "move",
            format!(
                "Error: Record fails schema validation: {}. Edit the record before moving.",
                mulch::schema::render_subs(&subs).join("; ")
            ),
        ));
    }

    // Target allowed_types gate (--force bypasses it only).
    let allowed = store.config().allowed_types(target).map_err(|source_err| {
        Failure::handled("move", crate::output::chain_message(&source_err))
    })?;
    if let Some(allowed) = allowed
        && !allowed.iter().any(|t| t == &kind)
        && !force
    {
        return Err(Failure::handled_on_stderr(
            "move",
            format!(
                "Error: Type \"{kind}\" is not in target domain \"{target}\" allowed_types ({}). Pass --force to override, or adjust mulch.config.yaml.",
                allowed.join(", ")
            ),
        ));
    }

    // Target required_fields gate (enforced even with --force).
    let required = store
        .config()
        .required_fields(target)
        .map_err(|source_err| {
            Failure::handled("move", crate::output::chain_message(&source_err))
        })?;
    if let Some(required) = required {
        let missing: Vec<String> = required
            .into_iter()
            .filter(|field| record.get(field).is_none_or(serde_json::Value::is_null))
            .collect();
        if !missing.is_empty() {
            let shown = record_id.clone().unwrap_or_else(|| id.to_string());
            let fields = missing
                .iter()
                .map(|field| format!("\"{field}\""))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Failure::handled_on_stderr(
                "move",
                format!(
                    "Error: Record is missing field(s) required by target domain \"{target}\": {fields}. Edit the record (`ml edit {shown}`) before moving."
                ),
            ));
        }
    }

    let summary = record_summary(&record);
    let incoming = incoming_references(
        store.root(),
        &source_file,
        &target_file,
        record_id.as_deref(),
    );

    if !dry_run {
        let survivors: Vec<Value> = lines
            .iter()
            .enumerate()
            .filter(|(position, _)| *position != index)
            .map(|(_, line)| line.record.clone())
            .collect();
        store
            .rewrite_domain(source, &survivors)
            .map_err(|source_err| {
                Failure::handled("move", crate::output::chain_message(&source_err))
            })?;

        // Target: the seam's append-verbatim policy (existing bytes
        // preserved, newline guard included).
        let mut moved = record.clone();
        mulch::assign_missing_id(&mut moved);
        let line = serde_json::to_string(&moved).unwrap_or_default();
        store
            .append_domain_line(target, &line)
            .map_err(|source_err| {
                Failure::handled("move", crate::output::chain_message(&source_err))
            })?;
    }

    if opts.json {
        let mut record_json = Map::new();
        record_json.insert(
            "id".into(),
            record_id.clone().map_or(Value::Null, Value::String),
        );
        record_json.insert("type".into(), Value::String(kind.clone()));
        record_json.insert("summary".into(), Value::String(summary.clone()));
        let mut fields = Map::new();
        if dry_run {
            fields.insert("dryRun".into(), Value::Bool(true));
        }
        fields.insert("sourceDomain".into(), Value::String(source.into()));
        fields.insert("targetDomain".into(), Value::String(target.into()));
        fields.insert("record".into(), Value::Object(record_json));
        fields.insert("incomingReferences".into(), Value::Array(incoming.clone()));
        print_json(&success_envelope("move", fields), false);
    } else if !opts.quiet {
        let id_part = record_id
            .as_ref()
            .map_or_else(String::new, |id| format!(" {id}"));
        let prefix = if dry_run {
            "[DRY RUN] Would move"
        } else {
            "✓ Moved"
        };
        let mut text = format!("{prefix} {kind}{id_part} from {source} → {target}: {summary}");
        if !incoming.is_empty() {
            if dry_run {
                let _ = std::fmt::Write::write_fmt(
                    &mut text,
                    format_args!(
                        "\n  {} inbound reference(s) detected; ID is preserved so links remain valid.",
                        incoming.len()
                    ),
                );
            } else {
                let _ = std::fmt::Write::write_fmt(
                    &mut text,
                    format_args!(
                        "\n  {} inbound reference(s) found; ID preserved so existing links still resolve:",
                        incoming.len()
                    ),
                );
                for reference in &incoming {
                    let domain = reference
                        .get("domain")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let ref_id = reference
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("(no id)");
                    let field = reference
                        .get("field")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let _ = std::fmt::Write::write_fmt(
                        &mut text,
                        format_args!("\n    {domain}/{ref_id} via {field}"),
                    );
                }
            }
        }
        print_line(false, &text);
    }
    Ok(())
}

/// Inbound references: every `expertise/*.jsonl` except the source and
/// target files (reference scans the directory).
fn incoming_references(
    store_root: &std::path::Path,
    source_file: &std::path::Path,
    target_file: &std::path::Path,
    id: Option<&str>,
) -> Vec<Value> {
    let Some(id) = id else {
        return Vec::new();
    };
    let mut incoming = Vec::new();
    let directory = store_root.join("expertise");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    // Directory order, not sorted: the reference reports the
    // readdir order it walks.
    let files: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    for file in files {
        let Some(file) = (!(file == source_file || file == target_file)).then_some(file) else {
            continue;
        };
        let domain = file
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let Ok(records) = mulch::read_strict(&file, true) else {
            continue;
        };
        for line in records {
            for field in ["relates_to", "supersedes"] {
                let refers = line
                    .record
                    .get(field)
                    .and_then(Value::as_array)
                    .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(id)));
                if refers {
                    let mut entry = Map::new();
                    entry.insert("domain".into(), Value::String(domain.clone()));
                    entry.insert(
                        "id".into(),
                        line.id()
                            .map_or(Value::Null, |id| Value::String(id.to_string())),
                    );
                    entry.insert("field".into(), Value::String(field.into()));
                    incoming.push(Value::Object(entry));
                }
            }
        }
    }
    incoming
}
