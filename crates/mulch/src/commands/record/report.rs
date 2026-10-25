//! The `record` report family: the create/skip/update action
//! envelopes and plain lines (mulch-351e).

use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, RecordArgs};
use crate::output::{print_json, print_line, success_envelope};

/// Which flag-path write decision is being reported (the upsert
/// plan's shape): create, or a duplicate skip/update against the
/// existing line at `index` (rendered 1-based, real runs only).
#[derive(Clone, Copy)]
pub(super) enum RecordAction {
    Create,
    Skip { index: usize },
    Update { index: usize },
}

impl RecordAction {
    /// The 0-based index of the existing duplicate line, if any.
    fn index(self) -> Option<usize> {
        match self {
            RecordAction::Create => None,
            RecordAction::Skip { index } | RecordAction::Update { index } => Some(index),
        }
    }
}

/// Builds the json success envelope for one flag-path decision:
/// `action`/`wouldDo?`/`domain`/`type`/`index?`/`record?`/`warnings?`
/// (key order pinned from the reference). Dry-run mirrors the write
/// decision — `action` reads `dry-run`, `wouldDo` carries the verb —
/// and the reported record drops its write-time id. The `warnings`
/// array rides LAST and only when non-empty (the reference spreads it
/// conditionally; hook warnings are the sole contributor here,
/// mulch-7f49).
fn record_envelope(
    action: RecordAction,
    domain: &str,
    record_type: Option<&str>,
    dry_run: bool,
    record: Option<Map<String, Value>>,
    warnings: &[String],
) -> Value {
    let kind = mulch::effective_type(record_type);
    let verb = match action {
        RecordAction::Create => "created",
        RecordAction::Skip { .. } => "skipped",
        RecordAction::Update { .. } => "updated",
    };
    let mut fields = serde_json::Map::new();
    if dry_run {
        fields.insert("action".into(), Value::String("dry-run".into()));
        fields.insert("wouldDo".into(), Value::String(verb.into()));
    } else {
        fields.insert("action".into(), Value::String(verb.into()));
    }
    fields.insert("domain".into(), Value::String(domain.into()));
    fields.insert("type".into(), Value::String(kind.to_string()));
    if !dry_run && let Some(index) = action.index() {
        fields.insert("index".into(), Value::from((index + 1) as u64));
    }
    if let Some(mut record) = record {
        if dry_run {
            record.remove("id");
        }
        fields.insert("record".into(), Value::Object(record));
    }
    if !warnings.is_empty() {
        fields.insert(
            "warnings".into(),
            Value::Array(warnings.iter().map(|w| Value::String(w.clone())).collect()),
        );
    }
    success_envelope("record", fields)
}

/// The plain line for one flag-path decision (wording pinned from the
/// reference): dry-run mirrors the write decision, skip/update render
/// the 1-based index of the existing duplicate.
fn record_plain_line(
    action: RecordAction,
    domain: &str,
    record_type: Option<&str>,
    dry_run: bool,
) -> String {
    let kind = mulch::effective_type(record_type);
    match (action, dry_run) {
        (RecordAction::Create, true) => format!(
            "✓ Dry-run: Would create {kind} in {domain}\n  Run without --dry-run to apply changes."
        ),
        (RecordAction::Create, false) => format!("✓ Recorded {kind} in {domain}"),
        (RecordAction::Skip { .. }, true) => format!(
            "Dry-run: Duplicate {kind} already exists in {domain}. Would skip.\n  Run without --dry-run to apply changes."
        ),
        (RecordAction::Skip { index }, false) => format!(
            "Duplicate {kind} already exists in {domain} (record #{}). Use --force to add anyway.",
            index + 1
        ),
        (RecordAction::Update { .. }, true) => format!(
            "✓ Dry-run: Would update existing {kind} in {domain}\n  Run without --dry-run to apply changes."
        ),
        (RecordAction::Update { index }, false) => format!(
            "✓ Updated existing {kind} in {domain} (record #{})",
            index + 1
        ),
    }
}

/// Renders one flag-path write decision on its channel: the json
/// success envelope (warnings array last, only when non-empty) or the
/// plain line (quiet-gated) followed by `Warning: {w}` lines on
/// stderr — unconditional, after the success line, in collection
/// order (reference: `for (const w of ...) console.error`).
pub(super) fn report_record_action(
    opts: &GlobalOpts,
    args: &RecordArgs,
    action: RecordAction,
    record: Option<Map<String, Value>>,
    warnings: &[String],
) {
    if opts.json {
        print_json(
            &record_envelope(
                action,
                &args.domain,
                args.record_type.as_deref(),
                args.dry_run,
                record,
                warnings,
            ),
            false,
        );
    } else {
        print_line(
            opts.quiet,
            &record_plain_line(
                action,
                &args.domain,
                args.record_type.as_deref(),
                args.dry_run,
            ),
        );
        if !warnings.is_empty() {
            #[allow(
                clippy::print_stderr,
                reason = "the warning lines are the CLI boundary"
            )]
            for warning in warnings {
                eprintln!("Warning: {warning}");
            }
        }
    }
}

/// Handles a detected duplicate without `--force` (reference
/// `record.ts` duplicate branch, mulch-ccf6): named types UPSERT in
/// place, anonymous types (`convention`/`failure`) skip with the
/// advisory.
/// The anonymous-duplicate advisory (flag path, `Skip` arm): nothing
/// is written; `--force` is the documented escape hatch. The
/// pre-record warnings (hooks fired before the decision was known)
/// surface with the advisory on both channels.
pub(super) fn skip_advisory(
    opts: &GlobalOpts,
    args: &RecordArgs,
    index: usize,
    record: Map<String, Value>,
    warnings: &[String],
) {
    // Dry-run mirrors the write decision through the incoming record
    // (reported id-less); the real skip reports no record at all.
    report_record_action(
        opts,
        args,
        RecordAction::Skip { index },
        args.dry_run.then_some(record),
        warnings,
    );
}

#[cfg(test)]
mod report_family_tests {
    use super::*;

    fn sample_record() -> Map<String, Value> {
        let mut record = Map::new();
        record.insert("type".into(), Value::String("pattern".into()));
        record.insert("name".into(), Value::String("p".into()));
        record.insert("id".into(), Value::String("mx-test".into()));
        record
    }

    fn keys(envelope: &Value) -> Vec<&str> {
        envelope
            .as_object()
            .expect("envelope object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    /// The three dry-run envelopes are one family: `dry-run` action,
    /// `wouldDo` verb, no index, and the reported record without its
    /// write-time id (key order is part of the parity contract).
    #[test]
    fn dry_run_envelopes_mirror_the_decision_and_drop_the_id() {
        for (action, would_do) in [
            (RecordAction::Create, "created"),
            (RecordAction::Skip { index: 3 }, "skipped"),
            (RecordAction::Update { index: 3 }, "updated"),
        ] {
            let envelope = record_envelope(
                action,
                "d",
                Some("pattern"),
                true,
                Some(sample_record()),
                &[],
            );
            assert_eq!(keys(&envelope), [
                "success", "command", "action", "wouldDo", "domain", "type", "record",
            ]);
            assert_eq!(envelope["action"], "dry-run");
            assert_eq!(envelope["wouldDo"], would_do);
            assert_eq!(envelope["type"], "pattern");
            assert!(envelope["record"].get("id").is_none());
        }
    }

    /// Real runs carry the action verb, the 1-based index for
    /// skip/update (never for create), and the written record — the
    /// skip advisory reports no record at all.
    #[test]
    fn real_run_envelopes_carry_action_index_and_record() {
        let create = record_envelope(
            RecordAction::Create,
            "d",
            None,
            false,
            Some(sample_record()),
            &[],
        );
        assert_eq!(keys(&create), [
            "success", "command", "action", "domain", "type", "record"
        ]);
        assert_eq!(create["action"], "created");
        assert_eq!(create["type"], "convention");
        assert_eq!(create["record"]["id"], "mx-test");

        let skip = record_envelope(RecordAction::Skip { index: 2 }, "d", None, false, None, &[]);
        assert_eq!(keys(&skip), [
            "success", "command", "action", "domain", "type", "index"
        ]);
        assert_eq!(skip["index"], 3);

        let update = record_envelope(
            RecordAction::Update { index: 2 },
            "d",
            None,
            false,
            Some(sample_record()),
            &[],
        );
        assert_eq!(keys(&update), [
            "success", "command", "action", "domain", "type", "index", "record"
        ]);
        assert_eq!(update["index"], 3);
        assert_eq!(update["record"]["id"], "mx-test");
    }

    /// The hook-warnings array rides LAST in the envelope and only
    /// when non-empty (mulch-7f49: pre-record warnings surface even
    /// on a skip, both families join pre-then-post).
    #[test]
    fn warnings_ride_the_envelope_last_and_only_when_non_empty() {
        let warned = record_envelope(RecordAction::Skip { index: 1 }, "d", None, false, None, &[
            "hook `exit 5` exited with code 5".to_string(),
        ]);
        assert_eq!(keys(&warned), [
            "success", "command", "action", "domain", "type", "index", "warnings"
        ]);
        assert_eq!(
            warned["warnings"],
            serde_json::json!(["hook `exit 5` exited with code 5"])
        );
    }

    /// The six plain lines, word-for-word; a missing `--type`
    /// reports the convention default.
    #[test]
    fn plain_lines_pin_the_reference_wording() {
        assert_eq!(
            record_plain_line(RecordAction::Create, "d", None, true),
            "✓ Dry-run: Would create convention in d\n  Run without --dry-run to apply changes."
        );
        assert_eq!(
            record_plain_line(RecordAction::Create, "d", Some("pattern"), false),
            "✓ Recorded pattern in d"
        );
        assert_eq!(
            record_plain_line(RecordAction::Skip { index: 1 }, "d", None, true),
            "Dry-run: Duplicate convention already exists in d. Would skip.\n  Run without --dry-run to apply changes."
        );
        assert_eq!(
            record_plain_line(RecordAction::Skip { index: 1 }, "d", None, false),
            "Duplicate convention already exists in d (record #2). Use --force to add anyway."
        );
        assert_eq!(
            record_plain_line(
                RecordAction::Update { index: 4 },
                "d",
                Some("reference"),
                true
            ),
            "✓ Dry-run: Would update existing reference in d\n  Run without --dry-run to apply changes."
        );
        assert_eq!(
            record_plain_line(RecordAction::Update { index: 4 }, "d", Some("guide"), false),
            "✓ Updated existing guide in d (record #5)"
        );
    }
}
