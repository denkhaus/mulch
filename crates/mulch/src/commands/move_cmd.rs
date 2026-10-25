//! `mulch move` — move a record between domains.
//!
//! Reference semantics: the source file is rewritten through the format
//! core (surviving records re-serialized compactly), the target file
//! gets the moved record APPENDED compactly (existing bytes preserved,
//! insertion order — not sorted). `move` validates the record (schema,
//! archived status, target `allowed_types` and `required_fields`);
//! `delete` does not.

use std::path::PathBuf;

use mulch::hooks::HookEvent;
use mulch::{record_summary, schema};
use serde_json::{Map, Value, json};

use crate::cli::GlobalOpts;
use crate::commands::hooks_surface::{Adopt, HookPhase};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// The pre-record block fallback (move.ts:234) — the move-specific
/// spelling, not the record path's.
const MOVE_BLOCK_DEFAULT: &str = "pre-record hook blocked the move";

/// Runs `move`.
pub(super) fn run(
    opts: &GlobalOpts,
    source: &str,
    id: &str,
    target: &str,
    dry_run: bool,
    force: bool,
) -> Result<(), Failure> {
    // Real runs hold the writer lock across the cycle (store-wide —
    // the reference nests target+source file locks); --dry-run reads
    // lock-free (mulch-228d).
    // Read-only for the validations AND the hooks (record's two-open
    // law, mulch-7f49: pre-record fires BEFORE the writer lock,
    // post-record AFTER it — a hook script running a mutating command
    // on this store must not poll the lock we hold). The write phase
    // opens its own locked store below.
    let store = crate::commands::store::open_store("move", true)?;

    // Same-domain check comes FIRST (reference order).
    if source == target {
        return Err(Failure::handled_on_stderr(
            "move",
            "Error: Source and target domain are the same — nothing to move.",
        ));
    }
    for domain in [source, target] {
        crate::commands::store::ensure_domain(
            "move",
            &store,
            domain,
            opts.json,
            crate::commands::failure::DomainFailure::Move,
        )?;
    }

    let source_file = store.domain_path(source);
    let target_file = store.domain_path(target);
    let (lines, index) = crate::commands::store::read_and_resolve(
        "move",
        &store,
        source,
        id,
        opts.allow_unknown_types,
    )?;
    let record = lines[index].record.clone();
    let kind = lines[index].record_type();
    let record_id = lines[index].id().map(str::to_string);

    // Archived records must be restored first (reference).
    let archived = record.get("status").and_then(Value::as_str) == Some("archived");
    if archived {
        let shown = record_id.clone().unwrap_or_else(|| id.to_string());
        return Err(Failure::error_line(
            "move",
            format!("Record {shown} is archived. Run `mulch restore {shown}` first."),
        ));
    }

    // move validates what delete does not — against the reference's
    // in-memory shape (the legacy `outcome` key included, mulch-7896).
    if let schema::FullVerdict::Invalid { subs, .. } =
        store.updated_line_verdict(source, lines[index].line, &record)
    {
        return Err(Failure::handled_on_stderr(
            "move",
            format!(
                "Error: Record fails schema validation: {}. Edit the record before moving.",
                schema::render_subs(&subs).join(schema::SUB_SEP)
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
                    "Error: Record is missing field(s) required by target domain \"{target}\": {fields}. Edit the record (`mulch edit {shown}`) before moving."
                ),
            ));
        }
    }

    let incoming = incoming_references(
        store.registry(),
        store.root(),
        &source_file,
        &target_file,
        record_id.as_deref(),
    );

    // Pre-record against the TARGET domain (move.ts:226-247). The
    // payload record KEEPS its id — the one deliberate divergence
    // from the record path (mulch-0e87; do not "fix" by stripping).
    // A block aborts with nothing written; an adopted mutation
    // validates and REPLACES the record verbatim (no id re-stamp —
    // the append's missing-id law covers it). Dry runs fire no hooks.
    let mut hook_warnings: Vec<String> = Vec::new();
    let mut record_to_write = record.clone();
    if !dry_run {
        let payload = json!({"domain": target, "record": record});
        let phase = HookPhase::run(HookEvent::PreRecord, payload, &store, "move")?;
        if let Some(reason) = phase.blocked_reason(MOVE_BLOCK_DEFAULT) {
            return Err(Failure::error_line("move", reason));
        }
        match phase.adopt_record(store.registry()) {
            Adopt::Keep => {}
            Adopt::Replace(adopted) => record_to_write = Value::Object(adopted),
            Adopt::Invalid(reason) => return Err(Failure::error_line("move", reason)),
        }
        hook_warnings = phase.into_warnings();
    }
    let summary = record_summary(store.registry(), &record_to_write);

    let mut moved_opt: Option<Value> = None;
    if !dry_run {
        // The locked write phase: re-read + re-resolve UNDER the lock
        // (the reference reads the source inside its lock too,
        // move.ts:262-266). The pre-lock read was advisory — gates and
        // verdicts judged it; the OUTPUT mirrors record_to_write, not
        // the re-read line.
        let locked = crate::commands::store::open_store_mut("move", true)?;
        let (lines, index) = crate::commands::store::read_and_resolve(
            "move",
            &locked,
            source,
            id,
            opts.allow_unknown_types,
        )?;
        let survivors: Vec<Value> = lines
            .iter()
            .enumerate()
            .filter(|(position, _)| *position != index)
            .map(|(_, line)| line.record.clone())
            .collect();
        crate::commands::store::persist("move", &locked, source, &survivors)?;

        // Target: the seam's append-verbatim policy (existing bytes
        // preserved, newline guard included).
        let mut moved = record_to_write.clone();
        mulch::assign_missing_id(locked.registry(), &mut moved);
        let line = serde_json::to_string(&moved).unwrap_or_default();
        locked
            .append_domain_line(target, &line)
            .map_err(|source_err| {
                Failure::handled("move", crate::output::chain_message(&source_err))
            })?;
        moved_opt = Some(moved);
    }
    // post-record with the WRITTEN record (move.ts:273-277): warn-only,
    // AFTER the locked scope dropped — outside every lock, like pre.
    if let Some(moved) = moved_opt {
        hook_warnings.extend(HookPhase::run_warn_only(
            HookEvent::PostRecord,
            json!({"domain": target, "record": moved, "action": "created"}),
            &store,
            "move",
        ));
    }

    if opts.json {
        let mut record_json = Map::new();
        record_json.insert(
            "id".into(),
            record_to_write.get("id").cloned().unwrap_or(Value::Null),
        );
        record_json.insert(
            "type".into(),
            record_to_write
                .get("type")
                .cloned()
                .unwrap_or(Value::String(kind.clone())),
        );
        record_json.insert("summary".into(), Value::String(summary.clone()));
        let mut fields = Map::new();
        if dry_run {
            fields.insert("dryRun".into(), Value::Bool(true));
        }
        fields.insert("sourceDomain".into(), Value::String(source.into()));
        fields.insert("targetDomain".into(), Value::String(target.into()));
        fields.insert("record".into(), Value::Object(record_json));
        fields.insert("incomingReferences".into(), Value::Array(incoming.clone()));
        if !hook_warnings.is_empty() {
            fields.insert(
                "warnings".into(),
                Value::Array(
                    hook_warnings
                        .iter()
                        .map(|warning| Value::String(warning.clone()))
                        .collect(),
                ),
            );
        }
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
        // move's OWN warning rendering (move.ts:308): stdout lines,
        // two-space indent, lowercase `warning:` — the ONE command
        // that differs from the shared stderr `Warning:` family
        // (record/prime/prune/compact).
        for warning in &hook_warnings {
            let _ = std::fmt::Write::write_fmt(&mut text, format_args!("\n  warning: {warning}"));
        }
        print_line(false, &text);
    }
    Ok(())
}

/// Inbound references: every `expertise/*.jsonl` except the source and
/// target files (reference scans the directory).
fn incoming_references(
    registry: &mulch::Registry,
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
        let Ok(records) = mulch::read_strict(registry, &file, true) else {
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
