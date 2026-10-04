//! `mulch outcome <domain> <id>` — append an outcome entry or list
//! the record's outcomes (mulch-b88b).

use mulch::value_text;
use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, OutcomeFlags};
use crate::commands::now_iso;
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
    let store = crate::commands::open_store("outcome", false)?;

    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        return Err(if opts.json {
            crate::commands::domain_not_found_json("outcome", domain, &domains)
        } else {
            crate::commands::domain_not_found("outcome", domain, &domains)
        });
    }

    match flags.status.as_deref() {
        Some(status) => append(opts, &store, domain, id, status, flags),
        None => list(opts, &store, domain, id),
    }
}

/// Reads the domain strictly and resolves `id` like delete/move
/// (exact id, bare hash, or a unique prefix — reference
/// `resolveRecordId`).
fn read_and_locate(
    store: &mulch::StoreFiles,
    domain: &str,
    id: &str,
    allow_unknown_types: bool,
) -> Result<(Vec<mulch::LineRecord>, usize), Failure> {
    let records = store
        .read_records(domain, allow_unknown_types)
        .map_err(|source| {
            Failure::handled_on_stderr("outcome", crate::commands::render_core_error(&source))
        })?;
    // Identifier resolution like delete/move (mulch-351d).
    let position = mulch::resolve_record_id(&records, id)
        .map_err(|error| crate::commands::resolve_failure("outcome", error))?;
    Ok((records, position))
}

/// The read-only branch (no `--status`): prints the record's outcomes
/// — empty notice, listing, or JSON envelope — and leaves the store
/// untouched (reference contract; the pre-b88b code appended a
/// schema-invalid outcome instead).
fn list(
    opts: &GlobalOpts,
    store: &mulch::StoreFiles,
    domain: &str,
    id: &str,
) -> Result<(), Failure> {
    let (records, position) = read_and_locate(store, domain, id, opts.allow_unknown_types)?;
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

/// The count the reference's header template prints for an outcomes
/// value: array length, string `.length` (UTF-16 units in JS, chars
/// here — astral-plane corrupt values would differ), `undefined` for
/// every other shape.
fn js_length(value: &Value) -> String {
    match value {
        Value::Array(items) => items.len().to_string(),
        Value::String(text) => text.chars().count().to_string(),
        _ => "undefined".to_string(),
    }
}

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
    domain: &str,
    id: &str,
    status: &str,
    flags: &OutcomeFlags,
) -> Result<(), Failure> {
    let (mut records, position) = read_and_locate(store, domain, id, opts.allow_unknown_types)?;
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
    let mut outcome = Map::new();
    outcome.insert("status".into(), Value::String(status.into()));
    outcome.insert("recorded_at".into(), Value::String(now_iso()));
    if let Some(number) = flags
        .duration
        .as_deref()
        .and_then(|d| d.parse::<u64>().ok())
    {
        outcome.insert("duration".into(), Value::from(number));
    }
    if let Some(agent) = &flags.agent {
        outcome.insert("agent".into(), Value::String(agent.clone()));
    }
    if let Some(notes) = &flags.notes {
        outcome.insert("notes".into(), Value::String(notes.clone()));
    }
    if let Some(test_results) = &flags.test_results {
        outcome.insert("test_results".into(), Value::String(test_results.clone()));
    }

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
    store
        .rewrite_domain(domain, &payload)
        .map_err(|source| Failure::handled("outcome", crate::output::chain_message(&source)))?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("action".into(), Value::String("appended".into()));
        fields.insert("domain".into(), Value::String(domain.into()));
        fields.insert("id".into(), Value::String(resolved_id.clone()));
        fields.insert("outcome".into(), Value::Object(outcome));
        fields.insert("total_outcomes".into(), Value::from(total as u64));
        print_json(&success_envelope("outcome", fields), false);
    } else {
        let attribution = flags
            .agent
            .as_deref()
            .map_or_else(String::new, |agent| format!(" ({agent})"));
        print_line(
            opts.quiet,
            &format!("✓ Outcome recorded: {status}{attribution} on {resolved_id}"),
        );
    }
    Ok(())
}

/// JS truthiness for the optional listing fields (the reference's bare
/// field checks): empty string, 0 and null are skipped; every array
/// and object is truthy, even an empty one.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
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

    #[test]
    fn js_length_matches_the_reference_header_count() {
        assert_eq!(js_length(&serde_json::json!([])), "0");
        assert_eq!(js_length(&serde_json::json!([1, 2])), "2");
        assert_eq!(js_length(&serde_json::json!("not-an-array")), "12");
        assert_eq!(js_length(&serde_json::json!(42)), "undefined");
        assert_eq!(js_length(&serde_json::json!({})), "undefined");
    }

    #[test]
    fn truthy_and_value_text_pin_the_js_contracts() {
        assert!(truthy(&serde_json::json!([])));
        assert!(truthy(&serde_json::json!({})));
        assert!(truthy(&serde_json::json!(1)));
        assert!(!truthy(&serde_json::json!("")));
        assert!(!truthy(&serde_json::json!(0)));
        assert!(!truthy(&serde_json::json!(null)));

        assert_eq!(value_text(&serde_json::json!("raw")), "raw");
        assert_eq!(value_text(&serde_json::json!(null)), "null");
        assert_eq!(value_text(&serde_json::json!(42)), "42");
        assert_eq!(value_text(&serde_json::json!(true)), "true");
        assert_eq!(value_text(&serde_json::json!(["a", "b"])), "a,b");
        assert_eq!(value_text(&serde_json::json!({})), "[object Object]");
    }
}
