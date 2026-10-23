//! Differential battery for `mulch prime` (mulch-16da): twin stores,
//! ours vs the reference `ml` — the four format families, the
//! manifest flip (threshold + `prime.default_mode`), the strict
//! budget parse, dry-run preview, export bytes, the guard matrix,
//! scoping (--files/--context/domains), the project-contract block,
//! the session-close styles, and the git auto-scope note. Probed
//! live 2026-10-23 (/tmp/sprint35b/probes).
//!
//! One deliberate deviation is pinned per the promote law (user
//! directive 2026-10-13): every self-referential `ml <cmd>` string
//! the reference's prime text emits reads `mulch <cmd>` in ours;
//! byte comparisons reverse it, `prime_output_promotes_mulch` pins
//! ours explicitly.

use std::fmt::Write as _;
use std::path::Path;

use super::support::{
    CONFIG_TAIL, TempDir, mulch_bin, normalize, normalize_dir, promote_theirs, reference_ml,
    run_in, twin_seeded,
};

/// The ranking fixture: two domains, mixed classifications, stars,
/// file and tracker anchors, an id-less record.
const DEV_SEED: &str = "{\
\"type\":\"pattern\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-20T10:00:00.000Z\",\
\"name\":\"Anchored\",\"description\":\"files match\",\"files\":[\"src/lib.rs\"],\"id\":\"mx-100001\",\
\"outcomes\":[{\"status\":\"success\",\"recorded_at\":\"2026-10-21T10:00:00.000Z\"},{\"status\":\"success\",\"recorded_at\":\"2026-10-22T10:00:00.000Z\"}]\
}\n{\
\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T10:00:00.000Z\",\"content\":\"universal one\",\"id\":\"mx-100002\"\
}\n{\
\"type\":\"decision\",\"classification\":\"observational\",\"recorded_at\":\"2026-10-10T10:00:00.000Z\",\"title\":\"Tracked\",\"rationale\":\"seeds evidence\",\"evidence\":{\"seeds\":\"mulch-9abc\"},\"id\":\"mx-100003\"\
}\n";

const TOOLING_SEED: &str = "{\
\"type\":\"guide\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T10:00:00.000Z\",\"name\":\"Anchored dir\",\"description\":\"dir anchor\",\"dir_anchors\":[\"docs\"],\"id\":\"mx-200001\"\
}\n{\
\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-16T10:00:00.000Z\",\"name\":\"No Id\",\"description\":\"id-less\"\
}\n";

/// Twin stores with the two-domain fixture.
fn prime_twins(tag: &str) -> (TempDir, TempDir) {
    twin_seeded(
        tag,
        None,
        &[("dev", DEV_SEED), ("tooling", TOOLING_SEED)],
        &[],
    )
}

/// Twin stores with a caller-written config body.
fn prime_twins_config(tag: &str, config: &str) -> (TempDir, TempDir) {
    twin_seeded(
        tag,
        Some(config),
        &[("dev", DEV_SEED), ("tooling", TOOLING_SEED)],
        &[],
    )
}

/// One prime invocation in both twins: asserts channel-split byte
/// equality (promote-reversed, stderr path-normalized per side) and
/// returns our run.
fn assert_prime_parity(ml: &Path, tag: &str, args: &[&str]) -> super::support::Run {
    let (ours, theirs) = prime_twins(tag);
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, ml, args);
    assert_eq!(our.code, their.code, "{args:?} exit code");
    assert_eq!(
        normalize(&our.stdout),
        normalize(&promote_theirs(&their.stdout)),
        "{args:?} stdout (theirs promoted)"
    );
    assert_eq!(
        normalize(&normalize_dir(&our.stderr, &ours.0)),
        normalize(&normalize_dir(&promote_theirs(&their.stderr), &theirs.0)),
        "{args:?} stderr"
    );
    our
}

/// The four families + the --full/--verbose detail switches: byte
/// parity end to end (headers, sections, footers).
#[test]
fn prime_families_match_the_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for (tag, args) in [
        ("compact", vec!["prime", "--all"]),
        ("full", vec!["prime", "--all", "--full"]),
        ("verbose", vec!["prime", "--all", "--verbose"]),
        ("xml", vec!["prime", "--all", "--format", "xml"]),
        ("plain", vec!["prime", "--all", "--format", "plain"]),
        ("explicit-compact", vec!["prime", "--all", "--compact"]),
        ("global-over-full", vec![
            "prime", "--all", "--full", "--format", "compact",
        ]),
        ("json", vec!["prime", "--all", "--json"]),
    ] {
        assert_prime_parity(&ml, &format!("fam-{tag}"), &args);
    }
}

/// Our output promotes THIS binary in every self-referential
/// command string (the pinned deviation; see module docs).
#[test]
fn prime_output_promotes_mulch() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let our = assert_prime_parity(&ml, "promote", &["prime", "--all"]);
    assert!(our.stdout.contains("`mulch search \"query\"`"));
    assert!(our.stdout.contains("mulch prime --files src/foo.ts"));
    assert!(!our.stdout.contains("`ml search"));
    let xml = assert_prime_parity(&ml, "promote-xml", &["prime", "--all", "--format", "xml"]);
    assert!(!xml.stdout.contains("<command>ml "));
}

/// The manifest flip: the size threshold (6 domains), every family,
/// and the JSON payload.
#[test]
fn prime_manifest_matches_the_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for (tag, args) in [
        ("md", vec!["prime", "--all", "--manifest"]),
        ("xml", vec![
            "prime",
            "--all",
            "--manifest",
            "--format",
            "xml",
        ]),
        ("plain", vec![
            "prime",
            "--all",
            "--manifest",
            "--format",
            "plain",
        ]),
        ("json", vec!["prime", "--all", "--manifest", "--json"]),
    ] {
        assert_prime_parity(&ml, &format!("man-{tag}"), &args);
    }

    // Six configured domains cross the auto-flip threshold on a
    // bare run (no --manifest, no scoping).
    let mut config = String::from("version: '1'\ndomains:\n");
    let six = ["a", "b", "c", "d", "e", "f"];
    for domain in six {
        let _ = writeln!(config, "  {domain}: {{}}");
    }
    config.push_str(CONFIG_TAIL);
    let (ours, theirs) = twin_seeded("man-flip", Some(&config), &[("a", DEV_SEED)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime"]);
    let their = run_in(&theirs.0, &ml, &["prime"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, promote_theirs(&their.stdout));
    assert!(
        their
            .stdout
            .starts_with("# Project Expertise Manifest (via Mulch)")
    );

    // `prime.default_mode` overrides both directions; unknown values
    // behave like unset (auto-flip).
    for (tag, mode) in [
        ("manifest", "manifest"),
        ("full", "full"),
        ("bogus", "bogus"),
    ] {
        let mut config = String::from("version: '1'\ndomains:\n  dev: {}\n");
        let _ = writeln!(config, "prime:\n  default_mode: {mode}");
        config.push_str(CONFIG_TAIL);
        let (ours, theirs) = twin_seeded(
            &format!("man-mode-{tag}"),
            Some(&config),
            &[("dev", DEV_SEED)],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime"]);
        let their = run_in(&theirs.0, &ml, &["prime"]);
        assert_eq!(our.stdout, promote_theirs(&their.stdout), "mode {mode}");
    }
}

/// The guard matrix and the strict budget parse (order: guards,
/// domains, --context gate, then budget).
#[test]
fn prime_guards_and_budget_parse_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for (tag, args) in [
        ("mfull", vec!["prime", "--manifest", "--full"]),
        ("mdry", vec!["prime", "--manifest", "--dry-run"]),
        ("mscope", vec!["prime", "--manifest", "dev"]),
        ("mfiles", vec![
            "prime",
            "--manifest",
            "--files",
            "src/lib.rs",
        ]),
        ("budget0", vec!["prime", "--budget", "0", "--all"]),
        ("budget37", vec!["prime", "--budget", "3.7", "--all"]),
        ("budgetabc", vec!["prime", "--budget", "abc", "--all"]),
        ("budgethuge", vec![
            "prime",
            "--budget",
            "999999999999999999999999",
            "--all",
        ]),
        ("jsonbudget", vec![
            "--json", "prime", "--budget", "0", "--all",
        ]),
        ("jsonmfull", vec!["--json", "prime", "--manifest", "--full"]),
    ] {
        assert_prime_parity(&ml, &format!("guard-{tag}"), &args);
    }
}

/// Domain scoping: positional + --domain + --exclude-domain, the
/// unknown-domain surfaces (plain hint, json sentence), and the
/// empty-union-minus-exclusion law (mx-b68e4d).
#[test]
fn prime_domain_scoping_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for (tag, args) in [
        ("pos", vec!["prime", "dev"]),
        ("pos2", vec!["prime", "tooling", "dev"]),
        ("flag", vec!["prime", "--domain", "dev"]),
        ("mixed", vec!["prime", "dev", "--domain", "tooling"]),
        ("dup", vec!["prime", "dev", "dev"]),
        ("excl", vec!["prime", "--exclude-domain", "dev"]),
        ("allexcl", vec![
            "prime",
            "--exclude-domain",
            "dev",
            "--exclude-domain",
            "tooling",
        ]),
        ("exclflag", vec![
            "prime",
            "tooling",
            "--exclude-domain",
            "dev",
        ]),
        ("unknown", vec!["prime", "nope"]),
        ("unknown2", vec!["prime", "dev", "nope"]),
        ("unknownjson", vec!["--json", "prime", "nope"]),
        ("exclunknown", vec!["prime", "--exclude-domain", "nope"]),
        ("exclunknownjson", vec![
            "--json",
            "prime",
            "--exclude-domain",
            "nope",
        ]),
    ] {
        assert_prime_parity(&ml, &format!("scope-{tag}"), &args);
    }

    // Our unknown-domain hint promotes THIS binary.
    let our = assert_prime_parity(&ml, "scope-hint", &["prime", "nope"]);
    assert!(our.stderr.contains("Hint: Run `mulch add nope`"));
}

/// --files (match, no-match, json skip-empty) and --context (outside
/// git, no changed files plain/json).
#[test]
fn prime_files_and_context_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for (tag, args) in [
        ("match", vec!["prime", "--files", "src/lib.rs"]),
        ("suffix", vec!["prime", "--files", "lib.rs"]),
        ("nomatch", vec!["prime", "--files", "nope.rs"]),
        ("jsonmatch", vec![
            "--json",
            "prime",
            "--files",
            "src/lib.rs",
        ]),
        ("jsonnomatch", vec!["--json", "prime", "--files", "nope.rs"]),
    ] {
        assert_prime_parity(&ml, &format!("files-{tag}"), &args);
    }

    // --context outside a git repository.
    let our = assert_prime_parity(&ml, "ctx-nogit", &["prime", "--context"]);
    assert_eq!(our.code, 1);

    // No changed files (clean tree): plain prints the notice on
    // STDOUT with exit 0; json prints the failure envelope on
    // STDERR, still exit 0.
    let (clean, clean_ref) = twin_git("ctx-clean");
    let our = run_in(&clean.0, Path::new(mulch_bin()), &["prime", "--context"]);
    let their = run_in(&clean_ref.0, &ml, &["prime", "--context"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "No changed files found. Nothing to filter by.\n"
    );
    assert_eq!(their.stdout, our.stdout);
    let our = run_in(&clean.0, Path::new(mulch_bin()), &[
        "--json",
        "prime",
        "--context",
    ]);
    let their = run_in(&clean_ref.0, &ml, &["--json", "prime", "--context"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        "{\n  \"success\": false,\n  \"command\": \"prime\",\n  \"error\": \"No changed files found. Nothing to filter by.\"\n}\n"
    );
    assert_eq!(our.stderr, promote_theirs(&their.stderr));
}

/// --dry-run: the preview payload, the budget cut, and --no-limit's
/// nulls; budgetUsed serializes integer-valued floats without the
/// decimal point (the JS number law).
#[test]
fn prime_dry_run_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for (tag, args) in [
        ("default", vec!["prime", "--all", "--dry-run"]),
        ("budget50", vec![
            "prime",
            "--all",
            "--dry-run",
            "--budget",
            "50",
        ]),
        ("budget1", vec![
            "prime",
            "--all",
            "--dry-run",
            "--budget",
            "1",
        ]),
        ("nolimit", vec!["prime", "--all", "--dry-run", "--no-limit"]),
        ("jsonmode", vec!["--json", "prime", "--all", "--dry-run"]),
    ] {
        assert_prime_parity(&ml, &format!("dry-{tag}"), &args);
    }
    let our = assert_prime_parity(&ml, "dry-int", &[
        "prime",
        "--all",
        "--dry-run",
        "--budget",
        "1",
    ]);
    assert!(our.stdout.contains("\"budgetUsed\": 0,"));

    // The truncation summary lands between sections and the footer;
    // the plural and the domain count follow the drops (probed).
    let our = assert_prime_parity(&ml, "budget-cut", &["prime", "--all", "--budget", "40"]);
    assert!(
        our.stdout
            .contains("... and 3 more records across 2 domains (use --budget <n> to show more)")
    );
    let our = assert_prime_parity(&ml, "budget-cut-1", &["prime", "--all", "--budget", "60"]);
    assert!(
        our.stdout
            .contains("... and 2 more records across 1 domain (use --budget <n> to show more)")
    );
}

/// --export: the file bytes equal the console output plus one
/// newline, the confirmation prints unless quiet, json stays silent.
#[test]
fn prime_export_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = prime_twins("export");
    std::fs::create_dir_all("/tmp/opencode/prime-export").expect("export dir");
    let our_path = "/tmp/opencode/prime-export/ours.txt";
    let their_path = "/tmp/opencode/prime-export/theirs.txt";
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--export", our_path,
    ]);
    let their = run_in(&theirs.0, &ml, &["prime", "--all", "--export", their_path]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, format!("✓ Exported to {our_path}\n"));
    assert_eq!(their.stdout, format!("✓ Exported to {their_path}\n"));
    let our_file = std::fs::read(our_path).expect("our export");
    let their_file = std::fs::read(their_path).expect("their export");
    assert_eq!(
        String::from_utf8_lossy(&our_file),
        promote_theirs(&String::from_utf8_lossy(&their_file))
    );
    assert_eq!(our.stdout, format!("✓ Exported to {our_path}\n"));

    let (ours, _) = prime_twins("export-q");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--export", our_path, "--quiet",
    ]);
    assert_eq!(our.stdout, "");
}

/// The project-contract block: custom types, disabled types,
/// per-domain rules — all three families, plus json (no block).
#[test]
fn prime_contract_block_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let config = "version: '1'\ndomains:\n  dev:\n    allowed_types:\n      - pattern\n      - convention\n    required_fields:\n      - files\n  tooling: {}\n\
custom_types:\n  hypothesis:\n    required: [statement, prediction]\n    optional: [evidence_files]\n    dedup_key: statement\n    summary: \"{statement}\"\ndisabled_types:\n  - failure\n"
        .to_string() + CONFIG_TAIL;
    for (tag, args) in [
        ("md", vec!["prime", "--all"]),
        ("xml", vec!["prime", "--all", "--format", "xml"]),
        ("plain", vec!["prime", "--all", "--format", "plain"]),
        ("json", vec!["--json", "prime", "--all"]),
        ("man", vec!["prime", "--all", "--manifest"]),
    ] {
        let (ours, theirs) = prime_twins_config(&format!("contract-{tag}"), &config);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "{args:?}");
        assert_eq!(our.stdout, promote_theirs(&their.stdout), "{args:?}");
        assert_eq!(
            normalize_dir(&our.stderr, &ours.0),
            normalize_dir(&promote_theirs(&their.stderr), &theirs.0),
            "{args:?}"
        );
    }
    // The markdown block leads the output and promotes mulch.
    let (ours, _) = prime_twins_config("contract-pin", &config);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    assert!(our.stdout.starts_with(
        "## Project Contract\n\nWrite-side gates `mulch record` enforces in this project."
    ));
    assert!(
        our.stdout
            .contains("**Disabled types**: `failure` (writes emit a deprecation warning)")
    );
}

/// The session-close styles x formats matrix, plus a custom footer
/// and the empty-custom fallback.
#[test]
fn prime_session_close_styles_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for style in ["conditional", "minimal", "directive", "none"] {
        let config = format!(
            "version: '1'\ndomains:\n  dev: {{}}\nprime:\n  session_close:\n    style: {style}\n"
        ) + CONFIG_TAIL;
        for fmt in ["markdown", "compact", "xml", "plain"] {
            let (ours, theirs) = twin_seeded(
                &format!("sc-{style}-{fmt}"),
                Some(&config),
                &[("dev", DEV_SEED)],
                &[],
            );
            let args = ["prime", "--all", "--format", fmt];
            let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
            let their = run_in(&theirs.0, &ml, &args);
            assert_eq!(our.stdout, promote_theirs(&their.stdout), "{style} {fmt}");
        }
    }
    // custom wins; empty custom falls back to the style.
    for (tag, tail) in [
        ("custom", "    custom: 'CUSTOM FOOTER TEXT'\n"),
        ("empty", "    custom: ''\n    style: none\n"),
    ] {
        let config =
            format!("version: '1'\ndomains:\n  dev: {{}}\nprime:\n  session_close:\n{tail}")
                + CONFIG_TAIL;
        let (ours, theirs) = twin_seeded(
            &format!("sc-{tag}"),
            Some(&config),
            &[("dev", DEV_SEED)],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
        let their = run_in(&theirs.0, &ml, &["prime", "--all"]);
        assert_eq!(our.stdout, promote_theirs(&their.stdout), "{tag}");
    }
}

/// Strict reads surface the reference's malformed-line and
/// unknown-type errors (prefix-pinned; the parse-reason tail is the
/// accepted serde-vs-JS divergence, see query's battery).
#[test]
fn prime_read_errors_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let malformed = DEV_SEED.to_string() + "{ BAD\n";
    let (ours, theirs) = twin_seeded(
        "read-bad",
        None,
        &[("dev", &malformed), ("tooling", TOOLING_SEED)],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    let their = run_in(&theirs.0, &ml, &["prime", "--all"]);
    assert_eq!(our.code, their.code, "malformed exit code");
    let our_err = normalize_dir(&our.stderr, &ours.0);
    let their_err = normalize_dir(&their.stderr, &theirs.0);
    assert!(
        their_err.starts_with("Error: Malformed JSONL at <DIR>/.mulch/expertise/dev.jsonl:4: ")
    );
    assert_eq!(their_err.split(": ").next(), our_err.split(": ").next());

    let unknown = DEV_SEED.to_string()
        + "{\"type\":\"zzz\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-23T20:00:01.000Z\",\"content\":\"unknown\",\"id\":\"mx-400002\"}\n";
    for (tag, args) in [
        ("plain", vec!["prime", "--all"]),
        ("allow", vec!["prime", "--all", "--allow-unknown-types"]),
        ("json", vec!["--json", "prime", "--all"]),
    ] {
        let (ours, theirs) =
            twin_seeded(&format!("read-unk-{tag}"), None, &[("dev", &unknown)], &[]);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "{tag} exit code");
        assert_eq!(
            normalize_dir(&our.stderr, &ours.0),
            normalize_dir(&promote_theirs(&their.stderr), &theirs.0),
            "{tag} stderr"
        );
        assert_eq!(our.stdout, promote_theirs(&their.stdout), "{tag} stdout");
    }
}

/// The no-store contract in both channels.
#[test]
fn prime_no_store_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("nostore-o");
    let theirs = TempDir::new("nostore-t");
    for (tag, args) in [("plain", vec!["prime"]), ("json", vec!["--json", "prime"])] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "{tag}");
        assert_eq!(our.stderr, their.stderr, "{tag}");
        assert_eq!(our.stdout, their.stdout, "{tag}");
    }
    assert_eq!(
        run_in(&ours.0, Path::new(mulch_bin()), &["prime"]).stderr,
        "Error: No .mulch/ directory found. Run `mulch init` to set up this project.\n"
    );
}

// ---- git fixtures (the learn battery's recipe: fixed dates) ----

fn run_git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", "2026-10-23T10:00:00 +0000")
        .env("GIT_COMMITTER_DATE", "2026-10-23T10:00:00 +0000")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Twin git repos with the seed store committed once (so HEAD~1
/// diffs work) and identical working trees.
fn twin_git(tag: &str) -> (TempDir, TempDir) {
    let (ours, theirs) = prime_twins(tag);
    for dir in [&ours.0, &theirs.0] {
        run_git(dir, &["init", "-q"]);
        run_git(dir, &["config", "user.email", "t@example.io"]);
        run_git(dir, &["config", "user.name", "Twin"]);
        run_git(dir, &["add", "-A"]);
        run_git(dir, &["commit", "-qm", "c0"]);
    }
    (ours, theirs)
}

fn twin_write(dirs: [&TempDir; 2], relative: &str, body: &str) {
    for dir in dirs {
        let path = dir.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("dirs");
        }
        std::fs::write(path, body).expect("write");
    }
}

fn twin_commit(dirs: [&TempDir; 2], message: &str) {
    for dir in dirs {
        run_git(&dir.0, &["add", "-A"]);
        run_git(&dir.0, &["commit", "-qm", message]);
    }
}

/// --context over a real HEAD~1 diff, and the plain auto-scope note
/// (git status + tracker signal, `--all` opting out).
#[test]
fn prime_git_scope_and_note_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // HEAD~1 diff: commit src/lib.rs change, then dirty the tree
    // identically in both twins.
    let (ours, theirs) = twin_git("git-ctx");
    twin_write([&ours, &theirs], "src/lib.rs", "changed\n");
    twin_commit([&ours, &theirs], "c1");
    twin_write([&ours, &theirs], "src/other.rs", "uncommitted\n");
    for (tag, args) in [
        ("ctx", vec!["prime", "--context"]),
        ("allof", vec!["prime", "--all"]),
        ("bare", vec!["prime"]),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "{tag}");
        assert_eq!(our.stdout, promote_theirs(&their.stdout), "{tag} stdout");
        assert_eq!(
            normalize_dir(&our.stderr, &ours.0),
            normalize_dir(&promote_theirs(&their.stderr), &theirs.0),
            "{tag} stderr"
        );
    }

    // Tracker signal: an in-progress seeds row scopes by evidence.
    let (ours, theirs) = twin_git("git-track");
    twin_write(
        [&ours, &theirs],
        ".seeds/issues.jsonl",
        "{\"id\":\"mulch-9abc\",\"status\":\"in_progress\",\"title\":\"x\"}\n",
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime"]);
    let their = run_in(&theirs.0, &ml, &["prime"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, promote_theirs(&their.stdout));
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        normalize_dir(&promote_theirs(&their.stderr), &theirs.0)
    );
    // 5 records, 3 kept: the anchored pattern and the dir-anchored
    // guide drop (no matching changed file), the tracker-evidenced
    // decision and the anchorless pair stay.
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        "prime: scoped to 3 of 5 records based on git status + active-work (seeds:mulch-9abc); run with --all for the full corpus\n"
    );
}

/// A store with no records at all: the empty-state text in every
/// family, manifest included.
#[test]
fn prime_empty_store_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let config = "version: '1'\ndomains:\n  dev: {}\n".to_string() + CONFIG_TAIL;
    let (ours, theirs) = twin_seeded("empty", Some(&config), &[], &[]);
    for (tag, args) in [
        ("compact", vec!["prime", "--all"]),
        ("full", vec!["prime", "--all", "--full"]),
        ("xml", vec!["prime", "--all", "--format", "xml"]),
        ("plain", vec!["prime", "--all", "--format", "plain"]),
        ("man", vec!["prime", "--all", "--manifest"]),
        ("json", vec!["--json", "prime", "--all"]),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "{tag}");
        assert_eq!(our.stdout, promote_theirs(&their.stdout), "{tag}");
    }
    // A configured domain with zero records still manifests (count 0).
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--manifest"]);
    assert!(our.stdout.contains("- **dev**: 0 records"));
}
