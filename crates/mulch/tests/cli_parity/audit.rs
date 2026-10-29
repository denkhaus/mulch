//! `mulch audit` probes — OUR byte pins for the no-store and
//! unknown-domain error channels, the JSON report shape, the `--ci`
//! exit law, the human rendering, `--suggest` groups, and the
//! thresholds config override flipping a verdict (ADR-0008: the
//! differential battery is dissolved; our bytes are the regression
//! memory).

use std::path::{Path, PathBuf};

use super::support::*;

/// One mixed store exercising most report paths: four record types,
/// seeds/gh/linear/commit/relates_to evidence, a stale tactical
/// record, a missing seeds id, an unknown-type record (allowed), and
/// a five-record low-high-value domain (weak-domains law). Fixed
/// timestamps keep the report deterministic.
const MIXED_CONFIG: &str = "version: '1'\ndomains:\n  dev: {}\n  ops: {}\n";

fn mixed_dev() -> String {
    [
        r#"{"type":"pattern","classification":"tactical","recorded_at":"2026-10-20T10:00:00.000Z","name":"p1","description":"d","id":"mx-a1"}"#,
        r#"{"type":"convention","classification":"foundational","recorded_at":"2026-10-20T10:00:01.000Z","content":"always do X because Y","id":"mx-b2"}"#,
        r#"{"type":"decision","classification":"tactical","recorded_at":"2026-10-20T10:00:02.000Z","title":"t","rationale":"r","id":"mx-c3","evidence":{"seeds":"mulch-1"}}"#,
        r#"{"type":"failure","classification":"tactical","recorded_at":"2026-10-20T10:00:03.000Z","title":"f","resolution":"r","id":"mx-d4","evidence":{"gh":"42"}}"#,
        r#"{"type":"pattern","classification":"tactical","recorded_at":"2026-09-01T10:00:00.000Z","name":"stale","description":"d","id":"mx-e5"}"#,
        r#"{"type":"reference","classification":"foundational","recorded_at":"2026-10-20T10:00:04.000Z","name":"ref","id":"mx-f6","evidence":{"linear":"LIN-9"}}"#,
        r#"{"type":"pattern","classification":"tactical","recorded_at":"2026-10-20T10:00:05.000Z","name":"rel","description":"d","id":"mx-g7","relates_to":["mx-a1"]}"#,
        r#"{"type":"pattern","classification":"tactical","recorded_at":"2026-10-20T10:00:06.000Z","name":"commit","description":"d","id":"mx-h8","evidence":{"commit":"abc123"}}"#,
        r#"{"type":"zzzcustom","classification":"foundational","recorded_at":"2026-10-20T10:00:07.000Z","name":"odd","id":"mx-i9"}"#,
        r#"{"type":"pattern","classification":"foundational","recorded_at":"2026-10-20T10:00:08.000Z","name":"float1","description":"d","id":"mx-j10","evidence":{"seeds":"mulch-77"}}"#,
    ]
    .join("\n")
        + "\n"
}

fn mixed_ops() -> String {
    // five fresh patterns with no high-value records: the
    // weak-domains law fires (>=5 records, high-value < 20%)
    (0..5)
        .map(|i| {
            format!(
                r#"{{"type":"pattern","classification":"foundational","recorded_at":"2026-10-20T11:00:0{i}.000Z","name":"ops{i}","description":"d","id":"mx-o{i}"}}"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
            + "\n"
}

/// The seeds index: one closed row resolving mulch-1, a comment, a
/// malformed row, and a row without an id (mulch-77 stays missing).
const SEEDS_INDEX: &str = "# banner\n\
             {\"id\":\"mulch-1\",\"status\":\"closed\",\"title\":\"First seed title\"}\n\
             not json\n\
             {\"noid\":1}\n";

/// A fixture store with the fixed directory name `proj` (the report
/// carries `repo` = basename(cwd)) plus the seeds index. The TempDir
/// guard rides along — the returned path lives inside it.
fn proj_store(tag: &str, config: &str) -> (TempDir, PathBuf) {
    let root = TempDir::new(tag);
    let proj = root.0.join("proj");
    write_seed_with_config(&proj, config, &[
        ("dev", &mixed_dev()),
        ("ops", &mixed_ops()),
    ]);
    std::fs::create_dir_all(proj.join(".seeds")).expect("seeds dir");
    std::fs::write(proj.join(".seeds").join("issues.jsonl"), SEEDS_INDEX).expect("seeds index");
    (root, proj)
}

/// The no-store error: plain red stderr line / JSON envelope on
/// stderr under `--json` OR `--ci`, exit 1 (pinned on our bytes).
#[test]
fn audit_no_store_error_matches_in_both_modes() {
    let dir = TempDir::new("audit-nostore");
    let plain = run_in(&dir.0, Path::new(mulch_bin()), &["audit"]);
    assert_eq!(plain.code, 1);
    assert_eq!(plain.stdout, "");
    assert_eq!(
        plain.stderr,
        "No .mulch/ directory found. Run `mulch init` first.\n"
    );
    let json = run_in(&dir.0, Path::new(mulch_bin()), &["--json", "audit"]);
    assert_eq!(json.code, 1);
    assert_eq!(
        json.stderr,
        error_envelope(
            "audit",
            "No .mulch/ directory found. Run `mulch init` first."
        )
    );
    // --ci alone renders the error envelope too (outputJsonError)
    let ci = run_in(&dir.0, Path::new(mulch_bin()), &["audit", "--ci"]);
    assert_eq!(ci.code, 1);
    assert_eq!(ci.stderr, json.stderr);
    assert_eq!(ci.stdout, "");
}

/// The unknown-domain error carries the pinned shape, in plain,
/// json, and ci mode.
#[test]
fn audit_unknown_domain_error_matches() {
    let (_root, ours) = proj_store("audit-unkdomain", MIXED_CONFIG);
    for args in [
        vec!["audit", "--domain", "nope"],
        vec!["--json", "audit", "--domain", "nope"],
        vec!["audit", "--ci", "--domain", "nope"],
    ] {
        let our = run_in(&ours, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1, "{args:?}");
        assert!(our.stdout.is_empty(), "{args:?} stdout");
    }
    assert_eq!(
        run_in(&ours, Path::new(mulch_bin()), &[
            "audit", "--domain", "nope"
        ])
        .stderr,
        "Domain \"nope\" not found in config.\n"
    );
}

/// The full JSON report carries the pinned shape (same directory
/// basename, deterministic fixture).
#[test]
fn audit_json_report_carries_our_pinned_shape() {
    let (_root, ours) = proj_store("audit-json", MIXED_CONFIG);
    let our = run_in(&ours, Path::new(mulch_bin()), &["--json", "audit"]);
    assert_eq!(our.code, 0, "json mode always succeeds");
    let report: serde_json::Value = serde_json::from_str(&our.stdout).expect("json parses");
    assert_eq!(report["success"], serde_json::json!(true));
    assert_eq!(report["report"]["repo"], serde_json::json!("proj"));
    assert_eq!(report["report"]["total_records"], serde_json::json!(15));
    // the deprecated back-compat shapes ride along
    assert_eq!(
        report["report"]["evidence"]["with_seeds"],
        serde_json::json!(2)
    );
    assert_eq!(
        report["report"]["seed_citations"]["missing_in_seeds"],
        serde_json::json!(1)
    );
    // the unknown-type record passed through (allowUnknownTypes)
    assert_eq!(
        report["report"]["by_domain"][0]["type_counts"]["zzzcustom"],
        serde_json::json!(1)
    );
}

/// The `--ci` exit law: failures present → success:false envelope on
/// stdout + exit 1; a clean corpus → success:true + exit 0. `--ci`
/// with `--suggest` appends the suggestions after the report.
#[test]
fn audit_ci_exit_code_law() {
    let (_root, ours) = proj_store("audit-ci", MIXED_CONFIG);
    let our = run_in(&ours, Path::new(mulch_bin()), &["audit", "--ci"]);
    assert_eq!(our.code, 1, "failures present");
    let envelope: serde_json::Value = serde_json::from_str(&our.stdout).expect("json parses");
    assert_eq!(envelope["success"], serde_json::json!(false));
    assert_eq!(
        envelope["report"]["failures"],
        serde_json::json!(["floater_rate"])
    );
    // --ci --suggest carries the suggestions field
    let our_suggest = run_in(&ours, Path::new(mulch_bin()), &[
        "audit",
        "--ci",
        "--suggest",
    ]);
    assert_eq!(our_suggest.code, 1);
    let with_suggest: serde_json::Value =
        serde_json::from_str(&our_suggest.stdout).expect("json parses");
    assert!(with_suggest["suggestions"]["groups"].is_array());

    // a clean corpus: every record evidenced, no floaters, no stale
    let clean_config = "version: '1'\ndomains:\n  dev: {}\n";
    let clean_line = r#"{"type":"decision","classification":"foundational","recorded_at":"2026-10-20T10:00:00.000Z","title":"t","rationale":"r","id":"mx-c1","evidence":{"seeds":"mulch-1"}}"#;
    let clean_root = TempDir::new("audit-ci-clean");
    let clean = clean_root.0.join("proj");
    write_seed_with_config(&clean, clean_config, &[("dev", clean_line)]);
    std::fs::create_dir_all(clean.join(".seeds")).expect("seeds dir");
    std::fs::write(clean.join(".seeds").join("issues.jsonl"), SEEDS_INDEX).expect("index");
    let our_clean = run_in(&clean, Path::new(mulch_bin()), &["audit", "--ci"]);
    assert_eq!(our_clean.code, 0, "clean corpus passes");
    let clean_envelope: serde_json::Value =
        serde_json::from_str(&our_clean.stdout).expect("json parses");
    assert_eq!(clean_envelope["success"], serde_json::json!(true));
}

/// The human rendering (piped, chalk-less): the full report with the
/// per-domain table, tracker citations, domain age, summary signals,
/// and the weak-domains JSON.stringify line — pinned frame.
#[test]
fn audit_human_output_carries_our_pinned_frame() {
    let (_root, ours) = proj_store("audit-human", MIXED_CONFIG);
    let our = run_in(&ours, Path::new(mulch_bin()), &["audit"]);
    assert_eq!(our.code, 0);
    // our side never emits ANSI
    assert!(!our.stdout.contains('\u{1b}'), "no ANSI escapes");
    // the pinned frame: header, weak-domains line, final verdicts
    assert!(our.stdout.starts_with("=== mulch-audit: proj ===\n"));
    assert!(
        our.stdout
            .contains("weak-domains (≥5 records, high-value <20%): 1  -> [\"ops\"]")
    );
    assert!(our.stdout.contains("✗ FAIL: floater_rate"));
    assert!(our.stdout.contains("not-found-in-seeds.jsonl: 1"));
}

/// The `--suggest` block: header after a blank line, the three
/// groups with record-id previews and the copy-paste commands naming
/// our binary.
#[test]
fn audit_suggest_block_carries_our_pinned_lines() {
    let (_root, ours) = proj_store("audit-suggest", MIXED_CONFIG);
    let our = run_in(&ours, Path::new(mulch_bin()), &["audit", "--suggest"]);
    assert_eq!(our.code, 0);
    let block = our
        .stdout
        .split("=== suggested remediations ===")
        .nth(1)
        .expect("the suggest header rides after the report");
    assert!(block.contains("[ARCHIVE] 1 stale records past shelf life"));
    assert!(block.contains("mx-e5"));
    assert!(block.contains("[ATTRIBUTE] 9 records have no evidence or relates_to link"));
    // the copy-paste commands name our binary
    assert!(block.contains("mulch prune --ids mx-e5 --dry-run"));
    assert!(block.contains("mulch edit mx-e5 --evidence-seeds <id>"));
}

/// A thresholds override flips a verdict: floater_max 0.9 turns the
/// FAIL band PASS, and a per-domain max_stale silences the stale
/// WARN — the report carries the merged thresholds.
#[test]
fn audit_thresholds_override_flips_the_floater_verdict() {
    let config = "version: '1'\ndomains:\n  dev: {}\n  ops: {}\naudit:\n  thresholds:\n    floater_max: 0.9\n  per_domain:\n    dev:\n      max_stale: 5\n";
    let (_root, ours) = proj_store("audit-thresholds", config);
    let our = run_in(&ours, Path::new(mulch_bin()), &["--json", "audit"]);
    assert_eq!(our.code, 0);
    let report: serde_json::Value = serde_json::from_str(&our.stdout).expect("json parses");
    assert_eq!(
        report["report"]["thresholds"]["floater_max"],
        serde_json::json!(0.9)
    );
    assert_eq!(
        report["report"]["signals"]["floater_rate"]["verdict"],
        serde_json::json!("PASS")
    );
    assert_eq!(report["report"]["failures"], serde_json::json!([]));
    // the per-domain override reaches per_domain_thresholds and the
    // stale WARN is gone (only evidence_coverage may warn)
    assert_eq!(
        report["report"]["per_domain_thresholds"]["dev"]["max_stale"],
        serde_json::json!(5)
    );
    assert_ne!(
        report["report"]["warnings"],
        serde_json::json!(["evidence_coverage", "max_stale[dev]"])
    );
    assert!(
        !report["report"]["warnings"]
            .as_array()
            .expect("warnings array")
            .iter()
            .any(|warning| warning == "max_stale[dev]"),
        "the override silenced the stale warn: {}",
        report["report"]["warnings"]
    );
    // the default band still fails (the override did the flip)
    let (_root_d, ours_default) = proj_store("audit-thresholds-default", MIXED_CONFIG);
    let our_default = run_in(&ours_default, Path::new(mulch_bin()), &["--json", "audit"]);
    let default_report: serde_json::Value =
        serde_json::from_str(&our_default.stdout).expect("json parses");
    assert_eq!(
        default_report["report"]["signals"]["floater_rate"]["verdict"],
        serde_json::json!("FAIL")
    );
}

/// `--ignore-domains` merges with the config's audit.ignore_domains
/// (comma split, trim, drop empties).
#[test]
fn audit_ignore_domains_flag_and_config_merge() {
    let config =
        "version: '1'\ndomains:\n  dev: {}\n  ops: {}\naudit:\n  ignore_domains:\n    - ghost\n";
    let (_root, ours) = proj_store("audit-ignore", config);
    let our = run_in(&ours, Path::new(mulch_bin()), &[
        "--json",
        "audit",
        "--ignore-domains",
        " ops , ,dev",
    ]);
    let report: serde_json::Value = serde_json::from_str(&our.stdout).expect("json parses");
    assert_eq!(
        report["report"]["ignored_domains"],
        serde_json::json!(["ghost", "ops", "dev"])
    );
    assert_eq!(report["report"]["domains"], serde_json::json!([]));
    assert_eq!(report["report"]["total_records"], serde_json::json!(0));
    // --domain scopes despite ignores
    let scoped = run_in(&ours, Path::new(mulch_bin()), &[
        "--json", "audit", "--domain", "ops",
    ]);
    let report: serde_json::Value = serde_json::from_str(&scoped.stdout).expect("json parses");
    assert_eq!(report["report"]["domains"], serde_json::json!(["ops"]));
}
