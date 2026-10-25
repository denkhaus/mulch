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

// ---- pre-prune hooks (mulch-7f49; probed live 2026-10-25,
// /tmp/sprint42/childB/probes/prune) ----

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

/// Twin stores over the mixed fixture with the given hooks body (the
/// config carries the canonical tail plus a calm second domain).
fn prune_hook_twins(tag: &str, hook_yaml: &str) -> (TempDir, TempDir) {
    let calm = pattern("mx-calm001", "tactical", &days_ago(2), "Calm One", "");
    let config =
        format!("version: '1'\ndomains:\n  dev: {{}}\n  calm: {{}}\n{hook_yaml}{CONFIG_TAIL}");
    let body = hook_fixture_body();
    twin_seeded(
        tag,
        Some(config.as_str()),
        &[("dev", body.as_str()), ("calm", calm.as_str())],
        &[],
    )
}

/// The payload law (probe PR-A): `{"event":"pre-prune","payload":\
/// {"candidates":[{"domain":…,"stale":[…],"demote":[…],
/// "anchor_decay":[…]}]}}` — only candidate domains, raw records per
/// bucket in file order, a both-hits record riding demote AND
/// anchor_decay, staleness excluding a record from the other buckets.
/// Byte-compared against the reference's capture (timestamps
/// normalized), with the run's own output/store parity.
#[test]
fn prune_pre_prune_payload_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for (tag, args) in [
        ("prh-pay-anchors", &["prune", "--check-anchors"][..]),
        ("prh-pay-json", &["--json", "prune", "--check-anchors"][..]),
        ("prh-pay-plain", &["prune"][..]),
    ] {
        let (ours, theirs) =
            prune_hook_twins(tag, "hooks:\n  pre-prune:\n    - 'cat > hook-stdin.txt'\n");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{tag} exit");
        assert_eq!(our.stdout, promote_theirs(&their.stdout), "{tag} stdout");
        assert_eq!(our.stderr, their.stderr, "{tag} stderr");
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
        let (our_stdin, their_stdin) = (hook_stdin(&ours.0), hook_stdin(&theirs.0));
        assert!(our_stdin.is_some(), "{tag}: our hook fired");
        assert!(their_stdin.is_some(), "{tag}: reference hook fired");
        assert_eq!(
            normalize(our_stdin.as_deref().unwrap_or_default()),
            normalize(their_stdin.as_deref().unwrap_or_default()),
            "{tag}: hook stdin payload bytes"
        );
        // Bucket law (probe PR-A), asserted on the parsed capture:
        // file-order ids per bucket, staleness excluding a record
        // from demote/anchor_decay, the both-hits record riding
        // demote AND anchor_decay, calm absent.
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

/// The gate law: no candidates or --dry-run fire NO hooks (probe
/// PR-B/PR-C) — the preview must not trigger side-effecting hooks.
#[test]
fn prune_pre_prune_gates_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Nothing to do anywhere: no hook, the all-current notice.
    let calm = pattern("mx-fresh01", "foundational", &days_ago(2), "Fresh One", "");
    for (tag, args, body) in [
        ("prh-none", &["prune"][..], calm.clone()),
        ("prh-none-json", &["--json", "prune"][..], calm),
    ] {
        let (ours, theirs) = twin_seeded(
            tag,
            Some(
                format!(
                    "version: '1'\ndomains:\n  dev: {{}}\nhooks:\n  pre-prune:\n    - 'cat > \
                     hook-stdin.txt'\n{CONFIG_TAIL}\n"
                )
                .as_str(),
            ),
            &[("dev", body.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{tag} exit");
        assert_eq!(our.stdout, promote_theirs(&their.stdout), "{tag} stdout");
        assert_eq!(our.stderr, their.stderr, "{tag} stderr");
        assert!(hook_stdin(&ours.0).is_none(), "{tag}: our hook stayed cold");
        assert!(
            hook_stdin(&theirs.0).is_none(),
            "{tag}: reference hook stayed cold"
        );
    }

    // Candidates present but --dry-run: still no hook, nothing
    // written.
    let (ours, theirs) = prune_hook_twins(
        "prh-dry",
        "hooks:\n  pre-prune:\n    - 'cat > hook-stdin.txt'\n",
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prune", "--dry-run"]);
    let their = run_in(&theirs.0, &ml, &["prune", "--dry-run"]);
    assert_eq!(our.code, their.code, "dry exit");
    assert_eq!(our.stdout, promote_theirs(&their.stdout), "dry stdout");
    assert_eq!(our.stderr, their.stderr, "dry stderr");
    assert!(hook_stdin(&ours.0).is_none(), "our hook stayed cold");
    assert!(
        hook_stdin(&theirs.0).is_none(),
        "reference hook stayed cold"
    );
    assert_eq!(
        read_store_text_or_empty(&ours.0, "expertise/dev.jsonl"),
        read_store_text_or_empty(&theirs.0, "expertise/dev.jsonl"),
        "dry run left the live bytes"
    );
}

/// Blocking (probe PR-D/PR-E): a non-zero exit or a timeout aborts
/// between preview and write — plain `Error: …` stderr / the json
/// envelope, exit 1, and NOTHING written to the store.
#[test]
fn prune_pre_prune_blocking_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
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
        let (ours, theirs) = prune_hook_twins(tag, hook_yaml);
        let started = std::time::Instant::now();
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let our_elapsed = started.elapsed();
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{tag} exit");
        assert_eq!(our.code, 1, "{tag}: blocked exits 1");
        assert!(our.stdout.is_empty(), "{tag}: no stdout when blocked");
        assert!(their.stdout.is_empty(), "{tag}: reference no stdout");
        let expected = if json_mode {
            // The forwarded hook stderr precedes the envelope.
            format!(
                "prune-pfx\n{}",
                error_envelope("prune", reason).trim_end_matches('\n')
            ) + "\n"
        } else {
            format!("prune-pfx\nError: {reason}\n")
        };
        if tag == "prh-block-timeout" {
            // No stderr prefix on the timeout variant.
            assert_eq!(
                our.stderr,
                format!("Error: {reason}\n"),
                "{tag}: our stderr"
            );
        } else {
            assert_eq!(our.stderr, expected, "{tag}: our stderr");
        }
        assert_eq!(
            strip_reference_hook_noise(&their.stderr),
            if tag == "prh-block-timeout" {
                format!("Error: {reason}\n")
            } else {
                expected.clone()
            },
            "{tag}: reference stderr (noise stripped)"
        );
        if tag.contains("timeout") {
            assert!(
                our_elapsed.as_secs() < 10,
                "{tag}: group kill returned promptly"
            );
        }
        // The write phase never ran: live intact, no archive.
        assert_eq!(
            normalize(&read_store_text_or_empty(&ours.0, "expertise/dev.jsonl")),
            promote_theirs(&normalize(&read_store_text_or_empty(
                &theirs.0,
                "expertise/dev.jsonl"
            ))),
            "{tag} live bytes (untouched)"
        );
        assert!(
            read_store_text_or_empty(&ours.0, "archive/dev.jsonl").is_empty(),
            "{tag}: no archive written"
        );
        assert!(
            read_store_text_or_empty(&theirs.0, "archive/dev.jsonl").is_empty(),
            "{tag}: reference wrote no archive"
        );
    }
}

/// A successful hook's stderr forwards verbatim in both modes, and a
/// hook's STDOUT is ignored — block-or-allow, no mutation even though
/// pre-prune is a `pre-*` event (probe PR-F: candidate-reshaping JSON
/// changes nothing; the run matches a hookless baseline byte for
/// byte).
#[test]
fn prune_hook_stderr_forward_and_stdout_ignored_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Stderr forwarding on a successful run, plain and json.
    for (tag, args) in [
        ("prh-fwd-plain", &["prune", "--check-anchors"][..]),
        ("prh-fwd-json", &["--json", "prune", "--check-anchors"][..]),
    ] {
        let (ours, theirs) =
            prune_hook_twins(tag, "hooks:\n  pre-prune:\n    - 'printf side-noise >&2'\n");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_flaked_exit(our.code, their.code, &their.stderr, 0, tag);
        assert_eq!(our.stdout, promote_theirs(&their.stdout), "{tag} stdout");
        assert_eq!(our.stderr, "side-noise\n", "{tag}: forwarded verbatim");
        assert_eq!(
            strip_reference_hook_noise(&their.stderr),
            "side-noise\n",
            "{tag}: reference forwarded the same bytes"
        );
        assert_eq!(
            normalize(&read_store_text_or_empty(&ours.0, "expertise/dev.jsonl")),
            promote_theirs(&normalize(&read_store_text_or_empty(
                &theirs.0,
                "expertise/dev.jsonl"
            ))),
            "{tag} live bytes"
        );
    }

    // Stdout ignored: the same seed bytes go through a hookless and a
    // hooked run in two stores — the resulting files must match
    // (normalize masks the demotion stamps the run itself writes).
    let (hooked, baseline) = (
        prune_hook_twins(
            "prh-stdout",
            "hooks:\n  pre-prune:\n    - 'printf %s \"{\\\"candidates\\\":[]}\"'\n",
        ),
        prune_hook_twins("prh-stdout-base", ""),
    );
    let seed_bytes = read_store_text_or_empty(&hooked.0.0, "expertise/dev.jsonl");
    std::fs::write(
        baseline
            .0
            .0
            .join(".mulch")
            .join("expertise")
            .join("dev.jsonl"),
        &seed_bytes,
    )
    .expect("baseline seed overwritten");
    let our = run_in(&hooked.0.0, Path::new(mulch_bin()), &[
        "prune",
        "--check-anchors",
    ]);
    let their = run_in(&hooked.1.0, &ml, &["prune", "--check-anchors"]);
    assert_flaked_exit(our.code, their.code, &their.stderr, 0, "prh-stdout");
    assert_eq!(
        our.stdout,
        promote_theirs(&their.stdout),
        "stdout-ignored stdout"
    );
    assert!(our.stderr.is_empty(), "stdout-ignored: our stderr clean");
    assert_eq!(
        strip_reference_hook_noise(&their.stderr),
        "\n",
        "stdout-ignored: reference stderr is pure flake noise"
    );
    let base = run_in(&baseline.0.0, Path::new(mulch_bin()), &[
        "prune",
        "--check-anchors",
    ]);
    assert_eq!(our.code, base.code, "hook stdout changed no exit");
    assert_eq!(our.stdout, base.stdout, "hook stdout changed no output");
    assert_eq!(
        normalize(&read_store_text_or_empty(
            &hooked.0.0,
            "expertise/dev.jsonl"
        )),
        normalize(&read_store_text_or_empty(
            &baseline.0.0,
            "expertise/dev.jsonl"
        )),
        "hook stdout changed no bytes (same seed, hooked vs hookless run)"
    );
}
