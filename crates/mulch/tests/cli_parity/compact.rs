//! `mulch compact` probes (mulch-8a1b) — OUR byte pins (ADR-0008:
//! the differential battery is dissolved; our bytes are the
//! regression memory): analyze candidates, the auto path (dry-run
//! preview, `--max-records` budget + skip note, confirmation prompt,
//! cancel, `--yes` apply incl. live and archive bytes), the apply
//! path, and the registry compact strategies — plus the pre-compact
//! hook battery (mulch-7f49): block-skip warnings, replacement
//! adoption (supersedes backfill + id regeneration), invalid
//! replacements, the apply-path block/abort, stderr forwarding, and
//! the `hook_settings.timeout_ms` kill.

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

/// One captured run with PIPED stdin (the interactive confirmation
/// cases: the battery always feeds an explicit answer).
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

#[test]
fn compact_analyze_pins_our_candidate_table() {
    let body = seeded_body();
    let ours = seeded_store("cp-analyze", None, &[("dev", body.as_str())], &[]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["compact", "--analyze"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout.starts_with("\nCompaction candidates:\n"),
        "table header: {}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("Found 2 groups (7 records that could be compacted)"),
        "counts: {}",
        run.stdout
    );
    assert!(run.stdout.contains("pattern (3 records)"));
    assert!(run.stdout.contains("guide (4 records)"));
    assert!(
        run.stdout.contains("mx-aaa001: Pat 1"),
        "member previews: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains("... and 1 more"),
        "long groups elide: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains("mulch compact --auto [--dry-run]"),
        "hint names our binary: {}",
        run.stdout
    );

    // no candidates at all: fresh singles only (still exit 0)
    let fresh = format!(
        "{{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"{}\",\"content\":\"only\",\"id\":\"mx-ccc001\"}}\n",
        days_ago(2)
    );
    let ours = seeded_store("cp-empty", None, &[("dev", fresh.as_str())], &[]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["compact", "--analyze"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
}

#[test]
fn compact_auto_dry_run_pins_the_budget_note() {
    let body = seeded_body();
    let ours = seeded_store("cp-auto", None, &[("dev", body.as_str())], &[]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact",
        "--auto",
        "--dry-run",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout.contains("2 groups would be compacted"),
        "preview counts: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains("7 records → 2 records"),
        "reduction: {}",
        run.stdout
    );
    // the store is untouched by a dry run
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);

    // the guide group (4) fits, the pattern group (3) overflows 5
    let budget = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact",
        "--auto",
        "--dry-run",
        "--max-records",
        "5",
    ]);
    assert_eq!(budget.code, 0, "{}", budget.stderr);
    assert!(
        budget
            .stdout
            .contains("Note: 1 additional groups skipped due to --max-records limit"),
        "skip note: {}",
        budget.stdout
    );
    assert!(
        budget
            .stdout
            .contains("✓ Dry-run complete. Would compact 3 records across 1 groups."),
        "summary respects the budget: {}",
        budget.stdout
    );
}

#[test]
fn compact_auto_cancel_prompt_and_yes_pin_our_bytes() {
    let body = seeded_body();

    // cancel: everything untouched
    let ours = seeded_store("cp-cancel", None, &[("dev", body.as_str())], &[]);
    let our = run_in_feeding(
        &ours.0,
        Path::new(mulch_bin()),
        &["compact", "--auto"],
        "n\n",
    );
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout.ends_with("Compaction cancelled.\n"),
        "cancel line: {}",
        our.stdout
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);

    // y on the prompt equals --yes
    let ours = seeded_store("cp-confirm", None, &[("dev", body.as_str())], &[]);
    let our = run_in_feeding(
        &ours.0,
        Path::new(mulch_bin()),
        &["compact", "--auto"],
        "y\n",
    );
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        read_store_file(&ours.0, "expertise/dev.jsonl").contains("\"supersedes\":["),
        "confirm applies the merge"
    );

    // --yes: live rewrite, compacted archive
    let ours = seeded_store("cp-yes", None, &[("dev", body.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout
            .contains("✓ Auto-compacted 7 records across 2 groups"),
        "summary: {}",
        our.stdout
    );
    assert!(
        our.stdout.contains("dev/pattern: 3 records → 1"),
        "per-group line: {}",
        our.stdout
    );
    let live = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert_eq!(
        live.lines().count(),
        3,
        "convention + two merged records: {live}"
    );
    assert!(
        live.contains("\"supersedes\":[\"mx-aaa001\",\"mx-aaa002\",\"mx-aaa003\"]"),
        "supersedes backfill: {live}"
    );
    let archive = read_store_file(&ours.0, "archive/dev.jsonl");
    assert!(
        archive.contains("\"archive_reason\":\"compacted\""),
        "archived with the compacted reason: {archive}"
    );

    // the drained store: no candidates, exit 0
    let drained = format!(
        "{{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"{}\",\"content\":\"conv one\",\"id\":\"mx-ccc001\"}}\n",
        days_ago(2)
    );
    let ours = seeded_store("cp-drained", None, &[("dev", drained.as_str())], &[]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), drained);
}

#[test]
fn compact_apply_pins_our_success_and_error_bytes() {
    let body = seeded_body();
    // happy: pattern replacement from flags
    let ours = seeded_store("cp-apply", None, &[("dev", body.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
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
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(our.stdout, "✓ Compacted 2 pattern records into 1 in dev\n");
    let live = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(
        live.contains("\"name\":\"P one\",\"description\":\"merged\""),
        "flag-built replacement: {live}"
    );

    // a single record never groups
    let ours = seeded_store("cp-apply-single", None, &[("dev", body.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact",
        "dev",
        "--apply",
        "--records",
        "mx-aaa001",
        "--type",
        "pattern",
        "--name",
        "N",
        "--description",
        "D",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Error: Compaction requires at least 2 records.\n"
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);
}

#[test]
fn compact_apply_validation_failure_pins_the_prefix() {
    // NON-HEX source ids: the replacement's supersedes links violate
    // the id pattern, so --apply fails the unified-schema validation
    // with the oneOf cascade.
    let stale = days_ago(40);
    let body = format!(
        "{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"P one\",\"description\":\"d1\",\"id\":\"mx-g00111\"}}\n{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"P two\",\"description\":\"d2\",\"id\":\"mx-g00211\"}}\n"
    );
    let ours = seeded_store("cp-validate", None, &[("dev", body.as_str())], &[]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
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
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr
            .starts_with("Error: Replacement record failed validation: "),
        "our cascade: {}",
        run.stderr
    );
    assert!(
        run.stderr.contains("/supersedes/0 must match pattern"),
        "the id-pattern violation is named: {}",
        run.stderr
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);
}

#[test]
fn compact_strategies_pin_the_analyze_table() {
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
    let ours = seeded_store("cp-strategy", Some(config), &[("dev", body.as_str())], &[]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["compact", "--analyze"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("Found 3 groups (6 records that could be compacted)"),
        "custom groups surface: {}",
        run.stdout
    );
    assert!(run.stdout.contains("note (2 records)"));
    assert!(run.stdout.contains("journal (2 records)"));
    assert!(
        run.stdout.contains("(no id): old note"),
        "id-less customs render their summary: {}",
        run.stdout
    );
}

#[test]
fn compact_auto_unknown_type_group_is_a_clean_error() {
    // mulch-2809: with --allow-unknown-types the read KEEPS records of
    // unregistered types, they form a candidate group, and the
    // collector renders the unknown-type throw as a clean Error line
    // at the same point, so nothing prints and nothing writes (README
    // DEVIATIONS). The alpha domain carries a CLEAN compactable
    // group: the error must fire before any write, so alpha stays two
    // records and no archive appears (no partially compacted store).
    let config = "version: '1'\n\
domains:\n  dev: {}\n  alpha: {}\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";
    let stale = days_ago(40);
    let body = format!(
        "{{\"type\":\"weird\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"content\":\"w1\"}}\n\
{{\"type\":\"weird\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"content\":\"w2\"}}\n"
    );
    let alpha = format!(
        "{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"a1\",\"description\":\"d1\"}}\n\
{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"a2\",\"description\":\"d2\"}}\n"
    );
    let ours = seeded_store(
        "cp-unknown",
        Some(config),
        &[("dev", body.as_str()), ("alpha", alpha.as_str())],
        &[],
    );
    let dev_before = read_store_file(&ours.0, "expertise/dev.jsonl");
    let alpha_before = read_store_file(&ours.0, "expertise/alpha.jsonl");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact",
        "--auto",
        "--yes",
        "--allow-unknown-types",
    ]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "");
    assert_eq!(run.stderr, "Error: Unknown record type: weird\n");
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), dev_before);
    assert_eq!(
        read_store_file(&ours.0, "expertise/alpha.jsonl"),
        alpha_before,
        "no partially compacted store"
    );
    assert!(
        !ours.0.join(".mulch/archive/alpha.jsonl").exists(),
        "the clean domain was never archived"
    );
}

#[test]
fn compact_analyze_unknown_type_group_is_a_clean_error() {
    // The collector's unknown-type crash hits --analyze (and
    // --auto --dry-run) exactly like --auto — same empty stdout, same
    // clean Error line. Pin the analyze mode so a later fix cannot
    // re-open the divergence for the read-only modes.
    let config = "version: '1'\n\
domains:\n  dev: {}\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";
    let stale = days_ago(40);
    let body = format!(
        "{{\"type\":\"weird\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"content\":\"w1\"}}\n\
{{\"type\":\"weird\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"content\":\"w2\"}}\n"
    );
    let ours = seeded_store("cp-unknown-an", Some(config), &[("dev", body.as_str())], &[
    ]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact",
        "--analyze",
        "--allow-unknown-types",
    ]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "");
    assert_eq!(run.stderr, "Error: Unknown record type: weird\n");
}

#[test]
fn compact_manual_strategy_is_a_clean_abort() {
    // A manual-strategy group in --auto renders the thrown message as
    // a clean Error line and leaves the store untouched (README
    // DEVIATIONS).
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
    let ours = seeded_store("cp-manual", Some(config), &[("dev", body.as_str())], &[]);
    let before = read_store_file(&ours.0, "expertise/dev.jsonl");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(run.code, 1);
    // The summary block prints before the apply phase reaches the
    // merge.
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
/// so the hook's stdin write cannot stall mid-run).
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

/// One valid pattern replacement body (fixed stamp; the id is
/// regenerated deterministically from `pattern:Hooked one`).
const HOOKED_PATTERN: &str = "{\"type\":\"pattern\",\"name\":\"Hooked one\",\
\"description\":\"hook summary\",\"classification\":\"foundational\",\
\"recorded_at\":\"2026-10-01T00:00:00.000Z\"}";

#[test]
fn compact_auto_hook_block_skips_group_with_warning() {
    // The hook blocks ONLY the guide group (grep drains stdin); the
    // unrelated pattern group still compacts. Plain: per-group
    // `Warning:` lines on stderr naming the hook and its exit code.
    let body = seeded_body();
    let config = hooks_config(
        "hooks:\n  pre-compact:\n    - 'grep -q guide && exit 3 || \
              exit 0'\n",
    );
    let ours = seeded_store(
        "cph-block",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stderr.contains(
            "Warning: dev/guide: hook `grep -q guide && exit 3 || exit 0` exited with code 3"
        ),
        "skip warning: {}",
        run.stderr
    );
    assert!(
        run.stdout
            .contains("✓ Auto-compacted 3 records across 1 groups"),
        "the unblocked group still compacts: {}",
        run.stdout
    );

    // json mode stays exit 0 with the same block/skip outcome
    let ours = seeded_store(
        "cph-block-json",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "compact", "--auto", "--yes",
    ]);
    assert_eq!(json.code, 0, "{}", json.stderr);
}

#[test]
fn compact_auto_hook_replacement_is_adopted() {
    // A truthy `payload.replacement` overrides the mechanical merge:
    // copied verbatim, `supersedes` backfilled from the group's ids,
    // the id regenerated from the body. The guide group (the hook
    // stays silent for it) merges mechanically.
    let body = seeded_body();
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - 'grep -q pattern && printf %s \"{}\" || true'\n",
        hook_replacement_stdout(HOOKED_PATTERN)
    ));
    let ours = seeded_store(
        "cph-adopt",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let live = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(live.contains("Hooked one"), "adopted verbatim: {live}");
    assert!(
        live.contains("\"supersedes\":[\"mx-aaa001\",\"mx-aaa002\",\"mx-aaa003\"]"),
        "supersedes backfilled from the group: {live}"
    );
    assert!(
        live.contains("gd 1"),
        "the silent guide group merges mechanically: {live}"
    );

    // The explicit-supersedes-kept + incoming-id-dropped law: a
    // replacement that carries its own supersedes list keeps it, and
    // a bogus incoming id is regenerated.
    let explicit = "{\"type\":\"pattern\",\"name\":\"Keep sup\",\
\"description\":\"d\",\"classification\":\"foundational\",\
\"recorded_at\":\"2026-10-01T00:00:00.000Z\",\
\"supersedes\":[\"mx-9ea5ff\",\"mx-9ea5fe\"],\"id\":\"bogus\"}";
    let kept = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - 'cat > /dev/null; printf %s \"{}\"'\n",
        hook_replacement_stdout(explicit)
    ));
    let ours = seeded_store(
        "cph-adopt-explicit",
        Some(kept.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let live = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(live.contains("Keep sup"), "adopted: {live}");
    assert!(
        live.contains("\"supersedes\":[\"mx-9ea5ff\",\"mx-9ea5fe\"]"),
        "explicit supersedes kept: {live}"
    );
    assert!(
        !live.contains("\"id\":\"bogus\""),
        "incoming id dropped: {live}"
    );
}

#[test]
fn compact_auto_hook_invalid_replacement_skips_group() {
    // An invalid replacement skips the group with the validation
    // cascade warning (leading space per `${instancePath}
    // ${message}`, joined `; `); nothing is written for the group.
    let body = seeded_body();
    let missing = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - 'cat > /dev/null; printf %s \"{}\"'\n",
        hook_replacement_stdout(
            "{\"type\":\"pattern\",\"name\":\"NoDesc\",\"classification\":\"foundational\",\
\"recorded_at\":\"2026-10-01T00:00:00.000Z\"}"
        )
    ));
    let ours = seeded_store(
        "cph-invalid",
        Some(missing.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stderr
            .contains("Warning: dev/pattern: pre-compact hook produced an invalid record: "),
        "cascade warning: {}",
        run.stderr
    );
    assert!(
        run.stderr
            .contains("must match exactly one schema in oneOf"),
        "oneOf tail: {}",
        run.stderr
    );
    // the group is skipped, the store keeps every original record
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);
}

#[test]
fn compact_apply_hook_block_aborts_before_write() {
    // A blocked hook aborts --apply with exit 1 before any write:
    // store bytes identical to the seed.
    let body = seeded_body();
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook("exit 3")
    ));
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
        let ours = seeded_store(
            "cph-apply-block",
            Some(config.as_str()),
            &[("dev", body.as_str())],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 1);
        assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);
    }
}

#[test]
fn compact_apply_hook_replacement_overrides_flags() {
    // The hook's guide-typed replacement wins over the pattern flags.
    // The invalid variant aborts with exit 1 before any write.
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
    let ours = seeded_store(
        "cph-apply-adopt",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), apply);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let live = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(live.contains("Applied hook"), "hook body wins: {live}");
    assert!(
        !live.contains("\"name\":\"N\""),
        "flag body never lands: {live}"
    );

    let invalid = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook(&format!(
            "printf %s \"{}\"",
            hook_replacement_stdout("{\"type\":\"pattern\",\"name\":\"X\"}")
        ))
    ));
    let ours = seeded_store(
        "cph-apply-invalid",
        Some(invalid.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), apply);
    assert_eq!(run.code, 1);
    assert!(
        run.stderr.starts_with(
            "Error: pre-compact hook produced an invalid record:  must have required property \
             'content'"
        ),
        "stderr was: {}",
        run.stderr
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);
}

#[test]
fn compact_hook_stderr_is_forwarded() {
    // Captured hook stderr forwards right after each run, before any
    // product bytes — success and block paths alike.
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
    let ours = seeded_store(
        "cph-stderr",
        Some(noisy.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), apply);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stderr.starts_with("hook-boom\n"),
        "forwarded first: {}",
        run.stderr
    );

    // Block path: the forwarded bytes precede the Warning lines.
    let blocking = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook("printf hook-boom >&2; exit 3")
    ));
    let ours = seeded_store(
        "cph-stderr-block",
        Some(blocking.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stderr.starts_with("hook-boom\nhook-boom\nWarning: "),
        "stderr was: {}",
        run.stderr
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
    let ours = seeded_store(
        "cph-timeout",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stderr.contains("timed out after 150ms"),
        "timeout warning: {}",
        run.stderr
    );
    assert_eq!(
        run.stderr.lines().count(),
        2,
        "one warning per group: {}",
        run.stderr
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);

    let ours = seeded_store(
        "cph-timeout-apply",
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
    assert_eq!(run.code, 1);
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), body);
}

#[test]
fn compact_auto_dry_run_fires_no_hooks() {
    // The engine returns from --dry-run before the apply loop, so
    // pre-compact never fires (marker file absent even though the
    // hook would block).
    let body = seeded_body();
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook("printf ran > hook-ran.txt; exit 3")
    ));
    let ours = seeded_store(
        "cph-dryrun",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact",
        "--auto",
        "--dry-run",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(!ours.0.join("hook-ran.txt").exists());
}

#[test]
fn compact_hook_engine_warnings_surface() {
    // The engine's own warnings (non-JSON stdout, mutation ignored):
    // --apply plain prints `Warning: …` on stderr (not quiet-gated)
    // and the compaction proceeds via the user flags; --json and
    // --auto drop them entirely (resolveReplacement ignores them).
    // The reason's wording past the prefix is runtime vocabulary and
    // NOT pinned (README DEVIATIONS).
    let body = seeded_body();
    let config = hooks_config(&format!(
        "hooks:\n  pre-compact:\n    - '{}'\n",
        draining_hook("printf %s not-json")
    ));
    let ours = seeded_store(
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
        normalize_hook_parse_warning(&run.stderr).starts_with(
            "Warning: hook `cat > /dev/null; printf %s not-json` printed non-JSON on stdout \
             (mutation ignored): <parser text>"
        ),
        "engine warning was: {}",
        run.stderr
    );

    // --auto and --json drop the engine warnings; the mechanical
    // merge runs.
    let ours = seeded_store(
        "cph-engine-warn-auto",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let auto = run_in(&ours.0, Path::new(mulch_bin()), &[
        "compact", "--auto", "--yes",
    ]);
    assert_eq!(auto.code, 0, "{}", auto.stderr);
    assert!(
        !auto.stderr.contains("printed non-JSON"),
        "--auto drops engine warnings: {}",
        auto.stderr
    );

    let ours = seeded_store(
        "cph-engine-warn-json",
        Some(config.as_str()),
        &[("dev", body.as_str())],
        &[],
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
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
    assert_eq!(json.code, 0, "{}", json.stderr);
    assert!(
        !json.stderr.contains("printed non-JSON"),
        "--json drops engine warnings: {}",
        json.stderr
    );
    // the compaction ran despite the ignored mutation
    assert!(
        read_store_file(&ours.0, "expertise/dev.jsonl").contains("\"supersedes\":["),
        "mechanical merge ran: {}",
        read_store_file(&ours.0, "expertise/dev.jsonl")
    );
}
