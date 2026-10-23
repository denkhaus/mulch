//! `mulch rank` — records ranked by confirmation-frequency score
//! (mulch-16da). Reference: commands/rank.ts — strict flag parses
//! (limit first, then min-score), config-order domain walk, strict
//! record reads, `computeConfirmationScore` ordering with the
//! `< minScore` skip, stable descending sort, slice to the limit.
//! Probed live vs ml 0.10.7, 2026-10-23 (/tmp/opencode/childC).

use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::{open_store, render_core_error};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// One ranked row: the domain (for display), the normalized record,
/// and its confirmation score.
struct RankedRecord {
    domain: String,
    record: Value,
    score:  f64,
}

/// Runs `rank` (plain and `--json`).
pub(crate) fn run(
    opts: &GlobalOpts,
    domain: Option<&str>,
    record_type: Option<&str>,
    limit: &str,
    min_score: &str,
) -> Result<(), Failure> {
    // The strict parses run BEFORE the store opens (probed: a
    // no-store dir rejects `--limit abc` with the flag error, not the
    // no-store one) — limit first, then min-score.
    let limit = parse_strict_positive_int(limit)
        .ok_or_else(|| flag_failure("limit", "positive integer", limit))?;
    let min_score = parse_strict_non_negative_number(min_score)
        .ok_or_else(|| flag_failure("min-score", "non-negative number", min_score))?;

    let store = open_store("rank", true)?;
    let domains = store.domains();
    let domains_to_rank = match domain {
        Some(wanted) => {
            if !domains.iter().any(|known| known == wanted) {
                return Err(unknown_domain(opts.json, wanted, &domains));
            }
            vec![wanted.to_string()]
        }
        None => domains,
    };

    let mut ranked: Vec<RankedRecord> = Vec::new();
    for domain in domains_to_rank {
        let records = store
            .read_records(&domain, opts.allow_unknown_types)
            .map_err(|source| Failure::handled_on_stderr("rank", render_core_error(&source)))?;
        for record in records {
            let record = record.record;
            if let Some(want) = record_type
                && record.get("type").and_then(Value::as_str) != Some(want)
            {
                continue;
            }
            let score = mulch::confirmation_score(&record);
            if score < min_score {
                continue;
            }
            ranked.push(RankedRecord {
                domain: domain.clone(),
                record,
                score,
            });
        }
    }

    // Stable descending sort: ties keep the domain-walk order (probe:
    // the three score-0 records list alpha, beta, gamma — config
    // order).
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let limit = limit.min(ranked.len());
    let top = &ranked[..limit];

    if opts.json {
        // The summary read can fail per record (`getRecordSummary`
        // throws on unregistered types — reachable only under
        // --allow-unknown-types); the whole envelope builds before
        // anything prints, so the failure is a bare error envelope.
        let mut rows = Vec::with_capacity(top.len());
        for entry in top {
            rows.push(ranked_json(&store, entry)?);
        }
        let mut fields = Map::new();
        fields.insert("count".into(), Value::from(top.len()));
        fields.insert("records".into(), Value::Array(rows));
        print_json(&success_envelope("rank", fields), false);
        return Ok(());
    }

    if top.is_empty() {
        print_line(false, "No records match the ranking criteria.");
        return Ok(());
    }

    let scope = domain.unwrap_or("all domains");
    print_line(
        false,
        &format!(
            "Top {} record{} by confirmation score ({scope})",
            top.len(),
            if top.len() == 1 { "" } else { "s" }
        ),
    );
    print_line(false, "");
    for entry in top {
        // Plain mode streams row by row: rows before a failing
        // summary HAVE printed when the error surfaces (probed).
        print_line(false, &plain_row(&store, entry)?);
    }
    Ok(())
}

/// `parseStrictPositiveInt`: `/^\d+$/` plus a JS `Number()` that must
/// stay finite and `>= 1` — so `0`, `-1`, `3.7`, `10abc` reject while
/// `007` (7) and huge digit strings (1e24) pass; the slice clamps.
fn parse_strict_positive_int(raw: &str) -> Option<usize> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let number: f64 = raw.parse().ok()?;
    if !number.is_finite() || number < 1.0 {
        return None;
    }
    // JS slice(0, 1e24) clamps to the array length; usize::MAX clamps
    // the same way for every real store size.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounded by usize::MAX and finite (Number semantics)"
    )]
    Some(number.min(usize::MAX as f64) as usize)
}

/// `parseStrictNonNegativeNumber` (utils/numeric-flags.ts — the
/// `jsval` seam): digits with an optional fractional part, JS-Number
/// canonicalized, never negative.
fn parse_strict_non_negative_number(raw: &str) -> Option<f64> {
    mulch::parse_non_negative_number(raw).and_then(|value| value.as_f64())
}

/// The strict flag failure: `Error: --<flag> must be a <what> (got
/// "<raw>").` on stderr; the json envelope drops the `Error: `
/// prefix (the shared handled law).
fn flag_failure(flag: &str, what: &str, raw: &str) -> Failure {
    Failure::handled_on_stderr(
        "rank",
        format!("Error: --{flag} must be a {what} (got \"{raw}\")."),
    )
}

/// The unknown-domain failure: rank's own two-line plain shape (the
/// hint names THIS binary — the promote law, user directive
/// 2026-10-13; the battery normalizes the name) and the one-line
/// json error.
fn unknown_domain(json: bool, domain: &str, available: &[String]) -> Failure {
    let list = if available.is_empty() {
        "(none)".to_string()
    } else {
        available.join(", ")
    };
    let one_line = format!("Domain \"{domain}\" not found in config. Available domains: {list}");
    let message = if json {
        one_line
    } else {
        format!(
            "Error: {one_line}\nHint: Run `mulch add {domain}` to create this domain, or check .mulch/mulch.config.yaml"
        )
    };
    Failure::handled_on_stderr("rank", message)
}

/// One plain row: `  <id>  ★<score> <domain> [<type>] <summary>`
/// with JS padEnd columns (6 for the star, 14 for domain and type;
/// `★` counts as one UTF-16 unit there and one char here).
fn plain_row(store: &mulch::StoreFiles, entry: &RankedRecord) -> Result<String, Failure> {
    // JS truthiness gates the id column (`record.id ? … : ""`) — its
    // two trailing spaces vanish with it.
    let id = match entry.record.get("id") {
        Some(value) if mulch::truthy(value) => format!("{}  ", mulch::value_text(value)),
        _ => String::new(),
    };
    let score = format_score(entry.score);
    let record_type = format!(
        "[{}]",
        entry
            .record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
    );
    let summary = summary(store, &entry.record)?;
    Ok(format!(
        "  {id}{score:<6}{:<14}{record_type:<14}{summary}",
        entry.domain,
    ))
}

/// `formatScore`: integral scores print bare, others with one fixed
/// decimal (scores are always integral or half-integral, so one
/// decimal is exact — `1.5`, never `1.50`).
fn format_score(score: f64) -> String {
    if score.fract() == 0.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "fract()==0 and non-negative make the cast exact"
        )]
        let whole = score as u64;
        format!("★{whole}")
    } else {
        format!("★{score:.1}")
    }
}

/// The JSON row: `{domain, id (nullish → null), type, score,
/// summary, record}` — the score keeps the JS Number canonical form
/// (`2`, never `2.0`).
fn ranked_json(store: &mulch::StoreFiles, entry: &RankedRecord) -> Result<Value, Failure> {
    let id = entry.record.get("id").cloned().unwrap_or(Value::Null);
    let summary = summary(store, &entry.record)?;
    let score = if entry.score.fract() == 0.0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "fract()==0 makes the cast exact (Number.isInteger)"
        )]
        let whole = entry.score as i64;
        Value::from(whole)
    } else {
        Value::from(entry.score)
    };
    Ok(serde_json::json!({
        "domain": entry.domain,
        "id": id,
        "type": entry.record.get("type").cloned().unwrap_or(Value::Null),
        "score": score,
        "summary": summary,
        "record": entry.record,
    }))
}

/// `getRecordSummary`: the registry summary law — which THROWS on
/// unregistered types (`Unknown record type: <type>`), reachable only
/// under `--allow-unknown-types` (the strict read rejects them
/// otherwise).
fn summary(store: &mulch::StoreFiles, record: &Value) -> Result<String, Failure> {
    let raw_type = record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if store.registry().row(raw_type).is_none() {
        return Err(Failure::handled_on_stderr(
            "rank",
            format!("Error: Unknown record type: {raw_type}"),
        ));
    }
    Ok(mulch::record_summary(store.registry(), record))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The strict limit law: `007` and huge digit strings pass, `0`,
    /// signed, fractional, and garbage reject.
    #[test]
    fn limit_parse_is_strict_positive_int() {
        assert_eq!(parse_strict_positive_int("10"), Some(10));
        assert_eq!(parse_strict_positive_int("007"), Some(7));
        assert_eq!(parse_strict_positive_int("1"), Some(1));
        assert!(parse_strict_positive_int("999999999999999999999999").is_some());
        for reject in ["", "0", "-1", "3.7", "10abc", "+5", " 5", ".5", "5."] {
            assert_eq!(parse_strict_positive_int(reject), None, "{reject}");
        }
    }

    /// The min-score law rides the jsval seam: canonical numbers
    /// pass, everything else rejects.
    #[test]
    fn min_score_parse_rides_the_jsval_seam() {
        assert_eq!(parse_strict_non_negative_number("0"), Some(0.0));
        assert_eq!(parse_strict_non_negative_number("1.5"), Some(1.5));
        assert_eq!(parse_strict_non_negative_number("1.50"), Some(1.5));
        assert!(parse_strict_non_negative_number("999999999999999999999999").is_some());
        for reject in ["abc", "-1", "1e3", ".5", "5.", "1.2.3", ""] {
            assert_eq!(parse_strict_non_negative_number(reject), None, "{reject}");
        }
    }

    /// `formatScore`: integral bare, half-integral one decimal.
    #[test]
    fn score_format_matches_the_reference() {
        assert_eq!(format_score(0.0), "★0");
        assert_eq!(format_score(2.0), "★2");
        assert_eq!(format_score(1.5), "★1.5");
        assert_eq!(format_score(0.5), "★0.5");
    }
}
