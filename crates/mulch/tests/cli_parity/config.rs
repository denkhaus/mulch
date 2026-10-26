//! Differential battery for `mulch config` (mulch-176f): the four
//! subcommands against the reference `ml` — schema bytes, effective
//! show + --path fallbacks, the set/unset write cycle (silent
//! success, byte-identical rewrites, idempotent unset), and every
//! gate wording. Probed live 2026-10-26 (/tmp/opencode/sprint54A).
//!
//! Deviation families in play (README DEVIATIONS): parser-vocabulary
//! tails (serde vs js-yaml error text) compare by PREFIX only; the
//! store-wide writer lock replaces the reference's per-file lockfile
//! (already covered by the lock bullet); list-valued knob rewrites
//! carry serde_yaml's 4-space sequence indent where js-yaml emits 6
//! (the config-rewrite bullet) — asserted ungated on our side.

use std::path::Path;

use super::support::{TempDir, mulch_bin, reference_ml, run_in};

/// Twin stores via each binary's own `init` (the canonical template —
/// the show/set surfaces build on it).
fn twin_init(tag: &str, ml: &Path) -> (TempDir, TempDir) {
    let ours = TempDir::new(format!("{tag}-o").as_str());
    let theirs = TempDir::new(format!("{tag}-t").as_str());
    run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    run_in(&theirs.0, ml, &["init"]);
    (ours, theirs)
}

/// Runs the same config argv in both twins and asserts exit codes and
/// channel-split bytes.
fn assert_config_parity(
    ml: &Path,
    ours: &TempDir,
    theirs: &TempDir,
    args: &[&str],
) -> (super::support::Run, super::support::Run) {
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, ml, args);
    assert_eq!(our.code, their.code, "{args:?} exit code");
    assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
    assert_eq!(
        super::support::normalize_dir(&our.stderr, &ours.0),
        super::support::normalize_dir(&their.stderr, &theirs.0),
        "{args:?} stderr"
    );
    (our, their)
}

/// `schema` prints the pinned fixture — byte-identical to the
/// reference's JSON.stringify dump (23144 bytes, probed).
#[test]
fn config_schema_matches_reference_bytes() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_init("cfg-schema", &ml);
    assert_config_parity(&ml, &ours, &theirs, &["config", "schema"]);
    assert_config_parity(&ml, &ours, &theirs, &["--json", "config", "schema"]);
    assert_config_parity(&ml, &ours, &theirs, &["-q", "config", "schema"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["config", "schema"]);
    let expected = include_str!("../../fixtures/config/schema.json");
    assert_eq!(our.stdout, expected);
}

/// `show` over the fresh-init template (the effective defaults) and
/// the --path surface: a present knob, the schema-default fallbacks
/// (leaf, nested object with default-less children dropped), the
/// empty-path rejection, and the not-found wording.
#[test]
fn config_show_and_path_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_init("cfg-show", &ml);
    assert_config_parity(&ml, &ours, &theirs, &["config", "show"]);
    assert_config_parity(&ml, &ours, &theirs, &["--json", "config", "show"]);
    for path in [
        "governance.max_entries",
        "governance",
        "version",
        "domains",
        "search",
        "search.boost_factor",
        "prime",
        "decay",
        "hook_settings",
        "custom_types",
        "hooks",
        "governance..max_entries",
    ] {
        assert_config_parity(&ml, &ours, &theirs, &["config", "show", "--path", path]);
    }
    let (our, _) = assert_config_parity(&ml, &ours, &theirs, &[
        "config",
        "show",
        "--path",
        "search.boost_factor",
    ]);
    assert_eq!(our.stdout, "0.1\n");

    assert_config_parity(&ml, &ours, &theirs, &["config", "show", "--path", ""]);
    assert_config_parity(&ml, &ours, &theirs, &["config", "show", "--path", "bogus"]);
    assert_config_parity(&ml, &ours, &theirs, &[
        "config",
        "show",
        "--path",
        "governance.bogus",
    ]);
    let (our, _) = assert_config_parity(&ml, &ours, &theirs, &["config", "show", "--path", ""]);
    assert_eq!(our.stderr, "--path must not be empty.\n");
    let (our, _) =
        assert_config_parity(&ml, &ours, &theirs, &["config", "show", "--path", "bogus"]);
    assert_eq!(
        our.stderr,
        "Path 'bogus' not found in config and has no schema default.\n"
    );
}

/// The set/unset write cycle: silent success, byte-identical
/// rewrites (scalar knobs), append order, show-after-write, prune on
/// unset, and the idempotent no-op (bytes untouched).
#[test]
fn config_set_unset_cycle_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_init("cfg-cycle", &ml);
    // set: silent, exit 0, identical rewrites (comments stripped,
    // defaults backfilled, order preserved).
    assert_config_parity(&ml, &ours, &theirs, &[
        "config",
        "set",
        "governance.max_entries",
        "5",
    ]);
    super::support::assert_same_file(&ours, &theirs, "mulch.config.yaml");
    // a second set appends the new key at the end
    assert_config_parity(&ml, &ours, &theirs, &[
        "config",
        "set",
        "search.boost_factor",
        "0.5",
    ]);
    super::support::assert_same_file(&ours, &theirs, "mulch.config.yaml");
    // show reflects the writes
    assert_config_parity(&ml, &ours, &theirs, &["config", "show", "--path", "search"]);
    // unset removes the leaf AND prunes the empty parent; --path then
    // falls back to the schema default (probe-pinned: set+unset lands
    // in the same state as never-set)
    assert_config_parity(&ml, &ours, &theirs, &[
        "config",
        "unset",
        "search.boost_factor",
    ]);
    super::support::assert_same_file(&ours, &theirs, "mulch.config.yaml");
    assert_config_parity(&ml, &ours, &theirs, &["config", "show", "--path", "search"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config", "show", "--path", "search",
    ]);
    assert_eq!(our.stderr, "");
    assert_eq!(our.stdout, "{\n  \"boost_factor\": 0.1\n}\n");
    // unset a never-set path: silent no-op, bytes untouched
    let before = super::support::read_store_file(&ours.0, "mulch.config.yaml");
    assert_config_parity(&ml, &ours, &theirs, &[
        "config",
        "unset",
        "decay.anchor_validity.threshold",
    ]);
    assert_eq!(
        super::support::read_store_file(&ours.0, "mulch.config.yaml"),
        before,
        "a no-op unset never rewrites"
    );
    // the final bytes are the probed 12-line document
    let our_text = super::support::read_store_file(&ours.0, "mulch.config.yaml");
    assert_eq!(
        our_text,
        "version: '1'\ndomains: {}\ngovernance:\n  max_entries: 5\n  warn_entries: 150\n  hard_limit: 200\nclassification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n"
    );
}

/// The set/unset gate table: empty path (before the store gate), the
/// no-store wording with the promoted binary name, the unknown-path
/// listings (root, block, leaf `(none)`, open maps), the AJV error
/// lines (type, enum, minimum, required-at-root, required-in-block,
/// items index), and required-removal rejection on unset.
#[test]
fn config_set_unset_gates_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_init("cfg-gates", &ml);
    // plain-mode pins FIRST, on the intact template: one error each.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config",
        "set",
        "governance.max_entries",
        "abc",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Invalid config after set:\n  - /governance/max_entries: must be integer (Soft target)\n"
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config", "unset", "version",
    ]);
    assert_eq!(
        our.stderr,
        "Invalid config after unset:\n  - <root>: must have required property 'version'\n"
    );
    for args in [
        vec!["config", "set", "", "5"],
        vec!["config", "set", ".", "5"],
        vec!["config", "unset", ""],
        vec!["config", "set", "governance.typo", "5"],
        vec!["config", "set", "bogus", "5"],
        vec!["config", "set", "governance.max_entries.deep", "5"],
        vec!["config", "set", "hooks.bogus", "x"],
        vec!["config", "set", "domains.warren.bogus", "x"],
        vec!["config", "set", "governance.max_entries", "abc"],
        vec!["config", "set", "governance.max_entries", "0"],
        vec!["config", "set", "search.boost_factor", "-1"],
        vec!["config", "set", "prime.default_mode", "bogus"],
        vec!["config", "set", "prime.session_close.style", "bogus"],
        vec!["config", "set", "governance", "5"],
        vec!["config", "set", "domains.warren.allowed_types", "5"],
        vec!["config", "set", "disabled_types", "5"],
        vec!["config", "unset", "version"],
        vec!["config", "unset", "governance"],
        vec!["config", "unset", "governance.max_entries"],
        // a version-less config: the mutation validator demands it
        // back at the root
        vec!["config", "set", "hooks.pre-record", "[echo hi]"],
    ] {
        // hooks.pre-record only validates on a config WITHOUT version:
        // rewrite both twins for that one case first.
        if args[2] == "hooks.pre-record" {
            for dir in [&ours.0, &theirs.0] {
                std::fs::write(
                    dir.join(".mulch").join("mulch.config.yaml"),
                    "domains: {}\n",
                )
                .expect("version-less config");
            }
        }
        assert_config_parity(&ml, &ours, &theirs, &args);
    }
}

/// json mode changes NOTHING on any config surface (the reference
/// never builds envelopes here): a show error and a set error both
/// print the bare message on stderr.
#[test]
fn config_json_mode_stays_plain() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_init("cfg-json", &ml);
    assert_config_parity(&ml, &ours, &theirs, &[
        "--json", "config", "set", "bogus", "5",
    ]);
    assert_config_parity(&ml, &ours, &theirs, &[
        "--json", "config", "show", "--path", "bogus",
    ]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "config", "set", "bogus", "5",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        "Unknown config path: 'bogus' is not a known knob. Known keys at '<root>': version, domains, governance, classification_defaults, prime, search, custom_types, decay, audit, disabled_types, hooks, hook_settings.\n"
    );
}

/// The no-store wordings: show's readConfig message; set/unset's
/// before-clause with the promoted binary name (ours says `mulch
/// config set`; the battery promotes the reference's `ml config set`).
#[test]
fn config_no_store_gates_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("cfg-nostore-o");
    let theirs = TempDir::new("cfg-nostore-t");
    let (our, their) = (
        run_in(&ours.0, Path::new(mulch_bin()), &["config", "show"]),
        run_in(&theirs.0, &ml, &["config", "show"]),
    );
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "No .mulch/ directory found. Run `mulch init` to set up this project.\n"
    );
    assert_eq!(
        super::support::promote_theirs(&their.stderr),
        our.stderr,
        "the reference's wording promotes to ours"
    );
    let cases: [(&str, Vec<&str>); 2] = [
        ("set", vec!["config", "set", "governance.max_entries", "5"]),
        ("unset", vec!["config", "unset", "governance.max_entries"]),
    ];
    for (verb, args) in cases {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code);
        assert_eq!(our.code, 1);
        assert_eq!(
            our.stderr,
            format!(
                "No .mulch/ directory found. Run `mulch init` to set up this project before `mulch config {verb}`.\n"
            ),
            "our promoted wording"
        );
        assert_eq!(super::support::promote_theirs(&their.stderr), our.stderr);
    }
}

/// An unparsable YAML VALUE fails with the shared prefix; the tail is
/// parser vocabulary (serde vs js-yaml) — prefix-only comparison, the
/// documented deviation family.
#[test]
fn config_set_invalid_yaml_carries_the_prefix() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_init("cfg-badyaml", &ml);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config",
        "set",
        "search.boost_factor",
        "[",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "config",
        "set",
        "search.boost_factor",
        "[",
    ]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 1);
    assert!(
        our.stderr.starts_with("Invalid YAML for <value>: "),
        "our prefix: {}",
        our.stderr
    );
    assert!(
        their.stderr.starts_with("Invalid YAML for <value>: "),
        "their prefix: {}",
        their.stderr
    );
    // An unparsable config FILE fails both surfaces with the shared
    // prefix tail ("Check the YAML syntax." closes it either way).
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("mulch.config.yaml"),
            "version: [1\n",
        )
        .expect("bad");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["config", "show"]);
    let their = run_in(&theirs.0, &ml, &["config", "show"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 1);
    for text in [&our.stderr, &their.stderr] {
        assert!(
            text.starts_with("Failed to parse mulch.config.yaml: ")
                && text.ends_with(". Check the YAML syntax.\n"),
            "shared wrapper: {text}"
        );
    }
}

/// A list-valued knob set: OUR rewrite carries serde_yaml's sequence
/// indent (the documented config-rewrite deviation — the reference
/// emits 6 spaces); asserted ungated on our bytes, with exit-parity
/// and silence still compared to the reference.
#[test]
fn config_set_list_knob_is_silent_and_deviation_indented() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_init("cfg-list", &ml);
    let (our, their) = (
        run_in(&ours.0, Path::new(mulch_bin()), &[
            "config",
            "set",
            "domains.warren.allowed_types",
            "[convention, pattern]",
        ]),
        run_in(&theirs.0, &ml, &[
            "config",
            "set",
            "domains.warren.allowed_types",
            "[convention, pattern]",
        ]),
    );
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "");
    assert_eq!(our.stderr, "");
    // OUR rewrite: serde_yaml's 4-space sequence indent (the README
    // config-rewrite deviation; the reference emits 6 — asserted
    // below)
    let our_text = super::support::read_store_file(&ours.0, "mulch.config.yaml");
    assert_eq!(
        our_text,
        "version: '1'\ndomains:\n  warren:\n    allowed_types:\n    - convention\n    - pattern\ngovernance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\nclassification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n"
    );
    // the reference emits the same document with js-yaml's 6-space
    // sequence indent (the README deviation)
    let their_text = super::support::read_store_file(&theirs.0, "mulch.config.yaml");
    assert!(
        their_text.contains("\n      - convention\n      - pattern\n"),
        "reference indent: {their_text}"
    );
}
