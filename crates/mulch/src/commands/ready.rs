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

use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::{open_store, render_core_error};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// ECMA-262 TimeClip's bound: |milliseconds| above this is an invalid
/// date (`new Date(x).getTime()` is `NaN`).
const MAX_DATE_MS: f64 = 8.64e15;

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

    let limit = parse_strict_positive_int(limit).ok_or_else(|| limit_failure(limit))?;

    // `if (options.domain)` / `if (options.since)` are JS truthiness
    // gates: an empty flag value behaves exactly like an absent one
    // (probed: `--domain ""` walks every domain, `--since ""` filters
    // nothing).
    let domain = mulch::truthy_str(domain);
    let since = mulch::truthy_str(since);

    let domains = store.domains();
    let to_check = match domain {
        Some(wanted) => {
            if !domains.iter().any(|known| known == wanted) {
                return Err(unknown_domain(opts.json, wanted, &domains));
            }
            vec![wanted.to_string()]
        }
        None => domains,
    };

    let since_ms =
        match since {
            Some(raw) => Some(parse_duration(raw).map_err(|message| {
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
        let records = store
            .read_records(&domain, opts.allow_unknown_types)
            .map_err(|source| Failure::handled_on_stderr("ready", render_core_error(&source)))?;
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

/// `parseStrictPositiveInt`: `/^\d+$/` plus a JS `Number()` that must
/// stay finite and `>= 1` (rank.ts's law, shared verbatim by ready).
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

/// `parseDuration`: `/^(\d+)(h|d|w)$/`, the unit in milliseconds. The
/// digit run parses as a JS `Number` (huge runs overflow to infinity,
/// exactly like `parseInt`), so the arithmetic stays in `f64`.
fn parse_duration(raw: &str) -> Result<f64, String> {
    let invalid =
        || format!("Invalid duration: \"{raw}\". Use format like \"24h\", \"7d\", \"2w\".");
    let Some((at, unit)) = raw.char_indices().next_back() else {
        return Err(invalid());
    };
    let digits = &raw[..at];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    let factor = match unit {
        'h' => 3_600_000.0,
        'd' => 86_400_000.0,
        'w' => 7.0 * 86_400_000.0,
        _ => return Err(invalid()),
    };
    let value: f64 = digits.parse().map_err(|_| invalid())?;
    Ok(value * factor)
}

/// The strict `--limit` failure: `Error: --limit must be a positive
/// integer (got "<raw>").` on stderr (the envelope drops the prefix).
fn limit_failure(raw: &str) -> Failure {
    Failure::handled_on_stderr(
        "ready",
        format!("Error: --limit must be a positive integer (got \"{raw}\")."),
    )
}

/// The unknown-domain failure: ready's own single-line plain wording
/// (`domain "x" not found. Available: …` — no `in config`, no
/// `domains`) and the one-line json shape. An empty store renders the
/// trailing `Available: ` with nothing after it.
fn unknown_domain(json: bool, domain: &str, available: &[String]) -> Failure {
    let list = available.join(", ");
    let message = if json {
        format!("Domain \"{domain}\" not found in config. Available domains: {list}")
    } else {
        format!("Error: domain \"{domain}\" not found. Available: {list}")
    };
    Failure::handled_on_stderr("ready", message)
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

/// `new Date(record.recorded_at).getTime()` for one record; `None` is
/// the JS `NaN` (missing field included).
fn date_ms(record: &Value) -> Option<f64> {
    record.get("recorded_at").and_then(js_date_ms)
}

/// `new Date(value).getTime()` — the JS Date constructor's coercion
/// plus TimeClip.
fn js_date_ms(value: &Value) -> Option<f64> {
    let millis = match value {
        // `new Date(null)` is the epoch; booleans coerce to 0 / 1.
        Value::Null => 0.0,
        Value::Bool(flag) => f64::from(u8::from(*flag)),
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => parse_js_date(text)?,
        // Arrays and objects reach the parser through ToPrimitive
        // (`[]` → "", `[1,2]` → "1,2", `{}` → "[object Object]"):
        // none of those parse.
        Value::Array(_) | Value::Object(_) => return None,
    };
    // TimeClip truncates toward zero, then range-checks.
    (millis.abs() <= MAX_DATE_MS).then(|| millis.trunc())
}

/// The string half of [`js_date_ms`]: the ECMA-262 date-time format
/// (`YYYY-MM-DD`, a date-only string being UTC midnight), RFC 3339
/// timestamps with an explicit offset, JSC's space separator between
/// date and time, and a zone-less civil time read in the process time
/// zone. Anything else is `NaN`.
fn parse_js_date(text: &str) -> Option<f64> {
    if let Some(millis) = date_only_midnight(text) {
        return Some(millis);
    }
    // JSC accepts a single space where the format has `T`.
    let normalized;
    let candidate = if text.as_bytes().get(10) == Some(&b' ') {
        normalized = format!("{}T{}", &text[..10], &text[11..]);
        normalized.as_str()
    } else {
        text
    };
    if let Ok(timestamp) = candidate.parse::<jiff::Timestamp>() {
        return Some(timestamp.as_millisecond() as f64);
    }
    // No offset: the reference reads the wall clock in the process
    // time zone (ECMA-262's "local time" branch).
    let datetime = candidate.parse::<jiff::civil::DateTime>().ok()?;
    let zoned = jiff::tz::TimeZone::system().to_timestamp(datetime).ok()?;
    Some(zoned.as_millisecond() as f64)
}

/// `YYYY-MM-DD` → UTC midnight (the spec's date-only form), `None` for
/// every other shape.
fn date_only_midnight(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(at, byte)| at == 4 || at == 7 || byte.is_ascii_digit());
    if !shaped {
        return None;
    }
    let timestamp = format!("{text}T00:00:00Z")
        .parse::<jiff::Timestamp>()
        .ok()?;
    Some(timestamp.as_millisecond() as f64)
}

/// `Date.now()` in epoch milliseconds.
fn now_ms() -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "epoch milliseconds are exact in f64 below 2^53"
    )]
    let millis = jiff::Timestamp::now().as_millisecond() as f64;
    millis
}

/// `formatTimeAgo`: the minute / hour / day buckets, `just now` for
/// anything under a minute (future stamps included — the difference is
/// negative), and the reference's `NaNd ago` when the date is invalid
/// (`Math.floor(NaN)` falls through every bound).
fn time_ago(recorded: Option<f64>, now: f64) -> String {
    let Some(recorded) = recorded else {
        return "NaNd ago".into();
    };
    let diff = now - recorded;
    let minutes = (diff / 60_000.0).floor();
    if minutes < 1.0 {
        return "just now".into();
    }
    if minutes < 60.0 {
        return format!("{}m ago", whole(minutes));
    }
    let hours = (diff / 3_600_000.0).floor();
    if hours < 24.0 {
        return format!("{hours}h ago");
    }
    format!("{}d ago", whole((diff / 86_400_000.0).floor()))
}

/// An integral f64 rendered without a decimal tail (JS number
/// stringification for the bucket counts; `as` saturates, and the
/// TimeClip range keeps every count inside i64).
fn whole(value: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Math.floor of a clipped epoch span fits i64"
    )]
    let count = value as i64;
    count
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

    /// The strict limit law (rank's parse, byte-identical message).
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

    /// The duration law: `^(\d+)(h|d|w)$` in milliseconds, everything
    /// else rejected with the reference message.
    #[test]
    fn duration_parse_matches_the_reference() {
        assert_eq!(parse_duration("24h"), Ok(24.0 * 3_600_000.0));
        assert_eq!(parse_duration("7d"), Ok(7.0 * 86_400_000.0));
        assert_eq!(parse_duration("2w"), Ok(14.0 * 86_400_000.0));
        assert_eq!(parse_duration("0h"), Ok(0.0));
        assert_eq!(parse_duration("0007d"), Ok(7.0 * 86_400_000.0));
        // A 24-digit run is a finite JS number (1e24); only a run past
        // f64's range overflows to Infinity.
        assert_eq!(
            parse_duration("999999999999999999999999w"),
            Ok(1e24 * 604_800_000.0)
        );
        assert!(parse_duration(&format!("{}w", "9".repeat(400))).is_ok_and(f64::is_infinite));
        for reject in ["", "24", "1x", "7D", " 7d", "-1d", "1.5d", "h", "7d ", "é"] {
            let message = parse_duration(reject).expect_err(reject);
            assert_eq!(
                message,
                format!("Invalid duration: \"{reject}\". Use format like \"24h\", \"7d\", \"2w\".")
            );
        }
    }

    /// `new Date(value).getTime()`: offsets, date-only UTC midnight,
    /// the space separator, numeric / null / boolean coercion, and the
    /// NaN cases.
    #[test]
    fn js_date_parse_matches_the_constructor() {
        let ms = |text: &str| js_date_ms(&Value::String(text.into()));
        assert_eq!(ms("1970-01-01T00:00:00.000Z"), Some(0.0));
        assert_eq!(ms("2026-10-03T20:47:57.690Z"), Some(1_791_060_477_690.0));
        assert_eq!(ms("2026-10-03T20:47:57Z"), Some(1_791_060_477_000.0));
        assert_eq!(
            ms("2026-10-03T22:47:57.690+02:00"),
            Some(1_791_060_477_690.0)
        );
        assert_eq!(ms("2026-10-03"), Some(1_790_985_600_000.0));
        assert_eq!(ms("2026-10-03 20:47:57.690Z"), Some(1_791_060_477_690.0));
        assert_eq!(
            js_date_ms(&Value::from(1_234_567_890_123_i64)),
            Some(1_234_567_890_123.0)
        );
        assert_eq!(js_date_ms(&Value::Null), Some(0.0));
        assert_eq!(js_date_ms(&Value::Bool(true)), Some(1.0));
        assert_eq!(js_date_ms(&Value::Bool(false)), Some(0.0));
        // TimeClip: beyond ±8.64e15 is an invalid date.
        assert_eq!(js_date_ms(&Value::from(9.0e15)), None);
        assert_eq!(js_date_ms(&Value::from(-9.0e15)), None);
        for reject in ["not-a-date", "", "2026-13-01", "2026-10-03T25:00:00Z"] {
            assert_eq!(ms(reject), None, "{reject}");
        }
        assert_eq!(js_date_ms(&serde_json::json!({"a": 1})), None);
        assert_eq!(js_date_ms(&serde_json::json!([1, 2])), None);
    }

    /// `formatTimeAgo`'s buckets, the future clamp, and the invalid
    /// date's `NaNd ago`.
    #[test]
    fn time_ago_matches_the_reference_buckets() {
        let now = 1_000_000_000_000.0;
        assert_eq!(time_ago(Some(now), now), "just now");
        assert_eq!(time_ago(Some(now + 60_000.0), now), "just now");
        assert_eq!(time_ago(Some(now - 59_999.0), now), "just now");
        assert_eq!(time_ago(Some(now - 60_000.0), now), "1m ago");
        assert_eq!(time_ago(Some(now - 3_599_999.0), now), "59m ago");
        assert_eq!(time_ago(Some(now - 3_600_000.0), now), "1h ago");
        assert_eq!(time_ago(Some(now - 86_399_999.0), now), "23h ago");
        assert_eq!(time_ago(Some(now - 86_400_000.0), now), "1d ago");
        assert_eq!(time_ago(None, now), "NaNd ago");
    }

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
