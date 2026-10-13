//! JS value semantics — the one home of the JavaScript emulation
//! quirks the reference CLI bakes into its behavior: template-literal
//! coercion ([`value_text`]), truthiness for values ([`truthy`]) and
//! optional string flags ([`truthy_str`]), `.length` counting
//! ([`js_length`]), JSON.stringify string encoding ([`js_string`]),
//! JS object property-key enumeration order ([`js_key_sorted`]),
//! Math.round's non-negative clamp ([`js_round_u64`]), the strict
//! numeric-flag parse with Number canonicalization
//! ([`parse_non_negative_number`]), and the strict-equality comparison
//! the reference's dedup uses ([`js_dedup_eq`]). Command modules
//! import from here; private per-command copies and inline matches
//! delete (mulch-ec71; one quirk test surface, [`js_quirk_tests`]).

use serde_json::Value;

/// Renders a JSON value the way a JS template literal or property key
/// would stringify it: strings raw, `null` as "null", arrays joined
/// with "," (null items empty), objects as "[object Object]"
/// (probe-pinned 2026-10-04, ml 0.10.7).
pub fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".into(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => value_text(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
        other => other.to_string(),
    }
}

/// JS truthiness for a JSON value: `null`/`false`/`0`/`""` are falsy,
/// empty arrays and objects are TRUTHY (the reference's bare field
/// checks, e.g. the outcome listing's `if (o.agent)` gates).
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// JS truthiness for an optional string FLAG value: `Some("")` and
/// `None` are both absent (`if (options.agent)` gates — the
/// reference's truthiness store guards, mulch-1209).
pub fn truthy_str(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

/// The count a JS header template would print for `.length`: array
/// length, string `.length` (UTF-16 units in JS, chars here —
/// astral-plane corrupt values would differ), `undefined` for every
/// other shape.
pub fn js_length(value: &Value) -> String {
    match value {
        Value::Array(items) => items.len().to_string(),
        Value::String(text) => text.chars().count().to_string(),
        _ => "undefined".to_string(),
    }
}

/// JSON.stringify for one string: serde_json's string encoding is
/// byte-identical to the reference's quoting (mulch-2153).
pub fn js_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a str cannot fail")
}

/// JS strict equality (`===`) for the reference's dedup-field
/// comparison (`findDuplicate`, utils/expertise.ts): missing fields
/// compare equal (`undefined === undefined`), numbers compare as f64
/// (`1` equals `1.0` — both parse to the same JS number), and objects
/// or arrays never match because `===` compares object identity and
/// separately parsed lines never share one.
pub fn js_dedup_eq(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Value::Number(a)), Some(Value::Number(b))) => a.as_f64() == b.as_f64(),
        // objects/arrays never match (JS === compares identity), and
        // undefined never equals a present value (and vice versa) —
        // this arm MUST precede the structural-eq arm or identical
        // object shapes would wrongly match
        (Some(Value::Object(_) | Value::Array(_)), _)
        | (_, Some(Value::Object(_) | Value::Array(_)))
        | (None, Some(_))
        | (Some(_), None) => false,
        (Some(a), Some(b)) => a == b,
    }
}

/// The reference's strict numeric-flag parse
/// (`parseStrictNonNegativeNumber`, utils/numeric-flags.ts). A
/// deliberate public seam of its own: the reference feeds
/// `rank --min-score` from the same util (mulch-16da consumes it when
/// that surface lands). Accepts `/^\d+(\.\d+)?$/` — digits with an
/// optional fractional part, nothing else (`-5`, `1e3`, `.5`, `5.`,
/// `1.2.3`, spaces all reject). Values canonicalize like JS
/// `Number()` + `JSON.stringify`: `"42.0"` stores as the integer `42`,
/// `"42.5"` as the float `42.5`.
pub fn parse_non_negative_number(raw: &str) -> Option<Value> {
    let mut parts = raw.split('.');
    let integral = parts.next().unwrap_or_default();
    let fractional = parts.next();
    if parts.next().is_some()
        || integral.is_empty()
        || !integral.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    if let Some(frac) = fractional
        && (frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let number: f64 = raw.parse().ok()?;
    if !number.is_finite() {
        return None;
    }
    // Everything routes through f64 — JS Number rounds literals above
    // 2^53 to the nearest f64 BEFORE any canonicalization, so the
    // rounded value is what both sides store ("9007199254740993" is
    // 9007199254740992 on both). Integral values keep the full-digit
    // integer FORM while exactly castable (through i64::MAX ≈ 9.2e18 —
    // JS stringify expands digits to 1e21, and serde's f64 form
    // diverges from that above the i64 range: absurd territory for a
    // millisecond duration, documented in README DEVIATIONS).
    if number.fract() == 0.0 && number <= i64::MAX as f64 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "guarded by the fract()==0 and <= i64::MAX bounds"
        )]
        let exact = number as i64;
        return Some(Value::from(exact));
    }
    Some(Value::from(number))
}

/// Math.round's non-negative clamp: finite positive inputs round into
/// u64 (the reference's threshold computations feed it pre-rounded,
/// non-negative values), everything else falls to 0.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "guarded and rounded; mirrors Math.round"
)]
pub fn js_round_u64(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        // truncation cannot lose data past .round() on the
        // pre-rounded, non-negative inputs the callers feed it
        value as u64
    } else {
        0
    }
}

/// JS object enumeration order over `(key, value)` pairs: integer-like
/// keys (non-negative integers, parseable as u64 — no negatives, no
/// leading zeros, which JS treats as string keys) ascending first,
/// then string keys in first-seen order. The reference's counters and
/// every hand-ordered JSON boundary follow it (probe: `{"type": 5}`
/// counts under `"5"` before the seeded names).
pub fn js_key_sorted<T>(entries: &[(String, T)]) -> Vec<&(String, T)> {
    // one partition pass, one parse per entry: the JS array-index
    // check IS the u64 parse (plus the canonical-numeral shape)
    let mut integer_keys: Vec<(u64, &(String, T))> = Vec::new();
    let mut string_keys: Vec<&(String, T)> = Vec::new();
    for entry in entries {
        // JS array-index keys are CANONICAL numerals: digits only, no
        // leading zero (unless "0" itself) — "01" and "-1" are string
        // keys. The u64 bound rejects everything larger than JS
        // indexes themselves would order.
        let shape = !entry.0.is_empty()
            && entry.0.bytes().all(|byte| byte.is_ascii_digit())
            && (entry.0 == "0" || !entry.0.starts_with('0'));
        match entry.0.parse::<u64>() {
            Ok(index) if shape => integer_keys.push((index, entry)),
            _ => string_keys.push(entry),
        }
    }
    integer_keys.sort_by_key(|(index, _)| *index);
    integer_keys
        .into_iter()
        .map(|(_, entry)| entry)
        .chain(string_keys)
        .collect()
}

#[cfg(test)]
mod js_quirk_tests {
    use serde_json::json;

    use super::*;

    /// One table per quirk — the JS contracts every command surface
    /// inherits from this seam (the odd shapes were pinned piecemeal
    /// across the command modules before; they collapse here,
    /// mulch-ec71).
    #[test]
    fn value_text_coerces_like_a_js_template_literal() {
        assert_eq!(value_text(&json!("raw")), "raw");
        assert_eq!(value_text(&json!(null)), "null");
        assert_eq!(value_text(&json!(true)), "true");
        assert_eq!(value_text(&json!(42)), "42");
        assert_eq!(value_text(&json!({})), "[object Object]");
        assert_eq!(value_text(&json!([1, "x", null])), "1,x,");
        // nested arrays JOIN recursively — only objects coerce to
        // [object Object]
        assert_eq!(value_text(&json!([[1]])), "1");
        assert_eq!(value_text(&json!([{"k": 1}])), "[object Object]");
    }

    #[test]
    fn truthiness_covers_the_js_truth_table() {
        for value in [
            json!(1),
            json!("x"),
            json!([]),
            json!({}),
            json!(-1),
            json!(true),
        ] {
            assert!(truthy(&value), "{value} is truthy");
        }
        for value in [json!(0), json!(""), json!(null), json!(false)] {
            assert!(!truthy(&value), "{value} is falsy");
        }
        // the optional-string flag law: Some("") is absent
        assert_eq!(truthy_str(Some("x")), Some("x"));
        assert_eq!(truthy_str(Some("")), None);
        assert_eq!(truthy_str(None), None);
    }

    #[test]
    fn js_length_counts_like_the_header_template() {
        assert_eq!(js_length(&json!([])), "0");
        assert_eq!(js_length(&json!([1, 2])), "2");
        assert_eq!(js_length(&json!("not-an-array")), "12");
        assert_eq!(js_length(&json!(42)), "undefined");
        assert_eq!(js_length(&json!({})), "undefined");
    }

    #[test]
    fn js_string_encodes_like_json_stringify() {
        assert_eq!(js_string("n v"), "\"n v\"");
        assert_eq!(js_string("a\"b"), "\"a\\\"b\"");
        // the JSON escapes are two literal characters (backslash + t)
        assert_eq!(js_string("tab\there"), "\"tab\\there\"");
    }

    #[test]
    fn dedup_equality_compares_numbers_as_f64_and_objects_never() {
        assert!(js_dedup_eq(None, None));
        assert!(js_dedup_eq(Some(&json!(1)), Some(&json!(1.0))));
        assert!(!js_dedup_eq(Some(&json!(1)), Some(&json!(2))));
        assert!(!js_dedup_eq(None, Some(&json!(null))));
        // JS === compares object identity: equal shapes never match
        assert!(!js_dedup_eq(Some(&json!({})), Some(&json!({}))));
        assert!(!js_dedup_eq(Some(&json!([1])), Some(&json!([1]))));
    }

    #[test]
    fn key_enumeration_sorts_integer_keys_first_ascending() {
        let entries = vec![
            ("type".into(), 1),
            ("5".into(), 2),
            ("classification".into(), 3),
            ("01".into(), 4),
        ];
        let ordered: Vec<&str> = js_key_sorted(&entries)
            .into_iter()
            .map(|(key, _)| key.as_str())
            .collect();
        // "01" is NOT an integer key (leading zero) — string order
        assert_eq!(ordered, ["5", "type", "classification", "01"]);
    }

    #[test]
    fn rounding_and_parsing_pin_the_number_boundary() {
        assert_eq!(js_round_u64(14.0), 14);
        assert_eq!(js_round_u64(0.0), 0);
        assert_eq!(js_round_u64(f64::NAN), 0);

        // digits with optional fraction, JS Number canonicalization
        assert_eq!(parse_non_negative_number("42"), Some(json!(42)));
        assert_eq!(parse_non_negative_number("0"), Some(json!(0)));
        // leading zeros collapse like JS Number("007") === 7
        assert_eq!(parse_non_negative_number("007"), Some(json!(7)));
        // "42.0" is JS 42 — the canonical integer form
        assert_eq!(parse_non_negative_number("42.0"), Some(json!(42)));
        assert_eq!(parse_non_negative_number("0.500"), Some(json!(0.5)));
        assert_eq!(parse_non_negative_number("42.5"), Some(json!(42.5)));
        // JS Number rounds literals above 2^53 BEFORE storing
        assert_eq!(
            parse_non_negative_number("9007199254740993"),
            Some(json!(9_007_199_254_740_992i64))
        );
        // beyond the i64 range the f64 holds the same NUMBER as JS,
        // in serde's float byte form (README DEVIATIONS)
        assert_eq!(
            parse_non_negative_number("18446744073709551615").and_then(|value| value.as_f64()),
            Some(1.844_674_407_370_955_2e19)
        );
        // the reference's regex shape: nothing else parses
        for reject in [
            "", "abc", "-5", "+5", "1e3", ".5", "5.", "1.2.3", " 42", "42 ", "４２",
        ] {
            assert_eq!(
                parse_non_negative_number(reject),
                None,
                "raw {reject:?} must reject"
            );
        }
    }
}
