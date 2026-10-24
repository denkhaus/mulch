//! `mulch compact` parity probes (mulch-8a1b): analyze candidates
//! (plain/json/quiet, domain filter), the auto path (dry-run preview,
//! `--max-records` budget + skip note, confirmation prompt, cancel,
//! `--yes` apply incl. live and archive bytes), the apply path
//! (replacement flags per type, resolve/validation errors), and the
//! registry compact strategies (concat, keep_latest, merge_outcomes,
//! manual abort — the reference crashes on manual, so that case pins
//! OUR deviation bytes).

use std::path::Path;

use super::support::*;

/// Clock-derived stamp `days` before now (millisecond ISO).
fn days_ago(days: i64) -> String {
    (jiff::Timestamp::now() - jiff::Span::new().hours(days * 24)).to_string()
}

/// Three stale patterns (hex ids — the supersedes pattern law), a
/// fresh convention, and a four-member stale guide group.
fn seeded_body() -> String {
    let stale = days_ago(40);
    let fresh = days_ago(2);
    let mut lines = Vec::new();
    for i in 1..=3 {
        lines.push(format!(
            "{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"Pat {i}\",\"description\":\"desc {i}\",\"id\":\"mx-aaa00{i}\"}}"
        ));
    }
    for i in 1..=4 {
        lines.push(format!(
            "{{\"type\":\"guide\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"G {i}\",\"description\":\"gd {i}\",\"id\":\"mx-bbb00{i}\"}}"
        ));
    }
    lines.push(format!(
        "{{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"{fresh}\",\"content\":\"conv one\",\"id\":\"mx-ccc001\"}}"
    ));
    lines.join("\n") + "\n"
}

/// Asserts one twin-run pair: exit, both channels, live bytes, and
/// archive bytes (timestamps masked, `ml` self-references promoted).
fn assert_compact_parity(ours: &TempDir, our: &Run, theirs: &TempDir, their: &Run, label: &str) {
    assert_eq!(our.code, their.code, "{label} exit");
    assert_eq!(
        normalize(&our.stdout),
        promote_theirs(&normalize(&their.stdout)),
        "{label} stdout"
    );
    assert_eq!(
        normalize(&our.stderr),
        promote_theirs(&normalize(&their.stderr)),
        "{label} stderr"
    );
    assert_eq!(
        normalize(&read_store_text_or_empty(&ours.0, "expertise/dev.jsonl")),
        promote_theirs(&normalize(&read_store_text_or_empty(
            &theirs.0,
            "expertise/dev.jsonl"
        ))),
        "{label} live bytes"
    );
    assert_eq!(
        normalize(&read_store_text_or_empty(&ours.0, "archive/dev.jsonl")),
        promote_theirs(&normalize(&read_store_text_or_empty(
            &theirs.0,
            "archive/dev.jsonl"
        ))),
        "{label} archive bytes"
    );
}

/// One captured run with PIPED stdin (the interactive confirmation
/// cases: a closed stdin makes the reference's readline hang, so the
/// battery always feeds an explicit answer).
fn run_in_feeding(dir: &Path, program: &Path, args: &[&str], answer: &str) -> Run {
    use std::io::Write as _;
    let mut child = std::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    let mut stdin = child.stdin.take().expect("stdin pipe");
    stdin.write_all(answer.as_bytes()).expect("feed stdin");
    stdin.flush().expect("flush stdin");
    drop(stdin);
    let output = child.wait_with_output().expect("collect");
    Run {
        code:   output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Runs one command on fresh twins (compact mutates).
fn compacted_pair(tag: &str, body: &str, args: &[&str]) {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded(tag, None, &[("dev", body)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, &ml, args);
    assert_compact_parity(&ours, &our, &theirs, &their, tag);
}

#[test]
fn compact_analyze_matches_reference() {
    let body = seeded_body();
    for args in [
        &["compact", "--analyze"][..],
        &["--json", "compact", "--analyze"][..],
        &["--quiet", "compact", "--analyze"][..],
        &["compact", "dev", "--analyze"][..],
        &["--json", "compact", "nosuch", "--analyze"][..],
        &["compact", "nosuch", "--analyze"][..],
        // a single record never groups
        &["compact", "--analyze"][..],
    ] {
        compacted_pair("cp-analyze", &body, args);
    }
    // no candidates at all: fresh singles only
    let fresh = format!(
        "{{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"{}\",\"content\":\"only\",\"id\":\"mx-ccc001\"}}\n",
        days_ago(2)
    );
    for args in [
        &["compact", "--analyze"][..],
        &["--quiet", "compact", "--analyze"][..],
        &["--json", "compact", "--analyze"][..],
    ] {
        compacted_pair("cp-empty", &fresh, args);
    }
}

#[test]
fn compact_auto_dry_run_budget_and_flags_match_reference() {
    let body = seeded_body();
    for args in [
        &["compact", "--auto", "--dry-run"][..],
        &["--json", "compact", "--auto", "--dry-run"][..],
        // the guide group (4) fits, the pattern group (3) overflows 5
        &["compact", "--auto", "--dry-run", "--max-records", "5"][..],
        &[
            "--json",
            "compact",
            "--auto",
            "--dry-run",
            "--max-records",
            "5",
        ][..],
        // analyze keeps 3-member groups, auto needs --min-group 3
        // explicitly (default 5)
        &["compact", "--auto", "--dry-run", "--min-group", "3"][..],
        // strict numeric flags
        &["compact", "--auto", "--min-group", "abc"][..],
        &["--json", "compact", "--auto", "--min-group", "0"][..],
        &["compact", "--auto", "--max-records", "5x"][..],
    ] {
        compacted_pair("cp-auto", &body, args);
    }
}

#[test]
fn compact_auto_cancel_prompt_and_yes_match_reference() {
    let body = seeded_body();
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };

    // cancel: everything untouched on both sides (stdin piped: a
    // closed stdin makes the reference's readline hang)
    let (ours, theirs) = twin_seeded("cp-cancel", None, &[("dev", body.as_str())], &[]);
    let our = run_in_feeding(
        &ours.0,
        Path::new(mulch_bin()),
        &["compact", "--auto"],
        "n\n",
    );
    let their = run_in_feeding(&theirs.0, &ml, &["compact", "--auto"], "n\n");
    assert_compact_parity(&ours, &our, &theirs, &their, "cancel");
    // y on the prompt equals --yes
    let (ours, theirs) = twin_seeded("cp-confirm", None, &[("dev", body.as_str())], &[]);
    let our = run_in_feeding(
        &ours.0,
        Path::new(mulch_bin()),
        &["compact", "--auto"],
        "y\n",
    );
    let their = run_in_feeding(&theirs.0, &ml, &["compact", "--auto"], "y\n");
    assert_compact_parity(&ours, &our, &theirs, &their, "confirm");

    // --yes: live rewrite, compacted archive, plain + json + quiet
    for args in [
        &["compact", "--auto", "--yes"][..],
        &["--json", "compact", "--auto", "--yes"][..],
        &["--quiet", "compact", "--auto", "--yes"][..],
    ] {
        compacted_pair("cp-yes", &body, args);
    }
    // the drained store: the no-candidates json envelope and quiet line
    let drained = format!(
        "{{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"{}\",\"content\":\"conv one\",\"id\":\"mx-ccc001\"}}\n",
        days_ago(2)
    );
    for args in [
        &["--json", "compact", "--auto", "--yes"][..],
        &["--quiet", "compact", "--auto", "--yes"][..],
        &["compact", "--auto", "--yes"][..],
    ] {
        compacted_pair("cp-drained", &drained, args);
    }
}

#[test]
fn compact_apply_matches_reference() {
    let body = seeded_body();
    for args in [
        // happy: pattern replacement from flags
        &[
            "--json",
            "compact",
            "dev",
            "--apply",
            "--records",
            "mx-aaa001,mx-aaa002",
            "--type",
            "pattern",
            "--name",
            "P one",
            "--description",
            "merged",
        ][..],
        &[
            "compact",
            "dev",
            "--apply",
            "--records",
            "mx-bbb001,mx-bbb002",
            "--type",
            "guide",
            "--name",
            "G one",
            "--description",
            "gm",
        ][..],
        // type from the FIRST record (guide) without --type
        &[
            "compact",
            "dev",
            "--apply",
            "--records",
            "mx-bbb001,mx-bbb002",
            "--name",
            "G two",
            "--description",
            "gm2",
        ][..],
        // per-type flag errors
        &[
            "compact",
            "dev",
            "--apply",
            "--records",
            "mx-aaa001,mx-aaa002",
            "--type",
            "pattern",
            "--name",
            "only",
        ][..],
        &[
            "compact",
            "dev",
            "--apply",
            "--records",
            "mx-aaa001,mx-aaa002",
            "--type",
            "failure",
            "--description",
            "d",
        ][..],
        &[
            "compact",
            "dev",
            "--apply",
            "--records",
            "mx-aaa001,mx-aaa002",
            "--type",
            "decision",
            "--title",
            "t",
        ][..],
        &[
            "compact",
            "dev",
            "--apply",
            "--records",
            "mx-aaa001,mx-aaa002",
            "--type",
            "convention",
        ][..],
        // structural errors
        &["compact", "dev", "--apply", "--records", "mx-aaa001"][..],
        &[
            "compact",
            "dev",
            "--apply",
            "--records",
            "mx-zzz999,mx-aaa001",
        ][..],
        &["compact", "--apply"][..],
        &["compact", "dev", "--apply"][..],
        &["compact"][..],
    ] {
        compacted_pair("cp-apply", &body, args);
    }
}

#[test]
fn compact_apply_validation_failure_bytes_match_reference() {
    // NON-HEX source ids: the replacement's supersedes links violate
    // the id pattern, so --apply fails the unified-schema validation
    // with the reference's AJV cascade.
    let stale = days_ago(40);
    let body = format!(
        "{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"P one\",\"description\":\"d1\",\"id\":\"mx-g00111\"}}\n{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"P two\",\"description\":\"d2\",\"id\":\"mx-g00211\"}}\n"
    );
    compacted_pair("cp-validate", &body, &[
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-g00111,mx-g00211",
        "--type",
        "pattern",
        "--name",
        "P one",
        "--description",
        "m",
    ]);
    compacted_pair("cp-validate-json", &body, &[
        "--json",
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-g00111,mx-g00211",
        "--type",
        "pattern",
        "--name",
        "P one",
        "--description",
        "m",
    ]);
}

#[test]
fn compact_strategies_and_idless_groups_match_reference() {
    let config = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  note:\n    required: [text]\n    dedup_key: text\n    summary: \"{text}\"\n    compact: keep_latest\n  journal:\n    required: [entry]\n    dedup_key: entry\n    summary: \"{entry}\"\n    compact: merge_outcomes\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";
    let stale = days_ago(40);
    let fresh = days_ago(20);
    // keep_latest: tags union, newest body; merge_outcomes: grouped by
    // type + id key (single members never group); id-less patterns
    // compact through the undefined === undefined match.
    let body = format!(
        "{{\"type\":\"note\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"text\":\"old note\",\"tags\":[\"a\",\"b\"],\"outcomes\":[{{\"what\":\"x\"}}]}}\n\
{{\"type\":\"note\",\"classification\":\"tactical\",\"recorded_at\":\"{fresh}\",\"text\":\"new note\",\"tags\":[\"b\",\"c\"]}}\n\
{{\"type\":\"journal\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"entry\":\"j1\",\"outcomes\":[{{\"what\":\"o1\"}}]}}\n\
{{\"type\":\"journal\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"entry\":\"j1\",\"outcomes\":[{{\"what\":\"o2\"}}]}}\n\
{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"NP one\",\"description\":\"d1\"}}\n\
{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"NP two\",\"description\":\"d2\"}}\n"
    );
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for args in [
        &["compact", "--analyze"][..],
        &["--json", "compact", "--analyze"][..],
        &["compact", "--auto", "--yes"][..],
        &["--json", "compact", "--auto", "--yes"][..],
    ] {
        let (ours, theirs) =
            twin_seeded("cp-strategy", Some(config), &[("dev", body.as_str())], &[]);
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_compact_parity(&ours, &our, &theirs, &their, "strategy");
    }
}

#[test]
fn compact_manual_strategy_is_a_clean_abort() {
    // The reference CRASHES on a manual-strategy group in --auto (an
    // uncaught throw from mergeRecords dumps a Bun stack trace); this
    // port renders the thrown message as a clean Error line and
    // leaves the store untouched (README DEVIATIONS).
    let config = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  hypo:\n    required: [statement, prediction]\n    dedup_key: statement\n    summary: \"{statement}\"\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";
    let stale = days_ago(40);
    let body = format!(
        "{{\"type\":\"hypo\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"statement\":\"h1\",\"prediction\":\"p1\"}}\n\
{{\"type\":\"hypo\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"statement\":\"h2\",\"prediction\":\"p2\"}}\n"
    );
    let (ours, _theirs) = twin_seeded("cp-manual", Some(config), &[("dev", body.as_str())], &[]);
    let before = read_store_file(&ours.0, "expertise/dev.jsonl");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(run.code, 1);
    // The summary block prints before the apply phase reaches the
    // merge (the reference prints it too, then crashes).
    assert!(run.stdout.starts_with("\nCompaction summary:\n"));
    assert_eq!(
        run.stderr,
        "Error: Type \"hypo\" has compact strategy \"manual\" — use `mulch compact --apply` to compact manually.\n"
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), before);
}
