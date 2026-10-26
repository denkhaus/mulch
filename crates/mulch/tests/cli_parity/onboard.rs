//! Differential battery for `mulch onboard` (mulch-176f): twin dirs,
//! ours vs the reference `ml` 0.10.7 — the --stdout snippet, the
//! create/append/update/migrate/up-to-date write actions with their
//! on-disk bytes, the --check action table, duplicates warning,
//! quiet, json envelopes, and the session-close footer styles.
//! Probed 2026-10-26 (/tmp/opencode/sprint54B/probes/ob*).
//!
//! Two documented deviations flow through every comparison:
//! (a) the promote law — our messages/snippet say `mulch <cmd>`,
//! the reference's say `ml <cmd>` (promoted on their side before
//! comparing; the snippet's `ml --help` needs one extra mapping the
//! word-level law cannot see); (b) the embedded version — our
//! snippet carries this port's version, the reference's carries
//! 0.10.7 (both normalized to `<V>`).

use std::path::Path;

use super::support::{TempDir, mulch_bin, promote_theirs, reference_ml, run_in};

/// The reference's version (for the `<V>` normalization).
fn reference_version(ml: &Path) -> String {
    let run = run_in(&TempDir::new("ob-ver").0, ml, &["-v"]);
    run.stdout.trim().to_string()
}

/// Promotes the reference's snippet-bearing text: the shared law plus
/// the `` `ml --help` `` mention, which `promote_theirs`' word list
/// cannot reach (`--help` is not a command name).
fn promote_their_snippet(text: &str) -> String {
    promote_theirs(text).replace("`ml --help`", "`mulch --help`")
}

/// Normalizes a side's snippet-bearing text: version mentions (the
/// marker and the prose line) become `<V>` on both sides.
fn normalize_versions(text: &str, our_version: &str, their_version: &str) -> String {
    text.replace(&format!("v{our_version}"), "v<V>")
        .replace(&format!("v{their_version}"), "v<V>")
}

/// The twin-runner: runs the same argv in both dirs and asserts
/// channel-split byte equality (theirs promoted + versions
/// normalized).
fn assert_onboard_parity(
    ml: &Path,
    our_version: &str,
    their_version: &str,
    ours: &TempDir,
    theirs: &TempDir,
    args: &[&str],
) {
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, ml, args);
    assert_eq!(our.code, their.code, "{args:?}: exit codes");
    assert_eq!(
        normalize_versions(&our.stdout, our_version, their_version),
        normalize_versions(
            &promote_their_snippet(&their.stdout),
            our_version,
            their_version
        ),
        "{args:?}: stdout (theirs promoted + versions normalized)"
    );
    assert_eq!(our.stderr, their.stderr, "{args:?}: stderr");
}

/// Compares a candidate file's bytes across the twins (promoted +
/// versions normalized); missing files on BOTH sides are equal by
/// absence.
fn assert_same_candidate(
    ours: &TempDir,
    theirs: &TempDir,
    name: &str,
    our_version: &str,
    their_version: &str,
) {
    let our = std::fs::read_to_string(ours.0.join(name)).ok();
    let their = std::fs::read_to_string(theirs.0.join(name)).ok();
    match (&our, &their) {
        (None, None) => {}
        _ => assert_eq!(
            normalize_versions(&our.unwrap_or_default(), our_version, their_version),
            normalize_versions(
                &promote_their_snippet(&their.unwrap_or_default()),
                our_version,
                their_version
            ),
            "{name} bytes"
        ),
    }
}

/// Captures one side's own --stdout snippet (that side's promote
/// spelling and version).
fn own_snippet(dir: &Path, program: &Path) -> String {
    run_in(dir, program, &["onboard", "--stdout"]).stdout
}

/// The snippet and the fresh-create/up-to-date lifecycle: --stdout
/// bytes, the created message + file, the up-to-date rerun.
#[test]
fn onboard_stdout_and_lifecycle_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let our_version = env!("CARGO_PKG_VERSION").to_string();
    let their_version = reference_version(&ml);
    let ours = TempDir::new("ob-life-o");
    let theirs = TempDir::new("ob-life-t");

    // --stdout: no trailing newline, immune to --quiet and --json
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard", "--stdout",
    ]);
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["onboard", "--stdout"]);
    for extra in [["--quiet", "onboard", "--stdout"], [
        "--json", "onboard", "--stdout",
    ]] {
        let variant = run_in(&ours.0, Path::new(mulch_bin()), &extra);
        assert_eq!(
            variant.stdout, plain.stdout,
            "{extra:?} leaves stdout alone"
        );
        assert_eq!(variant.code, 0);
    }

    // fresh create, then up-to-date rerun, then --check agrees
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard",
    ]);
    assert_same_candidate(&ours, &theirs, "CLAUDE.md", &our_version, &their_version);
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard",
    ]);
    assert_same_candidate(&ours, &theirs, "CLAUDE.md", &our_version, &their_version);
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard", "--check",
    ]);
}

/// The json envelopes (file + action — no version bytes to
/// normalize) across the lifecycle.
#[test]
fn onboard_json_envelopes_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let our_version = env!("CARGO_PKG_VERSION").to_string();
    let their_version = reference_version(&ml);
    let ours = TempDir::new("ob-json-o");
    let theirs = TempDir::new("ob-json-t");
    for args in [
        vec!["--json", "onboard", "--check"], // not_installed
        vec!["--json", "onboard"],            // created
        vec!["--json", "onboard"],            // up_to_date
        vec!["--json", "onboard", "--check"], // up_to_date
    ] {
        assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &args);
    }
}

/// Target resolution: append to an existing AGENTS.md, and take the
/// FIRST snippet-bearing candidate as the target with the rest as
/// duplicates.
#[test]
fn onboard_target_resolution_and_duplicates_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let our_version = env!("CARGO_PKG_VERSION").to_string();
    let their_version = reference_version(&ml);

    // append to AGENTS.md (no CLAUDE.md anywhere)
    let ours = TempDir::new("ob-agents-o");
    let theirs = TempDir::new("ob-agents-t");
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("AGENTS.md"), "Existing notes here.\n").expect("agents");
    }
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard",
    ]);
    assert_same_candidate(&ours, &theirs, "AGENTS.md", &our_version, &their_version);

    // snippet in BOTH CLAUDE.md and .claude/CLAUDE.md: target is the
    // first candidate, the inner one is the reported duplicate
    let ours = TempDir::new("ob-dupe-o");
    let theirs = TempDir::new("ob-dupe-t");
    for (dir, program) in [
        (&ours.0, Path::new(mulch_bin()) as &Path),
        (&theirs.0, ml.as_path()),
    ] {
        std::fs::create_dir_all(dir.join(".claude")).expect("claude dir");
        let snippet = own_snippet(dir, program);
        std::fs::write(dir.join("CLAUDE.md"), &snippet).expect("outer");
        std::fs::write(dir.join(".claude/CLAUDE.md"), &snippet).expect("inner");
    }
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard", "--check",
    ]);
    // quiet prints NOTHING (message and warning both suppressed)
    let quiet = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--quiet", "onboard", "--check",
    ]);
    assert_eq!(quiet.code, 0);
    assert_eq!(quiet.stdout, "", "quiet silences message + warning");
}

/// The legacy and outdated migrations: an un-marked legacy block
/// (migrated action, header-to-fence splice) and a foreign-version
/// marker (updated action, in-place marker-section replace).
#[test]
fn onboard_legacy_and_outdated_migrations_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let our_version = env!("CARGO_PKG_VERSION").to_string();
    let their_version = reference_version(&ml);

    // legacy block with the reference tail inside a fence
    let ours = TempDir::new("ob-legacy-o");
    let theirs = TempDir::new("ob-legacy-t");
    let legacy = "# My project\n\n## Project Expertise (Mulch)\n\nOld-style snippet without \
                  markers.\n\n```bash\nmulch validate && git add .mulch/ && git commit -m \
                  \"mulch: record learnings\"\n```\n\nTrailing content after snippet.\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("CLAUDE.md"), legacy).expect("legacy");
    }
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard", "--check",
    ]);
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard",
    ]);
    assert_same_candidate(&ours, &theirs, "CLAUDE.md", &our_version, &their_version);

    // foreign-version marker: outdated, then updated in place
    let ours = TempDir::new("ob-stale-o");
    let theirs = TempDir::new("ob-stale-t");
    let versions: [(&TempDir, &str, &Path); 2] = [
        (&ours, &our_version, Path::new(mulch_bin())),
        (&theirs, &their_version, &ml),
    ];
    for (twin, version, program) in versions {
        let stale = own_snippet(&twin.0, program).replace(
            &format!("<!-- mulch-onboard:v{version} -->"),
            "<!-- mulch-onboard:v0.0.1 -->",
        );
        std::fs::write(
            twin.0.join("CLAUDE.md"),
            format!("Intro text\n\n{stale}\nMore after\n"),
        )
        .expect("stale");
    }
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard", "--check",
    ]);
    assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
        "onboard",
    ]);
    assert_same_candidate(&ours, &theirs, "CLAUDE.md", &our_version, &their_version);
}

/// The session-close footer styles flow from the config into the
/// snippet: none, minimal, directive, custom — full byte parity
/// modulo promote + version.
#[test]
fn onboard_session_close_styles_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let our_version = env!("CARGO_PKG_VERSION").to_string();
    let their_version = reference_version(&ml);
    for (tag, knob) in [
        ("none", "    style: none\n"),
        ("minimal", "    style: minimal\n"),
        ("directive", "    style: directive\n"),
        ("custom", "    custom: close with `ml sync`\n"),
    ] {
        let ours = TempDir::new(&format!("ob-sc-{tag}-o"));
        let theirs = TempDir::new(&format!("ob-sc-{tag}-t"));
        for dir in [&ours.0 as &Path, &theirs.0 as &Path] {
            std::fs::create_dir_all(dir.join(".mulch")).expect("store dir");
            std::fs::write(
                dir.join(".mulch/mulch.config.yaml"),
                format!("version: '1'\ndomains: {{}}\nprime:\n  session_close:\n{knob}"),
            )
            .expect("config");
        }
        assert_onboard_parity(&ml, &our_version, &their_version, &ours, &theirs, &[
            "onboard", "--stdout",
        ]);
    }
}
