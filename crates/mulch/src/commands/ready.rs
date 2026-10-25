//! `mulch ready` — recently added or updated expertise records
//! (mulch-16da). Reference: commands/ready.ts — the config read runs
//! BEFORE every flag parse (a no-store dir rejects `--limit abc` with
//! the config message, and a broken `custom_types` block wins over
//! both), then the strict `--limit` parse, the `--domain` scope check
//! (its own single-line plain wording), the `--since` duration parse,
//! the config-order domain walk (name validation + strict reads), the
//! `recorded_at` DESC sort, the `--since` cutoff, and the limit slice.
//! Probed live vs ml 0.10.7, 2026-10-24 (/tmp/sprint35b/ready).

use std::cmp::Ordering;

use mulch::{DomainSpec, date_ms, now_ms, time_ago};
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::failure::{
    DomainFailure, flag_value_failure, render_core_error, unknown_domain_failure,
};
use crate::commands::store::open_store;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// One collected row: the domain (for display) and the normalized
/// record.
struct ReadyEntry {
    domain: String,
    record: Value,
}

/// Runs `ready` (plain and `--json`).
pub(crate) fn run(
    opts: &GlobalOpts,
    limit: &str,
    domain: Option<&str>,
    since: Option<&str>,
) -> Result<(), Failure> {
    // The reference's readConfig is the command's first statement
    // (probed: `ready --limit abc` in a no-store dir reports the
    // config message, not the flag error — the inverse of rank).
    let store = open_store("ready", true)?;

    let limit = mulch::parse_strict_positive_int(limit)
        .ok_or_else(|| flag_value_failure("ready", "limit", "positive integer", limit))?;

    // `if (options.domain)` / `if (options.since)` are JS truthiness
    // gates: an empty flag value behaves exactly like an absent one
    // (probed: `--domain ""` walks every domain, `--since ""` filters
    // nothing).
    let domain = mulch::truthy_str(domain);
    let since = mulch::truthy_str(since);

    let domains = store.domains();
    let to_check = match domain {
        Some(wanted) => mulch::domain_set(&domains, &DomainSpec::One(wanted.to_string())).map_err(
            |unknown| {
                unknown_domain_failure(
                    "ready",
                    &unknown.domain,
                    &unknown.available,
                    opts.json,
                    DomainFailure::ReadyOneLine,
                )
            },
        )?,
        None => domains,
    };

    let since_ms =
        match since {
            Some(raw) => Some(mulch::parse_duration(raw).map_err(|message| {
                Failure::handled_on_stderr("ready", format!("Error: {message}"))
            })?),
            None => None,
        };

    let mut all: Vec<ReadyEntry> = Vec::new();
    for domain in to_check {
        // `getExpertisePath` validates the name per domain, in config
        // order — an invalid configured name aborts the walk before
        // its file is read (probed).
        if !valid_domain(&domain) {
            return Err(invalid_domain(&domain));
        }
        let records = crate::commands::store::read_domain_records(
            "ready",
            &store,
            &domain,
            opts.allow_unknown_types,
        )?;
        all.extend(records.into_iter().map(|line| ReadyEntry {
            domain: domain.clone(),
            record: line.record,
        }));
    }

    // `recorded_at` DESC, stable: equal timestamps keep the domain-walk
    // order, and an unparsable date compares equal to everything (the
    // JS `NaN` comparator, which Array#sort reads as +0).
    all.sort_by(|a, b| match (date_ms(&a.record), date_ms(&b.record)) {
        (Some(left), Some(right)) => right.partial_cmp(&left).unwrap_or(Ordering::Equal),
        _ => Ordering::Equal,
    });

    if let Some(span) = since_ms {
        let cutoff = now_ms() - span;
        // `new Date(x).getTime() >= cutoff` — an invalid date is NaN,
        // so it never passes the filter.
        all.retain(|entry| date_ms(&entry.record).is_some_and(|ms| ms >= cutoff));
    }

    all.truncate(limit);

    if opts.json {
        // The envelope builds completely before anything prints: a
        // failing summary read yields the bare error envelope.
        let mut rows = Vec::with_capacity(all.len());
        for entry in &all {
            rows.push(json_entry(&store, entry)?);
        }
        let mut fields = Map::new();
        fields.insert("count".into(), Value::from(all.len()));
        fields.insert("entries".into(), Value::Array(rows));
        print_json(&success_envelope("ready", fields), false);
        return Ok(());
    }

    if all.is_empty() {
        print_line(false, "No recent expertise records found.");
        return Ok(());
    }

    // `ready` prints through `console.log`, not the quiet-aware
    // helpers: `-q` does NOT suppress it (probed).
    let header = since.map_or_else(
        || format!("Recent Expertise (last {} records)", all.len()),
        |raw| format!("Recent Expertise (last {raw})"),
    );
    print_line(false, &header);
    print_line(false, "");

    // The reference reads `new Date()` per row; one instant for the
    // whole table only matters at a bucket boundary.
    let now = now_ms();
    for entry in &all {
        let age = time_ago(date_ms(&entry.record), now);
        // Plain mode streams row by row: rows before a failing summary
        // HAVE printed when the error surfaces (probed).
        let summary = summary(&store, &entry.record)?;
        let summary = summary_text(summary.as_ref());
        print_line(
            false,
            &plain_row(&entry.domain, &entry.record, &age, &summary),
        );
    }
    Ok(())
}

/// `getExpertisePath`'s name check failure (`validateDomainName`),
/// rendered through the shared typed error.
fn invalid_domain(domain: &str) -> Failure {
    Failure::handled_on_stderr(
        "ready",
        render_core_error(&mulch::Error::InvalidDomain {
            domain: domain.to_string(),
        }),
    )
}

/// Reference `validateDomainName` (`^[a-zA-Z0-9][a-zA-Z0-9_-]*$`).
fn valid_domain(domain: &str) -> bool {
    let mut chars = domain.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// One plain row: `  <id>  <age padded 10><domain padded
/// 14><[type] padded 14><summary>` — JS `padEnd` never truncates, so
/// over-long columns run into the next one.
fn plain_row(domain: &str, record: &Value, age: &str, summary: &str) -> String {
    // JS truthiness gates the id column (`record.id ? … : ""`); its
    // two trailing spaces vanish with it.
    let id = match record.get("id") {
        Some(value) if mulch::truthy(value) => format!("{}  ", mulch::value_text(value)),
        _ => String::new(),
    };
    let record_type = format!("[{}]", type_text(record));
    format!("  {id}{age:<10}{domain:<14}{record_type:<14}{summary}")
}

/// The record's `type` as the reference's template literal renders it
/// (`undefined` for a missing field, `value_text` otherwise).
fn type_text(record: &Value) -> String {
    record
        .get("type")
        .map_or_else(|| "undefined".to_string(), mulch::value_text)
}

/// The JSON entry: `{domain, id (nullish → null), type, recorded_at
/// (omitted when the field is missing, the reference's `undefined`),
/// summary, record}`.
fn json_entry(store: &mulch::StoreFiles, entry: &ReadyEntry) -> Result<Value, Failure> {
    let mut fields = Map::new();
    fields.insert("domain".into(), Value::String(entry.domain.clone()));
    fields.insert(
        "id".into(),
        entry.record.get("id").cloned().unwrap_or(Value::Null),
    );
    fields.insert("type".into(), type_text_value(&entry.record));
    if let Some(recorded) = entry.record.get("recorded_at") {
        fields.insert("recorded_at".into(), recorded.clone());
    }
    if let Some(summary) = summary(store, &entry.record)? {
        fields.insert("summary".into(), summary);
    }
    fields.insert("record".into(), entry.record.clone());
    Ok(Value::Object(fields))
}

/// The JSON `type` field: the raw record value (a registered string by
/// the time the envelope builds — an unknown or missing type throws in
/// [`summary`] first).
fn type_text_value(record: &Value) -> Value {
    record.get("type").cloned().unwrap_or(Value::Null)
}

/// `getRecordSummary` as the reference's raw JS value: `None` is
/// `undefined` (plain renders the word, json omits the key),
/// non-strings pass through verbatim, the truncating laws
/// (`convention`/`failure`) fail with the reference's TypeError on a
/// missing or non-string field, and unregistered types throw
/// `Unknown record type: <type>` (reachable under
/// `--allow-unknown-types` and for non-string `type` values — the
/// strict read only rejects unregistered STRING types).
fn summary(store: &mulch::StoreFiles, record: &Value) -> Result<Option<Value>, Failure> {
    let raw_type = record.get("type");
    if let Some(name) = raw_type.and_then(Value::as_str)
        && store.registry().row(name).is_some()
    {
        return mulch::record_summary_value(store.registry(), record).map_err(|not_a_string| {
            Failure::handled_on_stderr("ready", format!("Error: {not_a_string}"))
        });
    }
    Err(Failure::handled_on_stderr(
        "ready",
        format!("Error: Unknown record type: {}", type_text(record)),
    ))
}

/// The plain-row view of a raw summary value: `undefined` for the
/// JSON-omitted case, `String(value)` for everything else.
fn summary_text(summary: Option<&Value>) -> String {
    summary.map_or_else(|| "undefined".into(), mulch::value_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plain row's padEnd columns and the id-column truthiness
    /// gate (empty / zero / null ids drop the column and its spaces).
    #[test]
    fn plain_row_pads_like_the_reference() {
        let record = serde_json::json!({"type": "pattern", "id": "mx-1"});
        assert_eq!(
            plain_row("d", &record, "3d ago", "sum"),
            "  mx-1  3d ago    d             [pattern]     sum"
        );
        let idless = serde_json::json!({"type": "convention"});
        assert_eq!(
            plain_row("verylongdomainname", &idless, "NaNd ago", ""),
            "  NaNd ago  verylongdomainname[convention]  "
        );
        let empty_id = serde_json::json!({"type": "pattern", "id": ""});
        assert_eq!(
            plain_row("d", &empty_id, "1h ago", "s"),
            "  1h ago    d             [pattern]     s"
        );
    }

    /// The domain-name law (`validateDomainName`).
    #[test]
    fn domain_names_follow_the_reference_regex() {
        for valid in ["d", "dev-loop", "dev_loop", "A1", "9"] {
            assert!(valid_domain(valid), "{valid}");
        }
        for invalid in ["", "-d", "_d", "bad name", "d.e", "__proto__", "d/"] {
            assert!(!valid_domain(invalid), "{invalid}");
        }
    }
}
