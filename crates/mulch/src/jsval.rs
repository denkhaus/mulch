//! JS value semantics — the one home of the JavaScript emulation
//! quirks the reference CLI bakes into its behavior: template-literal
//! coercion ([`value_text`]), truthiness for values ([`truthy`]) and
//! optional string flags ([`truthy_str`]), `.length` counting
//! ([`js_length`]), JSON.stringify string encoding ([`js_string`]),
//! UTF-16 truncation ([`js_truncate`]), JS object property-key
//! enumeration order ([`js_key_sorted`]),
//! Math.round's non-negative clamp ([`js_round_u64`]), the day
//! truncation of an age quotient ([`days_u64`]), the abstract
//! relational comparison (`<`/`>` with ToPrimitive + ToNumber,
//! [`js_rel`]), the strict numeric-flag parse with Number
//! canonicalization ([`parse_non_negative_number`]), and the
//! strict-equality comparison the reference's dedup uses
//! ([`js_dedup_eq`]). Command modules import from here; private
//! per-command copies and inline matches delete (mulch-ec71; one quirk
//! test surface, `js_quirk_tests`).

use std::cmp::Ordering;

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

/// The JS abstract relational comparison (`<` / `>`) over two raw JSON
/// values — the law behind `status`'s oldest/newest `recorded_at`
/// selection: with BOTH ToPrimitive sides strings the comparison is
/// lexicographic; otherwise both sides go through ToNumber, where a
/// NaN on either side makes EVERY comparison false ([`None`] rather
/// than a fourth ordering).
///
/// Consequences pinned by probe (mulch-4d25): a numeric `recorded_at`
/// is never displaced by a date string (string-vs-number coerces the
/// date to NaN, so neither `<` nor `>` fires), and an empty array —
/// ToPrimitive yields the empty string — BEATS every date string
/// lexicographically as a candidate.
pub fn js_rel(left: &Value, right: &Value) -> Option<Ordering> {
    match (to_primitive(left), to_primitive(right)) {
        (Primitive::Str(a), Primitive::Str(b)) => Some(a.cmp(&b)),
        (a, b) => {
            let (a, b) = (to_number(&a), to_number(&b));
            if a.is_nan() || b.is_nan() {
                None
            } else {
                // partial_cmp is total once NaN is excluded
                Some(a.partial_cmp(&b).unwrap_or(Ordering::Equal))
            }
        }
    }
}

/// ToPrimitive (no hint) of a JSON value: strings stay strings;
/// null/booleans/numbers contribute their ToNumber side; arrays and
/// objects fall to their JS toString (`[]` is the empty string —
/// which BEATS every date string lexicographically as a candidate).
enum Primitive {
    Str(String),
    Num(f64),
}

fn to_primitive(value: &Value) -> Primitive {
    match value {
        Value::String(text) => Primitive::Str(text.clone()),
        Value::Number(number) => Primitive::Num(number.as_f64().unwrap_or(f64::NAN)),
        Value::Bool(flag) => Primitive::Num(f64::from(u8::from(*flag))),
        Value::Null => Primitive::Num(0.0),
        Value::Array(items) => Primitive::Str(array_to_string(items)),
        Value::Object(_) => Primitive::Str("[object Object]".into()),
    }
}

/// ToNumber of a primitive: numbers pass through; strings take the
/// numeric subset this port needs — a trimmed empty string is 0,
/// Infinity/NaN only by their exact spellings, `0x` hex, decimal
/// otherwise; every other shape is NaN (date strings always are,
/// which is what makes mixed-type comparisons false). The subset is
/// deliberately narrower than ECMAScript's StringNumericLiteral:
/// binary/octal prefixes and hex beyond u128 stay NaN, and no
/// consumer of the comparison needs them (recorded_at is schema-
/// required to be a date string; the JSON number side never parses
/// text). Widening this means re-probing the reference, not just
/// extending the parse.
fn to_number(primitive: &Primitive) -> f64 {
    match primitive {
        Primitive::Num(number) => *number,
        Primitive::Str(text) => js_to_number(text),
    }
}

fn js_to_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    match trimmed {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        // ToNumber resolves NaN's sign away; JS accepts the signed
        // spellings as the same NaN
        "NaN" | "+NaN" | "-NaN" => return f64::NAN,
        _ => {}
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u128::from_str_radix(hex, 16).map_or(f64::NAN, |value| value as f64);
    }
    // JS numeric literals start with a digit, sign, or decimal point —
    // the guard rejects Rust's case-insensitive "inf"/"nan" parses.
    let first = trimmed.as_bytes()[0];
    if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
        return f64::NAN;
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

/// Array.prototype.toString: elements joined by `,` with nested
/// arrays flattened and null rendering empty. Number elements render
/// Rust-style — a bounded approximation (the JS decimal
/// stringification agrees on every integral value; recorded_at is
/// schema-required to be a string, so arrays only reach the
/// comparison as a hostile store's oddity).
///
/// This is deliberately NOT [`value_text`]: both join arrays the same
/// way, but the number spellings diverge. `value_text` keeps serde's
/// JSON form — a float-typed `1.0` stays `1.0` — while this law is JS
/// Array.prototype.toString, which renders that same number `1`; a
/// genuine fraction (`1.5`) agrees on both. Merging the two would
/// silently move one caller's quirk.
fn array_to_string(items: &[Value]) -> String {
    items
        .iter()
        .map(|item| match item {
            Value::Null => String::new(),
            Value::String(text) => text.clone(),
            Value::Bool(flag) => flag.to_string(),
            Value::Number(number) => number
                .as_f64()
                .map_or_else(String::new, |value| format!("{value}")),
            Value::Array(nested) => array_to_string(nested),
            Value::Object(_) => "[object Object]".into(),
        })
        .collect::<Vec<_>>()
        .join(",")
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

/// `parseStrictPositiveInt` (utils/numeric-flags.ts — the jsval seam,
/// mulch-a878): `/^\d+$/` plus a JS `Number()` that must stay finite
/// and `>= 1` — so `0`, `-1`, `3.7`, `10abc` reject while `007` (7)
/// and huge digit strings (1e24) pass; the consuming slice clamps
/// (JS `slice(0, 1e24)` clamps to the array length; usize::MAX clamps
/// the same way for every real store size).
pub fn parse_strict_positive_int(raw: &str) -> Option<usize> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let number: f64 = raw.parse().ok()?;
    if !number.is_finite() || number < 1.0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounded by usize::MAX and finite (Number semantics)"
    )]
    Some(number.min(usize::MAX as f64) as usize)
}

/// `parseDuration` (utils/numeric-flags.ts): `/^(\d+)(h|d|w)$/`, the
/// unit in milliseconds. The digit run parses as a JS `Number` (huge
/// runs overflow to infinity, exactly like `parseInt`), so the
/// arithmetic stays in `f64`. `Err` carries the reference's message
/// verbatim.
pub fn parse_duration(raw: &str) -> Result<f64, String> {
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

/// A JS-number JSON value: integer-valued floats serialize without
/// the decimal point (`JSON.stringify(0)` is `0`, not `0.0`).
pub fn js_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9.007_199_254_740_992e15 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the bound check keeps the integer exact"
        )]
        Value::from(value as i64)
    } else {
        Value::from(value)
    }
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

/// The rotting day count as the rendered u64 shape. The caller
/// (`status`) has already applied `Math.floor` and established the
/// value strictly above the (u64) shelf, hence positive, and the
/// TimeClip-bounded span (|ms| <= 8.64e15) keeps the day quotient
/// under 2^53 — so the float-to-int cast is exact for every reachable
/// input. Exotic inputs saturate instead of wrapping (Rust's `as`
/// semantics): a negative or NaN quotient becomes 0, which the
/// caller's guard already excludes.
pub fn days_u64(age_days: f64) -> u64 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "positive by the > shelf comparison and far below 2^53"
    )]
    let days = age_days as u64;
    days
}

/// The reference's `truncate` (utils/format-helpers.ts): UTF-16 code
/// units are the counting law (live-probed 2026-10-16 vs ml 0.10.7:
/// 31 emoji = 62 units cut at 60 → 30 emoji + `...`; the same string
/// counts 31 chars / 124 bytes under the old private copies). A
/// sentence end (`[.!?]` + JS whitespace) inside the window wins;
/// a cut splitting a surrogate pair renders U+FFFD — Node's UTF-8
/// stdout conversion of a lone surrogate (29×🎉 + X + 🎉 + tail
/// probed: the 60-unit cut keeps 29 emoji, X, and the orphaned high
/// half as U+FFFD).
pub fn js_truncate(text: &str, max_len: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() <= max_len {
        return text.to_string();
    }
    if let Some(end) = sentence_end(&units)
        && end > 0
        && end < max_len
    {
        return String::from_utf16_lossy(&units[..=end]);
    }
    let mut cut = String::from_utf16_lossy(&units[..max_len]);
    cut.push_str("...");
    cut
}

/// The first `[.!?]` followed by JS whitespace, as a UTF-16 index —
/// `text.search(/[.!?]\s/)`. The ECMAScript `\s` set, not Rust's
/// `White_Space`: U+FEFF counts, U+0085 does not.
fn sentence_end(units: &[u16]) -> Option<usize> {
    units.windows(2).enumerate().find_map(|(index, pair)| {
        is_sentence_punctuation(pair[0])
            .then(|| is_js_whitespace(pair[1]).then_some(index))
            .flatten()
    })
}

fn is_sentence_punctuation(unit: u16) -> bool {
    // '.' | '!' | '?' as UTF-16 code units
    matches!(unit, 0x2E | 0x21 | 0x3F)
}

/// The ECMAScript `\s` set (unicode escape table of the spec's
/// `\s` class), as chars: the tokenize/trim/`\s` consumers work on
/// chars, the UTF-16 truncate below works on code units.
pub fn is_ecmascript_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{09}'..='\u{0D}'
            | '\u{20}'
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// The ECMAScript `\s` set over UTF-16 code units.
fn is_js_whitespace(unit: u16) -> bool {
    // Every char in the set is BMP — the unit is the char.
    char::from_u32(u32::from(unit)).is_some_and(is_ecmascript_whitespace)
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
mod numeric_flag_tests {
    use super::{parse_duration, parse_strict_positive_int};

    /// The strict positive-int law (utils/numeric-flags.ts): `007` and
    /// huge digit strings pass (clamping to usize::MAX — JS
    /// `slice(0, 1e24)` semantics), `0`, signed, fractional, and garbage
    /// reject. One law, three flags: rank `--limit`, ready `--limit`,
    /// prime `--budget` (the former per-command tests were identical).
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

    /// prime's clamp corner (the former prime/mod.rs arm): the huge
    /// digit run clamps to usize::MAX, dropping nothing.
    #[test]
    fn strict_int_clamps_huge_budgets_to_usize_max() {
        assert_eq!(parse_strict_positive_int("1"), Some(1));
        assert_eq!(parse_strict_positive_int("007"), Some(7));
        assert_eq!(
            parse_strict_positive_int("999999999999999999999999"),
            Some(usize::MAX)
        );
        assert_eq!(parse_strict_positive_int("0"), None);
        assert_eq!(parse_strict_positive_int("3.7"), None);
        assert_eq!(parse_strict_positive_int("10abc"), None);
        assert_eq!(parse_strict_positive_int(""), None);
    }

    /// The duration law: `^(\d+)(h|d|w)$` in milliseconds, everything
    /// else rejected with the reference message (ready `--since`).
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

    // ---- js_truncate (mulch-5e38): the UTF-16 law, live-probed vs
    // ml 0.10.7 on 2026-10-16 (delete summary 60 / compact render
    // 100 surfaces) ----

    #[test]
    fn truncate_counts_utf16_units() {
        // 58 umlauts + O-umlaut + U-umlaut = 60 units: no cut needed
        // at exactly the limit, `...` beyond it
        let sixty = format!("{}ÖÜ", "Ä".repeat(58));
        assert_eq!(js_truncate(&sixty, 60), sixty);
        let sixty_one = format!("{sixty} tail");
        assert_eq!(js_truncate(&sixty_one, 60), format!("{sixty}..."));
        // 31 emoji = 62 UTF-16 units: the cut keeps 30 — a char-based
        // copy would keep all 31, a byte-based one 15
        let emoji = "🎉".repeat(31);
        assert_eq!(js_truncate(&emoji, 60), format!("{}...", "🎉".repeat(30)));
        // the compact-render limit: 51 emoji = 102 units
        assert_eq!(
            js_truncate(&"🎉".repeat(51), 100),
            format!("{}...", "🎉".repeat(50))
        );
    }

    #[test]
    fn truncate_splitting_a_surrogate_pair_renders_the_replacement_char() {
        // 29 emoji (58 units) + X (1) + emoji (2) + tail: the 60-unit
        // cut orphans the second emoji's high half — Node prints a
        // lone surrogate as U+FFFD (probed)
        let text = format!("{}X🎉{}", "🎉".repeat(29), " tail beyond the cut");
        assert_eq!(
            js_truncate(&text, 60),
            format!("{}X\u{fffd}...", "🎉".repeat(29))
        );
    }

    #[test]
    fn truncate_sentence_end_wins_inside_the_window_only() {
        // the first sentence end INSIDE the window wins and keeps the
        // punctuation
        let text = "First sentence. And a much longer tail that exceeds the limit";
        assert_eq!(js_truncate(text, 20), "First sentence.");
        // a sentence end at or beyond the limit does not apply
        let late = format!("{} sentence", "x".repeat(60));
        assert_eq!(js_truncate(&late, 20), format!("{}...", "x".repeat(20)));
        // index 0 never wins (`sentenceEnd > 0`)
        let leading = ". starts with the period and runs far beyond the window";
        assert_eq!(js_truncate(leading, 10), format!("{}...", &leading[..10]));
    }

    #[test]
    fn truncate_whitespace_is_the_ecmascript_set() {
        // U+00A0 (NBSP) is JS \s and ends a sentence; U+0085 (NEL)
        // is NOT (Rust char::is_whitespace would flip both)
        let nbsp = "Ends here.\u{a0}Then more text well beyond the limit";
        assert_eq!(js_truncate(nbsp, 20), "Ends here.");
        let nel = "Not a break.\u{85}text continues past every window boundary";
        // no sentence end: the hard cut keeps 12 units — the period
        // IS the 12th — then the ellipsis
        assert_eq!(js_truncate(nel, 12), "Not a break....");
    }

    /// The pinned ToNumber subset (mulch-d5c3, moved from status): a
    /// trimmed empty string is 0, the signed Infinity/NaN spellings
    /// are what they say, `0x` hex parses up to u128, and the
    /// Rust-parse guard keeps identifiers (`inf`, `nan`) NaN where
    /// Rust's parser would accept them. Date strings are always NaN —
    /// the reason mixed recorded_at comparisons never fire. The
    /// ECMAScript-only shapes we do NOT implement are pinned too, so
    /// narrowing stays a decision rather than an accident.
    #[test]
    fn to_number_follows_the_pinned_numeric_subset() {
        assert_eq!(js_to_number(""), 0.0);
        assert_eq!(js_to_number("   "), 0.0);
        assert_eq!(js_to_number("Infinity"), f64::INFINITY);
        assert_eq!(js_to_number("+Infinity"), f64::INFINITY);
        assert_eq!(js_to_number("-Infinity"), f64::NEG_INFINITY);
        for spelling in ["NaN", "+NaN", "-NaN"] {
            assert!(js_to_number(spelling).is_nan(), "{spelling} is NaN");
        }
        assert_eq!(js_to_number("0x1F"), 31.0);
        assert_eq!(js_to_number("0X1f"), 31.0);
        assert!(js_to_number("0x").is_nan(), "an empty hex body is NaN");
        assert_eq!(js_to_number("  42  "), 42.0);
        assert_eq!(js_to_number("-1.5"), -1.5);
        assert_eq!(js_to_number(".5"), 0.5);
        assert_eq!(js_to_number("+7"), 7.0);
        // JS numeric literals start with a digit, sign, or dot: any
        // other first byte is NaN even where Rust would parse it
        for rejected in ["inf", "nan", "Infinityx", "1e", "true"] {
            assert!(js_to_number(rejected).is_nan(), "{rejected} is NaN");
        }
        assert!(
            js_to_number("2026-10-25T10:00:00.000Z").is_nan(),
            "a date string is NaN"
        );
        // Deliberately narrower than StringNumericLiteral: binary and
        // octal prefixes, and hex beyond u128, stay NaN (no consumer
        // needs them — see the to_number doc).
        for unsupported in ["0b101", "0o17", "0B11", "0O7"] {
            assert!(
                js_to_number(unsupported).is_nan(),
                "{unsupported} is outside the pinned subset"
            );
        }
        assert!(
            js_to_number(&format!("0x{}", "F".repeat(40))).is_nan(),
            "hex beyond u128 is outside the pinned subset"
        );
    }

    /// ToPrimitive: strings stay strings; numbers/null/booleans are
    /// numeric; arrays join through Array.prototype.toString; objects
    /// stringify to "[object Object]".
    #[test]
    fn to_primitive_maps_each_json_shape() {
        match to_primitive(&json!("text")) {
            Primitive::Str(text) => assert_eq!(text, "text"),
            Primitive::Num(_) => panic!("a string stays a string"),
        }
        match to_primitive(&json!(42)) {
            Primitive::Num(number) => assert_eq!(number, 42.0),
            Primitive::Str(_) => panic!("a number is numeric"),
        }
        match to_primitive(&json!(null)) {
            Primitive::Num(number) => assert_eq!(number, 0.0),
            Primitive::Str(_) => panic!("null is numeric 0"),
        }
        for (flag, expected) in [(true, 1.0), (false, 0.0)] {
            match to_primitive(&json!(flag)) {
                Primitive::Num(number) => assert_eq!(number, expected),
                Primitive::Str(_) => panic!("a boolean is numeric"),
            }
        }
        // [] is the empty string — the candidate that beats every date
        match to_primitive(&json!([])) {
            Primitive::Str(text) => assert_eq!(text, ""),
            Primitive::Num(_) => panic!("an array is a string"),
        }
        match to_primitive(&json!({})) {
            Primitive::Str(text) => assert_eq!(text, "[object Object]"),
            Primitive::Num(_) => panic!("an object is a string"),
        }
    }

    /// Array.prototype.toString: `,`-joined with nested arrays
    /// flattened and null empty; number spellings are JS's (an
    /// integral float renders `1`, unlike [`value_text`]'s serde
    /// spelling) — the divergence documented on the function.
    #[test]
    fn array_to_string_joins_flattens_and_spells_numbers_like_js() {
        assert_eq!(array_to_string(&[json!(1), json!(null)]), "1,");
        assert_eq!(array_to_string(&[json!("a"), json!(["b", "c"])]), "a,b,c");
        assert_eq!(array_to_string(&[json!(true), json!(false)]), "true,false");
        assert_eq!(array_to_string(&[json!(1.0)]), "1");
        assert_eq!(array_to_string(&[json!(1.5)]), "1.5");
        assert_eq!(array_to_string(&[json!({})]), "[object Object]");
        // the deliberate contrast: value_text keeps serde's `1.0`
        assert_eq!(value_text(&json!([1.0])), "1.0");
    }

    /// The abstract relational comparison: string/string is
    /// lexicographic, mixed types coerce through ToNumber, and a NaN
    /// on either side yields NO ordering (both `<` and `>` false) —
    /// the law behind the oldest/newest recorded_at selection.
    #[test]
    fn js_rel_is_lexicographic_for_strings_and_nan_false_otherwise() {
        assert_eq!(js_rel(&json!("a"), &json!("b")), Some(Ordering::Less));
        assert_eq!(js_rel(&json!("b"), &json!("a")), Some(Ordering::Greater));
        assert_eq!(js_rel(&json!("a"), &json!("a")), Some(Ordering::Equal));
        // the empty string beats every date string lexicographically
        assert_eq!(
            js_rel(&json!(""), &json!("2026-10-25T10:00:00.000Z")),
            Some(Ordering::Less)
        );
        // string/number coerce numerically
        assert_eq!(js_rel(&json!("42"), &json!(100)), Some(Ordering::Less));
        assert_eq!(js_rel(&json!("42"), &json!(42)), Some(Ordering::Equal));
        // a date string against a number is NaN on one side: no order
        assert_eq!(js_rel(&json!("2026-10-25T10:00:00.000Z"), &json!(5)), None);
        // a NaN on either numeric side: null is 0, the date string NaN
        assert_eq!(
            js_rel(&json!(null), &json!("2026-10-25T10:00:00.000Z")),
            None
        );
        assert_eq!(js_rel(&json!("not-a-date"), &json!(5)), None);
        // arrays/objects coerce via toString before comparing
        assert_eq!(js_rel(&json!([]), &json!("x")), Some(Ordering::Less));
        assert_eq!(js_rel(&json!({}), &json!("x")), Some(Ordering::Less));
    }

    /// The rotting day truncation and the cast's saturation arms:
    /// Math.floor already ran for reachable inputs, so the truncation
    /// and the unreachable negative/NaN fallbacks are pinned.
    #[test]
    fn days_u64_truncates_and_saturates_like_the_cast() {
        assert_eq!(days_u64(0.0), 0);
        assert_eq!(days_u64(1.9), 1);
        assert_eq!(days_u64(2.0), 2);
        assert_eq!(days_u64(1_000_000.0), 1_000_000);
        // Rust's `as` saturates: the caller's guard excludes these,
        // the arms are pinned so a switch to a panicking cast fails.
        assert_eq!(days_u64(-1.5), 0, "a negative quotient saturates to 0");
        assert_eq!(days_u64(f64::NAN), 0, "NaN saturates to 0");
    }
}
