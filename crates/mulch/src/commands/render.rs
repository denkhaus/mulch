//! The record-rendering seam (mulch-8ba4): the four domain-section
//! format families of the reference `utils/format.ts` — markdown
//! (default), compact, xml, plain — plus the per-type render rows
//! keyed off the registry order (`registry/builtins.ts` hooks), the
//! format atoms (`utils/format-helpers.ts`), and the binary-name
//! promote law (user directive 2026-10-13: the shipped binary never
//! promotes the tool it replaces). Pure functions over records — no
//! store IO; `now` is a parameter so fixtures stay deterministic.
//! The 16da read/render commands (query/prime/search/rank/ready/
//! learn) consume this seam; the golden pins here capture live
//! reference bytes (ml 0.10.7, 2026-10-13) until the differential
//! battery covers the commands end to end.

#![allow(
    dead_code,
    reason = "16da prep (mulch-8ba4): the seam lands ahead of its               consumers; the battery goes differential when               query/prime/search land"
)]

use std::collections::HashMap;
use std::fmt::Write as _;

use jiff::Timestamp;
use serde_json::Value;

/// One per-builtin rendering row (the reference's specialized
/// `TypeDefinition` format hooks); section titles live on the
/// registry rows, not here.
struct TypeRender {
    /// `formatMarkdown`: the whole section block, "" for no records.
    markdown:     fn(&[Value], bool) -> String,
    /// `formatCompactLine`: one record's compact bullet WITHOUT the
    /// annotation suffix (the family appends it); takes `now` for
    /// the classification-age rendering.
    compact_line: fn(&Value, Timestamp) -> String,
    /// `formatXml`: the record's inner element lines (4-space indent).
    xml_lines:    fn(&Value) -> Vec<String>,
    /// `plainSection`: "Title:" + bullets + trailing "" (no records
    /// → no lines).
    plain:        fn(&[Value]) -> Vec<String>,
}

/// The six built-ins in registry order.
static RENDER_TABLE: [TypeRender; 6] = [
    TypeRender {
        markdown:     markdown_conventions,
        compact_line: compact_convention,
        xml_lines:    xml_convention,
        plain:        plain_conventions,
    },
    TypeRender {
        markdown:     markdown_patterns,
        compact_line: compact_pattern,
        xml_lines:    xml_pattern,
        plain:        plain_patterns,
    },
    TypeRender {
        markdown:     markdown_failures,
        compact_line: compact_failure,
        xml_lines:    xml_failure,
        plain:        plain_failures,
    },
    TypeRender {
        markdown:     markdown_decisions,
        compact_line: compact_decision,
        xml_lines:    xml_decision,
        plain:        plain_decisions,
    },
    TypeRender {
        markdown:     markdown_references,
        compact_line: compact_reference,
        xml_lines:    xml_reference,
        plain:        plain_references,
    },
    TypeRender {
        markdown:     markdown_guides,
        compact_line: compact_guide,
        xml_lines:    xml_guide,
        plain:        plain_guides,
    },
];

/// The specialized builtin hooks, name-keyed (the reference's own
/// `switch (def.name)` shape — registry order never matters here).
fn builtin_render(name: &str) -> Option<&'static TypeRender> {
    Some(match name {
        "convention" => &RENDER_TABLE[0],
        "pattern" => &RENDER_TABLE[1],
        "failure" => &RENDER_TABLE[2],
        "decision" => &RENDER_TABLE[3],
        "reference" => &RENDER_TABLE[4],
        "guide" => &RENDER_TABLE[5],
        _ => return None,
    })
}

/// The render row for a record type over the MERGED registry: the
/// six builtins keep their specialized hooks, customs render through
/// the generic hooks built from their [`mulch::Row`] (registry/custom.ts).
/// `None` for unregistered types — families skip those like the
/// reference's `registry.get` miss.
enum RenderRow<'a> {
    Builtin(&'static TypeRender),
    Custom(&'a mulch::Row),
}

fn render_row<'a>(registry: &'a mulch::Registry, record_type: &str) -> Option<RenderRow<'a>> {
    let row = registry.row(record_type)?;
    Some(builtin_render(record_type).map_or(RenderRow::Custom(row), RenderRow::Builtin))
}

/// `estimateRecordText`: the per-record compact line the token
/// budget estimates over; unregistered types (reachable only under
/// `--allow-unknown-types`) render as `[<type>]`.
pub(crate) fn estimate_record_text(
    registry: &mulch::Registry,
    record: &Value,
    now: Timestamp,
) -> String {
    let record_type = record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match render_row(registry, record_type) {
        Some(RenderRow::Builtin(builtin)) => (builtin.compact_line)(record, now),
        Some(RenderRow::Custom(row)) => compact_custom(registry, row, record, now),
        None => format!("[{record_type}]"),
    }
}

// ---- the format atoms (format-helpers.ts) ----

/// `formatTimeAgo`: relative age, minute-granular — the lib law's
/// Timestamp-typed facade (call sites pass parsed stamps: file mtimes,
/// `last_updated`). The old clamp+trunc body was outcome-equivalent
/// to the lib's ms-floor math on every input (future stamps and
/// bucket boundaries included — pinned in
/// `time_ago_boundaries_match_the_lib_law`).
pub(crate) fn time_ago(then: Timestamp, now: Timestamp) -> String {
    mulch::time_ago_ts(then, now)
}

/// `formatEvidence`: the commit/date/issue/file bracket (the other
/// evidence keys never render here).
fn evidence_suffix(record: &Value) -> String {
    let Some(evidence) = record.get("evidence").and_then(Value::as_object) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for key in ["commit", "date", "issue", "file"] {
        // JS truthiness gates each member (an empty value renders
        // nothing — `if (evidence.commit)`)
        if let Some(value) = mulch::truthy_str(evidence.get(key).and_then(Value::as_str)) {
            parts.push(format!("{key}: {value}"));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" [{}]", parts.join(", "))
    }
}

/// `formatOutcome`: the LATEST outcome's status symbol with duration,
/// agent, and the repeat count.
fn outcome_suffix(record: &Value) -> String {
    let Some(outcomes) = record.get("outcomes").and_then(Value::as_array) else {
        return String::new();
    };
    let Some(latest) = outcomes.last() else {
        return String::new();
    };
    let symbol = match latest.get("status").and_then(Value::as_str) {
        Some("success") => "✓",
        Some("partial") => "~",
        _ => "✗",
    };
    let mut parts = vec![symbol.to_string()];
    if let Some(duration) = latest.get("duration") {
        // JSON number display matches JS template interpolation for
        // the integer/short-decimal durations the schema carries
        parts.push(format!("{duration}ms"));
    }
    if let Some(agent) = latest.get("agent").and_then(Value::as_str) {
        parts.push(format!("@{agent}"));
    }
    if outcomes.len() > 1 {
        parts.push(format!("({}x)", outcomes.len()));
    }
    format!(" [{}]", parts.join(" "))
}

/// `formatLinks`: the relates-to/supersedes bracket.
fn links_suffix(record: &Value) -> String {
    let mut parts = Vec::new();
    for (key, label) in [("relates_to", "relates to"), ("supersedes", "supersedes")] {
        // the array-length gate, not joined-emptiness: an all-empty
        // member list still renders (`r.relates_to.length > 0`)
        if let Some(items) = record.get(key).and_then(Value::as_array)
            && !items.is_empty()
        {
            let joined = items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("{label}: {joined}"));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" [{}]", parts.join("; "))
    }
}

/// `formatRecordMeta`: links only, or the full meta in verbose mode.
fn record_meta(record: &Value, full: bool) -> String {
    if !full {
        return links_suffix(record);
    }
    let classification = record
        .get("classification")
        .and_then(Value::as_str)
        .unwrap_or("undefined");
    let mut parts = vec![format!("({classification}){}", evidence_suffix(record))];
    if let Some(tags) = record.get("tags").and_then(Value::as_array)
        && !tags.is_empty()
    {
        let joined = tags
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("[tags: {joined}]"));
    }
    format!(" {}{}", parts.join(" "), links_suffix(record))
}

/// `idTag`: the `[mx-…] ` prefix for id-carrying records.
fn id_tag(record: &Value) -> String {
    match record.get("id").and_then(Value::as_str) {
        Some(id) => format!("[{id}] "),
        None => String::new(),
    }
}

/// `compactMeta`'s classification+age part: foundational renders bare,
/// every other classification (including the coerced `undefined` of a
/// missing field) renders with the recorded_at age. The age parse is
/// the lib's constructor law (`new Date(x)` — date-only strings,
/// space separators, zone-less civil time all parse); an invalid date
/// renders the reference's Invalid-Date template: `NaNd ago`
/// (Math.floor(NaN) falls through every bound).
fn classification_age(record: &Value, now: Timestamp) -> String {
    let classification = record
        .get("classification")
        .and_then(Value::as_str)
        .unwrap_or("undefined");
    if classification == "foundational" {
        return classification.into();
    }
    format!(
        "{} {}",
        classification,
        mulch::time_ago(mulch::date_ms(record), mulch::timestamp_ms(now))
    )
}

/// `compactMeta`: id + classification/age + the confirmation star.
fn compact_meta(record: &Value, now: Timestamp) -> String {
    let mut parts = Vec::new();
    if let Some(id) = record.get("id").and_then(Value::as_str) {
        parts.push(id.into());
    }
    parts.push(classification_age(record, now));
    let score = mulch::confirmation_score(record);
    if score > 0.0 {
        // Number.isInteger ? ★N : ★N.toFixed(1) — a DISPLAY decision,
        // not a Value build, so the fract check stays (mulch::js_number
        // is for the latter); scores are always integral or
        // half-integral, so one decimal is exact
        #[expect(
            clippy::cast_possible_truncation,
            reason = "fract()==0 makes the cast exact (Number.isInteger)"
        )]
        let star = if score.fract() == 0.0 {
            #[expect(
                clippy::cast_sign_loss,
                reason = "fract()==0 and score>0 make the cast exact (Number.isInteger)"
            )]
            let whole = score as u64;
            format!("★{whole}")
        } else {
            format!("★{score:.1}")
        };
        parts.push(star);
    }
    format!(" ({})", parts.join(", "))
}

/// `xmlEscape` (format-helpers.ts): the XML text atoms. One owner
/// since mulch-7bc2 — prime/format.rs imports this copy.
pub(crate) fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `xmlAttrEscape`: the text law plus the double-quote.
pub(crate) fn xml_attr_escape(text: &str) -> String {
    xml_escape(text).replace('"', "&quot;")
}

// ---- the four domain-section families (format.ts) ----

/// `formatDomainExpertise` (markdown default): the `## domain`
/// header plus one section per registry type with records.
pub(crate) fn format_domain_expertise(
    registry: &mulch::Registry,
    domain: &str,
    records: &[Value],
    last_updated: Option<Timestamp>,
    full: bool,
    annotations: Option<&HashMap<String, String>>,
    now: Timestamp,
) -> String {
    let updated = updated_suffix(last_updated, now);
    let mut sections = Vec::new();
    // registry order: the six builtins canonical, then customs in
    // config order (the reference's enabled() walk)
    for row in registry.rows() {
        let subset: Vec<Value> = records
            .iter()
            .filter(|r| r.get("type").and_then(Value::as_str) == Some(row.name.as_str()))
            .cloned()
            .collect();
        let block = builtin_render(&row.name).map_or_else(
            || markdown_custom(registry, row, &subset, full),
            |builtin| (builtin.markdown)(&subset, full),
        );
        if !block.is_empty() {
            sections.push(annotate_markdown_block(block, annotations));
        }
    }
    format!(
        "## {domain} ({} records{updated})\n\n{}",
        records.len(),
        sections.join("\n\n")
    )
}

/// `formatDomainExpertiseCompact`: the header plus one bullet per
/// record in STORE order (registry miss → skip).
pub(crate) fn format_domain_expertise_compact(
    registry: &mulch::Registry,
    domain: &str,
    records: &[Value],
    last_updated: Option<Timestamp>,
    annotations: Option<&HashMap<String, String>>,
    now: Timestamp,
) -> String {
    let updated = updated_suffix(last_updated, now);
    let mut lines = vec![format!("## {domain} ({} records{updated})", records.len())];
    for record in records {
        let Some(render) = render_row(
            registry,
            record
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ) else {
            continue;
        };
        let mut line = match render {
            RenderRow::Builtin(builtin) => (builtin.compact_line)(record, now),
            RenderRow::Custom(row) => compact_custom(registry, row, record, now),
        };
        if let Some(why) = annotation_for(record, annotations) {
            let _ = write!(line, " — {why}");
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// `formatDomainExpertiseXml`.
pub(crate) fn format_domain_expertise_xml(
    registry: &mulch::Registry,
    domain: &str,
    records: &[Value],
    last_updated: Option<Timestamp>,
    annotations: Option<&HashMap<String, String>>,
    now: Timestamp,
) -> String {
    let updated = last_updated.map_or_else(String::new, |then| {
        format!(" updated=\"{}\"", time_ago(then, now))
    });
    let mut lines = vec![format!(
        "<domain name=\"{}\" entries=\"{}\"{updated}>",
        xml_escape(domain),
        records.len()
    )];
    for record in records {
        let Some(render) = render_row(
            registry,
            record
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ) else {
            continue;
        };
        let id_attr = record
            .get("id")
            .and_then(Value::as_str)
            .map_or_else(String::new, |id| format!(" id=\"{}\"", xml_escape(id)));
        let why_attr = annotation_for(record, annotations).map_or_else(String::new, |why| {
            format!(" why=\"{}\"", xml_attr_escape(why))
        });
        let classification = record
            .get("classification")
            .and_then(Value::as_str)
            .unwrap_or("undefined");
        let tag = record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        lines.push(format!(
            "  <{tag}{id_attr} classification=\"{classification}\"{why_attr}>"
        ));
        lines.extend(match render {
            RenderRow::Builtin(builtin) => (builtin.xml_lines)(record),
            RenderRow::Custom(row) => xml_custom_lines(row, record),
        });
        lines.extend(xml_common_elements(record));
        lines.push(format!("  </{tag}>"));
    }
    lines.push("</domain>".into());
    lines.join("\n")
}

/// `formatDomainExpertisePlain`: `[domain] N records` plus per-type
/// sections (registry order), trailing blanks trimmed.
pub(crate) fn format_domain_expertise_plain(
    registry: &mulch::Registry,
    domain: &str,
    records: &[Value],
    last_updated: Option<Timestamp>,
    annotations: Option<&HashMap<String, String>>,
    now: Timestamp,
) -> String {
    let updated = last_updated.map_or_else(String::new, |then| {
        format!(" (updated {})", time_ago(then, now))
    });
    let mut lines = vec![
        format!("[{domain}] {} records{updated}", records.len()),
        String::new(),
    ];
    for spec in registry.rows() {
        // customs are INVISIBLE in plain (reference plainSection's
        // default arm — they still count in the header above)
        let Some(builtin) = builtin_render(&spec.name) else {
            continue;
        };
        let subset: Vec<Value> = records
            .iter()
            .filter(|r| r.get("type").and_then(Value::as_str) == Some(spec.name.as_str()))
            .cloned()
            .collect();
        if subset.is_empty() {
            continue;
        }
        for line in (builtin.plain)(&subset) {
            lines.push(annotate_plain_line(line, annotations));
        }
    }
    lines.join("\n").trim_end().to_string()
}

/// The shared `, updated <ago>` header suffix.
fn updated_suffix(last_updated: Option<Timestamp>, now: Timestamp) -> String {
    last_updated.map_or_else(String::new, |then| {
        format!(", updated {}", time_ago(then, now))
    })
}

/// The annotation for a record, when one exists.
fn annotation_for<'a>(
    record: &'a Value,
    annotations: Option<&'a HashMap<String, String>>,
) -> Option<&'a str> {
    let id = record.get("id").and_then(Value::as_str)?;
    annotations.and_then(|map| map.get(id)).map(String::as_str)
}

/// `annotateMarkdownBlock`: append ` — why` to `- ` bullets carrying an
/// id tag.
fn annotate_markdown_block(block: String, annotations: Option<&HashMap<String, String>>) -> String {
    let Some(map) = annotations else { return block };
    if map.is_empty() {
        return block;
    }
    let mut out = Vec::with_capacity(block.lines().count());
    for line in block.split('\n') {
        let mut replaced = line.to_string();
        if line.starts_with("- ")
            && let Some(id) = markdown_id_tag(line)
            && let Some(why) = map.get(&id)
        {
            replaced = format!("{line} — {why}");
        }
        out.push(replaced);
    }
    out.join("\n")
}

/// The `[mx-…]` id inside a markdown bullet line, if any.
fn markdown_id_tag(line: &str) -> Option<String> {
    let start = line.find("[mx-")?;
    let rest = &line[start + 4..];
    let end = rest.find(']')?;
    let hex = &rest[..end];
    if hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(format!("mx-{hex}"))
    } else {
        None
    }
}

/// `annotatePlainLine`: append `  (why)` to `  - [mx-…]` bullets.
fn annotate_plain_line(line: String, annotations: Option<&HashMap<String, String>>) -> String {
    let Some(map) = annotations else { return line };
    let trimmed_start = line.len() - line.trim_start().len();
    let bullet = &line[trimmed_start..];
    if !bullet.starts_with("- [") {
        return line;
    }
    match markdown_id_tag(bullet) {
        Some(id) => match map.get(&id) {
            Some(why) => format!("{line}  ({why})"),
            None => line,
        },
        None => line,
    }
}

/// The xml family's common tail elements (tags/links/outcomes).
fn xml_common_elements(record: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(tags) = record.get("tags").and_then(Value::as_array)
        && !tags.is_empty()
    {
        let joined = tags
            .iter()
            .filter_map(Value::as_str)
            .map(xml_escape)
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("    <tags>{joined}</tags>"));
    }
    for (key, tag) in [("relates_to", "relates_to"), ("supersedes", "supersedes")] {
        if let Some(items) = record.get(key).and_then(Value::as_array)
            && !items.is_empty()
        {
            let joined = items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("    <{tag}>{joined}</{tag}>"));
        }
    }
    if let Some(outcomes) = record.get("outcomes").and_then(Value::as_array) {
        for outcome in outcomes {
            let status = outcome.get("status").and_then(Value::as_str).unwrap_or("");
            let duration_attr = outcome
                .get("duration")
                .map_or_else(String::new, |d| format!(" duration=\"{d}\""));
            let agent_attr = outcome
                .get("agent")
                .and_then(Value::as_str)
                .map_or_else(String::new, |a| format!(" agent=\"{}\"", xml_escape(a)));
            let test_results = outcome
                .get("test_results")
                .and_then(Value::as_str)
                .map_or_else(String::new, xml_escape);
            lines.push(format!(
                "    <outcome status=\"{status}\"{duration_attr}{agent_attr}>{test_results}</outcome>"
            ));
        }
    }
    lines
}

// ---- the promote law (user directive 2026-10-13) ----

/// The reference's self-referential command strings, rewritten to
/// THIS binary: `ml <cmd>` becomes `mulch <cmd>` wherever the word
/// `ml` (word-bounded) introduces one of the command names. Prime's
/// workflow text bodies run through this one law before rendering.
pub(crate) fn promote_mulch(text: &str) -> String {
    const COMMANDS: [&str; 27] = [
        "init",
        "add",
        "record",
        "edit",
        "query",
        "setup",
        "prime",
        "onboard",
        "status",
        "validate",
        "prune",
        "archive",
        "restore",
        "search",
        "rank",
        "outcome",
        "doctor",
        "ready",
        "sync",
        "delete",
        "delete-domain",
        "move",
        "learn",
        "compact",
        "config",
        "diff",
        "upgrade",
    ];
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'm'
            && text[i..].starts_with("ml ")
            && (i == 0 || !is_word_byte(bytes[i - 1]))
        {
            let rest = &text[i + 3..];
            // COMMANDS is pre-sorted longest-first
            let matched = COMMANDS.iter().find(|cmd| {
                rest.starts_with(**cmd) && {
                    let after = &rest[cmd.len()..];
                    after.is_empty()
                        || !is_word_byte(after.as_bytes().first().copied().unwrap_or(b' '))
                }
            });
            if let Some(cmd) = matched {
                out.push_str("mulch ");
                out.push_str(cmd);
                i += 3 + cmd.len();
                continue;
            }
        }
        let ch_len = text[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&text[i..i + ch_len]);
        i += ch_len;
    }
    out
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

// ---- the per-type hooks (builtins.ts), markdown family ----

fn markdown_conventions(records: &[Value], full: bool) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut lines = vec!["### Conventions".to_string()];
    for record in records {
        lines.push(format!(
            "- {}{}{}",
            id_tag(record),
            str_field(record, "content"),
            record_meta(record, full)
        ));
    }
    lines.join("\n")
}

fn markdown_patterns(records: &[Value], full: bool) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut lines = vec!["### Patterns".to_string()];
    for record in records {
        let files = files_paren(record);
        lines.push(format!(
            "- {}**{}**: {}{files}{}",
            id_tag(record),
            str_field(record, "name"),
            str_field(record, "description"),
            record_meta(record, full)
        ));
    }
    lines.join("\n")
}

fn markdown_failures(records: &[Value], full: bool) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut lines = vec!["### Known Failures".to_string()];
    for record in records {
        lines.push(format!(
            "- {}{}{}",
            id_tag(record),
            str_field(record, "description"),
            record_meta(record, full)
        ));
        lines.push(format!("  → {}", str_field(record, "resolution")));
    }
    lines.join("\n")
}

fn markdown_decisions(records: &[Value], full: bool) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut lines = vec!["### Decisions".to_string()];
    for record in records {
        lines.push(format!(
            "- {}**{}**: {}{}",
            id_tag(record),
            str_field(record, "title"),
            str_field(record, "rationale"),
            record_meta(record, full)
        ));
    }
    lines.join("\n")
}

fn markdown_references(records: &[Value], full: bool) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut lines = vec!["### References".to_string()];
    for record in records {
        let files = files_paren(record);
        lines.push(format!(
            "- {}**{}**: {}{files}{}",
            id_tag(record),
            str_field(record, "name"),
            str_field(record, "description"),
            record_meta(record, full)
        ));
    }
    lines.join("\n")
}

fn markdown_guides(records: &[Value], full: bool) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut lines = vec!["### Guides".to_string()];
    for record in records {
        lines.push(format!(
            "- {}**{}**: {}{}",
            id_tag(record),
            str_field(record, "name"),
            str_field(record, "description"),
            record_meta(record, full)
        ));
    }
    lines.join("\n")
}

// ---- the per-type hooks, compact family ----

fn compact_convention(record: &Value, now: Timestamp) -> String {
    format!(
        "- [convention] {}{}{}{}",
        mulch::js_truncate(&str_field(record, "content"), 100),
        compact_meta(record, now),
        outcome_suffix(record),
        links_suffix(record)
    )
}

fn compact_pattern(record: &Value, now: Timestamp) -> String {
    format!(
        "- [pattern] {}: {}{}{}{}",
        str_field(record, "name"),
        mulch::js_truncate(&str_field(record, "description"), 100),
        files_paren(record),
        compact_meta(record, now),
        outcome_suffix(record) + &links_suffix(record)
    )
}

fn compact_failure(record: &Value, now: Timestamp) -> String {
    format!(
        "- [failure] {} → {}{}{}{}",
        mulch::js_truncate(&str_field(record, "description"), 100),
        mulch::js_truncate(&str_field(record, "resolution"), 100),
        compact_meta(record, now),
        outcome_suffix(record),
        links_suffix(record)
    )
}

fn compact_decision(record: &Value, now: Timestamp) -> String {
    format!(
        "- [decision] {}: {}{}{}{}",
        str_field(record, "title"),
        mulch::js_truncate(&str_field(record, "rationale"), 100),
        compact_meta(record, now),
        outcome_suffix(record),
        links_suffix(record)
    )
}

fn compact_reference(record: &Value, now: Timestamp) -> String {
    let files = files_list(record);
    let ref_files = if files.is_empty() {
        format!(
            ": {}",
            mulch::js_truncate(&str_field(record, "description"), 100)
        )
    } else {
        format!(": {files}")
    };
    format!(
        "- [reference] {}{ref_files}{}{}{}",
        str_field(record, "name"),
        compact_meta(record, now),
        outcome_suffix(record),
        links_suffix(record)
    )
}

fn compact_guide(record: &Value, now: Timestamp) -> String {
    format!(
        "- [guide] {}: {}{}{}{}",
        str_field(record, "name"),
        mulch::js_truncate(&str_field(record, "description"), 100),
        compact_meta(record, now),
        outcome_suffix(record),
        links_suffix(record)
    )
}

// ---- the per-type hooks, xml family ----

fn xml_convention(record: &Value) -> Vec<String> {
    vec![format!("    {}", xml_escape(&str_field(record, "content")))]
}

fn xml_pattern(record: &Value) -> Vec<String> {
    let mut lines = vec![
        format!(
            "    <name>{}</name>",
            xml_escape(&str_field(record, "name"))
        ),
        format!(
            "    <description>{}</description>",
            xml_escape(&str_field(record, "description"))
        ),
    ];
    let files = files_list(record);
    if !files.is_empty() {
        lines.push(format!("    <files>{files}</files>"));
    }
    lines
}

fn xml_failure(record: &Value) -> Vec<String> {
    vec![
        format!(
            "    <description>{}</description>",
            xml_escape(&str_field(record, "description"))
        ),
        format!(
            "    <resolution>{}</resolution>",
            xml_escape(&str_field(record, "resolution"))
        ),
    ]
}

fn xml_decision(record: &Value) -> Vec<String> {
    vec![
        format!(
            "    <title>{}</title>",
            xml_escape(&str_field(record, "title"))
        ),
        format!(
            "    <rationale>{}</rationale>",
            xml_escape(&str_field(record, "rationale"))
        ),
    ]
}

fn xml_reference(record: &Value) -> Vec<String> {
    let mut lines = vec![
        format!(
            "    <name>{}</name>",
            xml_escape(&str_field(record, "name"))
        ),
        format!(
            "    <description>{}</description>",
            xml_escape(&str_field(record, "description"))
        ),
    ];
    let files = files_list(record);
    if !files.is_empty() {
        lines.push(format!("    <files>{files}</files>"));
    }
    lines
}

fn xml_guide(record: &Value) -> Vec<String> {
    vec![
        format!(
            "    <name>{}</name>",
            xml_escape(&str_field(record, "name"))
        ),
        format!(
            "    <description>{}</description>",
            xml_escape(&str_field(record, "description"))
        ),
    ]
}

// ---- the per-type hooks, plain family ----

fn plain_conventions(records: &[Value]) -> Vec<String> {
    let mut out = vec!["Conventions:".to_string()];
    for record in records {
        out.push(format!(
            "  - {}{}{}",
            id_tag(record),
            str_field(record, "content"),
            links_suffix(record)
        ));
    }
    out.push(String::new());
    out
}

fn plain_patterns(records: &[Value]) -> Vec<String> {
    let mut out = vec!["Patterns:".to_string()];
    for record in records {
        out.push(format!(
            "  - {}{}: {}{}{}",
            id_tag(record),
            str_field(record, "name"),
            str_field(record, "description"),
            files_paren(record),
            links_suffix(record)
        ));
    }
    out.push(String::new());
    out
}

fn plain_failures(records: &[Value]) -> Vec<String> {
    let mut out = vec!["Known Failures:".to_string()];
    for record in records {
        out.push(format!(
            "  - {}{}{}",
            id_tag(record),
            str_field(record, "description"),
            links_suffix(record)
        ));
        out.push(format!("    Fix: {}", str_field(record, "resolution")));
    }
    out.push(String::new());
    out
}

fn plain_decisions(records: &[Value]) -> Vec<String> {
    let mut out = vec!["Decisions:".to_string()];
    for record in records {
        out.push(format!(
            "  - {}{}: {}{}",
            id_tag(record),
            str_field(record, "title"),
            str_field(record, "rationale"),
            links_suffix(record)
        ));
    }
    out.push(String::new());
    out
}

fn plain_references(records: &[Value]) -> Vec<String> {
    let mut out = vec!["References:".to_string()];
    for record in records {
        out.push(format!(
            "  - {}{}: {}{}{}",
            id_tag(record),
            str_field(record, "name"),
            str_field(record, "description"),
            files_paren(record),
            links_suffix(record)
        ));
    }
    out.push(String::new());
    out
}

fn plain_guides(records: &[Value]) -> Vec<String> {
    let mut out = vec!["Guides:".to_string()];
    for record in records {
        out.push(format!(
            "  - {}{}: {}{}",
            id_tag(record),
            str_field(record, "name"),
            str_field(record, "description"),
            links_suffix(record)
        ));
    }
    out.push(String::new());
    out
}

// ---- the generic custom hooks (registry/custom.ts) ----

/// `custom.ts formatMarkdown`: `### <sectionTitle>` + one generic
/// bullet per record — summary bold (NOT truncated), ` — ` +
/// `field: value` pairs over the declared fields (null AND "" skip;
/// the summary's own source fields REPEAT in the detail; files
/// never appear in the detail), files parens after the detail, then
/// the shared meta suffix.
fn markdown_custom(
    registry: &mulch::Registry,
    row: &mulch::Row,
    records: &[Value],
    full: bool,
) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut lines = vec![format!("### {}", row.section_title)];
    for record in records {
        let mut line = format!(
            "- {}**{}**",
            id_tag(record),
            summary_coerced(registry, record)
        );
        let mut detail = Vec::new();
        for field in row.payload.iter().chain(row.optional.iter()) {
            if row.files_array.as_deref() == Some(field.as_str()) {
                continue;
            }
            let Some(value) = record.get(field) else {
                continue;
            };
            if value.is_null() || value.as_str() == Some("") {
                continue;
            }
            detail.push(format!("{field}: {}", mulch::value_text(value)));
        }
        if !detail.is_empty() {
            let _ = write!(line, " — {}", detail.join("; "));
        }
        if let Some(files) = row
            .files_array
            .as_deref()
            .and_then(|field| files_list_field(record, field))
        {
            let _ = write!(line, " ({files})");
        }
        line.push_str(&record_meta(record, full));
        lines.push(line);
    }
    lines.join("\n")
}

/// `String(getRecordSummary(record))` for the render formats
/// (mulch-f06a): the raw law's coercion — `undefined` for an absent
/// summary field, non-strings coerced. The truncating laws' throws
/// keep the string view (the reference crashes there; accepted
/// divergence).
fn summary_coerced(registry: &mulch::Registry, record: &Value) -> String {
    match mulch::record_summary_value(registry, record) {
        Ok(raw) => crate::commands::summary_text(raw.as_ref()),
        Err(_) => mulch::record_summary(registry, record),
    }
}

/// `custom.ts formatCompactLine`: `- [type] truncate(summary)` +
/// files parens + the shared meta/outcome/links suffixes (TWO paren
/// groups when files exist).
fn compact_custom(
    registry: &mulch::Registry,
    row: &mulch::Row,
    record: &Value,
    now: Timestamp,
) -> String {
    // customs: truncate(getRecordSummary(custom), 100) — the
    // compiled template, NOT the raw field
    let summary = mulch::js_truncate(&summary_coerced(registry, record), 100);
    let files = row
        .files_array
        .as_deref()
        .and_then(|field| files_list_field(record, field))
        .map_or_else(String::new, |files| format!(" ({files})"));
    format!(
        "- [{}] {}{}{}{}{}",
        row.name,
        summary,
        files,
        compact_meta(record, now),
        outcome_suffix(record),
        links_suffix(record)
    )
}

/// `custom.ts formatXml`: one `<field>` line per DECLARED field
/// (payload-then-optional order, files included): arrays join
/// `", "`, only null/absent skip — `""` and `[]` render EMPTY
/// elements (probe bytes; markdown skips both).
fn xml_custom_lines(row: &mulch::Row, record: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for field in row.payload.iter().chain(row.optional.iter()) {
        let Some(value) = record.get(field) else {
            continue;
        };
        if let Value::Array(items) = value {
            let joined = items
                .iter()
                .map(|item| xml_escape(&mulch::value_text(item)))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("    <{field}>{joined}</{field}>"));
        } else if !value.is_null() {
            lines.push(format!(
                "    <{field}>{}</{field}>",
                xml_escape(&mulch::value_text(value))
            ));
        }
    }
    lines
}

// ---- shared field access ----

/// A string field, "" when absent/non-string. The rendered fields are
/// schema-REQUIRED for their types, so real stores always carry them;
/// the empty fallback only covers hand-crafted schema-invalid lines
/// (JS templates would print `undefined` there — accepted divergence
/// until the 16da differential pins the crafted-store edges).
/// The JS template-literal coercion of a field read (mulch-f06a):
/// a missing field renders `undefined`, non-strings coerce through
/// [`mulch::value_text`] (probe: `name: 42` renders `42`, a missing
/// name renders `undefined` in plain, compact, and markdown).
fn str_field(record: &Value, field: &str) -> String {
    record
        .get(field)
        .map_or_else(|| "undefined".into(), mulch::value_text)
}

/// The comma-joined files list ("" when none).
fn files_list(record: &Value) -> String {
    record
        .get("files")
        .and_then(Value::as_array)
        .filter(|files| !files.is_empty())
        .map_or_else(String::new, |files| {
            files
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
}

/// The comma-joined string-array field, `None` when absent/empty —
/// the files-paren source for rows whose files field is named
/// (`files_array`).
fn files_list_field(record: &Value, field: &str) -> Option<String> {
    record
        .get(field)
        .and_then(Value::as_array)
        .filter(|files| !files.is_empty())
        .map(|files| {
            files
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
}

/// The ` (files)` paren suffix for markdown/compact lines.
fn files_paren(record: &Value) -> String {
    let files = files_list(record);
    if files.is_empty() {
        String::new()
    } else {
        format!(" ({files})")
    }
}

#[cfg(test)]
mod render_gold_tests {
    use super::*;

    fn records() -> Vec<Value> {
        serde_json::from_str(
            r#"
[
  {"type":"convention","content":"Conv one content. Second sentence here! And more words to push past the one hundred character mark for truncation.","id":"mx-111111","classification":"tactical","recorded_at":"2026-01-01T10:00:00.000Z","evidence":{"commit":"abc1234","issue":"M-7"},"tags":["alpha","beta"]},
  {"type":"pattern","name":"Pat One","description":"Pattern description here","id":"mx-222222","classification":"foundational","recorded_at":"2026-01-02T10:00:00.000Z","files":["src/a.rs","src/b.rs"],"outcomes":[{"status":"success","recorded_at":"2026-01-03T10:00:00.000Z","duration":42,"agent":"ra"},{"status":"partial","recorded_at":"2026-01-04T10:00:00.000Z"}]},
  {"type":"failure","description":"Fail desc","resolution":"Fix res","id":"mx-333333","recorded_at":"2026-01-05T10:00:00.000Z"},
  {"type":"decision","title":"Decide <X> & Co","rationale":"Because Y","id":"mx-444444","recorded_at":"2026-01-06T10:00:00.000Z"},
  {"type":"reference","name":"Ref One","description":"Ref desc","id":"mx-555555","recorded_at":"2026-01-07T10:00:00.000Z","relates_to":["mulch:mx-111111"],"supersedes":["mulch-abc"]},
  {"type":"guide","name":"Guide One","description":"Guide desc","id":"mx-666666","recorded_at":"2026-01-08T10:00:00.000Z"}
]
"#,
        )
        .expect("fixture parses")
    }

    fn now() -> Timestamp {
        // the reference capture instant: the golden ages (285d ago …)
        // are relative to THIS fixed point, so the pins stay
        // deterministic forever
        "2026-10-13T15:19:55.000Z".parse().expect("capture now")
    }

    fn builtins() -> mulch::Registry {
        mulch::Registry::builtins()
    }

    /// Golden pins: live reference bytes (ml 0.10.7, 2026-10-13) for
    /// the six-type store, one per format family. `last_updated` was
    /// the just-written file mtime → "just now".
    #[test]
    fn markdown_family_matches_the_reference_bytes() {
        let got = format_domain_expertise(
            &builtins(),
            "d",
            &records(),
            Some(now()),
            false,
            None,
            now(),
        );
        assert_eq!(
            got,
            "## d (6 records, updated just now)\n\n### Conventions\n- [mx-111111] Conv one content. Second sentence here! And more words to push past the one hundred character mark for truncation.\n\n### Patterns\n- [mx-222222] **Pat One**: Pattern description here (src/a.rs, src/b.rs)\n\n### Known Failures\n- [mx-333333] Fail desc\n  → Fix res\n\n### Decisions\n- [mx-444444] **Decide <X> & Co**: Because Y\n\n### References\n- [mx-555555] **Ref One**: Ref desc [relates to: mulch:mx-111111; supersedes: mulch-abc]\n\n### Guides\n- [mx-666666] **Guide One**: Guide desc"
        );
    }

    #[test]
    fn compact_family_matches_the_reference_bytes() {
        let got =
            format_domain_expertise_compact(&builtins(), "d", &records(), Some(now()), None, now());
        assert_eq!(
            got,
            "## d (6 records, updated just now)\n- [convention] Conv one content. (mx-111111, tactical 285d ago)\n- [pattern] Pat One: Pattern description here (src/a.rs, src/b.rs) (mx-222222, foundational, ★1.5) [~ (2x)]\n- [failure] Fail desc → Fix res (mx-333333, undefined 281d ago)\n- [decision] Decide <X> & Co: Because Y (mx-444444, undefined 280d ago)\n- [reference] Ref One: Ref desc (mx-555555, undefined 279d ago) [relates to: mulch:mx-111111; supersedes: mulch-abc]\n- [guide] Guide One: Guide desc (mx-666666, undefined 278d ago)"
        );
    }

    #[test]
    fn xml_family_matches_the_reference_bytes() {
        let got =
            format_domain_expertise_xml(&builtins(), "d", &records(), Some(now()), None, now());
        assert_eq!(
            got,
            "<domain name=\"d\" entries=\"6\" updated=\"just now\">\n  <convention id=\"mx-111111\" classification=\"tactical\">\n    Conv one content. Second sentence here! And more words to push past the one hundred character mark for truncation.\n    <tags>alpha, beta</tags>\n  </convention>\n  <pattern id=\"mx-222222\" classification=\"foundational\">\n    <name>Pat One</name>\n    <description>Pattern description here</description>\n    <files>src/a.rs, src/b.rs</files>\n    <outcome status=\"success\" duration=\"42\" agent=\"ra\"></outcome>\n    <outcome status=\"partial\"></outcome>\n  </pattern>\n  <failure id=\"mx-333333\" classification=\"undefined\">\n    <description>Fail desc</description>\n    <resolution>Fix res</resolution>\n  </failure>\n  <decision id=\"mx-444444\" classification=\"undefined\">\n    <title>Decide &lt;X&gt; &amp; Co</title>\n    <rationale>Because Y</rationale>\n  </decision>\n  <reference id=\"mx-555555\" classification=\"undefined\">\n    <name>Ref One</name>\n    <description>Ref desc</description>\n    <relates_to>mulch:mx-111111</relates_to>\n    <supersedes>mulch-abc</supersedes>\n  </reference>\n  <guide id=\"mx-666666\" classification=\"undefined\">\n    <name>Guide One</name>\n    <description>Guide desc</description>\n  </guide>\n</domain>"
        );
    }

    #[test]
    fn plain_family_matches_the_reference_bytes() {
        let got =
            format_domain_expertise_plain(&builtins(), "d", &records(), Some(now()), None, now());
        assert_eq!(
            got,
            "[d] 6 records (updated just now)\n\nConventions:\n  - [mx-111111] Conv one content. Second sentence here! And more words to push past the one hundred character mark for truncation.\n\nPatterns:\n  - [mx-222222] Pat One: Pattern description here (src/a.rs, src/b.rs)\n\nKnown Failures:\n  - [mx-333333] Fail desc\n    Fix: Fix res\n\nDecisions:\n  - [mx-444444] Decide <X> & Co: Because Y\n\nReferences:\n  - [mx-555555] Ref One: Ref desc [relates to: mulch:mx-111111; supersedes: mulch-abc]\n\nGuides:\n  - [mx-666666] Guide One: Guide desc"
        );
    }

    /// The annotation laws: markdown appends ` — why` to id-carrying
    /// bullets, plain appends `  (why)`, xml carries a why attribute.
    #[test]
    fn annotations_render_per_family() {
        let mut why = HashMap::new();
        why.insert("mx-111111".to_string(), "file match".to_string());
        let markdown = format_domain_expertise(
            &builtins(),
            "d",
            &records(),
            Some(now()),
            false,
            Some(&why),
            now(),
        );
        assert!(markdown.contains("truncation. — file match"));
        let compact = format_domain_expertise_compact(
            &builtins(),
            "d",
            &records(),
            Some(now()),
            Some(&why),
            now(),
        );
        assert!(compact.contains("285d ago) — file match"), "{compact}");
        let plain = format_domain_expertise_plain(
            &builtins(),
            "d",
            &records(),
            Some(now()),
            Some(&why),
            now(),
        );
        assert!(plain.contains("truncation.  (file match)"));
        let xml = format_domain_expertise_xml(
            &builtins(),
            "d",
            &records(),
            Some(now()),
            Some(&why),
            now(),
        );
        assert!(xml.contains(r#"classification="tactical" why="file match""#));
    }

    /// The full-meta law (query does not exercise it; prime does):
    /// classification + evidence + tags brackets ahead of the links.
    #[test]
    fn verbose_meta_renders_classification_evidence_tags() {
        let got =
            format_domain_expertise(&builtins(), "d", &records(), Some(now()), true, None, now());
        assert!(
            got.contains(
                "truncation. (tactical) [commit: abc1234, issue: M-7] [tags: alpha, beta]"
            )
        );
    }

    /// The promote law: self-referential command strings name THIS
    /// binary (user directive 2026-10-13).
    #[test]
    fn promote_mulch_rewrites_self_referential_commands() {
        assert_eq!(
            promote_mulch("Run `ml prime` after compaction, then `ml record <domain>`"),
            "Run `mulch prime` after compaction, then `mulch record <domain>`"
        );
        assert_eq!(
            promote_mulch("ml add and ml delete-domain go"),
            "mulch add and mulch delete-domain go"
        );
        // word boundaries: 'ml' inside a larger word stays
        assert_eq!(promote_mulch("html record stays"), "html record stays");
        assert_eq!(promote_mulch("mlprime stays"), "mlprime stays");
        // ml without a command word stays (e.g. 'the ml binary')
        assert_eq!(promote_mulch("the ml binary"), "the ml binary");
    }

    /// The truthiness edges of the atoms (spec-review round): empty
    /// evidence members render nothing; an all-empty-member list still
    /// renders (array-length gate); an unparsable recorded_at renders
    /// the reference's `NaNd ago`.
    #[test]
    fn truthiness_edges_pin_the_reference_laws() {
        let record: Value = serde_json::from_str(
            r#"{"type":"convention","content":"c","id":"mx-e1","classification":"tactical","recorded_at":"not-a-date","evidence":{"commit":"","issue":"M-9"},"tags":[""],"relates_to":[""]}"#,
        )
        .expect("edge fixture parses");
        // evidence: only the truthy member renders
        assert_eq!(evidence_suffix(&record), " [issue: M-9]");
        // links: the array-length gate renders empty members verbatim
        assert_eq!(links_suffix(&record), " [relates to: ]");
        // full-meta tags: same gate
        assert!(record_meta(&record, true).contains("[tags: ]"));
        // unparsable recorded_at: the reference's Invalid-Date template
        let compact_line = format_domain_expertise_compact(
            &builtins(),
            "d",
            std::slice::from_ref(&record),
            None,
            None,
            now(),
        );
        assert!(compact_line.contains("tactical NaNd ago"), "{compact_line}");
    }

    /// The Timestamp-typed `time_ago` facade delegates to the lib law
    /// byte-neutrally at the edges: future stamps clamp to `just now`
    /// (the old body's `.max(0.0)` and the lib's floor-of-negative both
    /// land under the minute bound) and every bucket boundary floors
    /// like the reference's `Math.floor`. The parse unification rides
    /// along: a date-only `recorded_at` renders its UTC-midnight age
    /// through `classification_age` (the lib constructor law).
    #[test]
    fn time_ago_boundaries_match_the_lib_law() {
        let now: Timestamp = "2026-10-13T15:19:55.500Z".parse().expect("now");
        let at = |offset_ms: i64| {
            now.checked_add(jiff::Span::new().milliseconds(offset_ms))
                .expect("offset in range")
        };
        // future stamps and the sub-minute band
        assert_eq!(time_ago(at(1), now), "just now");
        assert_eq!(time_ago(at(-59_999), now), "just now");
        // minute boundaries (the .500 base crosses sub-seconds too)
        assert_eq!(time_ago(at(-60_000), now), "1m ago");
        assert_eq!(time_ago(at(-3_599_999), now), "59m ago");
        assert_eq!(time_ago(at(-3_600_000), now), "1h ago");
        assert_eq!(time_ago(at(-86_399_999), now), "23h ago");
        assert_eq!(time_ago(at(-86_400_000), now), "1d ago");

        // the classification-age parse is the constructor law now:
        // date-only means UTC midnight, not `NaNd ago`
        let corner: Value =
            serde_json::from_str(r#"{"classification":"tactical","recorded_at":"2020-01-01"}"#)
                .expect("corner fixture parses");
        let midnight: Timestamp = "2020-01-01T00:00:00Z".parse().expect("midnight");
        assert_eq!(
            classification_age(&corner, now),
            format!("tactical {}", time_ago(midnight, now))
        );
    }

    /// The builtin dispatch: every builtin registry name resolves to
    /// its specialized row; nothing else does (customs render through
    /// the generic hooks instead — the reference's switch shape).
    #[test]
    fn builtin_render_covers_exactly_the_builtins() {
        for spec in mulch::REGISTRY {
            assert!(
                builtin_render(spec.name).is_some(),
                "no builtin row for {}",
                spec.name
            );
        }
        assert!(builtin_render("recipe").is_none());
        assert!(builtin_render("").is_none());
    }

    // ---- the generic custom hooks (registry/custom.ts law, probed
    // live 2026-10-18, /tmp/sprint34/c1-probes) ----

    fn custom_config() -> &'static str {
        // config order: zzz before deploy (markdown sections follow
        // CONFIG order, not alphabetical — the F-store law)
        r#"
version: '1'
domains:
  probe: {}
custom_types:
  zzz_note:
    required: [content]
    optional: [note]
    dedup_key: content
    summary: "{content}"
  deploy_note:
    required: [service, action]
    optional: [files, rollback]
    dedup_key: service
    summary: "{service}: {action}"
    extracts_files: true
  run_step:
    extends: pattern
    optional: [page_index]
    section_title: 123
  legacy:
    extends: pattern
"#
    }

    fn custom_registry() -> mulch::Registry {
        let config = mulch::Config::parse(custom_config()).expect("config parses");
        mulch::Registry::from_config(&config).expect("custom types validate")
    }

    fn custom_records() -> Vec<Value> {
        serde_json::from_str(
            r#"
[
  {"type":"pattern","name":"Pat One","description":"Pat desc","id":"mx-222222","classification":"foundational","recorded_at":"2026-01-02T10:00:00.000Z"},
  {"type":"zzz_note","content":"Zeta note first. Second sentence pushes on past the hundred character boundary for the truncate law to fire and add the ellipsis!","note":"","id":"mx-zzz001","classification":"tactical","recorded_at":"2026-01-03T10:00:00.000Z"},
  {"type":"deploy_note","service":"api","action":"restart pods","rollback":"kubectl undo","files":["k8s/api.yml","deploy.sh"],"id":"mx-dep001","classification":"foundational","recorded_at":"2026-01-04T10:00:00.000Z","outcomes":[{"status":"success","recorded_at":"2026-01-05T10:00:00.000Z","duration":80},{"status":"failure","recorded_at":"2026-01-06T10:00:00.000Z","duration":80,"agent":"ra"}]},
  {"type":"deploy_note","service":"web","action":"roll","files":[],"rollback":"","id":"mx-dep002","classification":"tactical","recorded_at":"2026-01-07T10:00:00.000Z"},
  {"type":"run_step","name":"RS","description":"x","page_index":null,"id":"mx-rs0001","recorded_at":"2026-01-08T10:00:00.000Z"},
  {"type":"legacy","name":"LG","description":"y","id":"mx-lg0001","recorded_at":"2026-01-09T10:00:00.000Z"}
]
"#,
        )
        .expect("fixture parses")
    }

    /// Markdown: builtins first, then customs in CONFIG order
    /// (zzz before deploy); the title chain (default `Zzz_notes`,
    /// yaml-display `123`, extends-fallback `Patterns`); the generic
    /// bullet (summary bold UNTRUNCATED and REPEATED in the detail,
    /// ` — ` + `field: value` pairs, "" skipped, files parens after
    /// the detail).
    #[test]
    fn markdown_custom_sections_match_the_law() {
        let got = format_domain_expertise(
            &custom_registry(),
            "probe",
            &custom_records(),
            Some(now()),
            false,
            None,
            now(),
        );
        let zeta = "Zeta note first. Second sentence pushes on past the hundred character boundary for the truncate law to fire and add the ellipsis!";
        assert_eq!(
            got,
            format!(
                "## probe (6 records, updated just now)\n\n\
                 ### Patterns\n- [mx-222222] **Pat One**: Pat desc\n\n\
                 ### Zzz_notes\n- [mx-zzz001] **{zeta}** — content: {zeta}\n\n\
                 ### Deploy_notes\n- [mx-dep001] **api: restart pods** — service: api; action: restart pods; rollback: kubectl undo (k8s/api.yml, deploy.sh)\n- [mx-dep002] **web: roll** — service: web; action: roll\n\n\
                 ### 123\n- [mx-rs0001] **RS** — name: RS; description: x\n\n\
                 ### Patterns\n- [mx-lg0001] **LG** — name: LG; description: y"
            )
        );
    }

    /// Compact: disk order; the sentence-boundary truncate
    /// (`Zeta note first.`); files parens BEFORE the meta parens;
    /// foundational has no age; ★ over the confirmation score; the
    /// outcome bracket carries the LAST entry but the TOTAL count.
    #[test]
    fn compact_custom_lines_match_the_law() {
        let got = format_domain_expertise_compact(
            &custom_registry(),
            "probe",
            &custom_records(),
            Some(now()),
            None,
            now(),
        );
        assert_eq!(
            got,
            "## probe (6 records, updated just now)\n\
             - [pattern] Pat One: Pat desc (mx-222222, foundational)\n\
             - [zzz_note] Zeta note first. (mx-zzz001, tactical 283d ago)\n\
             - [deploy_note] api: restart pods (k8s/api.yml, deploy.sh) (mx-dep001, foundational, ★1) [✗ 80ms @ra (2x)]\n\
             - [deploy_note] web: roll (mx-dep002, tactical 279d ago)\n\
             - [run_step] RS (mx-rs0001, undefined 278d ago)\n\
             - [legacy] LG (mx-lg0001, undefined 277d ago)"
        );
    }

    /// XML: one element per DECLARED field (payload-then-optional,
    /// files included), arrays joined ", "; only null skips — `""`
    /// and `[]` render EMPTY elements (markdown skips both).
    #[test]
    fn xml_custom_elements_match_the_law() {
        let got = format_domain_expertise_xml(
            &custom_registry(),
            "probe",
            &custom_records(),
            Some(now()),
            None,
            now(),
        );
        let zeta = "Zeta note first. Second sentence pushes on past the hundred character boundary for the truncate law to fire and add the ellipsis!";
        assert_eq!(
            got,
            format!(
                "<domain name=\"probe\" entries=\"6\" updated=\"just now\">\n\
                   \x20 <pattern id=\"mx-222222\" classification=\"foundational\">\n    <name>Pat One</name>\n    <description>Pat desc</description>\n  </pattern>\n\
                   \x20 <zzz_note id=\"mx-zzz001\" classification=\"tactical\">\n    <content>{zeta}</content>\n    <note></note>\n  </zzz_note>\n\
                   \x20 <deploy_note id=\"mx-dep001\" classification=\"foundational\">\n    <service>api</service>\n    <action>restart pods</action>\n    <files>k8s/api.yml, deploy.sh</files>\n    <rollback>kubectl undo</rollback>\n    <outcome status=\"success\" duration=\"80\"></outcome>\n    <outcome status=\"failure\" duration=\"80\" agent=\"ra\"></outcome>\n  </deploy_note>\n\
                   \x20 <deploy_note id=\"mx-dep002\" classification=\"tactical\">\n    <service>web</service>\n    <action>roll</action>\n    <files></files>\n    <rollback></rollback>\n  </deploy_note>\n\
                   \x20 <run_step id=\"mx-rs0001\" classification=\"undefined\">\n    <name>RS</name>\n    <description>x</description>\n  </run_step>\n\
                   \x20 <legacy id=\"mx-lg0001\" classification=\"undefined\">\n    <name>LG</name>\n    <description>y</description>\n  </legacy>\n\
                 </domain>"
            )
        );
    }

    /// Plain: customs are INVISIBLE (the reference plainSection
    /// default arm) — they still count in the header.
    #[test]
    fn plain_customs_are_invisible_but_counted() {
        let got = format_domain_expertise_plain(
            &custom_registry(),
            "probe",
            &custom_records(),
            Some(now()),
            None,
            now(),
        );
        assert_eq!(
            got,
            "[probe] 6 records (updated just now)\n\nPatterns:\n  - [mx-222222] Pat One: Pat desc"
        );
    }
}
