//! The JS Date law as the reference's record timestamps use it
//! (`new Date(x).getTime()`): the constructor coercion, TimeClip, the
//! string parser (ECMA-262 date-time, date-only UTC midnight, JSC's
//! space separator, zone-less civil time), and the relative-age
//! buckets (`formatTimeAgo`). One owner since mulch-c5c4 — ready,
//! render, status, budget, and prime_ranking had three fidelities of
//! this law between them (the two lib approximations silently
//! diverged on every non-ISO shape the full law handles).

use serde_json::Value;

/// ECMA-262 TimeClip's bound: |milliseconds| above this is an invalid
/// date (`new Date(x).getTime()` is `NaN`).
const MAX_DATE_MS: f64 = 8.64e15;

/// `new Date(record.recorded_at).getTime()` for one record; `None` is
/// the JS `NaN` (missing field included).
pub fn date_ms(record: &Value) -> Option<f64> {
    record.get("recorded_at").and_then(js_date_ms)
}

/// `new Date(value).getTime()` — the JS Date constructor's coercion
/// plus TimeClip.
pub fn js_date_ms(value: &Value) -> Option<f64> {
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
pub fn parse_js_date(text: &str) -> Option<f64> {
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
pub fn now_ms() -> f64 {
    timestamp_ms(jiff::Timestamp::now())
}

/// A jiff Timestamp as the JS law's f64 milliseconds (the one cast
/// site — `as_millisecond` is exact in f64 below 2^53).
pub fn timestamp_ms(then: jiff::Timestamp) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "epoch milliseconds are exact in f64 below 2^53"
    )]
    let millis = then.as_millisecond() as f64;
    millis
}

/// `formatTimeAgo` over jiff Timestamps — the parsed-stamp view the
/// CLI's render/status adapters delegate to.
pub fn time_ago_ts(then: jiff::Timestamp, now: jiff::Timestamp) -> String {
    time_ago(Some(timestamp_ms(then)), timestamp_ms(now))
}

/// `formatTimeAgo`: the minute / hour / day buckets, `just now` for
/// anything under a minute (future stamps included — the difference is
/// negative), and the reference's `NaNd ago` when the date is invalid
/// (`Math.floor(NaN)` falls through every bound).
pub fn time_ago(recorded: Option<f64>, now: f64) -> String {
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

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{js_date_ms, time_ago};

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
}
