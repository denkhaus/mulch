//! Battery for `mulch config` (mulch-176f) — OUR byte pins
//! (ADR-0008: the differential battery is dissolved; our bytes are
//! the regression memory): schema bytes, effective show + --path
//! fallbacks, the set/unset write cycle (silent success, byte-exact
//! rewrites, idempotent unset), and the gate wordings. Probed live
//! 2026-10-26 (/tmp/opencode/sprint54A), frozen at ADR-0008.
//!
//! Deviation families in play (README DEVIATIONS, historical): the
//! store-wide writer lock and list-valued knob rewrites carry
//! serde_yaml's 4-space sequence indent — asserted ungated on our
//! side.

use std::path::Path;

use super::support::{TempDir, mulch_bin, normalize_dir, read_store_file, run_in};

/// A store via our own `init` (the canonical template — the show/set
/// surfaces build on it).
fn init_store(tag: &str) -> TempDir {
    let ours = TempDir::new(tag);
    run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    ours
}

/// `schema` prints the pinned fixture (23144 bytes, probed).
#[test]
fn config_schema_prints_the_pinned_fixture() {
    let ours = init_store("cfg-schema");
    for args in [
        vec!["config", "schema"],
        vec!["--json", "config", "schema"],
        vec!["-q", "config", "schema"],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?}: exit");
        let expected = include_str!("../../fixtures/config/schema.json");
        assert_eq!(our.stdout, expected, "{args:?}: the schema bytes");
        assert_eq!(our.stderr, "", "{args:?}: silent");
    }
}

/// `show` over the fresh-init template (the effective defaults) and
/// the --path surface: a present knob, the schema-default fallback,
/// the empty-path rejection, and the not-found wording.
#[test]
fn config_show_and_path_pin_our_bytes() {
    let ours = init_store("cfg-show");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["config", "show"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    // show renders the effective config as pretty JSON
    assert_eq!(
        our.stdout,
        "{\n  \"version\": \"1\",\n  \"domains\": {},\n  \"governance\": {\n    \"max_entries\": 100,\n    \"warn_entries\": 150,\n    \"hard_limit\": 200\n  },\n  \"classification_defaults\": {\n    \"shelf_life\": {\n      \"tactical\": 14,\n      \"observational\": 30\n    }\n  }\n}\n"
    );
    // the schema-default fallback for a leaf knob
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config",
        "show",
        "--path",
        "search.boost_factor",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "0.1\n");
    // a nested object with default-less children drops to {}
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config", "show", "--path", "search",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "{\n  \"boost_factor\": 0.1\n}\n");
    // the empty-path rejection
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config", "show", "--path", "",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, "--path must not be empty.\n");
    // the not-found wording
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config", "show", "--path", "bogus",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Path 'bogus' not found in config and has no schema default.\n"
    );
}

/// The set/unset write cycle: silent success, byte-exact rewrites
/// (scalar knobs), append order, show-after-write, prune on unset,
/// and the idempotent no-op (bytes untouched).
#[test]
fn config_set_unset_cycle_pins_our_bytes() {
    let ours = init_store("cfg-cycle");
    // set: silent, exit 0 (comments stripped, defaults backfilled,
    // order preserved).
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config",
        "set",
        "governance.max_entries",
        "5",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(our.stdout, "");
    assert_eq!(our.stderr, "");
    // a second set appends the new key at the end
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config",
        "set",
        "search.boost_factor",
        "0.5",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    // show reflects the write
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config", "show", "--path", "search",
    ]);
    assert_eq!(our.stdout, "{\n  \"boost_factor\": 0.5\n}\n");
    // unset removes the leaf AND prunes the empty parent; --path then
    // falls back to the schema default (set+unset lands in the same
    // state as never-set)
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config",
        "unset",
        "search.boost_factor",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config", "show", "--path", "search",
    ]);
    assert_eq!(our.stderr, "");
    assert_eq!(our.stdout, "{\n  \"boost_factor\": 0.1\n}\n");
    // unset a never-set path: silent no-op, bytes untouched
    let before = read_store_file(&ours.0, "mulch.config.yaml");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config",
        "unset",
        "decay.anchor_validity.threshold",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        before,
        "a no-op unset never rewrites"
    );
    // the final bytes are the probed 12-line document
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        "version: '1'\ndomains: {}\ngovernance:\n  max_entries: 5\n  warn_entries: 150\n  hard_limit: 200\nclassification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n"
    );
}

/// The set/unset gate table (pinned plain-mode bytes on the intact
/// template): the AJV error lines for a type mismatch and
/// required-removal rejection on unset.
#[test]
fn config_set_unset_gates_pin_our_bytes() {
    let ours = init_store("cfg-gates");
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
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Invalid config after unset:\n  - <root>: must have required property 'version'\n"
    );
    // the unknown-path listing names every known root key
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config", "set", "bogus", "5",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Unknown config path: 'bogus' is not a known knob. Known keys at '<root>': version, domains, governance, classification_defaults, prime, search, custom_types, decay, audit, disabled_types, hooks, hook_settings.\n"
    );
}

/// json mode changes NOTHING on any config surface (no envelopes
/// here): a set error prints the bare message on stderr.
#[test]
fn config_json_mode_stays_plain() {
    let ours = init_store("cfg-json");
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
/// before-clause naming our binary.
#[test]
fn config_no_store_gates_pin_our_bytes() {
    let ours = TempDir::new("cfg-nostore");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["config", "show"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        "No .mulch/ directory found. Run `mulch init` to set up this project.\n"
    );
    for (verb, args) in [
        ("set", vec!["config", "set", "governance.max_entries", "5"]),
        ("unset", vec!["config", "unset", "governance.max_entries"]),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1);
        assert_eq!(
            normalize_dir(&our.stderr, &ours.0),
            format!(
                "No .mulch/ directory found. Run `mulch init` to set up this project before \
                 `mulch config {verb}`.\n"
            ),
            "our wording names our binary"
        );
    }
}

/// An unparsable YAML VALUE fails with the shared prefix; the tail is
/// parser vocabulary (serde) and pinned only as a non-empty tail. An
/// unparsable config FILE fails with the shared wrapper.
#[test]
fn config_set_invalid_yaml_carries_the_prefix() {
    let ours = init_store("cfg-badyaml");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config",
        "set",
        "search.boost_factor",
        "[",
    ]);
    assert_eq!(our.code, 1);
    assert!(
        our.stderr.starts_with("Invalid YAML for <value>: "),
        "our prefix: {}",
        our.stderr
    );
    // An unparsable config FILE fails with the shared wrapper
    // ("Check the YAML syntax." closes it).
    std::fs::write(
        ours.0.join(".mulch").join("mulch.config.yaml"),
        "version: [1\n",
    )
    .expect("bad");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["config", "show"]);
    assert_eq!(our.code, 1);
    assert!(
        normalize_dir(&our.stderr, &ours.0).starts_with("Failed to parse mulch.config.yaml: "),
        "shared wrapper: {}",
        our.stderr
    );
    assert!(
        normalize_dir(&our.stderr, &ours.0).ends_with(". Check the YAML syntax.\n"),
        "shared wrapper close: {}",
        our.stderr
    );
}

/// A list-valued knob set: OUR rewrite carries serde_yaml's sequence
/// indent (the documented config-rewrite deviation — 4 spaces where
/// js-yaml emitted 6); asserted ungated on our bytes.
#[test]
fn config_set_list_knob_is_silent_and_deviation_indented() {
    let ours = init_store("cfg-list");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "config",
        "set",
        "domains.warren.allowed_types",
        "[convention, pattern]",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(our.stdout, "");
    assert_eq!(our.stderr, "");
    // OUR rewrite: serde_yaml's 4-space sequence indent (the README
    // config-rewrite deviation)
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        "version: '1'\ndomains:\n  warren:\n    allowed_types:\n    - convention\n    - pattern\ngovernance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\nclassification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n"
    );
}
