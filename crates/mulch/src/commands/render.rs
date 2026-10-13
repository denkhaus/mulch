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
use mulch::REGISTRY;
use serde_json::Value;

/// One per-type rendering row, parallel to [`REGISTRY`] order (the
/// reference's `TypeDefinition` format hooks).
struct TypeRender {
    /// The markdown section heading (e.g. "Conventions").
    section_title: &'static str,
    /// `formatMarkdown`: the whole section block, "" for no records.
    markdown:      fn(&[Value], bool) -> String,
    /// `formatCompactLine`: one record's compact bullet WITHOUT the
    /// annotation suffix (the family appends it); takes `now` for
    /// the classification-age rendering.
    compact_line:  fn(&Value, Timestamp) -> String,
    /// `formatXml`: the record's inner element lines (4-space indent).
    xml_lines:     fn(&Value) -> Vec<String>,
    /// `plainSection`: "Title:" + bullets + trailing "" (no records
    /// → no lines).
    plain:         fn(&[Value]) -> Vec<String>,
}

/// The six built-ins in registry order.
static RENDER_TABLE: [TypeRender; 6] = [
    TypeRender {
        section_title: "Conventions",
        markdown:      markdown_conventions,
        compact_line:  compact_convention,
        xml_lines:     xml_convention,
        plain:         plain_conventions,
    },
    TypeRender {
        section_title: "Patterns",
        markdown:      markdown_patterns,
        compact_line:  compact_pattern,
        xml_lines:     xml_pattern,
        plain:         plain_patterns,
    },
    TypeRender {
        section_title: "Known Failures",
        markdown:      markdown_failures,
        compact_line:  compact_failure,
        xml_lines:     xml_failure,
        plain:         plain_failures,
    },
    TypeRender {
        section_title: "Decisions",
        markdown:      markdown_decisions,
        compact_line:  compact_decision,
        xml_lines:     xml_decision,
        plain:         plain_decisions,
    },
    TypeRender {
        section_title: "References",
        markdown:      markdown_references,
        compact_line:  compact_reference,
        xml_lines:     xml_reference,
        plain:         plain_references,
    },
    TypeRender {
        section_title: "Guides",
        markdown:      markdown_guides,
        compact_line:  compact_guide,
        xml_lines:     xml_guide,
        plain:         plain_guides,
    },
];

/// The render row for a record type (registry order; None for
/// unregistered types — families skip those like the reference's
/// `registry.get` miss).
fn row(record_type: &str) -> Option<&'static TypeRender> {
    REGISTRY
        .iter()
        .position(|spec| spec.name == record_type)
        .map(|index| &RENDER_TABLE[index])
}

// ---- the format atoms (format-helpers.ts) ----

/// `formatTimeAgo`: relative age, minute-granular.
fn time_ago(then: Timestamp, now: Timestamp) -> String {
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "clamped non-negative ages; mirrors Math.floor semantics"
    )]
    let diff_ms = ((now - then)
        .total(jiff::Unit::Millisecond)
        .unwrap_or(0.0)
        .max(0.0)) as u64;
    let minutes = diff_ms / 60_000;
    if minutes < 1 {
        "just now".into()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else {
        let hours = diff_ms / 3_600_000;
        if hours < 24 {
            format!("{hours}h ago")
        } else {
            format!("{}d ago", diff_ms / 86_400_000)
        }
    }
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

/// `truncate`: the first sentence end inside the limit, else the hard
/// cut with an ellipsis.
fn truncate(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.into();
    }
    // sentence end: [.!?] followed by whitespace (JS /[.!?]\s/; the
    // length check rides bytes — the field text is CLI-written ASCII
    // in practice, and the hard cut clamps to a char boundary)
    for (index, window) in text.as_bytes().windows(2).enumerate() {
        let is_end = matches!(window[0], b'.' | b'!' | b'?') && window[1].is_ascii_whitespace();
        if is_end && index > 0 && index < max_len {
            return text[..=index].into();
        }
    }
    let mut cut = max_len;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}...", &text[..cut])
}

/// `compactMeta`'s classification+age part: foundational renders bare,
/// every other classification (including the coerced `undefined` of a
/// missing field) renders with the recorded_at age.
fn classification_age(record: &Value, now: Timestamp) -> String {
    let classification = record
        .get("classification")
        .and_then(Value::as_str)
        .unwrap_or("undefined");
    if classification == "foundational" {
        return classification.into();
    }
    // an unparsable recorded_at renders the reference's Invalid-Date
    // template: `NaNd ago` (Math.floor(NaN) falls through every bound)
    let age = record
        .get("recorded_at")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<Timestamp>().ok())
        .map_or_else(|| "NaNd ago".into(), |then| time_ago(then, now));
    format!("{classification} {age}")
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
        // Number.isInteger ? ★N : ★N.toFixed(1) — scores are always
        // integral or half-integral, so one decimal is exact
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

/// `xmlEscape` / `xmlAttrEscape`.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_attr_escape(text: &str) -> String {
    xml_escape(text).replace('"', "&quot;")
}

// ---- the four domain-section families (format.ts) ----

/// `formatDomainExpertise` (markdown default): the `## domain`
/// header plus one section per registry type with records.
pub(crate) fn format_domain_expertise(
    domain: &str,
    records: &[Value],
    last_updated: Option<Timestamp>,
    full: bool,
    annotations: Option<&HashMap<String, String>>,
    now: Timestamp,
) -> String {
    let updated = updated_suffix(last_updated, now);
    let mut sections = Vec::new();
    for spec in REGISTRY {
        let subset: Vec<Value> = records
            .iter()
            .filter(|r| r.get("type").and_then(Value::as_str) == Some(spec.name))
            .cloned()
            .collect();
        let block = (row(spec.name)
            .expect("registry parallel to RENDER_TABLE")
            .markdown)(&subset, full);
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
    domain: &str,
    records: &[Value],
    last_updated: Option<Timestamp>,
    annotations: Option<&HashMap<String, String>>,
    now: Timestamp,
) -> String {
    let updated = updated_suffix(last_updated, now);
    let mut lines = vec![format!("## {domain} ({} records{updated})", records.len())];
    for record in records {
        let Some(r#struct) = row(record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default())
        else {
            continue;
        };
        let mut line = (r#struct.compact_line)(record, now);
        if let Some(why) = annotation_for(record, annotations) {
            let _ = write!(line, " — {why}");
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// `formatDomainExpertiseXml`.
pub(crate) fn format_domain_expertise_xml(
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
        let Some(render) = row(record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default())
        else {
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
        lines.extend((render.xml_lines)(record));
        lines.extend(xml_common_elements(record));
        lines.push(format!("  </{tag}>"));
    }
    lines.push("</domain>".into());
    lines.join("\n")
}

/// `formatDomainExpertisePlain`: `[domain] N records` plus per-type
/// sections (registry order), trailing blanks trimmed.
pub(crate) fn format_domain_expertise_plain(
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
    for spec in REGISTRY {
        let subset: Vec<Value> = records
            .iter()
            .filter(|r| r.get("type").and_then(Value::as_str) == Some(spec.name))
            .cloned()
            .collect();
        if subset.is_empty() {
            continue;
        }
        for line in (row(spec.name).expect("registry parallel").plain)(&subset) {
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
        truncate(&str_field(record, "content"), 100),
        compact_meta(record, now),
        outcome_suffix(record),
        links_suffix(record)
    )
}

fn compact_pattern(record: &Value, now: Timestamp) -> String {
    format!(
        "- [pattern] {}: {}{}{}{}",
        str_field(record, "name"),
        truncate(&str_field(record, "description"), 100),
        files_paren(record),
        compact_meta(record, now),
        outcome_suffix(record) + &links_suffix(record)
    )
}

fn compact_failure(record: &Value, now: Timestamp) -> String {
    format!(
        "- [failure] {} → {}{}{}{}",
        truncate(&str_field(record, "description"), 100),
        truncate(&str_field(record, "resolution"), 100),
        compact_meta(record, now),
        outcome_suffix(record),
        links_suffix(record)
    )
}

fn compact_decision(record: &Value, now: Timestamp) -> String {
    format!(
        "- [decision] {}: {}{}{}{}",
        str_field(record, "title"),
        truncate(&str_field(record, "rationale"), 100),
        compact_meta(record, now),
        outcome_suffix(record),
        links_suffix(record)
    )
}

fn compact_reference(record: &Value, now: Timestamp) -> String {
    let files = files_list(record);
    let ref_files = if files.is_empty() {
        format!(": {}", truncate(&str_field(record, "description"), 100))
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
        truncate(&str_field(record, "description"), 100),
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

// ---- shared field access ----

/// A string field, "" when absent/non-string. The rendered fields are
/// schema-REQUIRED for their types, so real stores always carry them;
/// the empty fallback only covers hand-crafted schema-invalid lines
/// (JS templates would print `undefined` there — accepted divergence
/// until the 16da differential pins the crafted-store edges).
fn str_field(record: &Value, field: &str) -> String {
    record
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
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

    /// Golden pins: live reference bytes (ml 0.10.7, 2026-10-13) for
    /// the six-type store, one per format family. `last_updated` was
    /// the just-written file mtime → "just now".
    #[test]
    fn markdown_family_matches_the_reference_bytes() {
        let got = format_domain_expertise("d", &records(), Some(now()), false, None, now());
        assert_eq!(
            got,
            "## d (6 records, updated just now)\n\n### Conventions\n- [mx-111111] Conv one content. Second sentence here! And more words to push past the one hundred character mark for truncation.\n\n### Patterns\n- [mx-222222] **Pat One**: Pattern description here (src/a.rs, src/b.rs)\n\n### Known Failures\n- [mx-333333] Fail desc\n  → Fix res\n\n### Decisions\n- [mx-444444] **Decide <X> & Co**: Because Y\n\n### References\n- [mx-555555] **Ref One**: Ref desc [relates to: mulch:mx-111111; supersedes: mulch-abc]\n\n### Guides\n- [mx-666666] **Guide One**: Guide desc"
        );
    }

    #[test]
    fn compact_family_matches_the_reference_bytes() {
        let got = format_domain_expertise_compact("d", &records(), Some(now()), None, now());
        assert_eq!(
            got,
            "## d (6 records, updated just now)\n- [convention] Conv one content. (mx-111111, tactical 285d ago)\n- [pattern] Pat One: Pattern description here (src/a.rs, src/b.rs) (mx-222222, foundational, ★1.5) [~ (2x)]\n- [failure] Fail desc → Fix res (mx-333333, undefined 281d ago)\n- [decision] Decide <X> & Co: Because Y (mx-444444, undefined 280d ago)\n- [reference] Ref One: Ref desc (mx-555555, undefined 279d ago) [relates to: mulch:mx-111111; supersedes: mulch-abc]\n- [guide] Guide One: Guide desc (mx-666666, undefined 278d ago)"
        );
    }

    #[test]
    fn xml_family_matches_the_reference_bytes() {
        let got = format_domain_expertise_xml("d", &records(), Some(now()), None, now());
        assert_eq!(
            got,
            "<domain name=\"d\" entries=\"6\" updated=\"just now\">\n  <convention id=\"mx-111111\" classification=\"tactical\">\n    Conv one content. Second sentence here! And more words to push past the one hundred character mark for truncation.\n    <tags>alpha, beta</tags>\n  </convention>\n  <pattern id=\"mx-222222\" classification=\"foundational\">\n    <name>Pat One</name>\n    <description>Pattern description here</description>\n    <files>src/a.rs, src/b.rs</files>\n    <outcome status=\"success\" duration=\"42\" agent=\"ra\"></outcome>\n    <outcome status=\"partial\"></outcome>\n  </pattern>\n  <failure id=\"mx-333333\" classification=\"undefined\">\n    <description>Fail desc</description>\n    <resolution>Fix res</resolution>\n  </failure>\n  <decision id=\"mx-444444\" classification=\"undefined\">\n    <title>Decide &lt;X&gt; &amp; Co</title>\n    <rationale>Because Y</rationale>\n  </decision>\n  <reference id=\"mx-555555\" classification=\"undefined\">\n    <name>Ref One</name>\n    <description>Ref desc</description>\n    <relates_to>mulch:mx-111111</relates_to>\n    <supersedes>mulch-abc</supersedes>\n  </reference>\n  <guide id=\"mx-666666\" classification=\"undefined\">\n    <name>Guide One</name>\n    <description>Guide desc</description>\n  </guide>\n</domain>"
        );
    }

    #[test]
    fn plain_family_matches_the_reference_bytes() {
        let got = format_domain_expertise_plain("d", &records(), Some(now()), None, now());
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
        let markdown =
            format_domain_expertise("d", &records(), Some(now()), false, Some(&why), now());
        assert!(markdown.contains("truncation. — file match"));
        let compact =
            format_domain_expertise_compact("d", &records(), Some(now()), Some(&why), now());
        assert!(compact.contains("285d ago) — file match"), "{compact}");
        let plain = format_domain_expertise_plain("d", &records(), Some(now()), Some(&why), now());
        assert!(plain.contains("truncation.  (file match)"));
        let xml = format_domain_expertise_xml("d", &records(), Some(now()), Some(&why), now());
        assert!(xml.contains(r#"classification="tactical" why="file match""#));
    }

    /// The full-meta law (query does not exercise it; prime does):
    /// classification + evidence + tags brackets ahead of the links.
    #[test]
    fn verbose_meta_renders_classification_evidence_tags() {
        let got = format_domain_expertise("d", &records(), Some(now()), true, None, now());
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
        let compact_line =
            format_domain_expertise_compact("d", std::slice::from_ref(&record), None, None, now());
        assert!(compact_line.contains("tactical NaNd ago"), "{compact_line}");
    }

    /// The RENDER_TABLE stays parallel to REGISTRY (row() never misses
    /// for a registry type).
    #[test]
    fn render_table_covers_the_registry() {
        for spec in REGISTRY {
            assert!(row(spec.name).is_some(), "no render row for {}", spec.name);
        }
    }
}
