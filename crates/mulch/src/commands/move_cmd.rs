//! `mulch move` — move a record between domains: the source file is
//! rewritten from surviving raw lines, the target file gets the raw
//! line appended (byte-identical, unsorted).

use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::{
    NO_STORE_MESSAGE, StoreLocation, domain_file, locate, parsed_lines, read_domain_lines,
    record_not_found, record_summary,
};
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
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("move", format!("resolving cwd: {source}")))?;
    let store = match locate(&cwd) {
        Ok(StoreLocation::Open(store)) => store,
        Ok(_) => {
            let mut failure = Failure::handled("move", NO_STORE_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Err(source) => {
            return Err(Failure::handled(
                "move",
                crate::output::chain_message(&source),
            ));
        }
    };

    let domains = store.domains();
    if !domains.iter().any(|d| d == source) {
        let mut failure = move_domain_not_found(source, &domains);
        failure.envelope_to_stderr = true;
        return Err(failure);
    }
    // No auto-create: an unknown target is an error (reference).
    if !domains.iter().any(|d| d == target) {
        let mut failure = move_domain_not_found(target, &domains);
        failure.envelope_to_stderr = true;
        return Err(failure);
    }
    if source == target {
        let mut failure = Failure::handled(
            "move",
            "Error: Source and target domain are the same — nothing to move.",
        );
        failure.envelope_to_stderr = true;
        return Err(failure);
    }

    let source_lines = read_domain_lines(&store.root, source).map_err(|source_err| {
        Failure::handled("move", format!("reading domain file: {source_err}"))
    })?;
    let parsed = parsed_lines(&source_lines);
    let Some((index, record, raw)) = parsed
        .iter()
        .find(|(_, record, _)| record.get("id").and_then(Value::as_str) == Some(id))
        .cloned()
    else {
        let mut failure = record_not_found("move", id);
        failure.envelope_to_stderr = true;
        return Err(failure);
    };

    // move validates the moved record (delete does not).
    if let crate::commands::schema::FullVerdict::Invalid { subs, .. } =
        crate::commands::schema::full_verdict(&record)
    {
        let mut failure = Failure::handled(
            "move",
            format!(
                "Error: Record fails schema validation: {}. Edit the record before moving.",
                subs.join("; ")
            ),
        );
        failure.envelope_to_stderr = true;
        return Err(failure);
    }

    let kind = record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("convention")
        .to_string();
    // Target allowed_types gate (--force bypasses it; required_fields
    // stay enforced through the schema verdict above).
    if let Some(allowed) = store.config.allowed_types(target)
        && !allowed.iter().any(|t| t == &kind)
        && !force
    {
        let mut failure = Failure::handled(
            "move",
            format!(
                "Type \"{kind}\" is not in target domain \"{target}\" allowed_types ({}). Pass --force to override, or adjust mulch.config.yaml.",
                allowed.join(", ")
            ),
        );
        failure.envelope_to_stderr = true;
        return Err(failure);
    }

    let summary = record_summary(&record);
    let incoming = incoming_references(domains.as_slice(), target, id, &store)?;

    if !dry_run {
        // Source: surviving raw lines (junk dropped) — like delete.
        let survivors: Vec<String> = parsed
            .iter()
            .filter(|(position, _, _)| *position != index)
            .map(|(_, _, raw)| raw.clone())
            .collect();
        crate::commands::write_domain_lines(&store.root, source, &survivors).map_err(
            |source_err| {
                Failure::handled("move", format!("writing source domain file: {source_err}"))
            },
        )?;

        // Target: raw append, existing bytes preserved.
        let target_file = domain_file(&store.root, target);
        let mut text = std::fs::read_to_string(&target_file).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&raw);
        text.push('\n');
        std::fs::write(&target_file, text).map_err(|source_err| {
            Failure::handled("move", format!("writing target domain file: {source_err}"))
        })?;
    }

    if opts.json {
        let mut record_json = Map::new();
        record_json.insert("id".into(), Value::String(id.into()));
        record_json.insert("type".into(), Value::String(kind));
        record_json.insert("summary".into(), Value::String(summary.clone()));
        let mut fields = Map::new();
        if dry_run {
            fields.insert("dryRun".into(), Value::Bool(true));
        }
        fields.insert("sourceDomain".into(), Value::String(source.into()));
        fields.insert("targetDomain".into(), Value::String(target.into()));
        fields.insert("record".into(), Value::Object(record_json));
        fields.insert("incomingReferences".into(), Value::Array(incoming));
        print_json(&success_envelope("move", fields), false);
    } else {
        let prefix = if dry_run {
            "[DRY RUN] Would move"
        } else {
            "✓ Moved"
        };
        print_line(
            opts.quiet,
            &format!("{prefix} {kind} {id} from {source} → {target}: {summary}"),
        );
    }
    Ok(())
}

/// Referrers outside the target domain (`relates_to`/`supersedes`),
/// informational only.
fn incoming_references(
    domains: &[String],
    target: &str,
    id: &str,
    store: &crate::commands::ConfigStore,
) -> Result<Vec<Value>, Failure> {
    let mut incoming = Vec::new();
    for domain in domains {
        if domain == target {
            continue;
        }
        let lines = read_domain_lines(&store.root, domain).map_err(|source| {
            Failure::handled("move", format!("reading domain file {domain}: {source}"))
        })?;
        for (_, record, _) in parsed_lines(&lines) {
            for field in ["relates_to", "supersedes"] {
                let refers = record
                    .get(field)
                    .and_then(Value::as_array)
                    .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(id)));
                if refers {
                    let mut entry = Map::new();
                    entry.insert("domain".into(), Value::String(domain.clone()));
                    entry.insert(
                        "id".into(),
                        Value::String(
                            record
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        ),
                    );
                    entry.insert("field".into(), Value::String(field.into()));
                    incoming.push(Value::Object(entry));
                }
            }
        }
    }
    Ok(incoming)
}

/// `move`'s own unknown-domain text (capital D, single line — differs
/// from the delete/edit family).
fn move_domain_not_found(domain: &str, available: &[String]) -> Failure {
    Failure::handled(
        "move",
        format!(
            "Error: Domain \"{domain}\" not found in config. Available domains: {}",
            available.join(", ")
        ),
    )
}
