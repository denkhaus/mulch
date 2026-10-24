//! `mulch prune` parity probes (mulch-7c55): the shelf-life archive
//! path (`--hard` deletion, `--dry-run`), the supersession-demotion
//! ladder (`--aggressive`, `--explain`, the JSON envelope), the
//! anchor-decay demotion (`--check-anchors`), and the cycle warning
//! — output channels plus the on-disk live and archive bytes.
//!
//! Every fixture stamp is clock-derived a full day away from the
//! nearest boundary (`/tmp`-independent): a 40-day tactical is past
//! the 14-day shelf, a 30-day anchored foundational is past the
//! 7-day anchor grace, a 2-day record is inside every window.

use std::path::Path;

use super::support::*;

/// One clock-derived stamp `days` before now, in the reference's
/// millisecond ISO form.
fn days_ago(days: i64) -> String {
    let hours = days * 24;
    (jiff::Timestamp::now() - jiff::Span::new().hours(hours)).to_string()
}

/// A schema-valid `pattern` line.
fn pattern(id: &str, classification: &str, recorded: &str, name: &str, extra: &str) -> String {
    format!(
        "{{\"type\":\"pattern\",\"classification\":\"{classification}\",\"recorded_at\":\"{recorded}\",\"name\":\"{name}\",\"description\":\"{name} body\",\"id\":\"{id}\"{extra}}}"
    )
}

/// Asserts one twin-run pair: exit, both channels, live bytes, and
/// archive bytes (timestamps masked, the reference's `ml`
/// self-references promoted — our banner says `mulch restore`).
fn assert_prune_parity(ours: &TempDir, theirs: &TempDir, tag: &str, args: &[&str]) {
    let ml = reference_ml().expect("caller checked the reference");
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, &ml, args);
    assert_eq!(our.code, their.code, "{tag} exit");
    assert_eq!(our.stdout, promote_theirs(&their.stdout), "{tag} stdout");
    assert_eq!(our.stderr, promote_theirs(&their.stderr), "{tag} stderr");
    assert_eq!(
        normalize(&read_store_text_or_empty(&ours.0, "expertise/dev.jsonl")),
        promote_theirs(&normalize(&read_store_text_or_empty(
            &theirs.0,
            "expertise/dev.jsonl"
        ))),
        "{tag} live bytes"
    );
    assert_eq!(
        normalize(&read_store_text_or_empty(&ours.0, "archive/dev.jsonl")),
        promote_theirs(&normalize(&read_store_text_or_empty(
            &theirs.0,
            "archive/dev.jsonl"
        ))),
        "{tag} archive bytes"
    );
}

/// Seeds fresh twins per call (prune mutates, so cases must not share
/// a store) and asserts the parity of one command invocation.
fn pruned_pair(tag: &str, body: &str, args: &[&str]) {
    if reference_ml().is_none() {
        eprintln!("skipped: no ml on PATH");
        return;
    }
    let (ours, theirs) = twin_seeded(tag, None, &[("dev", body)], &[]);
    assert_prune_parity(&ours, &theirs, tag, args);
}

#[test]
fn prune_empty_store_matches_reference() {
    for args in [
        &["prune"][..],
        &["--json", "prune"][..],
        &["--quiet", "prune"][..],
    ] {
        pruned_pair("pr-empty", "", args);
    }
}

#[test]
fn prune_stale_archive_delete_and_dry_run_match_reference() {
    let body = [
        pattern("mx-old001", "tactical", &days_ago(40), "Old Tactical", ""),
        pattern("mx-fre001", "foundational", &days_ago(2), "Fresh One", ""),
    ]
    .join("\n")
        + "\n";
    for args in [
        &["prune", "--dry-run"][..],
        &["prune"][..],
        &["--json", "prune"][..],
        &["--quiet", "prune"][..],
    ] {
        pruned_pair("pr-stale", &body, args);
    }
    // --hard deletes instead of archiving (no archive directory at
    // all; the dry-run pass mutates nothing either).
    pruned_pair("pr-hard-dry", &body, &["prune", "--hard", "--dry-run"]);
    let (ours, theirs) = twin_seeded("pr-hard", None, &[("dev", body.as_str())], &[]);
    assert_prune_parity(&ours, &theirs, "pr-hard", &["prune", "--hard"]);
    assert!(
        !ours.0.join(".mulch/archive").exists(),
        "hard leaves no archive"
    );
}

#[test]
fn prune_supersession_ladder_matches_reference() {
    let body = [
        pattern("mx-old001", "foundational", &days_ago(3), "Old One", ""),
        pattern(
            "mx-new001",
            "tactical",
            &days_ago(3),
            "New One",
            ",\"supersedes\":[\"mx-old001\"]",
        ),
        pattern("mx-mid001", "observational", &days_ago(3), "Mid One", ""),
        pattern(
            "mx-mid002",
            "tactical",
            &days_ago(3),
            "Mid Two",
            ",\"supersedes\":[\"mx-mid001\"]",
        ),
    ]
    .join("\n")
        + "\n";
    for args in [
        &["prune", "--dry-run"][..],
        &["prune"][..],
        &["--json", "prune", "--explain"][..],
        &["prune", "--aggressive", "--dry-run"][..],
        &["prune", "--aggressive"][..],
    ] {
        pruned_pair("pr-sup", &body, args);
    }
    // One tier per pass: the second run demotes the next tier.
    if reference_ml().is_some() {
        let (ours, theirs) = twin_seeded("pr-sup2", None, &[("dev", body.as_str())], &[]);
        assert_prune_parity(&ours, &theirs, "pr-sup2-1", &["prune"]);
        assert_prune_parity(&ours, &theirs, "pr-sup2-2", &["prune"]);
    }
}

#[test]
fn prune_anchor_decay_matches_reference() {
    let body = [
        pattern(
            "mx-anc001",
            "foundational",
            &days_ago(30),
            "Anchored One",
            ",\"files\":[\"src/missing.rs\"]",
        ),
        pattern("mx-exe001", "foundational", &days_ago(30), "Exempt One", ""),
    ]
    .join("\n")
        + "\n";
    for args in [
        &["prune", "--check-anchors", "--explain"][..],
        &["--json", "prune", "--check-anchors"][..],
        &["prune", "--check-anchors", "--dry-run"][..],
    ] {
        pruned_pair("pr-anchor", &body, args);
    }
}

#[test]
fn prune_cycle_warning_matches_reference() {
    let body = [
        pattern(
            "mx-aaa001",
            "foundational",
            &days_ago(3),
            "A One",
            ",\"supersedes\":[\"mx-bbb001\"]",
        ),
        pattern(
            "mx-bbb001",
            "foundational",
            &days_ago(3),
            "B One",
            ",\"supersedes\":[\"mx-aaa001\"]",
        ),
    ]
    .join("\n")
        + "\n";
    for args in [
        &["prune"][..],
        &["--json", "prune"][..],
        &["--quiet", "prune"][..],
    ] {
        pruned_pair("pr-cycle", &body, args);
    }
}
