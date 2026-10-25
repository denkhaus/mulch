//! `mulch compact` parity probes (mulch-8a1b): analyze candidates
//! (plain/json/quiet, domain filter), the auto path (dry-run preview,
//! `--max-records` budget + skip note, confirmation prompt, cancel,
//! `--yes` apply incl. live and archive bytes), the apply path
//! (replacement flags per type, resolve/validation errors), and the
//! registry compact strategies (concat, keep_latest, merge_outcomes,
//! manual abort — the reference crashes on manual, so that case pins
//! OUR deviation bytes) — plus the pre-compact hook battery
//! (mulch-7f49): block-skip warnings, replacement adoption
//! (supersedes backfill + id regeneration), invalid replacements,
//! the apply-path block/abort, stderr forwarding, and the
//! `hook_settings.timeout_ms` kill.

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

// ---- pre-compact hooks (mulch-7f49) ----

/// A config whose `hooks:`/`hook_settings:` blocks carry `hook_config`
/// (single-quoted YAML entries, plain sh, stdin always drained first
/// so the reference's async EPIPE-on-stdin-write bug cannot fire
/// mid-run and pollute the compared bytes).
fn hooks_config(hook_config: &str) -> String {
    format!("version: '1'\ndomains:\n  dev: {{}}\n{hook_config}{CONFIG_TAIL}")
}

/// The `{"payload":{"replacement":…}}` stdout body for an adoption
/// hook, backslash-escaped for the inner double-quoted printf string
/// (`replacement_json` is plain JSON with only double quotes).
fn hook_replacement_stdout(replacement_json: &str) -> String {
    format!(
        "{{\\\"payload\\\":{{\\\"replacement\\\":{escaped}}}}}",
        escaped = replacement_json.replace('"', "\\\"")
    )
}

/// A pre-compact hook command that drains stdin, then runs `command`.
fn draining_hook(command: &str) -> String {
    format!("cat > /dev/null; {command}")
}

/// Drops the reference's pino warn lines (volatile time/pid/hostname)
/// from a stderr stream — the landed engine never prints diagnostics
/// (README DEVIATIONS: hook-run logging), so both sides fold to the
/// product bytes.
fn strip_hook_logs(text: &str) -> String {
    let filtered = text
        .lines()
        .filter(|line| !line.starts_with("{\"level\":"))
        .collect::<Vec<_>>()
        .join("\n");
    if text.ends_with('\n') && !filtered.is_empty() {
        filtered + "\n"
    } else {
        filtered
    }
}

/// [`assert_compact_parity`] with the reference's pino warn lines
/// folded out of both stderr streams (hook-configured stores).
fn assert_compact_hook_parity(
    ours: &TempDir,
    our: &Run,
    theirs: &TempDir,
    their: &Run,
    label: &str,
) {
    assert_eq!(our.code, their.code, "{label} exit");
    assert_eq!(
        normalize(&our.stdout),
        promote_theirs(&normalize(&their.stdout)),
        "{label} stdout"
    );
    assert_eq!(
        normalize(&strip_hook_logs(&our.stderr)),
        promote_theirs(&normalize(&strip_hook_logs(&their.stderr))),
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

/// Twin hook-configured stores, one run each (compact mutates).
fn hooked_pair(tag: &str, body: &str, config: &str, args: &[&str]) {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded(tag, Some(config), &[("dev", body)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, &ml, args);
    assert_compact_hook_parity(&ours, &our, &theirs, &their, tag);
}

/// One valid pattern replacement body (fixed stamp; the id is
/// regenerated deterministically from `pattern:Hooked one`).
const HOOKED_PATTERN: &str = "{\"type\":\"pattern\",\"name\":\"Hooked one\",\
\"description\":\"hook summary\",\"classification\":\"foundational\",\
\"recorded_at\":\"2026-10-01T00:00:00.000Z\"}";

#[test]
fn compact_auto_hook_block_skips_group_with_warning() {
    // The hook blocks ONLY the guide group (grep drains stdin); the
    // unrelated pattern group still compacts. Plain: per-group
    // `Warning:` lines on stderr; json: the envelope's `warnings`
    // array; quiet: the warning lines suppress.
    let body = seeded_body();
    let config = hooks_config(
        "hooks:\n  pre-compact:\n    - 'grep -q guide && exit 3 || \
              exit 0'\n",
    );
    hooked_pair("cph-block", &body, &config, &["compact", "--auto", "--yes"]);
    hooked_pair("cph-block-json", &body, &config, &[
        "--json", "compact", "--auto", "--yes",
    ]);
    hooked_pair("cph-block-quiet", &body, &config, &[
        "--quiet", "compact", "--auto", "--yes",
    ]);
}

#[test]
fn compact_auto_hook_replacement_is_adopted() {
    // A truthy `payload.replacement` overrides the mechanical merge:
    // copied verbatim, `supersedes` backfilled from the group's ids,
    // the id regenerated from the body. The guide group (the hook
    // stays silent for it) merges mechanically. The second config
    // pins the explicit-supersedes-kept + incoming-id-dropped law.
    let body = seeded_body();
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - 'grep -q pattern && printf %s \"{}\" || true'\n",
        hook_replacement_stdout(HOOKED_PATTERN)
    ));
    hooked_pair("cph-adopt", &body, &config, &["compact", "--auto", "--yes"]);
    hooked_pair("cph-adopt-json", &body, &config, &[
        "--json", "compact", "--auto", "--yes",
    ]);

    let explicit = "{\"type\":\"pattern\",\"name\":\"Keep sup\",\
\"description\":\"d\",\"classification\":\"foundational\",\
\"recorded_at\":\"2026-10-01T00:00:00.000Z\",\
\"supersedes\":[\"mx-9ea5ff\",\"mx-9ea5fe\"],\"id\":\"bogus\"}";
    let kept = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - 'cat > /dev/null; printf %s \"{}\"'\n",
        hook_replacement_stdout(explicit)
    ));
    hooked_pair("cph-adopt-explicit", &body, &kept, &[
        "compact", "--auto", "--yes",
    ]);
}

#[test]
fn compact_auto_hook_invalid_replacement_skips_group() {
    // An invalid replacement skips the group with the AJV cascade
    // warning (leading space per `${instancePath} ${message}`, joined
    // `; `); the second config spreads `{}` — supersedes backfills,
    // then every branch misses `type`.
    let body = seeded_body();
    let missing = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - 'cat > /dev/null; printf %s \"{}\"'\n",
        hook_replacement_stdout(
            "{\"type\":\"pattern\",\"name\":\"NoDesc\",\"classification\":\"foundational\",\
\"recorded_at\":\"2026-10-01T00:00:00.000Z\"}"
        )
    ));
    hooked_pair("cph-invalid", &body, &missing, &[
        "compact", "--auto", "--yes",
    ]);
    hooked_pair("cph-invalid-json", &body, &missing, &[
        "--json", "compact", "--auto", "--yes",
    ]);

    let empty = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - 'cat > /dev/null; printf %s \"{}\"'\n",
        hook_replacement_stdout("{}")
    ));
    hooked_pair("cph-empty-repl", &body, &empty, &[
        "compact", "--auto", "--yes",
    ]);
}

#[test]
fn compact_apply_hook_block_aborts_before_write() {
    // A blocked hook aborts --apply with exit 1 before any write:
    // plain `Error: {reason}` / the json error envelope, store bytes
    // identical to the seed on both sides.
    let body = seeded_body();
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook("exit 3")
    ));
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for args in [
        &[
            "compact",
            "dev",
            "--apply",
            "--records",
            "mx-aaa001,mx-aaa002",
            "--type",
            "pattern",
            "--name",
            "N",
            "--description",
            "D",
        ][..],
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
            "N",
            "--description",
            "D",
        ][..],
    ] {
        let (ours, theirs) = twin_seeded(
            "cph-apply-block",
            Some(config.as_str()),
            &[("dev", body.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_compact_hook_parity(&ours, &our, &theirs, &their, "apply-block");
        assert_eq!(our.code, 1);
        assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);
        assert_eq!(read_store_file(&theirs.0, "expertise/dev.jsonl"), body);
    }
}

#[test]
fn compact_apply_hook_replacement_overrides_flags() {
    // The hook's guide-typed replacement wins over the pattern flags;
    // the success line still names the RESOLVED type ("pattern"). The
    // invalid variant aborts with exit 1 before any write.
    let body = seeded_body();
    let apply = &[
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-aaa001,mx-aaa002",
        "--type",
        "pattern",
        "--name",
        "N",
        "--description",
        "D",
    ][..];
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook(&format!(
            "printf %s \"{}\"",
            hook_replacement_stdout(
                "{\"type\":\"guide\",\"name\":\"Applied hook\",\"description\":\"via hook\",\
\"classification\":\"foundational\",\"recorded_at\":\"2026-10-01T00:00:00.000Z\"}"
            )
        ))
    ));
    hooked_pair("cph-apply-adopt", &body, &config, apply);
    hooked_pair("cph-apply-adopt-json", &body, &config, &[
        "--json",
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-aaa001,mx-aaa002",
        "--type",
        "pattern",
        "--name",
        "N",
        "--description",
        "D",
    ]);

    let invalid = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook(&format!(
            "printf %s \"{}\"",
            hook_replacement_stdout("{\"type\":\"pattern\",\"name\":\"X\"}")
        ))
    ));
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded(
        "cph-apply-invalid",
        Some(invalid.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), apply);
    let their = run_in(&theirs.0, &ml, apply);
    assert_compact_hook_parity(&ours, &our, &theirs, &their, "apply-invalid");
    assert_eq!(our.code, 1);
    assert!(
        strip_hook_logs(&our.stderr).starts_with(
            "Error: pre-compact hook produced an invalid record:  must have required property \
             'content'"
        ),
        "stderr was: {}",
        our.stderr
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);
}

#[test]
fn compact_hook_stderr_is_forwarded() {
    // Captured hook stderr forwards right after each run, before any
    // product bytes — success and block paths, plain and json alike.
    let body = seeded_body();
    let apply = &[
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-aaa001,mx-aaa002",
        "--type",
        "pattern",
        "--name",
        "N",
        "--description",
        "D",
    ][..];
    let noisy = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook(&format!(
            "printf hook-boom >&2; printf %s \"{}\"",
            hook_replacement_stdout(HOOKED_PATTERN)
        ))
    ));
    hooked_pair("cph-stderr", &body, &noisy, apply);
    hooked_pair("cph-stderr-json", &body, &noisy, &[
        "--json",
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-aaa001,mx-aaa002",
        "--type",
        "pattern",
        "--name",
        "N",
        "--description",
        "D",
    ]);

    // Block path: the forwarded bytes precede the Warning lines.
    let blocking = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook("printf hook-boom >&2; exit 3")
    ));
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded(
        "cph-stderr-block",
        Some(blocking.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    let their = run_in(&theirs.0, &ml, &["compact", "--auto", "--yes"]);
    assert_compact_hook_parity(&ours, &our, &theirs, &their, "stderr-block");
    assert!(
        strip_hook_logs(&our.stderr).starts_with("hook-boom\nhook-boom\nWarning: "),
        "stderr was: {}",
        our.stderr
    );
}

#[test]
fn compact_hook_timeout_blocks_the_group() {
    // A 150 ms `hook_settings.timeout_ms` SIGKILLs the whole process
    // group (exit 124): --auto skips each group with the timed-out
    // warning, --apply aborts with the Error line.
    let body = seeded_body();
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\nhook_settings:\n  timeout_ms: 150\n",
        draining_hook("sleep 30 & wait")
    ));
    hooked_pair("cph-timeout", &body, &config, &[
        "compact", "--auto", "--yes",
    ]);
    hooked_pair("cph-timeout-json", &body, &config, &[
        "--json", "compact", "--auto", "--yes",
    ]);
    hooked_pair("cph-timeout-apply", &body, &config, &[
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-aaa001,mx-aaa002",
        "--type",
        "pattern",
        "--name",
        "N",
        "--description",
        "D",
    ]);
}

#[test]
fn compact_auto_dry_run_fires_no_hooks() {
    // The reference returns from --dry-run before the apply loop, so
    // pre-compact never fires (marker file absent on both sides even
    // though the hook would block).
    let body = seeded_body();
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook("printf ran > hook-ran.txt; exit 3")
    ));
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded(
        "cph-dryrun",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact",
        "--auto",
        "--dry-run",
    ]);
    let their = run_in(&theirs.0, &ml, &["compact", "--auto", "--dry-run"]);
    assert_compact_hook_parity(&ours, &our, &theirs, &their, "dry-run");
    assert!(!ours.0.join("hook-ran.txt").exists());
    assert!(!theirs.0.join("hook-ran.txt").exists());
}

#[test]
fn compact_hook_engine_warnings_surface() {
    // The engine's own warnings (non-JSON stdout, mutation ignored):
    // --apply plain prints `Warning: …` on stderr (not quiet-gated)
    // and the compaction proceeds via the user flags; --json and
    // --auto drop them entirely (resolveReplacement ignores them).
    // The reason's wording past the prefix is serde-vs-V8 and NOT
    // pinned (README DEVIATIONS), so the plain case pins OUR bytes.
    let body = seeded_body();
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook("printf %s not-json")
    ));
    let (ours, _theirs) = twin_seeded(
        "cph-engine-warn",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-aaa001,mx-aaa002",
        "--type",
        "pattern",
        "--name",
        "N",
        "--description",
        "D",
    ]);
    assert_eq!(run.code, 0);
    assert_eq!(run.stdout, "✓ Compacted 2 pattern records into 1 in dev\n");
    assert!(
        strip_hook_logs(&run.stderr).starts_with(
            "Warning: hook `cat > /dev/null; printf %s not-json` printed non-JSON on stdout \
             (mutation ignored): "
        ),
        "engine warning was: {}",
        run.stderr
    );

    // --auto and --json stay byte-identical to the reference (both
    // drop the engine warnings; the mechanical merge runs).
    hooked_pair("cph-engine-warn-auto", &body, &config, &[
        "compact", "--auto", "--yes",
    ]);
    hooked_pair("cph-engine-warn-json", &body, &config, &[
        "--json",
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-aaa001,mx-aaa002",
        "--type",
        "pattern",
        "--name",
        "N",
        "--description",
        "D",
    ]);
}
