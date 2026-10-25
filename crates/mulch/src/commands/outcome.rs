//! `mulch outcome <domain> <id>` — append an outcome entry or list
//! the record's outcomes (mulch-b88b).

use mulch::{js_length, truthy, value_text};
use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, OutcomeFlags};
use crate::commands::failure::{DomainFailure, now_iso};
use crate::commands::store::locate_record;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `outcome`: with `--status` it appends
/// `{status, recorded_at, duration?, agent?, notes?, test_results?}`
/// (only provided keys) to the record's `outcomes` array at line end.
/// Without `--status` it is read-only: it lists the record's outcomes
/// and never writes (reference `commands/outcome.ts`).
pub(super) fn run(
    opts: &GlobalOpts,
    domain: &str,
    id: &str,
    flags: &OutcomeFlags,
) -> Result<(), Failure> {
    let (store, records, position) = locate_record(
        opts,
        "outcome",
        domain,
        id,
        DomainFailure::Standard,
        false,
        true,
    )?;

    match flags.status.as_deref() {
        Some(status) => append(opts, &store, records, position, domain, id, status, flags),
        None => list(opts, &records, position, domain),
    }
}

/// The read-only branch (no `--status`): prints the record's outcomes
/// — empty notice, listing, or JSON envelope — and leaves the store
/// untouched (reference contract; the pre-b88b code appended a
/// schema-invalid outcome instead).
fn list(
    opts: &GlobalOpts,
    records: &[mulch::LineRecord],
    position: usize,
    domain: &str,
) -> Result<(), Failure> {
    // The header carries the record's own id: resolution only ever
    // matches records that have one (the reference's `record.id ?? id`
    // fallback is equally dead there).
    let record_id = records[position]
        .id()
        .expect("resolve_record_id matches only identified records");
    // `record.outcomes ?? []`: absent and null both behave as empty;
    // any other value passes through raw (the json envelope echoes it
    // verbatim, the plain listing trips over it below).
    let outcomes = match records[position].record.get("outcomes") {
        Some(value) if !value.is_null() => value.clone(),
        _ => Value::Array(Vec::new()),
    };

    if opts.json {
        let mut fields = Map::new();
        fields.insert("domain".into(), Value::String(domain.into()));
        fields.insert("id".into(), Value::String(record_id.into()));
        fields.insert("outcomes".into(), outcomes);
        print_json(&success_envelope("outcome", fields), false);
        return Ok(());
    }
    match &outcomes {
        Value::Array(items) => {
            for line in listing_lines(record_id, items) {
                print_line(opts.quiet, &line);
            }
        }
        // A hand-corrupted `outcomes` value: the reference templates
        // `outcomes.length` into the header, then its `.entries()` call
        // throws into the outer catch — header on stdout, engine error
        // on stderr, exit 1 (probe-pinned 2026-10-04, ml 0.10.7).
        other => {
            print_line(
                opts.quiet,
                &format!("Outcomes for {record_id} ({}):", js_length(other)),
            );
            return Err(Failure::handled_on_stderr("outcome", JS_ENTRIES_ERROR));
        }
    }
    Ok(())
}

/// The reference's engine error for a non-array `outcomes` value
/// (`outcome.ts:90-97`, JSC text via the outer catch).
const JS_ENTRIES_ERROR: &str = "Error: outcomes.entries is not a function. (In 'outcomes.entries()', 'outcomes.entries' is undefined)";

/// The plain listing (reference rendering; chalk styling strips when
/// piped, which is the byte contract our output pins).
fn listing_lines(id: &str, outcomes: &[Value]) -> Vec<String> {
    if outcomes.is_empty() {
        return vec!["No outcomes recorded for this record.".into()];
    }
    let mut lines = Vec::with_capacity(outcomes.len() * 5 + 1);
    lines.push(format!("Outcomes for {id} ({}):", outcomes.len()));
    for (index, outcome) in outcomes.iter().enumerate() {
        let fields = outcome.as_object();
        // The reference template-stringifies a missing status as
        // "undefined"; keep that byte shape for hand-edited stores.
        let status = fields
            .and_then(|fields| fields.get("status"))
            .map_or_else(|| "undefined".into(), value_text);
        let attribution = fields
            .and_then(|fields| fields.get("agent"))
            .filter(|agent| truthy(agent))
            .map_or_else(String::new, |agent| format!(" ({})", value_text(agent)));
        lines.push(format!("  {}. {status}{attribution}", index + 1));
        // `duration` prints when present (even null); the other detail
        // lines only for truthy fields, like the reference's checks.
        if let Some(duration) = fields.and_then(|fields| fields.get("duration")) {
            lines.push(format!("     duration: {}ms", value_text(duration)));
        }
        if let Some(tests) = fields
            .and_then(|fields| fields.get("test_results"))
            .filter(|tests| truthy(tests))
        {
            lines.push(format!("     tests: {}", value_text(tests)));
        }
        if let Some(notes) = fields
            .and_then(|fields| fields.get("notes"))
            .filter(|notes| truthy(notes))
        {
            lines.push(format!("     notes: {}", value_text(notes)));
        }
        if let Some(recorded_at) = fields
            .and_then(|fields| fields.get("recorded_at"))
            .filter(|recorded_at| truthy(recorded_at))
        {
            lines.push(format!("     recorded: {}", value_text(recorded_at)));
        }
    }
    lines
}

/// The append branch (`--status` set): appends the outcome and
/// rewrites the domain file compactly.
fn append(
    opts: &GlobalOpts,
    store: &mulch::StoreFiles,
    mut records: Vec<mulch::LineRecord>,
    position: usize,
    domain: &str,
    id: &str,
    status: &str,
    flags: &OutcomeFlags,
) -> Result<(), Failure> {
    // Output surfaces carry the resolved record's own id (the
    // reference's `record.id ?? id` fallback is dead: resolution
    // matches only identified records).
    let resolved_id = records[position]
        .id()
        .expect("resolve_record_id matches only identified records")
        .to_string();
    let mut record: Map<String, Value> = match records[position].record.as_object().cloned() {
        Some(object) => object,
        None => {
            return Err(Failure::handled_on_stderr(
                "outcome",
                format!("Error: Record \"{id}\" is not a JSON object."),
            ));
        }
    };
    // The timestamped entry family (status, recorded_at, duration,
    // agent, notes, test_results) and the strict duration parse live
    // in the lib seam.
    let now = now_iso();
    let outcome = mulch::OutcomeEntry {
        timestamped: true,
        status,
        now: &now,
        duration: flags.duration.as_deref(),
        duration_flag: "--duration",
        agent: flags.agent.as_deref(),
        notes: flags.notes.as_deref(),
        test_results: flags.test_results.as_deref(),
    }
    .build()
    .map_err(|message| Failure::handled_on_stderr("outcome", format!("Error: {message}")))?;

    let total = {
        let array = record
            .entry("outcomes")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("outcomes stays an array");
        array.push(Value::Object(outcome.clone()));
        array.len()
    };
    records[position].record = Value::Object(record);
    let payload: Vec<Value> = records.iter().map(|line| line.record.clone()).collect();
    crate::commands::store::persist("outcome", store, domain, &payload)?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("action".into(), Value::String("appended".into()));
        fields.insert("domain".into(), Value::String(domain.into()));
        fields.insert("id".into(), Value::String(resolved_id.clone()));
        fields.insert("outcome".into(), Value::Object(outcome));
        fields.insert("total_outcomes".into(), Value::from(total as u64));
        print_json(&success_envelope("outcome", fields), false);
    } else {
        // JS truthiness (mulch-1209): an empty `--agent` renders no
        // attribution parens (the entry itself already dropped it).
        let attribution = mulch::truthy_str(flags.agent.as_deref())
            .map_or_else(String::new, |agent| format!(" ({agent})"));
        print_line(
            opts.quiet,
            &format!("✓ Outcome recorded: {status}{attribution} on {resolved_id}"),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_lines_render_the_reference_surfaces() {
        let notice = listing_lines("mx-x", &[]);
        assert_eq!(notice, vec![
            "No outcomes recorded for this record.".to_string()
        ]);

        let outcomes: Vec<Value> = vec![
            serde_json::json!({
                "status": "success",
                "agent": "probe",
                "duration": 42,
                "test_results": "3 passed",
                "notes": "went fine",
                "recorded_at": "2026-10-04T08:25:50.396Z"
            }),
            serde_json::json!({
                "status": "failure",
                "notes": "second",
                "recorded_at": "2026-10-04T08:25:50.984Z"
            }),
        ];
        assert_eq!(listing_lines("mx-x", &outcomes), vec![
            "Outcomes for mx-x (2):",
            "  1. success (probe)",
            "     duration: 42ms",
            "     tests: 3 passed",
            "     notes: went fine",
            "     recorded: 2026-10-04T08:25:50.396Z",
            "  2. failure",
            "     notes: second",
            "     recorded: 2026-10-04T08:25:50.984Z",
        ]);
    }

    #[test]
    fn listing_lines_follow_js_truthiness_on_odd_shapes() {
        let outcomes: Vec<Value> = vec![serde_json::json!({
            // a missing status stringifies as "undefined"
            "agent": {},
            "duration": null,
            "notes": [],
            "test_results": ["a", null, "b"],
            "recorded_at": ""
        })];
        assert_eq!(listing_lines("mx-x", &outcomes), vec![
            "Outcomes for mx-x (1):",
            "  1. undefined ([object Object])",
            "     duration: nullms",
            "     tests: a,,b",
            // empty array is truthy: the line prints with no value
            "     notes: ",
        ]);
    }
}
