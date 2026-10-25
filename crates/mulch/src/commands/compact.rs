//! `mulch compact` — analyze candidates or apply a compaction
//! (reference compact.ts, mulch-8a1b).
//!
//! Law map (probe-pinned 2026-10-24, ml 0.10.7): three modes dispatch
//! in the order analyze → auto → apply; `--apply` needs the positional
//! domain and `--records`. `--auto` groups records per type (custom
//! `merge_outcomes` types group by type + id-key value instead), keeps
//! groups with a stale record or `--min-group` members (analyze always
//! uses 3), walks them in order until `--max-records` would overflow
//! (break, not skip), confirms on stdin unless `--yes`, merges each
//! group per the registry's compact strategy, rewrites the live file,
//! and archives the originals with reason "compacted".
//!
//! Id matching is the reference's strict-equality law: an id-less
//! record matches an id-less candidate entry (`undefined ===
//! undefined`), so id-less originals ARE compacted and removed (the
//! id Set contains `undefined`).
//!
//! Pre-compact hooks (mulch-7f49, probe-pinned 2026-10-25): every
//! compacting group fires the store's `pre-compact` hooks with
//! `{domain, type, records}` (the raw source records) — a hook's
//! truthy `payload.replacement` overrides the mechanical merge
//! (copied verbatim, `supersedes` backfilled from the truthy source
//! ids only when the hook left it absent/empty, any incoming id
//! dropped, validated, the deterministic id stamped). A block or an
//! invalid replacement SKIPS the group with a warning (`--auto`:
//! `{domain}/{type}: {reason}`, plain `Warning:` stderr lines or the
//! json envelope's `warnings`) or aborts with exit 1 before any
//! write (`--apply`); engine warnings print plain-only there.
//! `--dry-run` and `--analyze` never fire hooks. A `manual`-strategy
//! group makes the reference CRASH (uncaught throw); this port
//! renders the thrown message as a clean `Error:` line and aborts
//! before any write.

use std::collections::HashSet;
use std::io::Write as _;

use mulch::schema::{Verdict, render_subs, verdict};
use mulch::stale::{StaleRule, StaleVerdict};
use mulch::{CompactStrategy, LineRecord, record_summary_value};
use serde_json::{Map, Value};

use crate::cli::{CompactArgs, GlobalOpts};
use crate::commands::failure::now_iso;
use crate::commands::hooks_surface::{HookPhase, print_hook_warnings};
use crate::commands::store::{open_store, open_store_mut, read_domain_records};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// One candidate member line (the reference's display triple). The
/// summary stays the RAW registry value (mulch-f06a): absent OMITS
/// the json key, non-strings pass through verbatim, plain renders
/// `String(value)` — `undefined` for absent.
#[derive(Clone)]
struct CandLine {
    id:          Option<String>,
    summary:     Option<Value>,
    recorded_at: String,
}

impl CandLine {
    /// The JSONL shape: `id` is OMITTED when absent (JSON.stringify
    /// drops undefined object values).
    fn to_json(&self) -> Value {
        let mut entry = Map::new();
        if let Some(id) = &self.id {
            entry.insert("id".into(), Value::String(id.clone()));
        }
        if let Some(summary) = &self.summary {
            entry.insert("summary".into(), summary.clone());
        }
        entry.insert(
            "recorded_at".into(),
            Value::String(self.recorded_at.clone()),
        );
        Value::Object(entry)
    }
}

/// One analyze/auto candidate: a group of same-key records.
#[derive(Clone)]
struct Candidate {
    domain:  String,
    r#type:  String,
    records: Vec<CandLine>,
}

impl Candidate {
    /// The JSON shape `{domain, type, records}`.
    fn to_json(&self) -> Value {
        let mut entry = Map::new();
        entry.insert("domain".into(), Value::String(self.domain.clone()));
        entry.insert("type".into(), Value::String(self.r#type.clone()));
        entry.insert(
            "records".into(),
            Value::Array(self.records.iter().map(CandLine::to_json).collect()),
        );
        Value::Object(entry)
    }
}

/// Runs `compact` (mode dispatch: analyze, auto, apply).
pub(super) fn run(opts: &GlobalOpts, args: &CompactArgs) -> Result<(), Failure> {
    if args.analyze {
        analyze(opts, args.domain.as_deref())
    } else if args.auto {
        auto(opts, args)
    } else if args.apply {
        match args.domain.as_deref() {
            Some(domain) => apply(opts, args, domain),
            None => Err(compact_failure(
                opts.json,
                "Domain is required for --apply.",
            )),
        }
    } else {
        Err(compact_failure(
            opts.json,
            "Specify --analyze, --auto, or --apply.",
        ))
    }
}

/// The shared error surface: plain `Error: <msg>` on stderr, json the
/// failure envelope on stderr with the raw message (probe-pinned).
fn compact_failure(json: bool, message: impl AsRef<str>) -> Failure {
    let message = message.as_ref();
    if json {
        Failure::handled_on_stderr("compact", message.to_string())
    } else {
        Failure::handled_on_stderr("compact", format!("Error: {message}"))
    }
}

/// `--analyze`: candidates across the given domain or all domains.
fn analyze(opts: &GlobalOpts, domain: Option<&str>) -> Result<(), Failure> {
    let store = open_store("compact", true)?;
    let domains = domain_check(&store, domain, opts.json)?;
    let rule = stale_rule(&store);
    let now = jiff::Timestamp::now();

    let mut candidates = Vec::new();
    for domain in domains {
        let records = read_domain_records("compact", &store, &domain, opts.allow_unknown_types)?;
        if records.len() < 2 {
            continue;
        }
        candidates.extend(find_candidates(
            &domain,
            &records,
            &rule,
            now,
            3,
            store.registry(),
        ));
    }

    if opts.json {
        let mut fields = Map::new();
        fields.insert("action".into(), Value::String("analyze".into()));
        fields.insert(
            "candidates".into(),
            Value::Array(candidates.iter().map(Candidate::to_json).collect()),
        );
        print_json(&success_envelope("compact", fields), false);
        return Ok(());
    }

    if candidates.is_empty() {
        print_line(opts.quiet, "No compaction candidates found.");
        return Ok(());
    }

    // One blank per DOMAIN (the byDomain loop's trailing console.log),
    // never between groups of the same domain.
    let mut lines: Vec<String> = vec![
        String::new(),
        "Compaction candidates:".into(),
        String::new(),
    ];
    let total_records: usize = candidates.iter().map(|c| c.records.len()).sum();
    lines.push(format!(
        "Found {} groups ({} records that could be compacted)",
        candidates.len(),
        total_records
    ));
    lines.push(String::new());
    let mut domain = String::new();
    for candidate in &candidates {
        if candidate.domain != domain {
            if !domain.is_empty() {
                lines.push(String::new());
            }
            domain.clone_from(&candidate.domain);
            lines.push(format!("{domain}:"));
        }
        lines.push(format!(
            "  {} ({} records)",
            candidate.r#type,
            candidate.records.len()
        ));
        lines.extend(preview_lines(&candidate.records, "    "));
    }
    lines.push(String::new());
    lines.push("To compact manually:".into());
    lines.push("  mulch compact <domain> --apply --records <ids> --type <type> [fields...]".into());
    lines.push(String::new());
    lines.push("To compact automatically:".into());
    lines.push("  mulch compact --auto [--dry-run]".into());
    print_block(&lines.join("\n"));
    Ok(())
}

/// `--auto`: compact every candidate group (or preview with
/// `--dry-run`).
fn auto(opts: &GlobalOpts, args: &CompactArgs) -> Result<(), Failure> {
    let min_group = strict_flag(
        opts.json,
        "--min-group",
        args.min_group.as_deref().or(Some("5")),
    )?;
    let max_records = strict_flag(
        opts.json,
        "--max-records",
        args.max_records.as_deref().or(Some("50")),
    )?;

    let store = open_store_mut("compact", true)?;
    let domains = domain_check(&store, args.domain.as_deref(), opts.json)?;
    let rule = stale_rule(&store);
    let now = jiff::Timestamp::now();

    let mut all: Vec<(String, Candidate)> = Vec::new();
    for domain in domains {
        let records = read_domain_records("compact", &store, &domain, opts.allow_unknown_types)?;
        if records.len() < 2 {
            continue;
        }
        for candidate in find_candidates(&domain, &records, &rule, now, min_group, store.registry())
        {
            all.push((domain.clone(), candidate));
        }
    }

    if all.is_empty() {
        if opts.json {
            let mut fields = Map::new();
            fields.insert(
                "action".into(),
                Value::String(if args.dry_run { "dry-run" } else { "auto" }.into()),
            );
            fields.insert("compacted".into(), Value::from(0));
            fields.insert("results".into(), Value::Array(Vec::new()));
            print_json(&success_envelope("compact", fields), false);
        } else {
            print_line(opts.quiet, "No compaction candidates found.");
        }
        return Ok(());
    }

    // The budget walk: stop at the first group that would overflow
    // (break — later smaller groups are NOT considered).
    let mut total = 0;
    let group_count = all.len();
    let mut selected: Vec<(String, Candidate)> = Vec::new();
    for item in all {
        if total + item.1.records.len() > max_records {
            break;
        }
        total += item.1.records.len();
        selected.push(item);
    }
    let skipped_groups = group_count - selected.len();

    if !opts.json && !args.dry_run {
        print_block(
            &group_block(
                "Compaction summary",
                "will be compacted",
                &selected,
                total,
                skipped_groups,
            )
            .join("\n"),
        );
    }

    if args.dry_run {
        if opts.json {
            let groups: Vec<Value> = selected
                .iter()
                .map(|(domain, candidate)| {
                    let mut entry = Map::new();
                    entry.insert("domain".into(), Value::String(domain.clone()));
                    entry.insert("type".into(), Value::String(candidate.r#type.clone()));
                    entry.insert("count".into(), Value::from(candidate.records.len()));
                    entry.insert(
                        "records".into(),
                        Value::Array(candidate.records.iter().map(CandLine::to_json).collect()),
                    );
                    Value::Object(entry)
                })
                .collect();
            let mut fields = Map::new();
            fields.insert("action".into(), Value::String("dry-run".into()));
            fields.insert("wouldCompact".into(), Value::from(total));
            fields.insert("groups".into(), Value::Array(groups));
            print_json(&success_envelope("compact", fields), false);
        } else {
            print_block(
                &group_block(
                    "Dry-run preview",
                    "would be compacted",
                    &selected,
                    total,
                    skipped_groups,
                )
                .join("\n"),
            );
            print_line(
                opts.quiet,
                &format!(
                    "✓ Dry-run complete. Would compact {total} records across {} groups.",
                    selected.len()
                ),
            );
            print_line(opts.quiet, "  Run without --dry-run to apply changes.");
        }
        return Ok(());
    }

    // Confirmation (plain mode only): y/yes lowercase, no trim.
    if !opts.json && !args.yes {
        #[allow(clippy::print_stdout, reason = "prompt renders on stdout")]
        {
            print!("Proceed with compaction? (y/N): ");
            let _ = std::io::stdout().flush();
        }
        let answer = crate::commands::input::read_confirmation().map_or_else(
            |_error| String::new(),
            |line| line.trim_end_matches(['\n', '\r']).to_lowercase(),
        );
        if answer != "y" && answer != "yes" {
            print_block("Compaction cancelled.");
            return Ok(());
        }
    }

    // Apply per domain, preserving candidate order (a blank after
    // every group, unlike analyze's per-domain blank).
    let mut total_compacted = 0;
    let mut results: Vec<(String, String, usize)> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut index = 0;
    while index < selected.len() {
        let domain = selected[index].0.clone();
        let mut end = index;
        while end < selected.len() && selected[end].0 == domain {
            end += 1;
        }
        let groups: Vec<&Candidate> = selected[index..end].iter().map(|(_, c)| c).collect();

        let records = read_domain_records("compact", &store, &domain, opts.allow_unknown_types)?;
        let mut updated: Vec<Value> = records.iter().map(|line| line.record.clone()).collect();
        let mut to_archive: Vec<Value> = Vec::new();
        let mut changed = false;

        for candidate in groups {
            // Strict-equality id matching: None matches None.
            let matched: Vec<Value> = updated
                .iter()
                .filter(|record| {
                    type_of(record) == candidate.r#type.as_str()
                        && candidate
                            .records
                            .iter()
                            .any(|line| line.id == id_of(record))
                })
                .cloned()
                .collect();
            if matched.len() < 2 {
                continue;
            }
            // Pre-compact hook (resolveReplacement's law): a truthy
            // `payload.replacement` overrides the mechanical merge; a
            // block or an invalid replacement SKIPS this group with a
            // warning so unrelated groups still compact.
            let phase = HookPhase::run(
                mulch::hooks::HookEvent::PreCompact,
                pre_compact_payload(&domain, &candidate.r#type, &matched),
                &store,
                "compact",
            )?;
            if let Some(reason) = phase.blocked_reason(PRE_COMPACT_BLOCKED) {
                warnings.push(group_warning(&domain, &candidate.r#type, reason));
                continue;
            }
            let replacement = match adopt_replacement(
                phase.payload(),
                &truthy_ids(&matched),
                store.registry(),
            ) {
                Err(reason) => {
                    warnings.push(group_warning(&domain, &candidate.r#type, &reason));
                    continue;
                }
                Ok(Some(record)) => record,
                Ok(None) => {
                    let Some(row) = store.registry().row(&candidate.r#type) else {
                        continue;
                    };
                    // The mechanical merge is a lib law (ADR-0005);
                    // the channel renders the two refusals here.
                    match mulch::compact::merge(&matched, row, &now_iso(), store.registry()) {
                        mulch::compact::Outcome::Merged(record) => record,
                        mulch::compact::Outcome::Manual => {
                            return Err(compact_failure(
                                opts.json,
                                format!(
                                    "Type \"{}\" has compact strategy \"manual\" — use `mulch compact --apply` to compact manually.",
                                    row.name
                                ),
                            ));
                        }
                        mulch::compact::Outcome::Empty => {
                            return Err(compact_failure(
                                opts.json,
                                "Cannot compact empty record list",
                            ));
                        }
                    }
                }
            };
            let remove: HashSet<Option<String>> = matched.iter().map(id_of).collect();
            updated.retain(|record| !remove.contains(&id_of(record)));
            updated.push(replacement);
            to_archive.extend(matched.iter().cloned());
            total_compacted += matched.len();
            results.push((domain.clone(), candidate.r#type.clone(), matched.len()));
            changed = true;
        }

        if changed {
            // Both halves share compact's handled shape.
            store
                .move_to_archive(&domain, &updated, &to_archive, &now_iso(), "compacted")
                .map_err(|error| {
                    Failure::handled("compact", crate::output::chain_message(error.as_error()))
                })?;
        }
        index = end;
    }

    if opts.json {
        let list: Vec<Value> = results
            .iter()
            .map(|(domain, r#type, count)| {
                let mut entry = Map::new();
                entry.insert("domain".into(), Value::String(domain.clone()));
                entry.insert("type".into(), Value::String(r#type.clone()));
                entry.insert("count".into(), Value::from(*count));
                Value::Object(entry)
            })
            .collect();
        let mut fields = Map::new();
        fields.insert("action".into(), Value::String("auto".into()));
        fields.insert("compacted".into(), Value::from(total_compacted));
        fields.insert("results".into(), Value::Array(list));
        // The skip warnings ride the envelope only when collected
        // (JSON.stringify drops undefined).
        if !warnings.is_empty() {
            fields.insert(
                "warnings".into(),
                Value::Array(
                    warnings
                        .iter()
                        .map(|warning| Value::String(warning.clone()))
                        .collect(),
                ),
            );
        }
        print_json(&success_envelope("compact", fields), false);
        return Ok(());
    }

    print_line(
        opts.quiet,
        &format!(
            "\n✓ Auto-compacted {total_compacted} records across {} groups",
            results.len()
        ),
    );
    // The per-group detail lines print only when a group compacted —
    // a fully-skipped run ends at the ✓ line (hook blocks can empty
    // the results while candidates existed).
    if !results.is_empty() {
        let details: Vec<String> = results
            .iter()
            .map(|(domain, kind, count)| format!("  {domain}/{kind}: {count} records → 1"))
            .collect();
        print_line(opts.quiet, &details.join("\n"));
    }
    if !opts.quiet {
        print_hook_warnings(&warnings);
    }
    Ok(())
}

/// `--apply`: replace `--records` with one explicit replacement.
fn apply(opts: &GlobalOpts, args: &CompactArgs, domain: &str) -> Result<(), Failure> {
    let store = open_store_mut("compact", true)?;
    domain_check(&store, Some(domain), opts.json)?;

    let Some(raw) = args.records.as_deref() else {
        return Err(compact_failure(
            opts.json,
            "--records is required for --apply.",
        ));
    };

    let records = read_domain_records("compact", &store, domain, opts.allow_unknown_types)?;
    let identifiers: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect();
    let mut indices: Vec<usize> = Vec::new();
    for identifier in &identifiers {
        let index = mulch::resolve_record_id(&records, identifier)
            .map_err(|error| crate::commands::failure::resolve_failure("compact", error))?;
        indices.push(index);
    }
    if indices.len() < 2 {
        return Err(compact_failure(
            opts.json,
            "Compaction requires at least 2 records.",
        ));
    }

    let record_type = args
        .record_type
        .clone()
        .or_else(|| {
            records[indices[0]]
                .record
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "convention".into());
    let source: Vec<Value> = indices
        .iter()
        .map(|index| records[*index].record.clone())
        .collect();
    // The reference's truthy-id filter (`filter(Boolean)`): empty
    // strings drop alongside absent ids.
    let compacted_from: Vec<String> = truthy_ids(&source);

    // Pre-compact hook: when the store schedules one it runs BEFORE
    // the replacement is built — a block aborts before any write; the
    // engine's own warnings print plain-mode only (not quiet-gated);
    // a truthy `payload.replacement` then overrides the user flags.
    let phase = HookPhase::run(
        mulch::hooks::HookEvent::PreCompact,
        pre_compact_payload(domain, &record_type, &source),
        &store,
        "compact",
    )?;
    if let Some(reason) = phase.blocked_reason(PRE_COMPACT_BLOCKED) {
        return Err(compact_failure(opts.json, reason));
    }
    if !opts.json {
        print_hook_warnings(phase.warnings());
    }
    let replacement = match adopt_replacement(phase.payload(), &compacted_from, store.registry()) {
        Err(reason) => return Err(compact_failure(opts.json, reason)),
        Ok(Some(record)) => record,
        Ok(None) => {
            let mut replacement = match record_type.as_str() {
                "convention" => {
                    let content = args.content.clone().or_else(|| args.description.clone());
                    match content {
                        Some(content) => replacement_body("convention", &[("content", content)]),
                        None => {
                            return Err(compact_failure(
                                opts.json,
                                "Replacement convention requires --content or --description.",
                            ));
                        }
                    }
                }
                "pattern" | "reference" | "guide" => {
                    match (args.name.clone(), args.description.clone()) {
                        (Some(name), Some(description)) => replacement_body(&record_type, &[
                            ("name", name),
                            ("description", description),
                        ]),
                        _ => {
                            return Err(compact_failure(
                                opts.json,
                                format!(
                                    "Replacement {record_type} requires --name and --description."
                                ),
                            ));
                        }
                    }
                }
                "failure" => match (args.description.clone(), args.resolution.clone()) {
                    (Some(description), Some(resolution)) => replacement_body("failure", &[
                        ("description", description),
                        ("resolution", resolution),
                    ]),
                    _ => {
                        return Err(compact_failure(
                            opts.json,
                            "Replacement failure requires --description and --resolution.",
                        ));
                    }
                },
                "decision" => match (args.title.clone(), args.rationale.clone()) {
                    (Some(title), Some(rationale)) => {
                        replacement_body("decision", &[("title", title), ("rationale", rationale)])
                    }
                    _ => {
                        return Err(compact_failure(
                            opts.json,
                            "Replacement decision requires --title and --rationale.",
                        ));
                    }
                },
                other => {
                    return Err(compact_failure(
                        opts.json,
                        format!("Unknown record type \"{other}\"."),
                    ));
                }
            };

            if !compacted_from.is_empty()
                && let Some(object) = replacement.as_object_mut()
            {
                object.insert(
                    "supersedes".to_string(),
                    Value::Array(
                        compacted_from
                            .iter()
                            .map(|id| Value::String(id.clone()))
                            .collect(),
                    ),
                );
            }
            mulch::compact::stamp_id(&mut replacement, store.registry());

            if let Verdict::OneOf(subs) = verdict(store.registry(), &replacement) {
                return Err(compact_failure(
                    opts.json,
                    format!(
                        "Replacement record failed validation: {}",
                        render_subs(&subs).join("; ")
                    ),
                ));
            }
            replacement
        }
    };

    let remove: HashSet<usize> = indices.iter().copied().collect();
    let mut remaining: Vec<Value> = records
        .iter()
        .enumerate()
        .filter(|(index, _)| !remove.contains(index))
        .map(|(_, line)| line.record.clone())
        .collect();
    remaining.push(replacement.clone());
    // Both halves share compact's handled shape.
    store
        .move_to_archive(domain, &remaining, &source, &now_iso(), "compacted")
        .map_err(|error| {
            Failure::handled("compact", crate::output::chain_message(error.as_error()))
        })?;

    if opts.json {
        let mut fields = Map::new();
        fields.insert("action".into(), Value::String("applied".into()));
        fields.insert("domain".into(), Value::String(domain.to_string()));
        fields.insert("removed".into(), Value::from(indices.len()));
        fields.insert("replacement".into(), replacement);
        print_json(&success_envelope("compact", fields), false);
    } else {
        print_line(
            opts.quiet,
            format!(
                "✓ Compacted {} {record_type} records into 1 in {domain}",
                indices.len()
            )
            .as_str(),
        );
    }
    Ok(())
}

/// The validated domain list: one domain or all, with the shared
/// not-found error.
fn domain_check(
    store: &mulch::StoreFiles,
    domain: Option<&str>,
    json: bool,
) -> Result<Vec<String>, Failure> {
    let domains = store.domains();
    match domain {
        Some(domain) if domains.iter().any(|d| d == domain) => Ok(vec![domain.to_string()]),
        Some(domain) => Err(compact_failure(
            json,
            format!("Domain \"{domain}\" not found in config."),
        )),
        None => Ok(domains),
    }
}

/// The strict positive-int flag law (rank's `parseStrictPositiveInt`).
fn strict_flag(json: bool, flag: &str, raw: Option<&str>) -> Result<usize, Failure> {
    match raw.and_then(mulch::parse_strict_positive_int) {
        Some(value) => Ok(value),
        None => Err(compact_failure(
            json,
            format!(
                "{flag} must be a positive integer (got \"{}\").",
                raw.unwrap_or_default()
            ),
        )),
    }
}

/// The store's staleness rule.
fn stale_rule(store: &mulch::StoreFiles) -> StaleRule {
    StaleRule::from_shelf_life(&store.config().effective_shelf_life())
}

// ---- pre-compact hooks (mulch-7f49; reference resolveReplacement) ----

/// The blocked fallback reason (the reference's
/// `blockReason ?? "pre-compact hook blocked the compaction"`).
const PRE_COMPACT_BLOCKED: &str = "pre-compact hook blocked the compaction";

/// The `{domain, type, records}` pre-compact payload both compact
/// paths send (`records` = the group's raw source records).
fn pre_compact_payload(domain: &str, record_type: &str, records: &[Value]) -> Value {
    serde_json::json!({
        "domain": domain,
        "type": record_type,
        "records": records,
    })
}

/// The per-group skip warning (`{domain}/{type}: {reason}`).
fn group_warning(domain: &str, record_type: &str, reason: &str) -> String {
    format!("{domain}/{record_type}: {reason}")
}

/// The truthy source ids — the reference's `r.id` `filter(Boolean)`
/// (an empty string is falsy and drops alongside an absent id).
fn truthy_ids(records: &[Value]) -> Vec<String> {
    records
        .iter()
        .filter_map(id_of)
        .filter(|id| !id.is_empty())
        .collect()
}

/// JS truthiness of `payload.replacement` (the reference's
/// `hookRes.payload?.replacement` guard): absent, null, false, 0 and
/// `""` fall back to the mechanical merge; any object or array —
/// even an empty one — is adopted.
fn truthy_replacement(payload: &Value) -> bool {
    match payload.get("replacement") {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Bool(true) | Value::Array(_) | Value::Object(_)) => true,
    }
}

/// The reference's `{ ...payload.replacement }` spread: objects copy
/// shallow; arrays and strings spread to index keys; every other
/// scalar spreads to an empty object.
fn spread_record(value: &Value) -> Value {
    let mut out = Map::new();
    match value {
        Value::Object(map) => return Value::Object(map.clone()),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                out.insert(index.to_string(), item.clone());
            }
        }
        // JS spreads UTF-16 code units; chars are equivalent for the
        // BMP and astral input is already undefined behavior territory.
        Value::String(text) => {
            for (index, unit) in text.chars().enumerate() {
                out.insert(index.to_string(), Value::String(unit.to_string()));
            }
        }
        _ => {}
    }
    Value::Object(out)
}

/// The hook-replacement adoption half shared by both compact paths
/// (reference `resolveReplacement` / `handleApply`): when the payload
/// carries a truthy `replacement`, take a copy, backfill `supersedes`
/// from the source records' truthy ids only when the hook left it
/// absent or an empty array, DELETE any incoming id, validate against
/// the unified schema, and stamp the deterministic id. `Err` carries
/// the invalid-record reason; `Ok(None)` means no replacement (the
/// caller takes its mechanical/user-flag path).
fn adopt_replacement(
    payload: &Value,
    source_ids: &[String],
    registry: &mulch::Registry,
) -> Result<Option<Value>, String> {
    if !truthy_replacement(payload) {
        return Ok(None);
    }
    let mut record = spread_record(payload.get("replacement").unwrap_or(&Value::Null));
    if let Some(object) = record.as_object_mut() {
        // Backfill only when absent or empty — an explicit non-empty
        // array is kept verbatim (bad ids then fail validation).
        let backfill = object
            .get("supersedes")
            .and_then(Value::as_array)
            .is_none_or(std::vec::Vec::is_empty);
        if backfill {
            object.insert(
                "supersedes".into(),
                Value::Array(
                    source_ids
                        .iter()
                        .map(|id| Value::String(id.clone()))
                        .collect(),
                ),
            );
        }
        // The id is recomputed deterministically from the body.
        object.shift_remove("id");
    }
    if let mulch::schema::FullVerdict::Invalid { subs, .. } =
        mulch::schema::full_verdict(registry, &record)
    {
        return Err(format!(
            "pre-compact hook produced an invalid record: {}",
            render_subs(&subs).join("; ")
        ));
    }
    mulch::compact::stamp_id(&mut record, registry);
    Ok(Some(record))
}

/// Groups records per compact key and keeps stale-or-large groups.
fn find_candidates(
    domain: &str,
    records: &[LineRecord],
    rule: &StaleRule,
    now: jiff::Timestamp,
    min_group_size: usize,
    registry: &mulch::Registry,
) -> Vec<Candidate> {
    // Insertion-ordered groups keyed by type (merge_outcomes: type +
    // id-key value).
    let mut groups: Vec<(String, String, Vec<&LineRecord>)> = Vec::new();
    for line in records {
        let record_type = type_of(&line.record).to_string();
        let key = match registry.row(&record_type) {
            Some(row) if row.compact == CompactStrategy::MergeOutcomes => {
                format!("{record_type}::{}", raw_text(&line.record, &row.id_key))
            }
            _ => record_type.clone(),
        };
        match groups.iter_mut().find(|(existing, _, _)| *existing == key) {
            Some((_, _, members)) => members.push(line),
            None => groups.push((key, record_type, vec![line])),
        }
    }

    let mut candidates = Vec::new();
    for (_, record_type, members) in groups {
        if members.len() < 2 {
            continue;
        }
        let has_stale = members
            .iter()
            .any(|line| matches!(rule.verdict(&line.record, now), StaleVerdict::Stale { .. }));
        if !has_stale && members.len() < min_group_size {
            continue;
        }
        candidates.push(Candidate {
            domain:  domain.to_string(),
            r#type:  record_type,
            records: members
                .iter()
                .map(|line| CandLine {
                    id:          id_of(&line.record),
                    summary:     record_summary_value(registry, &line.record).ok().flatten(),
                    recorded_at: raw_text(&line.record, "recorded_at"),
                })
                .collect(),
        });
    }
    candidates
}

/// The summary/dry-run block: a blank after EVERY group (the
/// candidate loop's console.log), the max-records note last.
fn group_block(
    title: &str,
    verb: &str,
    selected: &[(String, Candidate)],
    total: usize,
    skipped: usize,
) -> Vec<String> {
    let mut lines = vec![String::new(), format!("{title}:"), String::new()];
    lines.push(format!("  {} groups {}", selected.len(), verb));
    lines.push(format!("  {total} records → {} records", selected.len()));
    lines.push(String::new());
    for (domain, candidate) in selected {
        lines.push(format!(
            "{}/{} ({} records)",
            domain,
            candidate.r#type,
            candidate.records.len()
        ));
        lines.extend(preview_lines(&candidate.records, "  "));
        lines.push(String::new());
    }
    if skipped > 0 {
        lines.push(format!(
            "Note: {skipped} additional groups skipped due to --max-records limit"
        ));
        lines.push(String::new());
    }
    lines
}

/// Prints a rendered block on stdout (the reference's console.log
/// chains).
#[allow(clippy::print_stdout, reason = "block rendering is this command's job")]
fn print_block(text: &str) {
    println!("{text}");
}

/// The up-to-three member preview with its `... and N more` tail.
fn preview_lines(records: &[CandLine], indent: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for line in records.iter().take(3) {
        let summary = crate::commands::failure::summary_text(line.summary.as_ref());
        match &line.id {
            Some(id) => lines.push(format!("{indent}{id}: {summary}")),
            None => lines.push(format!("{indent}(no id): {summary}")),
        }
    }
    if records.len() > 3 {
        lines.push(format!("{indent}... and {} more", records.len() - 3));
    }
    lines
}

/// The replacement skeleton: `type`, the branch fields,
/// `classification`, `recorded_at` (the apply-path key order).
fn replacement_body(record_type: &str, fields: &[(&str, String)]) -> Value {
    let mut out = Map::new();
    out.insert("type".into(), Value::String(record_type.to_string()));
    for (field, value) in fields {
        out.insert((*field).to_string(), Value::String(value.clone()));
    }
    out.insert(
        "classification".into(),
        Value::String("foundational".into()),
    );
    out.insert("recorded_at".into(), Value::String(now_iso()));
    Value::Object(out)
}

/// `record.id` as an optional string.
fn id_of(record: &Value) -> Option<String> {
    record.get("id").and_then(Value::as_str).map(str::to_string)
}

/// The record's type string (missing/non-string: empty).
fn type_of(record: &Value) -> &str {
    record.get("type").and_then(Value::as_str).unwrap_or("")
}

/// `String(value ?? "")` of a field.
fn raw_text(record: &Value, field: &str) -> String {
    record
        .get(field)
        .map_or_else(String::new, mulch::value_text)
}
