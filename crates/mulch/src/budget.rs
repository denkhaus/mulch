//! The token budget applied by `prime` (and future corpus
//! consumers) — the reference's `utils/budget.ts`, byte-relevant
//! law. Pure: callers pass the per-record formatter whose text
//! cost is estimated; the lib owns the selection order.
//!
//! JS semantics pinned here:
//! - `estimateTokens` is `Math.ceil(text.length / 4)` where `.length` counts
//!   UTF-16 CODE UNITS (the jsval truncate law's family), not chars.
//! - Priority: builtin type order (custom types sort after ALL builtins),
//!   classification order (`foundational` first; an UNKNOWN classification
//!   keeps `indexOf`'s -1 and sorts before everything — reference parity, not a
//!   typo to fix), then confirmation score descending, then recency (newest
//!   first).
//! - An unparsable `recorded_at` yields `NaN` time; `NaN` compares as equal
//!   (the JS comparator returns NaN → 0), so such records fall through to the
//!   stable order instead of sorting by time.
//! - The walk is greedy in priority order and CONTINUES after a record does not
//!   fit — later, cheaper records may still fit.

use serde_json::Value;

use crate::scoring::confirmation_score;

/// The reference's `DEFAULT_BUDGET` (tokens).
pub const DEFAULT_BUDGET: usize = 4000;

/// Builtin type priority (reference `TYPE_PRIORITY`).
const TYPE_PRIORITY: [&str; 6] = [
    "convention",
    "decision",
    "pattern",
    "guide",
    "failure",
    "reference",
];

/// Classification priority (reference `CLASSIFICATION_PRIORITY`).
const CLASSIFICATION_PRIORITY: [&str; 3] = ["foundational", "tactical", "observational"];

/// One domain's records entering the budget.
#[derive(Clone, Debug)]
pub struct DomainRecords {
    pub domain:  String,
    pub records: Vec<Value>,
}

/// The kept records (original domain order, original per-domain
/// record order) plus the drop tallies.
#[derive(Clone, Debug)]
pub struct BudgetResult {
    pub kept:                 Vec<DomainRecords>,
    pub dropped_count:        usize,
    pub dropped_domain_count: usize,
}

/// `estimateTokens`: UTF-16 length over four, rounded up.
pub fn estimate_tokens(text: &str) -> usize {
    text.encode_utf16().count().div_ceil(4)
}

/// `recordSortKey` comparison: type, classification, confirmation
/// desc, recency desc. Stable on full ties.
fn compare_records(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let type_index = |record: &Value| {
        let name = record.get("type").and_then(Value::as_str).unwrap_or("");
        match TYPE_PRIORITY.iter().position(|t| *t == name) {
            Some(index) => index,
            // Custom/unknown types sort after all builtins.
            None => TYPE_PRIORITY.len(),
        }
    };
    let class_index = |record: &Value| {
        let name = record
            .get("classification")
            .and_then(Value::as_str)
            .unwrap_or("");
        // Reference parity: unknown classifications keep -1 and sort
        // BEFORE foundational.
        CLASSIFICATION_PRIORITY
            .iter()
            .position(|c| *c == name)
            .and_then(|index| i64::try_from(index).ok())
            .unwrap_or(-1)
    };

    let ord = type_index(a).cmp(&type_index(b));
    if ord != Ordering::Equal {
        return ord;
    }
    let ord = class_index(a).cmp(&class_index(b));
    if ord != Ordering::Equal {
        return ord;
    }
    let ord = confirmation_score(b)
        .partial_cmp(&confirmation_score(a))
        .unwrap_or(Ordering::Equal);
    if ord != Ordering::Equal {
        return ord;
    }
    recorded_ms(b)
        .partial_cmp(&recorded_ms(a))
        .unwrap_or(Ordering::Equal)
}

/// `new Date(r.recorded_at).getTime()` as f64 milliseconds;
/// unparsable/missing dates are `None` (the JS `NaN`).
fn recorded_ms(record: &Value) -> Option<f64> {
    record
        .get("recorded_at")
        .and_then(Value::as_str)?
        .parse::<jiff::Timestamp>()
        .ok()
        .map(|ts| ts.as_millisecond() as f64)
}

/// `applyBudget`: greedily keep records that fit, in priority
/// order, then rebuild the per-domain groups in the original order.
pub fn apply_budget<F>(domains: &[DomainRecords], budget: usize, format_record: F) -> BudgetResult
where
    F: Fn(&Value, &str) -> String,
{
    // Flatten with the domain name; sort stable by priority.
    let mut tagged: Vec<(usize, usize)> = Vec::new(); // (domain idx, record idx)
    for (domain_index, group) in domains.iter().enumerate() {
        for record_index in 0..group.records.len() {
            tagged.push((domain_index, record_index));
        }
    }
    tagged.sort_by(|a, b| compare_records(&domains[a.0].records[a.1], &domains[b.0].records[b.1]));

    let mut used_tokens = 0_usize;
    let mut kept: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for &(domain_index, record_index) in &tagged {
        let record = &domains[domain_index].records[record_index];
        let formatted = format_record(record, &domains[domain_index].domain);
        let cost = estimate_tokens(&formatted);
        if used_tokens + cost <= budget {
            used_tokens += cost;
            kept.insert((domain_index, record_index));
        }
    }

    // Rebuild in original domain order; per-domain order preserved.
    let mut result = Vec::new();
    let mut dropped_domains = std::collections::HashSet::new();
    for (domain_index, group) in domains.iter().enumerate() {
        let mut kept_records = Vec::new();
        let mut any_dropped = false;
        for (record_index, record) in group.records.iter().enumerate() {
            if kept.contains(&(domain_index, record_index)) {
                kept_records.push(record.clone());
            } else {
                any_dropped = true;
            }
        }
        if !kept_records.is_empty() {
            result.push(DomainRecords {
                domain:  group.domain.clone(),
                records: kept_records,
            });
        } else if !group.records.is_empty() {
            dropped_domains.insert(group.domain.clone());
        }
        if any_dropped {
            dropped_domains.insert(group.domain.clone());
        }
    }

    let dropped_count = tagged.len() - kept.len();
    BudgetResult {
        kept: result,
        dropped_count,
        dropped_domain_count: dropped_domains.len(),
    }
}

/// `formatBudgetSummary`: the truncation line shown when records
/// were dropped.
pub fn format_budget_summary(dropped_count: usize, dropped_domain_count: usize) -> String {
    let record_part = if dropped_count == 1 {
        "record"
    } else {
        "records"
    };
    let domain_part = if dropped_domain_count > 0 {
        format!(
            " across {dropped_domain_count} domain{}",
            if dropped_domain_count == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    };
    format!(
        "... and {dropped_count} more {record_part}{domain_part} (use --budget <n> to show more)"
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn record(type_: &str, classification: &str, recorded_at: &str) -> Value {
        json!({"type": type_, "classification": classification, "recorded_at": recorded_at})
    }

    /// UTF-16 units, not chars: two emoji are 4 units -> 1 token,
    /// three are 6 units -> 2.
    #[test]
    fn estimate_tokens_counts_utf16_units() {
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("😀😀"), 1);
        assert_eq!(estimate_tokens("😀😀😀"), 2);
    }

    /// Priority order: type first, unknown classification sorts
    /// BEFORE foundational (parity), recency breaks ties.
    #[test]
    fn apply_budget_selects_by_priority_and_keeps_domain_order() {
        let domains = vec![
            DomainRecords {
                domain:  "alpha".into(),
                records: vec![
                    record("reference", "foundational", "2026-01-01T00:00:00.000Z"),
                    record("convention", "tactical", "2026-01-02T00:00:00.000Z"),
                ],
            },
            DomainRecords {
                domain:  "zeta".into(),
                records: vec![record(
                    "decision",
                    "observational",
                    "2026-01-03T00:00:00.000Z",
                )],
            },
        ];
        // Budget only fits the convention (priority #1 by type).
        let result = apply_budget(&domains, 40, |record, domain| {
            format!(
                "{domain}:{}:{}",
                record["type"].as_str().unwrap(),
                "x".repeat(100)
            )
        });
        assert_eq!(result.kept.len(), 1);
        assert_eq!(result.kept[0].domain, "alpha");
        assert_eq!(result.kept[0].records[0]["type"], "convention");
        assert_eq!(result.dropped_count, 2);
        // Both domains lost a record (alpha's reference, zeta's
        // decision).
        assert_eq!(result.dropped_domain_count, 2);
    }

    /// The greedy walk continues past a record that does not fit.
    #[test]
    fn apply_budget_continues_after_a_misfit() {
        let domains = vec![DomainRecords {
            domain:  "one".into(),
            records: vec![
                record("convention", "tactical", "2026-01-01T00:00:00.000Z"),
                record("decision", "tactical", "2026-01-01T00:00:00.000Z"),
            ],
        }];
        let result = apply_budget(&domains, 110, |record, _| {
            // convention costs 104, decision costs 1 (priority 2).
            let size = if record["type"] == "convention" {
                400
            } else {
                4
            };
            "x".repeat(size)
        });
        assert_eq!(result.kept[0].records.len(), 2);
        assert_eq!(result.dropped_count, 0);
    }

    #[test]
    fn budget_summary_pluralizes() {
        assert_eq!(
            format_budget_summary(1, 0),
            "... and 1 more record (use --budget <n> to show more)"
        );
        assert_eq!(
            format_budget_summary(3, 2),
            "... and 3 more records across 2 domains (use --budget <n> to show more)"
        );
    }
}
