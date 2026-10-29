//! Battery for `mulch prime` (mulch-16da), ADR-0008 edition: OUR
//! byte pins for the render laws. The per-record bullet lines carry
//! relative ages by design, so the age-free law blocks (headers, the
//! Quick Reference, the session-close footers, the manifest table)
//! are frozen verbatim while the record lines pin membership and
//! structure; the guard matrix, error surfaces, hook payload law,
//! blocking texts, and mutation law are all our-side pins.

use std::fmt::Write as _;
use std::path::Path;

use super::support::{
    CONFIG_TAIL, TempDir, error_envelope, hook_stdin, mulch_bin, normalize, normalize_dir, run_in,
    seeded_store,
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

/// A store with the two-domain fixture.
fn prime_store(tag: &str) -> TempDir {
    seeded_store(
        tag,
        None,
        &[("dev", DEV_SEED), ("tooling", TOOLING_SEED)],
        &[],
    )
}

/// A store with a caller-written config body.
fn prime_store_config(tag: &str, config: &str) -> TempDir {
    seeded_store(
        tag,
        Some(config),
        &[("dev", DEV_SEED), ("tooling", TOOLING_SEED)],
        &[],
    )
}

/// The shared Quick Reference block (age-free, frozen).
const QUICK_REFERENCE: &str = "## Quick Reference\n\n\
- `mulch search \"query\"` — find relevant records before implementing\n\
- `mulch prime --files src/foo.ts` — prime **before** editing a file, not just at session start\n\
- `mulch prime --context` — load records for git-changed files\n\
- `mulch record <domain> --type <type> --description \"...\"`\n\
\x20 - Evidence: commit + files auto-populate from git. Trackers: `--evidence-seeds` / \
`--evidence-gh` / `--evidence-linear` / `--evidence-bead`. Override commit: \
`--evidence-commit <sha>`. Link records: `--relates-to <mx-id>`\n\
- `mulch doctor` — check record health\n";

/// The record-types table (age-free, frozen).
const TYPES_TABLE: &str = "\n**Record types and required flags:**\n\n\
| Type | Required flags |\n\
|------|----------------|\n\
| `convention` | `\"<content>\"` (positional) |\n\
| `pattern` | `--name \"...\" --description \"...\"` |\n\
| `failure` | `--description \"...\" --resolution \"...\"` |\n\
| `decision` | `--title \"...\" --rationale \"...\"` |\n\
| `reference` | `--name \"...\" --description \"...\"` |\n\
| `guide` | `--name \"...\" --description \"...\"` |\n\n";

/// The default (conditional) session-close footer (frozen).
const SESSION_CLOSE: &str = "# 🚨 SESSION CLOSE 🚨\n\n\
**If you discovered insights worth preserving** — a new convention, a pattern that worked, a \
decision made, a failure encountered — record them before closing this session:\n\n\
```bash\n\
mulch learn                              # see what files changed\n\
mulch record <domain> --type <type> ...  # record the insight\n\
mulch sync                               # validate, stage, commit\n\
```\n\n\
Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records are also \
noise.\n";

/// The compact family render: headers, section law, the why markers,
/// and the frozen footer blocks.
#[test]
fn prime_compact_pins_the_render_law() {
    let ours = prime_store("fam-compact");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(our.stderr, "");
    assert!(
        our.stdout.starts_with(
            "# Project Expertise (via Mulch)\n\n## dev (3 records, updated just now)\n"
        ),
        "{}",
        our.stdout
    );
    // record bullets: membership + the why markers (ages volatile)
    assert!(our.stdout.contains("Anchored: files match (src/lib.rs)"));
    assert!(our.stdout.contains("— why: ★2 confirmations"));
    assert!(our.stdout.contains("universal one"));
    assert!(our.stdout.contains("— why: applies broadly (no anchors)"));
    assert!(
        our.stdout
            .contains("## tooling (2 records, updated just now)")
    );
    assert!(our.stdout.contains("No Id: id-less"));
    // the frozen age-free tail: Quick Reference + types table +
    // session close
    assert!(
        our.stdout
            .ends_with(&format!("{QUICK_REFERENCE}{TYPES_TABLE}{SESSION_CLOSE}")),
        "the footer law: {}",
        our.stdout
    );
    // --full and --verbose succeed and stay compact-verbose switches
    for flag in ["--full", "--verbose"] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all", flag]);
        assert_eq!(run.code, 0, "{flag}: {}", run.stderr);
        assert!(
            run.stdout.starts_with("# Project Expertise (via Mulch)"),
            "{flag}"
        );
    }
    // the other families render the same corpus
    for format in ["xml", "plain"] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), &[
            "prime", "--all", "--format", format,
        ]);
        assert_eq!(run.code, 0, "{format}: {}", run.stderr);
        assert!(run.stdout.contains("files match"), "{format}");
    }
    // --compact is explicit; the global --format beats --full
    let explicit = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "--all",
        "--compact",
    ]);
    assert_eq!(explicit.code, 0);
    let global = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--full", "--format", "compact",
    ]);
    assert_eq!(explicit.stdout, global.stdout, "the global format wins");
    // json rides the envelope
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--json",
    ]);
    assert_eq!(json.code, 0);
    assert!(
        json.stdout.contains("\"type\": \"expertise\""),
        "{}",
        json.stdout
    );
}

/// Our output promotes THIS binary in every self-referential command
/// string (the pinned deviation; see module docs).
#[test]
fn prime_output_promotes_mulch() {
    let ours = prime_store("promote");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    assert!(our.stdout.contains("`mulch search \"query\"`"));
    assert!(our.stdout.contains("mulch prime --files src/foo.ts"));
    assert!(!our.stdout.contains("`ml search"));
    let xml = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--format", "xml",
    ]);
    assert!(!xml.stdout.contains("<command>ml "));
}

/// The manifest flip: the manifest render (frozen age-free blocks),
/// every family, the JSON payload, and the six-domain auto-flip.
#[test]
fn prime_manifest_pins_the_render() {
    let ours = prime_store("man-md");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "--all",
        "--manifest",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout.starts_with(
            "# Project Expertise Manifest (via Mulch)\n\n> Manifest mode lists available \
             domains. Load records on demand with `mulch prime <domain>` or `mulch prime \
             --files <path>`.\n\n"
        ),
        "{}",
        our.stdout
    );
    assert!(
        our.stdout.contains(
            "## Available Domains\n\n- **dev**: 3 records (1 pattern, 1 \
             convention, 1 decision) — updated just now\n- **tooling**: 2 records (1 pattern, 1 \
             guide) — updated just now\n"
        ),
        "{}",
        our.stdout
    );
    assert!(our.stdout.ends_with(SESSION_CLOSE));
    // the other families + json stay healthy manifests
    for args in [
        vec!["prime", "--all", "--manifest", "--format", "xml"],
        vec!["prime", "--all", "--manifest", "--format", "plain"],
        vec!["prime", "--all", "--manifest", "--json"],
    ] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(run.code, 0, "{args:?}: {}", run.stderr);
    }

    // Six configured domains cross the auto-flip threshold on a bare
    // run (no --manifest, no scoping).
    let mut config = String::from("version: '1'\ndomains:\n");
    for domain in ["a", "b", "c", "d", "e", "f"] {
        let _ = writeln!(config, "  {domain}: {{}}");
    }
    config.push_str(CONFIG_TAIL);
    let ours = seeded_store("man-flip", Some(&config), &[("a", DEV_SEED)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime"]);
    assert_eq!(our.code, 0);
    assert!(
        our.stdout
            .starts_with("# Project Expertise Manifest (via Mulch)")
    );

    // `prime.default_mode` overrides both directions; unknown values
    // behave like unset (auto-flip).
    for (tag, mode, is_manifest) in [
        ("manifest", "manifest", true),
        ("full", "full", false),
        ("bogus", "bogus", false),
    ] {
        let mut config = String::from("version: '1'\ndomains:\n  dev: {}\n");
        let _ = writeln!(config, "prime:\n  default_mode: {mode}");
        config.push_str(CONFIG_TAIL);
        let ours = seeded_store(
            &format!("man-mode-{tag}"),
            Some(&config),
            &[("dev", DEV_SEED)],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime"]);
        let flipped = our
            .stdout
            .starts_with("# Project Expertise Manifest (via Mulch)");
        assert_eq!(flipped, is_manifest, "mode {mode}");
    }
}

/// The guard matrix and the strict budget parse (order: guards,
/// domains, --context gate, then budget).
#[test]
fn prime_guards_and_budget_parse() {
    let ours = prime_store("guard");
    for (tag, args, expected) in [
        (
            "mfull",
            vec!["prime", "--manifest", "--full"],
            "Error: Cannot combine --manifest with --full.\n",
        ),
        (
            "mdry",
            vec!["prime", "--manifest", "--dry-run"],
            "Error: Cannot combine --dry-run with --manifest. Manifest mode lists domains, not \
             records; --dry-run previews which records would be primed.\n",
        ),
        (
            "mscope",
            vec!["prime", "--manifest", "dev"],
            "Error: --manifest cannot be combined with scoping arguments. Manifest mode lists \
             available domains; use `mulch prime <domain>` or `mulch prime --files <path>` to \
             load records.\n",
        ),
        (
            "mfiles",
            vec!["prime", "--manifest", "--files", "src/lib.rs"],
            "Error: --manifest cannot be combined with scoping arguments. Manifest mode lists \
             available domains; use `mulch prime <domain>` or `mulch prime --files <path>` to \
             load records.\n",
        ),
        (
            "budget0",
            vec!["prime", "--budget", "0", "--all"],
            "Error: --budget must be a positive integer (got \"0\").\n",
        ),
        (
            "budget37",
            vec!["prime", "--budget", "3.7", "--all"],
            "Error: --budget must be a positive integer (got \"3.7\").\n",
        ),
        (
            "budgetabc",
            vec!["prime", "--budget", "abc", "--all"],
            "Error: --budget must be a positive integer (got \"abc\").\n",
        ),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1, "{tag}: {}", our.stderr);
        assert_eq!(our.stdout, "", "{tag}");
        assert!(our.stderr.starts_with(expected), "{tag}: {}", our.stderr);
    }
    // a huge budget accepts (JS Number law)
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "--budget",
        "999999999999999999999999",
        "--all",
    ]);
    assert_eq!(our.code, 0, "budgethuge: {}", our.stderr);
    // json failures ride the envelope on stderr
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "prime", "--budget", "0", "--all",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        error_envelope("prime", "--budget must be a positive integer (got \"0\").")
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "prime",
        "--manifest",
        "--full",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        error_envelope("prime", "Cannot combine --manifest with --full.")
    );
}

/// Domain scoping: positional + --domain + --exclude-domain, the
/// unknown-domain surfaces (plain hint, json sentence), and the
/// empty-union-minus-exclusion law (mx-b68e4d).
#[test]
fn prime_domain_scoping() {
    let ours = prime_store("scope");
    // the union renders everything
    let _all = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    let pos = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "tooling", "dev",
    ]);
    assert!(pos.stdout.contains("## tooling (2 records"));
    assert!(pos.stdout.contains("## dev (3 records"));
    assert!(
        pos.stdout.find("## tooling") < pos.stdout.find("## dev"),
        "positionals union in the GIVEN order"
    );
    let flagged = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--domain", "dev",
    ]);
    assert!(flagged.stdout.contains("files match"));
    assert!(!flagged.stdout.contains("dir anchor"));
    // mixed positional + flag; duplicates collapse
    let mixed = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "dev", "--domain", "tooling",
    ]);
    assert!(mixed.stdout.contains("files match"));
    assert!(mixed.stdout.contains("dir anchor"));
    let dup = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "dev", "dev"]);
    assert_eq!(dup.stdout, flagged.stdout, "duplicates collapse");
    // exclusions
    let excl = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "--exclude-domain",
        "dev",
    ]);
    assert!(!excl.stdout.contains("files match"));
    assert!(excl.stdout.contains("dir anchor"));
    // excluding everything is the empty union (mx-b68e4d): healthy
    let allexcl = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "--exclude-domain",
        "dev",
        "--exclude-domain",
        "tooling",
    ]);
    assert_eq!(allexcl.code, 0, "{}", allexcl.stderr);
    // exclusion loses to an explicit scope
    let exclflag = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "tooling",
        "--exclude-domain",
        "dev",
    ]);
    assert!(exclflag.stdout.contains("dir anchor"));

    // unknown domains: the two-line plain hint (the exclusion arm has
    // its own wording), the one-line json
    for args in [vec!["prime", "nope"], vec!["prime", "dev", "nope"]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1, "{args:?}");
        assert_eq!(
            our.stderr,
            "Error: Domain \"nope\" not found in config. Available domains: dev, tooling\nHint: \
             Run `mulch add nope` to create this domain, or check .mulch/mulch.config.yaml\n",
            "{args:?}"
        );
    }
    let excluded = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "--exclude-domain",
        "nope",
    ]);
    assert_eq!(excluded.code, 1);
    assert_eq!(
        excluded.stderr,
        "Error: Excluded domain \"nope\" not found in config. Available domains: dev, \
         tooling\nHint: Run `mulch add nope` to create this domain, or check \
         .mulch/mulch.config.yaml\n"
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "prime", "nope",
    ]);
    assert_eq!(json.code, 1);
    assert_eq!(
        json.stderr,
        error_envelope(
            "prime",
            "Domain \"nope\" not found in config. Available domains: dev, tooling"
        )
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "prime",
        "--exclude-domain",
        "nope",
    ]);
    assert_eq!(json.code, 1);
    assert_eq!(
        json.stderr,
        error_envelope(
            "prime",
            "Excluded domain \"nope\" not found in config. Available domains: dev, tooling"
        )
    );
}

/// --files (match, no-match, json skip-empty) and --context (outside
/// git, no changed files plain/json).
#[test]
fn prime_files_and_context() {
    let ours = prime_store("files");
    let match_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "--files",
        "src/lib.rs",
    ]);
    assert_eq!(match_run.code, 0);
    assert!(match_run.stdout.contains("files match"));
    // anchorless records ride along under --files (applies broadly)
    // a suffix matches
    let suffix = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--files", "lib.rs",
    ]);
    assert!(suffix.stdout.contains("files match"));
    // no match: empty scope, healthy exit
    let nomatch = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--files", "nope.rs",
    ]);
    assert_eq!(nomatch.code, 0, "{}", nomatch.stderr);
    let jsonmatch = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "prime",
        "--files",
        "src/lib.rs",
    ]);
    assert_eq!(jsonmatch.code, 0);
    assert!(jsonmatch.stdout.contains("files match"));
    let jsonnomatch = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "prime", "--files", "nope.rs",
    ]);
    assert_eq!(jsonnomatch.code, 0);
    assert!(!jsonnomatch.stdout.contains("files match"));

    // --context outside a git repository fails.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--context"]);
    assert_eq!(our.code, 1);

    // No changed files (clean tree): plain prints the notice on
    // STDOUT with exit 0; json prints the failure envelope on
    // STDERR, still exit 0.
    let clean = git_primes("ctx-clean");
    let our = run_in(&clean.0, Path::new(mulch_bin()), &["prime", "--context"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "No changed files found. Nothing to filter by.\n"
    );
    let our = run_in(&clean.0, Path::new(mulch_bin()), &[
        "--json",
        "prime",
        "--context",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        "{\n  \"success\": false,\n  \"command\": \"prime\",\n  \"error\": \"No changed files \
         found. Nothing to filter by.\"\n}\n"
    );
}

/// --dry-run: the preview payload, the budget cut, and --no-limit's
/// nulls; budgetUsed serializes integer-valued floats without the
/// decimal point (the JS number law).
#[test]
fn prime_dry_run_pins_the_shapes() {
    let ours = prime_store("dry");
    for args in [
        vec!["prime", "--all", "--dry-run"],
        vec!["prime", "--all", "--dry-run", "--budget", "50"],
        vec!["prime", "--all", "--dry-run", "--budget", "1"],
        vec!["prime", "--all", "--dry-run", "--no-limit"],
        vec!["--json", "prime", "--all", "--dry-run"],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?}: {}", our.stderr);
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "--all",
        "--dry-run",
        "--budget",
        "1",
    ]);
    assert!(our.stdout.contains("\"budgetUsed\": 0,"));

    // The truncation summary lands between sections and the footer;
    // the plural and the domain count follow the drops.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--budget", "40",
    ]);
    assert!(
        our.stdout
            .contains("... and 3 more records across 2 domains (use --budget <n> to show more)")
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--budget", "60",
    ]);
    assert!(
        our.stdout
            .contains("... and 2 more records across 1 domain (use --budget <n> to show more)")
    );
}

/// --export: the file bytes equal the console output exactly, the
/// confirmation prints unless quiet, json stays silent.
#[test]
fn prime_export_law() {
    let ours = prime_store("export");
    let path = "/tmp/opencode/prime-export-pin.md";
    let console = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--export", path,
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(our.stdout, format!("✓ Exported to {path}\n"));
    let file = std::fs::read_to_string(path).expect("export file");
    assert_eq!(file, console.stdout);

    let ours = prime_store("export-q");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--export", path, "--quiet",
    ]);
    assert_eq!(our.stdout, "");
}

/// The project-contract block: custom types, disabled types,
/// per-domain rules — the block leads the markdown and promotes
/// mulch (json renders no block).
#[test]
fn prime_contract_block_pins_its_bytes() {
    let config = "version: '1'\ndomains:\n  dev:\n    allowed_types:\n      - pattern\n      - convention\n    required_fields:\n      - files\n  tooling: {}\n\
custom_types:\n  hypothesis:\n    required: [statement, prediction]\n    optional: [evidence_files]\n    dedup_key: statement\n    summary: \"{statement}\"\ndisabled_types:\n  - failure\n"
        .to_string() + CONFIG_TAIL;
    let ours = prime_store_config("contract-pin", &config);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    assert_eq!(our.code, 0);
    assert!(our.stdout.starts_with(
        "## Project Contract\n\nWrite-side gates `mulch record` enforces in this project."
    ));
    assert!(
        our.stdout
            .contains("**Disabled types**: `failure` (writes emit a deprecation warning)")
    );
    assert!(our.stdout.contains("`hypothesis`"));
    assert!(our.stdout.contains("required fields — files"));
    // xml/plain/json/manifest stay healthy with the contract config
    for args in [
        vec!["prime", "--all", "--format", "xml"],
        vec!["prime", "--all", "--format", "plain"],
        vec!["--json", "prime", "--all"],
        vec!["prime", "--all", "--manifest"],
    ] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(run.code, 0, "{args:?}: {}", run.stderr);
    }
    // json renders no contract block
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "prime", "--all",
    ]);
    assert!(!json.stdout.contains("Project Contract"));
}

/// The session-close styles x the markdown render: each style's
/// footer is frozen age-free; a custom footer wins; an empty custom
/// falls back to the style.
#[test]
fn prime_session_close_styles() {
    for (style, tail) in [
        ("conditional", SESSION_CLOSE),
        (
            "minimal",
            "**Before closing**: record any insight worth preserving — `mulch learn` → \
             `mulch record <domain> --type <type> …` → `mulch sync`. Skip if no insight \
             surfaced.\n",
        ),
        (
            "directive",
            "**Do not record**: restatements of CLAUDE.md or prior records, generic truisms \
             (\"use TypeScript\"), single-PR-scoped facts (\"this PR adds X\"), ritual \
             confirmations with no new information.\n\nSkip if no insight surfaced. Unrecorded \
             learnings are lost; ritual filler records are also noise.\n",
        ),
    ] {
        let config = format!(
            "version: '1'\ndomains:\n  dev: {{}}\nprime:\n  session_close:\n    style: {style}\n"
        ) + CONFIG_TAIL;
        let ours = seeded_store(
            &format!("sc-{style}"),
            Some(&config),
            &[("dev", DEV_SEED)],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
        assert_eq!(our.code, 0, "{style}: {}", our.stderr);
        assert!(our.stdout.ends_with(tail), "{style}: {}", our.stdout);
    }
    // `none` drops the footer entirely (the table is the last block)
    let config = "version: '1'\ndomains:\n  dev: {}\nprime:\n  session_close:\n    style: none\n"
        .to_string()
        + CONFIG_TAIL;
    let ours = seeded_store("sc-none", Some(&config), &[("dev", DEV_SEED)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    assert!(
        our.stdout
            .contains("| `guide` | `--name \"...\" --description \"...\"` |"),
        "none-style stdout: {}",
        our.stdout
    );
    assert!(!our.stdout.contains("SESSION CLOSE"));
    // custom wins; empty custom falls back to the style
    for (tag, tail_line, custom) in [
        ("custom", Some("CUSTOM FOOTER TEXT"), "'CUSTOM FOOTER TEXT'"),
        ("empty", None, "''"),
    ] {
        let config = format!(
            "version: '1'\ndomains:\n  dev: {{}}\nprime:\n  session_close:\n    style: none\n    \
             custom: {custom}\n"
        ) + CONFIG_TAIL;
        let ours = seeded_store(
            &format!("sc-{tag}"),
            Some(&config),
            &[("dev", DEV_SEED)],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
        if let Some(line) = tail_line {
            assert!(our.stdout.contains(line), "{tag}: {}", our.stdout);
        } else {
            assert!(!our.stdout.contains("SESSION CLOSE"), "{tag}");
        }
    }
}

/// Strict reads surface the malformed-line and unknown-type errors
/// (template-pinned; the parse-reason tail is the accepted
/// serde-vs-JS divergence).
#[test]
fn prime_read_errors_pin_the_templates() {
    let malformed = DEV_SEED.to_string() + "{ BAD\n";
    let ours = seeded_store(
        "read-bad",
        None,
        &[("dev", &malformed), ("tooling", TOOLING_SEED)],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    assert_eq!(our.code, 1, "malformed exit code");
    let our_err = normalize_dir(&our.stderr, &ours.0);
    assert!(
        our_err.starts_with("Error: Malformed JSONL at <DIR>/.mulch/expertise/dev.jsonl:4: "),
        "{our_err}"
    );

    let unknown = DEV_SEED.to_string()
        + "{\"type\":\"zzz\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-23T20:00:01.000Z\",\"content\":\"unknown\",\"id\":\"mx-400002\"}\n";
    // plain: the registry text aborts
    let ours = seeded_store("read-unk-plain", None, &[("dev", &unknown)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    assert_eq!(our.code, 1);
    assert!(
        normalize_dir(&our.stderr, &ours.0).starts_with(
            "Error: Unknown record type \"zzz\" at <DIR>/.mulch/expertise/dev.jsonl:4 \
             (id=mx-400002)."
        ),
        "{}",
        our.stderr
    );
    // --allow-unknown-types admits the record (render succeeds)
    let ours = seeded_store("read-unk-allow", None, &[("dev", &unknown)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime",
        "--all",
        "--allow-unknown-types",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    // json rides the envelope
    let ours = seeded_store("read-unk-json", None, &[("dev", &unknown)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "prime", "--all",
    ]);
    assert_eq!(our.code, 1);
    assert!(normalize(&our.stderr).contains("Unknown record type"));
}

/// The no-store contract in both channels.
#[test]
fn prime_no_store_pins_the_wording() {
    let ours = TempDir::new("nostore-o");
    let message = "No .mulch/ directory found. Run `mulch init` to set up this project.";
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["prime"]);
    assert_eq!(plain.code, 1);
    assert_eq!(plain.stderr, format!("Error: {message}\n"));
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "prime"]);
    assert_eq!(json.code, 1);
    assert_eq!(json.stderr, error_envelope("prime", message));
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

/// A git repo with the seed store committed once (so HEAD~1 diffs
/// work).
fn git_primes(tag: &str) -> TempDir {
    let ours = prime_store(tag);
    run_git(&ours.0, &["init", "-q"]);
    run_git(&ours.0, &["config", "user.email", "t@example.io"]);
    run_git(&ours.0, &["config", "user.name", "Twin"]);
    run_git(&ours.0, &["add", "-A"]);
    run_git(&ours.0, &["commit", "-qm", "c0"]);
    ours
}

fn git_write(dir: &TempDir, relative: &str, body: &str) {
    let path = dir.0.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("dirs");
    }
    std::fs::write(path, body).expect("write");
}

fn git_commit(dir: &TempDir, message: &str) {
    run_git(&dir.0, &["add", "-A"]);
    run_git(&dir.0, &["commit", "-qm", message]);
}

/// --context over a real HEAD~1 diff, and the plain auto-scope note
/// (git status + tracker signal, `--all` opting out).
#[test]
fn prime_git_scope_and_note_pin_the_law() {
    // HEAD~1 diff: commit src/lib.rs change, then dirty the tree.
    let ours = git_primes("git-ctx");
    git_write(&ours, "src/lib.rs", "changed\n");
    git_commit(&ours, "c1");
    git_write(&ours, "src/other.rs", "uncommitted\n");
    // --context scopes to the anchored pattern (lib.rs changed)
    let ctx = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--context"]);
    assert_eq!(ctx.code, 0, "{}", ctx.stderr);
    assert!(ctx.stdout.contains("files match"), "{}", ctx.stdout);
    // --all opts out
    let allof = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    assert!(allof.stdout.contains("universal one"));
    // a bare run scopes by git status too
    let bare = run_in(&ours.0, Path::new(mulch_bin()), &["prime"]);
    assert_eq!(bare.code, 0, "{}", bare.stderr);
    assert!(
        normalize_dir(&bare.stderr, &ours.0).starts_with("prime: scoped to"),
        "the auto-scope note: {}",
        bare.stderr
    );

    // Tracker signal: an in-progress seeds row scopes by evidence.
    let ours = git_primes("git-track");
    git_write(
        &ours,
        ".seeds/issues.jsonl",
        "{\"id\":\"mulch-9abc\",\"status\":\"in_progress\",\"title\":\"x\"}\n",
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    // 5 records, 3 kept: the anchored pattern and the dir-anchored
    // guide drop (no matching changed file), the tracker-evidenced
    // decision and the anchorless pair stay.
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        "prime: scoped to 3 of 5 records based on git status + active-work (seeds:mulch-9abc); \
         run with --all for the full corpus\n"
    );
}

/// A store with no records at all: the empty-state text in every
/// family, manifest included.
#[test]
fn prime_empty_store_pins_its_states() {
    let config = "version: '1'\ndomains:\n  dev: {}\n".to_string() + CONFIG_TAIL;
    let ours = seeded_store("empty", Some(&config), &[], &[]);
    for args in [
        vec!["prime", "--all"],
        vec!["prime", "--all", "--full"],
        vec!["prime", "--all", "--format", "xml"],
        vec!["prime", "--all", "--format", "plain"],
        vec!["prime", "--all", "--manifest"],
        vec!["--json", "prime", "--all"],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?}: {}", our.stderr);
    }
    // A configured domain with zero records still manifests (count 0).
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--manifest"]);
    assert!(our.stdout.contains("- **dev**: 0 records"));
}

// ---- mulch-c5c4: the time-law corners ----

/// The `recorded_at` shapes the old strict-ISO approximations
/// rejected — date-only (UTC midnight), the JSC space separator,
/// zone-less civil time — parse through the lib's `new Date(x)`
/// constructor law (compact ages, `Recent` surface reasons, and the
/// budget's recency tiebreak). The budget tiebreak is the byte-visible
/// discriminator: identical type/classification/score with the
/// date-only (ancient) record FIRST in corpus — recency must flip
/// corpus order and keep the ISO-fresh record.
#[test]
fn prime_time_law_corners_feed_the_budget_tiebreak() {
    // zone-less civil stamp 30h old: read in the process time zone
    // (ECMA-262 local-time branch), so the assertion holds in any zone
    let fresh_civil = (jiff::Timestamp::now() - jiff::Span::new().hours(30))
        .to_zoned(jiff::tz::TimeZone::system())
        .strftime("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let fresh_line = format!(
        r#"{{"type":"decision","classification":"tactical","recorded_at":"{fresh_civil}","title":"Fresh zone-less","rationale":"recency arm","id":"mx-900009"}}"#
    );
    let corner_seed = [
        r#"{"type":"convention","classification":"tactical","recorded_at":"2020-01-02T12:00:00.000Z","content":"iso control","id":"mx-900001"}"#,
        r#"{"type":"convention","classification":"tactical","recorded_at":"2020-01-01","content":"date only corner","id":"mx-900002"}"#,
        r#"{"type":"convention","classification":"tactical","recorded_at":"2020-01-01 12:00:00Z","content":"space separator corner","id":"mx-900003"}"#,
        r#"{"type":"convention","classification":"tactical","recorded_at":"2020-01-01T12:00:00","content":"zone-less corner","id":"mx-900004"}"#,
        r#"{"type":"pattern","classification":"foundational","recorded_at":"2020-01-01","name":"Foundational date-only","description":"bare classification","id":"mx-900005"}"#,
        r#"{"type":"failure","classification":"tactical","recorded_at":"2020-01-01T12:00:00","description":"zone-less failure","resolution":"r","id":"mx-900006"}"#,
        r#"{"type":"reference","classification":"tactical","recorded_at":"2020-01-01","name":"Date-only ref","description":"d","id":"mx-900007"}"#,
        r#"{"type":"guide","classification":"tactical","recorded_at":"2020-01-01 12:00:00Z","name":"Space-sep guide","description":"d","id":"mx-900008"}"#,
        fresh_line.as_str(),
    ]
    .join("\n")
        + "\n";
    for args in [
        vec!["prime", "--all"],
        vec!["prime", "--all", "--full"],
        vec!["prime", "--all", "--format", "xml"],
        vec!["prime", "--all", "--verbose"],
    ] {
        let ours = seeded_store(
            &format!("c5c4-{}", args.last().unwrap_or(&"")),
            None,
            &[("dev", &corner_seed)],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?}: {}", our.stderr);
        assert!(our.stdout.contains("Fresh zone-less"), "{args:?}");
        assert!(our.stdout.contains("date only corner"), "{args:?}");
    }

    // The budget tiebreak: 30 tokens fit exactly one of the two
    // equal-priority conventions.
    let budget_seed = concat!(
        r#"{"type":"convention","classification":"tactical","recorded_at":"2020-01-01","content":"date only old first in corpus","id":"mx-910001"}"#,
        "\n",
        r#"{"type":"convention","classification":"tactical","recorded_at":"2026-10-20T10:00:00.000Z","content":"iso fresh second in corpus","id":"mx-910002"}"#,
        "\n",
    );
    let ours = seeded_store("c5c4-budget", None, &[("dev", budget_seed)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--budget", "30",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    // recency, not corpus order, picks the survivor
    assert!(our.stdout.contains("iso fresh second in corpus"));
    assert!(!our.stdout.contains("date only old first in corpus"));
    assert!(our.stdout.contains("... and 1 more record across 1 domain"));
}

// ---- pre-prime hooks (mulch-7f49) ----

/// A store (the two-domain ranking fixture) whose config declares the
/// given `hooks:`/`hook_settings:` YAML body.
fn prime_hook_store(tag: &str, hook_yaml: &str) -> TempDir {
    let config = format!("version: '1'\ndomains:\n  dev: {{}}\n  tooling: {{}}\n{hook_yaml}");
    seeded_store(
        tag,
        Some(config.as_str()),
        &[("dev", DEV_SEED), ("tooling", TOOLING_SEED)],
        &[],
    )
}

/// The payload law: `{"event":"pre-prime","payload":{"domains":[…]}}`
/// over the loaded, trust-sorted, PRE-budget records (parsed and
/// asserted on OUR capture) plus healthy renders in plain and json.
#[test]
fn prime_pre_prime_payload_law() {
    for (tag, args) in [
        ("ph-pay-plain", &["prime", "--all"][..]),
        ("ph-pay-json", &["--json", "prime", "--all"][..]),
    ] {
        let ours = prime_hook_store(tag, "hooks:\n  pre-prime:\n    - 'cat > hook-stdin.txt'\n");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        let our_stdin = hook_stdin(&ours.0);
        assert!(our_stdin.is_some(), "{tag}: our hook fired");
        // The payload is trust-sorted: dev's foundational leads even
        // though the corpus file order puts it first-of-three with
        // the convention and decision behind it, and tooling rides
        // second.
        let stdin = our_stdin.unwrap_or_default();
        assert!(
            stdin.starts_with(
                "{\"event\":\"pre-prime\",\"payload\":{\"domains\":[{\"domain\":\"dev\",\"records\":["
            ),
            "{tag}: {stdin}"
        );
        let payload: serde_json::Value = serde_json::from_str(&stdin).expect("hook stdin parses");
        let domains = payload["payload"]["domains"]
            .as_array()
            .expect("domains array");
        assert_eq!(domains[0]["domain"], "dev", "{tag}");
        assert_eq!(domains[1]["domain"], "tooling", "{tag}");
        let dev_records = domains[0]["records"].as_array().expect("records");
        assert_eq!(
            dev_records[0]["classification"], "foundational",
            "{tag}: trust-sorted, foundational first"
        );
    }
}

/// The gate law: manifest mode and the dry-run preview (including its
/// export) fire NO hooks; a real-path --export fires them before the
/// file is written.
#[test]
fn prime_pre_prime_gates() {
    for (tag, args) in [
        ("ph-gate-manifest", &["prime", "--manifest"][..]),
        ("ph-gate-dryrun", &["prime", "--all", "--dry-run"][..]),
        (
            "ph-gate-dryexport",
            &["prime", "--all", "--dry-run", "--export", "out.md"][..],
        ),
    ] {
        let ours = prime_hook_store(tag, "hooks:\n  pre-prime:\n    - 'cat > hook-stdin.txt'\n");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        assert!(hook_stdin(&ours.0).is_none(), "{tag}: our hook stayed cold");
    }

    // A real-path export runs the hook first, then writes the file.
    let ours = prime_hook_store(
        "ph-gate-export",
        "hooks:\n  pre-prime:\n    - 'cat > hook-stdin.txt'\n",
    );
    let console = run_in(&ours.0, Path::new(mulch_bin()), &["prime", "--all"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "prime", "--all", "--export", "out.md",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        hook_stdin(&ours.0).is_some(),
        "our hook fired before export"
    );
    assert_eq!(
        std::fs::read_to_string(ours.0.join("out.md")).expect("our export"),
        console.stdout,
        "export bytes equal the console output"
    );
}

/// Blocking: a non-zero exit (and a timeout under a short
/// `hook_settings.timeout_ms`) aborts before any output — plain
/// `Error: …` on stderr, the json failure envelope in json mode,
/// exit 1.
#[test]
fn prime_pre_prime_blocking_pins_the_bytes() {
    for (tag, args, hook_yaml, json_mode, reason) in [
        (
            "ph-block-plain",
            &["prime", "--all"][..],
            "hooks:\n  pre-prime:\n    - 'exit 4'\n",
            false,
            "hook `exit 4` exited with code 4",
        ),
        (
            "ph-block-json",
            &["--json", "prime", "--all"][..],
            "hooks:\n  pre-prime:\n    - 'exit 4'\n",
            true,
            "hook `exit 4` exited with code 4",
        ),
        (
            "ph-block-timeout",
            &["prime", "--all"][..],
            "hooks:\n  pre-prime:\n    - 'sleep 30 & wait'\nhook_settings:\n  \
             timeout_ms: 150\n",
            false,
            "hook `sleep 30 & wait` timed out after 150ms",
        ),
        (
            "ph-block-timeout-json",
            &["--json", "prime", "--all"][..],
            "hooks:\n  pre-prime:\n    - 'sleep 30 & wait'\nhook_settings:\n  \
             timeout_ms: 150\n",
            true,
            "hook `sleep 30 & wait` timed out after 150ms",
        ),
    ] {
        let ours = prime_hook_store(tag, hook_yaml);
        let started = std::time::Instant::now();
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let our_elapsed = started.elapsed();
        assert_eq!(our.code, 1, "{tag}: blocked exits 1");
        assert!(our.stdout.is_empty(), "{tag}: no stdout when blocked");
        let expected = if json_mode {
            error_envelope("prime", reason)
        } else {
            format!("Error: {reason}\n")
        };
        assert_eq!(our.stderr, expected, "{tag}: our stderr");
        if tag.contains("timeout") {
            assert!(
                our_elapsed.as_secs() < 10,
                "{tag}: group kill returned promptly"
            );
        }
    }
}

/// A hook's stderr forwards verbatim (invocation order, trailing
/// newline backfilled) in BOTH modes; warnings print plain-only —
/// `--quiet` still warns, `--json` never does. The non-JSON parse
/// warning's reason tail carries the parser's wording (documented
/// deviation class), so the compare masks it.
#[test]
fn prime_hook_stderr_forward_and_warnings_law() {
    // Successful hook writing stderr: forwarded, run still succeeds.
    for (tag, args) in [
        ("ph-fwd-plain", &["prime", "--all"][..]),
        ("ph-fwd-json", &["--json", "prime", "--all"][..]),
    ] {
        let ours = prime_hook_store(tag, "hooks:\n  pre-prime:\n    - 'printf side-noise >&2'\n");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        assert_eq!(our.stderr, "side-noise\n", "{tag}: forwarded verbatim");
    }

    // Non-JSON stdout on a mutable event: one Warning line, plain
    // only (quiet included, json excluded).
    let warning_head = |stderr: &str| -> String {
        stderr
            .lines()
            .filter(|line| line.starts_with("Warning: hook `"))
            .map(|line| {
                format!(
                    "{}<PARSE-ERROR>",
                    line.split("(mutation ignored): ")
                        .next()
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    for (tag, args, warns) in [
        ("ph-warn-plain", &["prime", "--all"][..], true),
        ("ph-warn-quiet", &["prime", "--all", "--quiet"][..], true),
        ("ph-warn-json", &["--json", "prime", "--all"][..], false),
    ] {
        let ours = prime_hook_store(tag, "hooks:\n  pre-prime:\n    - 'printf %s not-json'\n");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        let warned = our
            .stderr
            .lines()
            .any(|line| line.starts_with("Warning: hook `"));
        assert_eq!(warned, warns, "{tag}: our warning presence");
        if warns {
            let head = warning_head(&our.stderr);
            assert_eq!(
                head, "Warning: hook `printf %s not-json` printed non-JSON on stdout <PARSE-ERROR>",
                "{tag}: warning line (parse tail masked)"
            );
        } else {
            assert!(our.stderr.is_empty(), "{tag}: json emits no warnings");
        }
    }
}

/// Mutation: a hook's domains JSON replaces the loaded records
/// per-domain — an UNKNOWN domain is ignored (never surface data the
/// user did not ask for), the surviving records RE-SORT by trust, and
/// untouched domains keep their own records.
#[test]
fn prime_hook_mutation_law() {
    let mutation_json = r#"{"domains":[
        {"domain":"dev","records":[
            {"type":"pattern","classification":"observational","recorded_at":"2026-10-18T10:00:00.000Z","name":"Mut Obs","description":"mutated obs","id":"mx-300001"},
            {"type":"pattern","classification":"foundational","recorded_at":"2026-10-18T10:00:00.000Z","name":"Mut Fnd","description":"mutated fnd","id":"mx-300002"}
        ]},
        {"domain":"ghost","records":[
            {"type":"pattern","classification":"foundational","recorded_at":"2026-10-18T10:00:00.000Z","name":"Ghost","description":"never surfaced","id":"mx-300003"}
        ]}
    ]}"#;
    let script = format!("printf %s \"{}\"", mutation_json.replace('"', "\\\""));
    let hook_yaml = format!("hooks:\n  pre-prime:\n    - '{script}'\n");
    for (tag, args) in [
        ("ph-mut-plain", &["prime", "--all"][..]),
        ("ph-mut-json", &["--json", "prime", "--all"][..]),
    ] {
        let ours = prime_hook_store(tag, hook_yaml.as_str());
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        assert!(
            our.stderr.is_empty(),
            "{tag}: our stderr clean, got: {:?}",
            our.stderr
        );
        // The mutation took: dev renders the two mutated records,
        // trust-re-sorted (Fnd before the hook's first-listed Obs);
        // the ghost never surfaces; tooling keeps its own records.
        let fnd = our
            .stdout
            .find("Mut Fnd")
            .expect("mutated foundational renders");
        let obs = our
            .stdout
            .find("Mut Obs")
            .expect("mutated observational renders");
        assert!(fnd < obs, "{tag}: re-sort puts foundational first");
        assert!(
            our.stdout.contains("Anchored dir"),
            "{tag}: untouched domain kept"
        );
        assert!(
            !our.stdout.contains("files match"),
            "{tag}: dev originals replaced"
        );
        assert!(
            !our.stdout.contains("never surfaced"),
            "{tag}: unknown domain ignored"
        );
    }
}
