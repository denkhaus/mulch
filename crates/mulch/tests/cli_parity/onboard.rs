//! Battery for `mulch onboard` (mulch-176f), ADR-0008 edition: OUR
//! byte pins for the --stdout snippet, the create/append/update/
//! migrate/up-to-date write actions with their on-disk bytes, the
//! --check action table, the duplicates warning, quiet, the json
//! envelopes, and the session-close footer styles (all literals
//! frozen from the parity era's twin runs; the reference side is
//! released).
//!
//! The version deviation is now plain ours: the snippet carries this
//! port's version (env!("CARGO_PKG_VERSION") below).

use std::path::Path;

use super::support::{TempDir, mulch_bin, run_in};

/// The session-close section the default (conditional) style emits.
const CONDITIONAL_FOOTER: &str = "### Before You Finish";

/// The snippet's stable head: markers, heading, the version marker
/// line (the only version-bearing bytes).
fn snippet_head() -> String {
    format!(
        "<!-- mulch:start -->\n## Project Expertise (Mulch)\n<!-- mulch-onboard:v{} -->",
        env!("CARGO_PKG_VERSION")
    )
}

/// `onboard --stdout` writes the snippet verbatim (markers included),
/// no trailing newline, immune to --quiet and --json.
#[test]
fn onboard_stdout_pins_the_snippet_bytes() {
    let dir = TempDir::new("ob-stdout");
    let plain = run_in(&dir.0, Path::new(mulch_bin()), &["onboard", "--stdout"]);
    assert_eq!(plain.code, 0);
    assert_eq!(plain.stderr, "");
    assert!(plain.stdout.starts_with(&snippet_head()));
    // no trailing newline on the channel
    assert!(!plain.stdout.ends_with('\n'), "stdout is the raw snippet");
    // the prose names THIS binary throughout
    assert!(plain.stdout.contains("```bash\nmulch prime\n```"));
    assert!(plain.stdout.contains("`mulch prime --files src/foo.ts`"));
    assert!(
        plain
            .stdout
            .contains("`mulch --help` for the full command list")
    );
    assert!(!plain.stdout.contains("`ml "), "no reference spellings");
    // the conditional footer is the default
    assert!(plain.stdout.contains(CONDITIONAL_FOOTER));
    assert!(plain.stdout.ends_with("<!-- mulch:end -->"));
    // --quiet and --json leave stdout alone
    for extra in [["--quiet", "onboard", "--stdout"], [
        "--json", "onboard", "--stdout",
    ]] {
        let variant = run_in(&dir.0, Path::new(mulch_bin()), &extra);
        assert_eq!(
            variant.stdout, plain.stdout,
            "{extra:?} leaves stdout alone"
        );
        assert_eq!(variant.code, 0);
    }
}

/// The create lifecycle: the created message, the CLAUDE.md bytes,
/// the up-to-date rerun, and the --check agreement (frozen bytes).
#[test]
fn onboard_lifecycle_pins_create_and_up_to_date() {
    let dir = TempDir::new("ob-life");
    // fresh create
    let created = run_in(&dir.0, Path::new(mulch_bin()), &["onboard"]);
    assert_eq!(created.code, 0);
    assert_eq!(
        created.stdout,
        "Mulch onboarding snippet written to CLAUDE.md.\n"
    );
    let file = std::fs::read_to_string(dir.0.join("CLAUDE.md")).expect("CLAUDE.md");
    // the file is the snippet with a trailing newline
    assert!(file.starts_with(&snippet_head()));
    assert!(file.ends_with("<!-- mulch:end -->\n"));
    assert!(file.contains("mulch learn"));

    // up-to-date rerun + check + the json envelope
    let rerun = run_in(&dir.0, Path::new(mulch_bin()), &["onboard"]);
    assert_eq!(rerun.code, 0);
    assert_eq!(
        rerun.stdout,
        "Mulch snippet in CLAUDE.md is already up to date. No changes made.\n"
    );
    let check = run_in(&dir.0, Path::new(mulch_bin()), &["onboard", "--check"]);
    assert_eq!(check.code, 0);
    assert_eq!(check.stdout, "Mulch snippet in CLAUDE.md is up to date.\n");
    let check_json = run_in(&dir.0, Path::new(mulch_bin()), &[
        "--json", "onboard", "--check",
    ]);
    assert_eq!(check_json.code, 0);
    assert_eq!(
        check_json.stdout,
        "{\n  \"success\": true,\n  \"command\": \"onboard\",\n  \"file\": \"CLAUDE.md\",\n  \
         \"action\": \"up_to_date\"\n}\n"
    );
    // the plain json run reports the same action
    let json = run_in(&dir.0, Path::new(mulch_bin()), &["--json", "onboard"]);
    assert_eq!(
        json.stdout,
        "{\n  \"success\": true,\n  \"command\": \"onboard\",\n  \"file\": \"CLAUDE.md\",\n  \
         \"action\": \"up_to_date\"\n}\n"
    );
}

/// Target resolution: append to an existing AGENTS.md (message + file
/// bytes), and the FIRST snippet-bearing candidate wins with the
/// inner one reported as a duplicate (quiet silences both).
#[test]
fn onboard_target_resolution_and_duplicates() {
    // append to AGENTS.md (no CLAUDE.md anywhere)
    let dir = TempDir::new("ob-agents");
    std::fs::write(dir.0.join("AGENTS.md"), "Existing notes here.\n").expect("agents");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &["onboard"]);
    assert_eq!(run.code, 0);
    assert_eq!(
        run.stdout,
        "Mulch onboarding snippet appended to AGENTS.md.\n"
    );
    let agents = std::fs::read_to_string(dir.0.join("AGENTS.md")).expect("agents");
    assert!(agents.starts_with("Existing notes here.\n\n<!-- mulch:start -->"));
    assert!(agents.ends_with("<!-- mulch:end -->\n"));

    // snippet in BOTH CLAUDE.md and .claude/CLAUDE.md: the outer file
    // is the target, the inner one is the reported duplicate
    let dir = TempDir::new("ob-dupe");
    std::fs::create_dir_all(dir.0.join(".claude")).expect("claude dir");
    let snippet = run_in(&dir.0, Path::new(mulch_bin()), &["onboard", "--stdout"]).stdout;
    std::fs::write(dir.0.join("CLAUDE.md"), &snippet).expect("outer");
    std::fs::write(dir.0.join(".claude/CLAUDE.md"), &snippet).expect("inner");
    let check = run_in(&dir.0, Path::new(mulch_bin()), &["onboard", "--check"]);
    assert_eq!(check.code, 0);
    assert_eq!(
        check.stdout,
        "Mulch snippet in CLAUDE.md is up to date.\nWarning: mulch snippet also found in: \
         .claude/CLAUDE.md\n"
    );
    // quiet prints NOTHING (message and warning both suppressed)
    let quiet = run_in(&dir.0, Path::new(mulch_bin()), &[
        "--quiet", "onboard", "--check",
    ]);
    assert_eq!(quiet.code, 0);
    assert_eq!(quiet.stdout, "", "quiet silences message + warning");
}

/// The legacy and outdated migrations: an un-marked legacy block
/// (migrated action, header-to-fence splice) and a foreign-version
/// marker (updated action, in-place marker-section replace).
#[test]
fn onboard_legacy_and_outdated_migrations() {
    // legacy block inside a fence: the migration replaces the whole
    // legacy section with the marked snippet, keeping the surrounding
    // content byte-for-byte.
    let dir = TempDir::new("ob-legacy");
    let legacy = "# My project\n\n## Project Expertise (Mulch)\n\nOld-style snippet without \
                  markers.\n\n```bash\nmulch validate && git add .mulch/ && git commit -m \
                  \"mulch: record learnings\"\n```\n\nTrailing content after snippet.\n";
    std::fs::write(dir.0.join("CLAUDE.md"), legacy).expect("legacy");
    let check = run_in(&dir.0, Path::new(mulch_bin()), &["onboard", "--check"]);
    assert_eq!(check.code, 0);
    assert_eq!(
        check.stdout,
        "Mulch snippet in CLAUDE.md uses legacy format (no markers). Run `mulch onboard` to \
         migrate.\n"
    );
    let migrate = run_in(&dir.0, Path::new(mulch_bin()), &["onboard"]);
    assert_eq!(migrate.code, 0);
    assert_eq!(
        migrate.stdout,
        "Mulch onboarding snippet migrated to marker format in CLAUDE.md.\n"
    );
    let file = std::fs::read_to_string(dir.0.join("CLAUDE.md")).expect("migrated");
    assert!(file.starts_with("# My project\n\n<!-- mulch:start -->"));
    assert!(file.ends_with("<!-- mulch:end -->\nTrailing content after snippet.\n"));

    // foreign-version marker: outdated, then updated in place
    let dir = TempDir::new("ob-stale");
    let snippet = run_in(&dir.0, Path::new(mulch_bin()), &["onboard", "--stdout"]).stdout;
    let stale = snippet.replace(
        &format!("<!-- mulch-onboard:v{} -->", env!("CARGO_PKG_VERSION")),
        "<!-- mulch-onboard:v0.0.1 -->",
    );
    std::fs::write(
        dir.0.join("CLAUDE.md"),
        format!("Intro text\n\n{stale}\nMore after\n"),
    )
    .expect("stale");
    let check = run_in(&dir.0, Path::new(mulch_bin()), &["onboard", "--check"]);
    assert_eq!(check.code, 0);
    assert_eq!(
        check.stdout,
        "Mulch snippet in CLAUDE.md is outdated. Run `mulch onboard` to update.\n"
    );
    let update = run_in(&dir.0, Path::new(mulch_bin()), &["onboard"]);
    assert_eq!(update.code, 0);
    assert_eq!(
        update.stdout,
        "Mulch onboarding snippet updated in CLAUDE.md.\n"
    );
    let file = std::fs::read_to_string(dir.0.join("CLAUDE.md")).expect("updated");
    assert!(file.starts_with("Intro text\n\n<!-- mulch:start -->"));
    assert!(file.ends_with("<!-- mulch:end -->\nMore after\n"));
    assert!(file.contains(&format!(
        "<!-- mulch-onboard:v{} -->",
        env!("CARGO_PKG_VERSION")
    )));
}

/// The session-close styles flow from the config into the snippet:
/// none drops the section, minimal is one line, directive is the long
/// block, custom wins verbatim (frozen from our own render).
#[test]
fn onboard_session_close_styles() {
    for (tag, knob, footer) in [
        ("none", "    style: none\n", "<!-- mulch:end -->"),
        (
            "minimal",
            "    style: minimal\n",
            "**Before you finish**: record any insight worth preserving with `mulch learn` → \
             `mulch record <domain> --type <type> …` → `mulch sync`. Skip if no insight \
             surfaced.\n<!-- mulch:end -->",
        ),
        (
            "directive",
            "    style: directive\n",
            // the directive section closes like the conditional one;
            // its numbered list is the differentiating marker
            "Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records \
             are also noise.\n<!-- mulch:end -->",
        ),
    ] {
        let dir = TempDir::new(&format!("ob-sc-{tag}"));
        std::fs::create_dir_all(dir.0.join(".mulch")).expect("store dir");
        std::fs::write(
            dir.0.join(".mulch/mulch.config.yaml"),
            format!("version: '1'\ndomains: {{}}\nprime:\n  session_close:\n{knob}"),
        )
        .expect("config");
        let run = run_in(&dir.0, Path::new(mulch_bin()), &["onboard", "--stdout"]);
        assert_eq!(run.code, 0, "{tag}");
        assert!(run.stdout.ends_with(footer), "{tag}: {}", run.stdout);
        assert!(
            run.stdout.ends_with("<!-- mulch:end -->"),
            "{tag}: the snippet closes at the marker"
        );
        // the head is invariant across styles
        assert!(run.stdout.starts_with(&snippet_head()), "{tag}");
    }

    // custom wins verbatim
    let dir = TempDir::new("ob-sc-custom");
    std::fs::create_dir_all(dir.0.join(".mulch")).expect("store dir");
    std::fs::write(
        dir.0.join(".mulch/mulch.config.yaml"),
        "version: '1'\ndomains: {}\nprime:\n  session_close:\n    custom: close with `ml \
         sync`\n",
    )
    .expect("config");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &["onboard", "--stdout"]);
    assert_eq!(run.code, 0);
    assert!(
        run.stdout
            .ends_with("close with `mulch sync`\n<!-- mulch:end -->")
    );
}
