//! `mulch prune` pins (mulch-7c55), ADR-0008 edition: the shelf-life
//! archive path (`--hard` deletion, `--dry-run`), the
//! supersession-demotion ladder (`--aggressive`, `--explain`, the
//! JSON envelope), the anchor-decay demotion (`--check-anchors`),
//! and the cycle warning — output channels plus the on-disk live and
//! archive bytes. Literals frozen from the parity era's twin runs.
//!
//! Every fixture stamp is clock-derived a full day away from the
//! nearest boundary: a 40-day tactical is past the 14-day shelf, a
//! 30-day anchored foundational is past the 7-day anchor grace, a
//! 2-day record is inside every window.

use std::path::Path;

use super::support::*;

/// One clock-derived stamp `days` before now, in the millisecond ISO
/// form.
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

/// Seeds a fresh store per call (prune mutates, so cases must not
/// share one) and returns it.
fn pruned_store(tag: &str, body: &str) -> TempDir {
    seeded_store(tag, None, &[("dev", body)], &[])
}

#[test]
fn prune_empty_store_pins_the_notice() {
    for (tag, args, expected) in [
        (
            "pr-empty",
            &["prune"][..],
            "No stale or superseded records found. All records are current.\n",
        ),
        (
            "pr-empty-json",
            &["--json", "prune"][..],
            "{\n  \"success\": true,\n  \"command\": \"prune\",\n  \"dryRun\": false,\n  \
             \"hard\": false,\n  \"aggressive\": false,\n  \"checkAnchors\": false,\n  \
             \"totalPruned\": 0,\n  \"totalDemoted\": 0,\n  \"totalAnchorDemoted\": 0,\n  \
             \"totalSupersessionDemoted\": 0,\n  \"results\": []\n}\n",
        ),
        ("pr-empty-quiet", &["--quiet", "prune"][..], ""),
    ] {
        let ours = pruned_store(tag, "");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        assert_eq!(our.stdout, expected, "{tag}");
    }
}

#[test]
fn prune_stale_archive_delete_and_dry_run_pin_their_bytes() {
    let body = [
        pattern("mx-old001", "tactical", &days_ago(40), "Old Tactical", ""),
        pattern("mx-fre001", "foundational", &days_ago(2), "Fresh One", ""),
    ]
    .join("\n")
        + "\n";
    // dry run: the preview, nothing written
    let ours = pruned_store("pr-stale-dry", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prune", "--dry-run"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "[DRY RUN] dev: Would archive 1 of 2 records (1 remaining)\n  mx-old001 [pattern]: \
         tactical → archived (stale)\n\n[DRY RUN] Total: would archive 1 stale record.\n"
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        body,
        "the dry run mutates nothing"
    );
    // the real run: notice lines + the archive side effect
    let ours = pruned_store("pr-stale-run", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prune"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "dev: Archived 1 of 2 records (1 remaining)\n  mx-old001 [pattern]: tactical → \
         archived (stale)\n\nTotal: archived 1 stale record.\nRecords moved to \
         .mulch/archive/. Restore with `mulch restore <id>` or use `--hard` next time to \
         permanently delete.\n"
    );
    let survivors = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(survivors.contains("mx-fre001"), "{survivors}");
    assert!(!survivors.contains("mx-old001"), "{survivors}");
    // json: the counts envelope
    let ours = pruned_store("pr-stale-json", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "prune"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        normalize(&our.stdout),
        "{\n  \"success\": true,\n  \"command\": \"prune\",\n  \"dryRun\": false,\n  \
         \"hard\": false,\n  \"aggressive\": false,\n  \"checkAnchors\": false,\n  \
         \"totalPruned\": 1,\n  \"totalDemoted\": 0,\n  \"totalAnchorDemoted\": 0,\n  \
         \"totalSupersessionDemoted\": 0,\n  \"results\": [\n    {\n      \"domain\": \"dev\",\n      \
         \"before\": 2,\n      \"pruned\": 1,\n      \"demoted\": 0,\n      \"anchor_demoted\": 0,\n      \
         \"supersession_demoted\": 0,\n      \"after\": 1\n    }\n  ]\n}\n"
    );
    // quiet: the summary line only
    let ours = pruned_store("pr-stale-quiet", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--quiet", "prune"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "Total: archived 1 stale record.\n");
    // --hard deletes instead of archiving (no archive directory at
    // all; the dry-run pass mutates nothing either)
    let ours = pruned_store("pr-hard-dry", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prune",
        "--hard",
        "--dry-run",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        body,
        "hard dry run mutates nothing"
    );
    let ours = pruned_store("pr-hard", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prune", "--hard"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "dev: Deleted 1 of 2 records (1 remaining)\n  mx-old001 [pattern]: tactical → \
         archived (stale)\n\nTotal: deleted 1 stale record.\n"
    );
    assert!(
        !ours.0.join(".mulch/archive").exists(),
        "hard leaves no archive"
    );
}

#[test]
fn prune_supersession_ladder_pins_its_bytes() {
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
    // dry run: one tier demotes, the next archives
    let ours = pruned_store("pr-sup-dry", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prune", "--dry-run"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "[DRY RUN] dev: Would archive 1, demoted 1 of 4 records (3 remaining)\n  mx-old001 \
         [pattern]: foundational → tactical (superseded)\n  mx-mid001 [pattern]: observational \
         → archived (superseded)\n\n[DRY RUN] Total: would archive 1 stale record; would demote \
         1 superseded record.\n"
    );
    // the real run says the same without the preview brackets
    let ours = pruned_store("pr-sup-run", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prune"]);
    assert_eq!(our.code, 0);
    assert!(
        our.stdout
            .starts_with("dev: Archived 1, demoted 1 of 4 records (3 remaining)")
    );
    assert!(our.stdout.contains("foundational → tactical (superseded)"));
    assert!(our.stdout.contains("observational → archived (superseded)"));
    // json + --explain: the explanations array rides the envelope
    let ours = pruned_store("pr-sup-json", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "prune",
        "--explain",
    ]);
    assert_eq!(our.code, 0);
    assert!(normalize(&our.stdout).contains("\"totalSupersessionDemoted\": 1"));
    assert!(normalize(&our.stdout).contains("\"id\": \"mx-old001\""));
    assert!(normalize(&our.stdout).contains("\"from\": \"foundational\""));
    assert!(normalize(&our.stdout).contains("\"to\": \"tactical\""));
    assert!(normalize(&our.stdout).contains("\"to\": \"archived\""));
    // aggressive: both superseded records archive at once
    let ours = pruned_store("pr-sup-aggr", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prune", "--aggressive"]);
    assert_eq!(our.code, 0);
    assert!(
        our.stdout
            .starts_with("dev: Archived 2 of 4 records (2 remaining)")
    );
    assert!(our.stdout.contains("foundational → archived (superseded)"));
}

#[test]
fn prune_anchor_decay_pins_its_bytes() {
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
    let ours = pruned_store("pr-anchor", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prune",
        "--check-anchors",
        "--explain",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "dev: Demoted 1 of 2 records (2 remaining)\n  mx-anc001 [pattern]: foundational → \
         tactical (anchor_decay)\n      anchors: 0/1 valid (0%)\n        ✗ file: \
         src/missing.rs\n\nTotal: demoted 1 anchor-decayed record.\n"
    );
    let ours = pruned_store("pr-anchor-json", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "prune",
        "--check-anchors",
    ]);
    assert_eq!(our.code, 0);
    assert!(normalize(&our.stdout).contains("\"totalAnchorDemoted\": 1"));
    // dry-run mutates nothing
    let ours = pruned_store("pr-anchor-dry", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prune",
        "--check-anchors",
        "--dry-run",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        body,
        "anchor dry run mutates nothing"
    );
}

#[test]
fn prune_cycle_warning_pins_its_bytes() {
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
    let ours = pruned_store("pr-cycle", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prune"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "No stale or superseded records found. All records are current.\n"
    );
    assert_eq!(
        our.stderr,
        "Warning: supersession cycle detected for 2 record(s); cycle members will not be \
         demoted. Run `mulch doctor` for details.\n"
    );
    // json carries no warning channel; the run stays a no-op
    let ours = pruned_store("pr-cycle-json", &body);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "prune"]);
    assert_eq!(our.code, 0);
    assert!(normalize(&our.stdout).contains("\"results\": []"));
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        body,
        "cycle members never move"
    );
}

// ---- pre-prune hooks (mulch-7f49) ----

/// The mixed-bucket fixture: two stale records (one also a
/// supersession target), a plain supersession hit, an anchor-decay
/// hit, a record hitting BOTH supersession and anchor decay, and the
/// fresh keeper whose `supersedes` drives the graph — plus a second
/// domain with nothing to do.
fn hook_fixture_body() -> String {
    [
        pattern("mx-old001", "tactical", &days_ago(40), "Stale Tac", ""),
        pattern("mx-sts001", "tactical", &days_ago(40), "Stale Super", ""),
        pattern("mx-sup001", "foundational", &days_ago(2), "Superseded", ""),
        pattern(
            "mx-anc001",
            "foundational",
            &days_ago(30),
            "Anchor Dead",
            ",\"files\":[\"missing-a.txt\"]",
        ),
        pattern(
            "mx-both001",
            "foundational",
            &days_ago(30),
            "Both Hits",
            ",\"files\":[\"missing-b.txt\"]",
        ),
        pattern(
            "mx-keep001",
            "foundational",
            &days_ago(2),
            "Keeper",
            ",\"supersedes\":[\"mx-sup001\",\"mx-both001\",\"mx-sts001\"]",
        ),
    ]
    .join("\n")
        + "\n"
}

/// A store over the mixed fixture with the given hooks body (the
/// config carries the canonical tail plus a calm second domain).
fn prune_hook_store(tag: &str, hook_yaml: &str) -> TempDir {
    let calm = pattern("mx-calm001", "tactical", &days_ago(2), "Calm One", "");
    let config =
        format!("version: '1'\ndomains:\n  dev: {{}}\n  calm: {{}}\n{hook_yaml}{CONFIG_TAIL}");
    let body = hook_fixture_body();
    seeded_store(
        tag,
        Some(config.as_str()),
        &[("dev", body.as_str()), ("calm", calm.as_str())],
        &[],
    )
}

/// The payload law: `{"event":"pre-prune","payload":{"candidates":
/// [...]}}` — only candidate domains, raw records per bucket in file
/// order, a both-hits record riding demote AND anchor_decay,
/// staleness excluding a record from the other buckets (all parsed
/// and asserted on OUR capture).
#[test]
fn prune_pre_prune_payload_law() {
    for (tag, args) in [
        ("prh-pay-anchors", &["prune", "--check-anchors"][..]),
        ("prh-pay-json", &["--json", "prune", "--check-anchors"][..]),
        ("prh-pay-plain", &["prune"][..]),
    ] {
        let ours = prune_hook_store(tag, "hooks:\n  pre-prune:\n    - 'cat > hook-stdin.txt'\n");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        let our_stdin = hook_stdin(&ours.0);
        assert!(our_stdin.is_some(), "{tag}: our hook fired");
        // Bucket law: file-order ids per bucket, staleness excluding
        // a record from demote/anchor_decay, the both-hits record
        // riding demote AND anchor_decay, calm absent.
        let payload: serde_json::Value =
            serde_json::from_str(our_stdin.as_deref().unwrap_or_default())
                .expect("hook stdin parses");
        assert_eq!(payload["event"], "pre-prune", "{tag}: event envelope");
        let candidates = payload["payload"]["candidates"]
            .as_array()
            .expect("candidates array");
        assert_eq!(candidates.len(), 1, "{tag}: calm has no candidate row");
        let row = &candidates[0];
        assert_eq!(row["domain"], "dev", "{tag}: row domain");
        let ids = |bucket: &str| -> Vec<&str> {
            row[bucket]
                .as_array()
                .expect("bucket array")
                .iter()
                .map(|record| record["id"].as_str().unwrap_or_default())
                .collect()
        };
        assert_eq!(
            ids("stale"),
            ["mx-old001", "mx-sts001"],
            "{tag}: stale bucket"
        );
        assert_eq!(
            ids("demote"),
            ["mx-sup001", "mx-both001"],
            "{tag}: demote bucket"
        );
        let expected_anchor: Vec<&str> = if args.contains(&"--check-anchors") {
            vec!["mx-anc001", "mx-both001"]
        } else {
            Vec::new()
        };
        assert_eq!(ids("anchor_decay"), expected_anchor, "{tag}: anchor bucket");
        // The row's key order is the wire order (domain, stale,
        // demote, anchor_decay).
        assert_eq!(
            row.as_object()
                .expect("row object")
                .keys()
                .collect::<Vec<_>>(),
            ["domain", "stale", "demote", "anchor_decay"],
            "{tag}: bucket key order"
        );
    }
}

/// The gate law: no candidates or --dry-run fire NO hooks — the
/// preview must not trigger side-effecting hooks.
#[test]
fn prune_pre_prune_gates() {
    // Nothing to do anywhere: no hook, the all-current notice.
    let calm = pattern("mx-fresh01", "foundational", &days_ago(2), "Fresh One", "");
    for (tag, args, expected) in [
        (
            "prh-none",
            &["prune"][..],
            "No stale or superseded records found. All records are current.\n",
        ),
        (
            "prh-none-json",
            &["--json", "prune"][..],
            "{\n  \"success\": true,\n  \"command\": \"prune\",\n  \"dryRun\": false,\n  \
             \"hard\": false,\n  \"aggressive\": false,\n  \"checkAnchors\": false,\n  \
             \"totalPruned\": 0,\n  \"totalDemoted\": 0,\n  \"totalAnchorDemoted\": 0,\n  \
             \"totalSupersessionDemoted\": 0,\n  \"results\": []\n}\n",
        ),
    ] {
        let ours = seeded_store(
            tag,
            Some(
                format!(
                    "version: '1'\ndomains:\n  dev: {{}}\nhooks:\n  pre-prune:\n    - 'cat > \
                     hook-stdin.txt'\n{CONFIG_TAIL}\n"
                )
                .as_str(),
            ),
            &[("dev", calm.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        assert_eq!(our.stdout, expected, "{tag}");
        assert!(hook_stdin(&ours.0).is_none(), "{tag}: our hook stayed cold");
    }

    // Candidates present but --dry-run: still no hook, nothing
    // written.
    let ours = prune_hook_store(
        "prh-dry",
        "hooks:\n  pre-prune:\n    - 'cat > hook-stdin.txt'\n",
    );
    let before = read_store_file(&ours.0, "expertise/dev.jsonl");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prune", "--dry-run"]);
    assert_eq!(our.code, 0, "dry: {}", our.stderr);
    assert!(hook_stdin(&ours.0).is_none(), "our hook stayed cold");
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        before,
        "dry run left the live bytes"
    );
}

/// Blocking: a non-zero exit or a timeout aborts between preview and
/// write — plain `Error: …` stderr / the json envelope, exit 1, and
/// NOTHING written to the store.
#[test]
fn prune_pre_prune_blocking_pins_the_bytes() {
    for (tag, args, hook_yaml, json_mode, reason) in [
        (
            "prh-block-plain",
            &["prune", "--check-anchors"][..],
            "hooks:\n  pre-prune:\n    - 'printf prune-pfx >&2; exit 4'\n",
            false,
            "hook `printf prune-pfx >&2; exit 4` exited with code 4",
        ),
        (
            "prh-block-json",
            &["--json", "prune", "--check-anchors"][..],
            "hooks:\n  pre-prune:\n    - 'printf prune-pfx >&2; exit 4'\n",
            true,
            "hook `printf prune-pfx >&2; exit 4` exited with code 4",
        ),
        (
            "prh-block-timeout",
            &["prune"][..],
            "hooks:\n  pre-prune:\n    - 'sleep 30 & wait'\nhook_settings:\n  \
             timeout_ms: 150\n",
            false,
            "hook `sleep 30 & wait` timed out after 150ms",
        ),
    ] {
        let ours = prune_hook_store(tag, hook_yaml);
        let before = read_store_file(&ours.0, "expertise/dev.jsonl");
        let started = std::time::Instant::now();
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let our_elapsed = started.elapsed();
        assert_eq!(our.code, 1, "{tag}: blocked exits 1");
        assert!(our.stdout.is_empty(), "{tag}: no stdout when blocked");
        // The forwarded hook stderr precedes the envelope (the
        // timeout variant has no stderr prefix).
        if tag == "prh-block-timeout" {
            assert_eq!(
                our.stderr,
                format!("Error: {reason}\n"),
                "{tag}: our stderr"
            );
            assert!(
                our_elapsed.as_secs() < 10,
                "{tag}: group kill returned promptly"
            );
        } else {
            let expected = if json_mode {
                format!(
                    "prune-pfx\n{}",
                    error_envelope("prune", reason).trim_end_matches('\n')
                ) + "\n"
            } else {
                format!("prune-pfx\nError: {reason}\n")
            };
            assert_eq!(our.stderr, expected, "{tag}: our stderr");
        }
        // The write phase never ran: live intact, no archive.
        assert_eq!(
            read_store_file(&ours.0, "expertise/dev.jsonl"),
            before,
            "{tag} live bytes (untouched)"
        );
        assert!(
            read_store_text_or_empty(&ours.0, "archive/dev.jsonl").is_empty(),
            "{tag}: no archive written"
        );
    }
}

/// A successful hook's stderr forwards verbatim in both modes, and a
/// hook's STDOUT is ignored — block-or-allow, no mutation even though
/// pre-prune is a `pre-*` event (candidate-reshaping JSON changes
/// nothing; the run matches a hookless baseline byte for byte).
#[test]
fn prune_hook_stderr_forward_and_stdout_ignored() {
    // Stderr forwarding on a successful run, plain and json.
    for (tag, args) in [
        ("prh-fwd-plain", &["prune", "--check-anchors"][..]),
        ("prh-fwd-json", &["--json", "prune", "--check-anchors"][..]),
    ] {
        let ours = prune_hook_store(tag, "hooks:\n  pre-prune:\n    - 'printf side-noise >&2'\n");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        assert_eq!(our.stderr, "side-noise\n", "{tag}: forwarded verbatim");
        assert!(
            normalize(&read_store_text_or_empty(&ours.0, "expertise/dev.jsonl"))
                .contains("\"id\":\"mx-keep001\""),
            "{tag} live bytes survived the prune"
        );
    }

    // Stdout ignored: the same seed bytes go through a hooked and a
    // hookless run in two stores — the resulting files must match
    // (normalize masks the demotion stamps the run itself writes).
    let hooked = prune_hook_store(
        "prh-stdout",
        "hooks:\n  pre-prune:\n    - 'printf %s \"{\\\"candidates\\\":[]}\"'\n",
    );
    let baseline = prune_hook_store("prh-stdout-base", "");
    let seed_bytes = read_store_text_or_empty(&hooked.0, "expertise/dev.jsonl");
    std::fs::write(
        baseline
            .0
            .join(".mulch")
            .join("expertise")
            .join("dev.jsonl"),
        &seed_bytes,
    )
    .expect("baseline seed overwritten");
    let our = run_in(&hooked.0, Path::new(mulch_bin()), &[
        "prune",
        "--check-anchors",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(our.stderr.is_empty(), "stdout-ignored: our stderr clean");
    let base = run_in(&baseline.0, Path::new(mulch_bin()), &[
        "prune",
        "--check-anchors",
    ]);
    assert_eq!(our.code, base.code, "hook stdout changed no exit");
    assert_eq!(our.stdout, base.stdout, "hook stdout changed no output");
    assert_eq!(
        normalize(&read_store_text_or_empty(&hooked.0, "expertise/dev.jsonl")),
        normalize(&read_store_text_or_empty(
            &baseline.0,
            "expertise/dev.jsonl"
        )),
        "hook stdout changed no bytes (same seed, hooked vs hookless run)"
    );
}
