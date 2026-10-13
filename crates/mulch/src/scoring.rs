//! Confirmation-frequency scoring — the reference `utils/scoring.ts`
//! law (mulch-228c's first slice; its compact-meta consumer landed with
//! the render seam, mulch-8ba4). Successful outcomes count 1, partial
//! ones 0.5; records without outcomes score 0. The BM25/search side of
//! the reference's scoring stays with the 16da rank/search sprint.

use serde_json::Value;

/// The record's successful outcome count (confirmed applications).
pub fn success_count(record: &Value) -> u64 {
    outcomes(record)
        .filter(|o| o.get("status").and_then(Value::as_str) == Some("success"))
        .count() as u64
}

/// The record's failed outcome count.
pub fn failure_count(record: &Value) -> u64 {
    outcomes(record)
        .filter(|o| o.get("status").and_then(Value::as_str) == Some("failure"))
        .count() as u64
}

/// The record's total outcome (application) count.
pub fn total_applications(record: &Value) -> u64 {
    outcomes(record).count() as u64
}

/// The record's success rate in `0.0..=1.0` (partials count 0.5; no
/// outcomes yields 0).
pub fn success_rate(record: &Value) -> f64 {
    let total = total_applications(record);
    if total == 0 {
        return 0.0;
    }
    let partials = outcomes(record)
        .filter(|o| o.get("status").and_then(Value::as_str) == Some("partial"))
        .count() as f64;
    (success_count(record) as f64 + partials * 0.5) / total as f64
}

/// The confirmation-frequency score: successes plus half the partials
/// (`computeConfirmationScore`).
pub fn confirmation_score(record: &Value) -> f64 {
    let partials = outcomes(record)
        .filter(|o| o.get("status").and_then(Value::as_str) == Some("partial"))
        .count() as f64;
    success_count(record) as f64 + partials * 0.5
}

/// Applies the confirmation-frequency boost to a base relevance score
/// (`applyConfirmationBoost`): zero-score records pass through.
pub fn apply_confirmation_boost(base: f64, record: &Value, boost_factor: f64) -> f64 {
    let score = confirmation_score(record);
    if score == 0.0 {
        return base;
    }
    base * (1.0 + boost_factor * score)
}

/// Sorts records by confirmation score, highest first, stable for
/// ties (`sortByConfirmationScore`).
pub fn sort_by_confirmation(records: &mut [Value]) {
    records.sort_by(|a, b| {
        confirmation_score(b)
            .partial_cmp(&confirmation_score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn outcomes(record: &Value) -> impl Iterator<Item = &Value> {
    record
        .get("outcomes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

#[cfg(test)]
mod scoring_tests {
    use serde_json::json;

    use super::*;

    fn record(outcomes: &serde_json::Value) -> Value {
        json!({"type": "pattern", "name": "p", "description": "d", "outcomes": outcomes})
    }

    /// The confirmation law: 1 per success, 0.5 per partial, failures
    /// and missing outcomes contribute nothing.
    #[test]
    fn confirmation_score_counts_successes_and_half_partials() {
        assert_eq!(confirmation_score(&record(&json!([]))), 0.0);
        assert_eq!(
            confirmation_score(&record(&json!([{"status": "success"}]))),
            1.0
        );
        assert_eq!(
            confirmation_score(&record(&json!([
                {"status": "success"}, {"status": "partial"}
            ]))),
            1.5
        );
        assert_eq!(
            confirmation_score(&record(&json!([
                {"status": "failure"}, {"status": "failure"}
            ]))),
            0.0
        );
        // missing outcomes field
        assert_eq!(
            confirmation_score(&json!({"type": "pattern", "name": "p", "description": "d"})),
            0.0
        );
    }

    /// Rates, counts, the boost, and the stable descending sort.
    #[test]
    fn rates_boost_and_sort_mirror_the_reference() {
        let two = record(&json!([{"status": "success"}, {"status": "partial"}]));
        assert_eq!(total_applications(&two), 2);
        assert_eq!(success_count(&two), 1);
        assert_eq!(failure_count(&two), 0);
        assert_eq!(success_rate(&two), 0.75);

        let none = record(&json!([]));
        assert_eq!(apply_confirmation_boost(2.0, &none, 0.1), 2.0);
        assert!((apply_confirmation_boost(2.0, &two, 0.1) - 2.3).abs() < 1e-9);

        let mut records = vec![none.clone(), two, record(&json!([{"status": "success"}]))];
        sort_by_confirmation(&mut records);
        let scores: Vec<f64> = records.iter().map(confirmation_score).collect();
        assert_eq!(scores, [1.5, 1.0, 0.0]);
    }
}
