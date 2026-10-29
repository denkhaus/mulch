//! `mulch doctor` (+`--fix`) probes — OUR byte pins (ADR-0008: the
//! differential battery is dissolved; our bytes are the regression
//! memory). Sprint banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn doctor_clean_store_pins_our_report_frame() {
    let ours = TempDir::new("doc-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p1",
        "--description",
        "dp1",
    ]);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    assert!(
        ours_run.stdout.starts_with("Mulch Doctor\n"),
        "header: {}",
        ours_run.stdout
    );
    // the clean-store frame: every check passes
    for line in [
        "  ✓ All records pass schema validation",
        "  ✓ No stale records",
        "  ✓ No duplicates",
        "  ✓ All file anchors resolve to existing paths",
        "  ✓ All domains within governance limits",
    ] {
        assert!(
            ours_run.stdout.contains(&format!("{line}\n")),
            "{line} missing: {}",
            ours_run.stdout
        );
    }
    assert!(
        ours_run
            .stdout
            .ends_with("17 passed, 0 warning(s), 0 failed\n"),
        "summary: {}",
        ours_run.stdout
    );

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    assert_eq!(ours_json.code, 0, "{}", ours_json.stderr);
    let report: serde_json::Value =
        serde_json::from_str(&ours_json.stdout).expect("doctor json parses");
    assert_eq!(report["success"], serde_json::json!(true));
    let checks = report["checks"].as_array().expect("checks array");
    assert!(
        checks
            .iter()
            .all(|check| check["status"] == serde_json::json!("pass")),
        "clean store: every check passes: {checks:?}"
    );
}

#[test]
fn doctor_fix_prunes_stale_record() {
    let ours = TempDir::new("fix-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "convention",
        "--content",
        "old",
    ]);
    // age the record beyond the tactical shelf life
    let file = ours.0.join(".mulch/expertise/dev.jsonl");
    let aged = std::fs::read_to_string(&file)
        .expect("domain file")
        .replace(
            // fresh timestamp -> 40 days ago (fixed instant)
            &recent_timestamp(&file),
            "2026-08-20T10:00:00.000Z",
        );
    std::fs::write(&file, aged).expect("age record");

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    assert!(ours_run.stdout.contains("Fixed:"));
    assert!(
        ours_run
            .stdout
            .contains("Pruned 1 stale record(s) from dev")
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), "");
    assert!(ours.0.join(".mulch/expertise/dev.jsonl").is_file());
}

#[test]
fn doctor_duplicates_warn_pins_our_lines() {
    let ours = TempDir::new("dup-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p1",
        "--description",
        "dp1",
    ]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "rust",
        "--type",
        "guide",
        "--name",
        "g",
        "--description",
        "dg",
    ]);
    // byte-identical repeat (dev: once, rust: twice) plus a distinct
    // record per domain — indexes count PARSED records per domain
    for (domain, kind, repeat) in [("dev", "pattern", 1), ("rust", "guide", 2)] {
        let name = if kind == "pattern" { "p1" } else { "g" };
        let file = ours.0.join(format!(".mulch/expertise/{domain}.jsonl"));
        let first = std::fs::read_to_string(&file).expect("domain file");
        let base = first.trim_end().to_string();
        let distinct = format!(
            "{{\"type\":\"{kind}\",\"name\":\"{name}-other\",\"description\":\"other\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-05T10:00:00.000Z\"}}"
        );
        let mut lines = vec![base.clone()];
        for _ in 0..repeat {
            lines.push(base.clone());
        }
        lines.push(distinct);
        std::fs::write(&file, lines.join("\n") + "\n").expect("duplicate fixture");
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    // the contract itself: redundant records counted per record,
    // 1-based parsed indexes, first prior occurrence as the match
    assert!(ours_run.stdout.contains("! 3 duplicate record(s) found"));
    assert!(
        ours_run
            .stdout
            .contains("dev: duplicate pattern at index 2 (matches #1)")
    );
    assert!(
        ours_run
            .stdout
            .contains("rust: duplicate guide at index 2 (matches #1)")
    );
    assert!(
        ours_run
            .stdout
            .contains("rust: duplicate guide at index 3 (matches #1)")
    );
}

#[test]
fn doctor_fix_keeps_duplicates() {
    let ours = TempDir::new("dupfix-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p1",
        "--description",
        "dp1",
    ]);
    let file = ours.0.join(".mulch/expertise/dev.jsonl");
    let base = std::fs::read_to_string(&file).expect("domain file");
    let repeated = format!("{base}{base}");
    std::fs::write(&file, repeated).expect("byte-identical repeat");

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    // duplicates warn (never fail) and --fix never touches them
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    assert!(!ours_run.stdout.contains("Fixed:"));
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl")
            .lines()
            .count(),
        2
    );
}

/// Our documented extension beyond the retired reference (README
/// DEVIATIONS, historical): a repeated id with divergent content
/// fails OUR report.
#[test]
fn doctor_flags_divergent_ids() {
    let ours = TempDir::new("div-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    // register dev via a real record (auto-creates the domain), then
    // overwrite the file: same id, different content — no dedup-field
    // match between them
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p1",
        "--description",
        "dp1",
    ]);
    let a = "{\"type\":\"failure\",\"description\":\"d1\",\"resolution\":\"r1\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-05T10:00:00.000Z\",\"id\":\"mx-ae4f85\"}";
    let b = "{\"type\":\"failure\",\"description\":\"d2\",\"resolution\":\"r2\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-05T11:00:00.000Z\",\"id\":\"mx-ae4f85\"}";
    std::fs::write(
        ours.0.join(".mulch/expertise/dev.jsonl"),
        format!("{a}\n{b}\n"),
    )
    .expect("divergent fixture");

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    // ours reports the divergence as a fail
    assert_eq!(ours_run.code, 1);
    assert!(ours_run.stdout.contains("✗ 1 divergent id(s) found"));
    assert!(
        ours_run
            .stdout
            .contains("dev: divergent id mx-ae4f85 at index 2 (matches #1, different content)")
    );

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    let report: serde_json::Value =
        serde_json::from_str(&ours_json.stdout).expect("doctor json parses");
    let duplicates = report["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .find(|check| check["name"] == "duplicates")
        .expect("duplicates check");
    assert_eq!(duplicates["status"], "fail");
    assert_eq!(duplicates["fixable"], false);
}

// ---- sprint 16 (mulch-2dc2): strict outcome-duration parse on all three
// write surfaces (record --outcome-duration, edit --outcome-duration,
// outcome --duration) ----

#[test]
fn doctor_stale_policy_pins_our_lines() {
    let ours = TempDir::new("stale-ours");
    // Timestamps derive from the runtime clock (a fixed calendar date
    // turns the fresh-side fixtures stale as the wall clock moves past
    // their (shelf+1)-day boundary — the boundary itself is unit-pinned
    // in stale.rs). The fresh cases sit a full day inside their shelf,
    // the stale cases a full day past it.
    let now = jiff::Timestamp::now();
    let days_ago = |days: i64| {
        let recorded = now - jiff::Span::new().hours(days * 24);
        recorded.to_string()
    };
    let record = |name: &str, classification: Option<&str>, days: i64| {
        let class =
            classification.map_or(String::new(), |c| format!(",\"classification\":\"{c}\""));
        format!(
            r#"{{"type":"pattern","name":"{name}","description":"d"{class},"recorded_at":"{}"}}"#,
            days_ago(days)
        )
    };
    let store = [
        record("missing-class", None, 40),
        record("tactical-14d", Some("tactical"), 14),
        record("tactical-15d", Some("tactical"), 15),
        record("observ-30d", Some("observational"), 30),
        record("foundational", Some("foundational"), 40),
    ]
    .join("\n")
        + "\n";
    let rust_store = record("rust-stale", Some("tactical"), 35) + "\n";
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "dev"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "rust"]);
    std::fs::create_dir_all(ours.0.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(ours.0.join(".mulch/expertise/dev.jsonl"), &store).expect("dev store");
    std::fs::write(ours.0.join(".mulch/expertise/rust.jsonl"), &rust_store).expect("rust store");

    // the stale check: the missing-classification record never decays
    // (it fails schema instead — hence rc 1)
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    assert_eq!(ours_run.code, 1);
    assert!(ours_run.stdout.contains("! 2 stale record(s) found"));
    assert!(ours_run.stdout.contains("dev: stale pattern (tactical)"));
    assert!(ours_run.stdout.contains("rust: stale pattern (tactical)"));
    assert!(ours_run.stdout.contains("rust: stale pattern (tactical)"));

    // --fix prunes the stale ones and removes the schema-invalid
    // missing-classification record; the report follows the CHECK
    // order (all Removed lines, then all Pruned lines).
    let ours_fix = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    assert_eq!(ours_fix.code, 1);
    assert!(
        ours_fix
            .stdout
            .contains("Removed 1 invalid record(s) from dev")
    );
    assert!(
        ours_fix
            .stdout
            .contains("Pruned 1 stale record(s) from dev")
    );
    assert!(
        ours_fix
            .stdout
            .contains("Pruned 1 stale record(s) from rust")
    );
    // the surviving store keeps the three fresh records
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl")
            .lines()
            .count(),
        3
    );
}

// ---- spec-review round 2: parse layer, schema-invalid strings, no-store
// doctor, --fix --json, quiet matrix, blank-line numbering ----

#[test]
fn doctor_no_store_pins_our_bytes() {
    let ours = TempDir::new("nostore-doc");

    let ours_plain = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    assert_eq!(ours_plain.code, 1);
    assert_eq!(
        ours_plain.stdout,
        "Mulch Doctor\n\n0 passed, 0 warnings, 1 failed\n"
    );
    assert_eq!(ours_plain.stderr, "  ✗ No .mulch/ directory found\n");

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    assert_eq!(ours_json.code, 1);
    assert_eq!(ours_json.stderr, "");
    assert_eq!(
        ours_json.stdout,
        "{\n  \"success\": false,\n  \"command\": \"doctor\",\n  \"checks\": [\n    {\n      \"name\": \"config\",\n      \"status\": \"fail\",\n      \"message\": \"No .mulch/ directory found\",\n      \"fixable\": false,\n      \"details\": []\n    }\n  ],\n  \"summary\": {\n    \"pass\": 0,\n    \"warn\": 0,\n    \"fail\": 1\n  }\n}\n"
    );
}

#[test]
fn doctor_fix_json_mutates_store() {
    let ours = TempDir::new("fixjson-ours");
    let record = "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\"}";
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let file = ours.0.join(".mulch/expertise/dev.jsonl");
    std::fs::write(file, format!("{record}\n")).expect("write record");
    let cfg = ours.0.join(".mulch/mulch.config.yaml");
    let text = std::fs::read_to_string(&cfg).expect("config");
    std::fs::write(cfg, text.replace("domains: {}", "domains:\n  dev: {}"))
        .expect("register domain");

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "doctor", "--fix", "--json",
    ]);
    // findings existed, so the run reports failure even though the
    // fix applied
    assert_eq!(ours_run.code, 1);
    // --json applies the fix: the invalid record is gone.
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), "");
}

// ---- sprint 19 (mulch-d45c): doctor --fix repair alignment ----

#[test]
fn doctor_fix_repair_order_pins_our_lines() {
    let ours = TempDir::new("fix19-ours");
    repair_fixture(&ours.0);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    // repairs applied, but the run still reports the findings: rc 1
    assert_eq!(ours_run.code, 1);
    // the report carries all four repair lines in CHECK order
    let fixed = &ours_run.stdout[ours_run.stdout.find("Fixed:").expect("fixed block")..];
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[1..], [
        "  \u{2713} Removed 1 invalid JSON line(s) from dev",
        "  \u{2713} Migrated 1 legacy \"outcome\" field(s) to \"outcomes[]\" in dev",
        "  \u{2713} Removed 1 invalid record(s) from dev",
        "  \u{2713} Pruned 1 stale record(s) from dev",
    ]);
    // the survivor is the fresh valid record (timestamps mask away)
    let after = mask_timestamps(&read_store_file(&ours.0, "expertise/dev.jsonl"));
    assert!(
        after.contains("\"name\":\"legacy\""),
        "the fresh record survives: {after}"
    );
}

#[test]
fn doctor_fix_keeps_unknown_type_records() {
    let ours = TempDir::new("fix19u-ours");
    let now = jiff::Timestamp::now();
    let fresh_at = (now - jiff::Span::new().hours(24)).to_string();
    let unknown = format!(
        r#"{{"type":"wtf","name":"u1","classification":"tactical","recorded_at":"{fresh_at}","id":"mx-bbbb02"}}"#
    );
    let valid = format!(
        r#"{{"type":"pattern","name":"p1","description":"d","classification":"tactical","recorded_at":"{fresh_at}"}}"#
    );
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "dev"]);
    std::fs::create_dir_all(ours.0.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(
        ours.0.join(".mulch/expertise/dev.jsonl"),
        format!("{valid}\n{unknown}\n"),
    )
    .expect("dev store");

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    // the unknown-type finding keeps the run failing; nothing fixable
    // fires — unknown-type records are flagged, never silently deleted
    assert!(!ours_run.stdout.contains("Fixed:"));
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        format!("{valid}\n{unknown}\n")
    );
}

#[test]
fn doctor_json_fix_envelope_carries_fixed_lines() {
    let ours = TempDir::new("fix19j-ours");
    repair_fixture(&ours.0);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "doctor", "--fix", "--json",
    ]);
    assert_eq!(ours_run.code, 1);
    let our_json: serde_json::Value = serde_json::from_str(&ours_run.stdout).expect("json");
    // the fixed field carries the four repair lines in check order
    let fixed = our_json
        .get("fixed")
        .and_then(|f| f.as_array())
        .expect("fixed field");
    let lines: Vec<&str> = fixed.iter().map(|l| l.as_str().expect("line")).collect();
    assert_eq!(lines, [
        "Removed 1 invalid JSON line(s) from dev",
        "Migrated 1 legacy \"outcome\" field(s) to \"outcomes[]\" in dev",
        "Removed 1 invalid record(s) from dev",
        "Pruned 1 stale record(s) from dev",
    ]);
}

/// DEVIATION PIN (README Deviations, historical): the retired
/// reference CRASHED on malformed lines — our port reports
/// jsonl-integrity findings and repairs; `--fix` removes the line.
#[test]
fn doctor_reports_and_repairs_malformed_lines() {
    let ours = TempDir::new("fix19m-ours");
    let now = jiff::Timestamp::now();
    let fresh_at = (now - jiff::Span::new().hours(24)).to_string();
    let valid = format!(
        r#"{{"type":"pattern","name":"p1","description":"d","classification":"tactical","recorded_at":"{fresh_at}"}}"#
    );
    std::fs::create_dir_all(ours.0.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(
        ours.0.join(".mulch/mulch.config.yaml"),
        "version: '1'\ndomains:\n  dev: {}\n",
    )
    .expect("config");
    std::fs::write(
        ours.0.join(".mulch/expertise/dev.jsonl"),
        format!("{{\"not json\",\n{valid}\n"),
    )
    .expect("dev store");

    let report = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    assert_eq!(report.code, 1);
    assert!(report.stdout.contains("1 invalid JSON line(s) found"));
    assert!(report.stdout.contains("dev:1 - Invalid JSON"));

    let fixed = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    assert!(
        fixed
            .stdout
            .contains("Removed 1 invalid JSON line(s) from dev")
    );
    assert_eq!(
        std::fs::read_to_string(ours.0.join(".mulch/expertise/dev.jsonl")).expect("file"),
        format!("{valid}\n")
    );
}

/// DEVIATION PIN (README Deviations, historical): the retired
/// reference's repair rewrite CRASHED on a kept id-less record of
/// unregistered type — we complete the repair and the writer assigns
/// the deterministic id.
#[test]
fn doctor_fix_survives_id_less_unknown_type_records() {
    let ours = TempDir::new("fix19i-ours");
    let now = jiff::Timestamp::now();
    let stale_at = (now - jiff::Span::new().hours(40 * 24)).to_string();
    let fresh_at = (now - jiff::Span::new().hours(24)).to_string();
    let valid = format!(
        r#"{{"type":"pattern","name":"p1","description":"d","classification":"tactical","recorded_at":"{fresh_at}"}}"#
    );
    let stale = format!(
        r#"{{"type":"pattern","name":"p2","description":"d","classification":"tactical","recorded_at":"{stale_at}"}}"#
    );
    let unknown_no_id = format!(
        r#"{{"type":"wtf","name":"u1","classification":"tactical","recorded_at":"{fresh_at}"}}"#
    );
    std::fs::create_dir_all(ours.0.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(
        ours.0.join(".mulch/mulch.config.yaml"),
        "version: '1'\ndomains:\n  dev: {}\n",
    )
    .expect("config");
    std::fs::write(
        ours.0.join(".mulch/expertise/dev.jsonl"),
        format!("{valid}\n{stale}\n{unknown_no_id}\n"),
    )
    .expect("dev store");

    let fixed = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    assert!(fixed.stdout.contains("Pruned 1 stale record(s) from dev"));
    let after = std::fs::read_to_string(ours.0.join(".mulch/expertise/dev.jsonl")).expect("file");
    // the id-less unknown-type record survives the rewrite WITH its
    // assigned deterministic id (same hash the known types get)
    assert_eq!(after.lines().count(), 2);
    let unknown_line = after
        .lines()
        .find(|l| l.contains("\"type\":\"wtf\""))
        .expect("unknown record survives");
    assert!(unknown_line.contains("\"id\":\"mx-"));
}

// ---- sprint 21 (mulch-53cb): config-owned reference defaults ----

#[test]
fn doctor_fix_on_odd_legacy_outcomes_pins_our_lines() {
    let ours = TempDir::new("odd1609-fix-ours");
    let fresh_at = odd_legacy_fixture(&ours.0);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    // repairs applied, findings still reported: rc 1
    assert_eq!(ours_run.code, 1);
    // seven legacy fields detected, six migrate (null never does); of
    // the migrated lines only the two with a schema-valid entry
    // survive the schema pass (the odd empty entries and the untouched
    // null line drop), each with outcomes appended at line end
    let fixed = &ours_run.stdout[ours_run.stdout.find("Fixed:").expect("fixed block")..];
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[1..], [
        "  \u{2713} Migrated 6 legacy \"outcome\" field(s) to \"outcomes[]\" in probe",
        "  \u{2713} Removed 5 invalid record(s) from probe",
    ]);
    assert_eq!(
        read_store_file(&ours.0, "expertise/probe.jsonl"),
        concat!(
            "{\"type\":\"pattern\",\"name\":\"obj\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"{at}\",\"id\":\"mx-666666\",\"outcomes\":[{\"status\":\"success\",\"agent\":\"a\"}]}\n",
            "{\"type\":\"pattern\",\"name\":\"mid\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"{at}\",\"id\":\"mx-777777\",\"outcomes\":[{\"status\":\"success\"}]}\n",
        )
        .replace("{at}", &fresh_at)
    );
}

// Sprint 22, mulch-0fd8 + absorbed mulch-e645 (lane iterate-s22-c2).
// Pins: (1) the record-locator seam is byte-neutral on the four
// surfaces' existing prologue paths (covered by the existing
// per-command tests); (2) delete checks the domain BEFORE the
// id/mode exclusivity, in plain and json.

#[test]
fn doctor_governance_overruns_never_rewrite_the_store() {
    let ours = TempDir::new("gov-ours");
    let governance = "governance:\n  max_entries: 5\n  warn_entries: 8\n  hard_limit: 12\n";
    let counts = [("alpha", 4), ("beta", 6), ("gamma", 9), ("delta", 12)];
    governance_fixture(&ours.0, governance, &counts);

    let ours_before = read_store_file(&ours.0, "expertise/delta.jsonl");
    // the hard-limit overrun fails the plain report
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    assert_eq!(our.code, 1, "hard limit hit");
    assert!(
        our.stdout.contains("delta: 12 records"),
        "delta's count renders: {}",
        our.stdout
    );
    for args in [
        &["doctor", "--json"][..],
        &["doctor", "--fix"][..],
        &["doctor", "--fix", "--json"][..],
    ] {
        let _ = run_in(&ours.0, Path::new(mulch_bin()), args);
    }
    // --fix never touches counts (fixable: false): the store bytes
    // stay byte-identical to the pre-run snapshot
    assert_eq!(
        read_store_file(&ours.0, "expertise/delta.jsonl"),
        ours_before
    );
    assert_eq!(ours_before.lines().count(), 12);
}

#[test]
fn doctor_governance_warn_only_keeps_exit_zero() {
    let ours = TempDir::new("govwarn-ours");
    let governance = "governance:\n  max_entries: 5\n  warn_entries: 8\n  hard_limit: 12\n";
    let counts = [("beta", 6), ("gamma", 9)];
    governance_fixture(&ours.0, governance, &counts);

    let our = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    // warn tiers never fail the run
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout
            .contains("beta: 6 records (approaching limit of 5)")
    );
    assert!(
        our.stdout
            .contains("gamma: 9 records (over warn threshold of 8)")
    );
}

#[test]
fn doctor_governance_partial_block_backfills_thresholds() {
    let ours = TempDir::new("govpart-ours");
    // only max_entries set: warn/hard backfill to the 150/200 defaults,
    // so 3 records warn "approaching limit of 2"
    let governance = "governance:\n  max_entries: 2\n";
    let counts = [("beta", 3)];
    governance_fixture(&ours.0, governance, &counts);

    let our = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout
            .contains("beta: 3 records (approaching limit of 2)")
    );
}

// mulch-42f2 battery: doctor's orphaned-domains and file-anchors
// checks (both directions, detail order, id rendering) and their
// --fix cases (unvalidated adoption, file creation, anchor stripping).

mod battery_s24 {
    use super::*;

    /// The sprint-24 store: domain `d` live with an id-carrying
    /// anchor record and an id-less one, config domain `ghost`
    /// without a file, orphan file `orphan.jsonl` with its own
    /// anchor record, and one existing anchor target.
    fn orphan_anchor_fixture(dir: &Path) {
        std::fs::write(dir.join("present.txt"), "x").expect("anchor target");
        write_seed_with_config(dir, "version: '1'\ndomains:\n  d: {}\n  ghost: {}\n", &[(
            "d",
            concat!(
                r#"{"type":"pattern","name":"a","description":"d","id":"mx-abc123","files":["present.txt","gone.txt"],"#,
                r#""dir_anchors":["gone-dir"],"evidence":{"commit":"c","file":"gone.md"},"#,
                r#""classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
                "\n",
                r#"{"type":"pattern","name":"b","description":"d","files":["/abs/gone"],"#,
                r#""classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
                "\n",
            ),
        )]);
        write_orphan(
            dir,
            "orphan.jsonl",
            concat!(
                r#"{"type":"pattern","name":"c","description":"d","id":"mx-777","files":["also-gone.txt"],"#,
                r#""classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
                "\n",
            ),
        );
    }

    #[test]
    fn doctor_orphan_and_anchor_checks_pin_our_lines() {
        let ours = TempDir::new("s24-check");
        orphan_anchor_fixture(&ours.0);

        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
        assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
        assert!(ours_run.stdout.contains("! 2 orphaned domain issue(s)"));
        assert!(
            ours_run.stdout.contains(
                "      File \"orphan.jsonl\" exists but domain \"orphan\" is not in config"
            )
        );
        assert!(
            ours_run
                .stdout
                .contains("      Domain \"ghost\" in config but no JSONL file exists")
        );
        assert!(ours_run.stdout.contains("! 4 broken file anchor(s) found"));
        assert!(
            ours_run
                .stdout
                .contains("      d[mx-abc123]: files[] path not found: gone.txt")
        );
        assert!(
            ours_run
                .stdout
                .contains("      d[mx-abc123]: dir_anchors[] path not found: gone-dir")
        );
        assert!(
            ours_run
                .stdout
                .contains("      d[mx-abc123]: evidence.file not found: gone.md")
        );
        assert!(
            ours_run
                .stdout
                .contains("      d: files[] path not found: /abs/gone")
        );
    }

    #[test]
    fn doctor_fix_adopts_orphans_and_strips_anchors() {
        let ours = TempDir::new("s24-fix");
        orphan_anchor_fixture(&ours.0);

        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
        assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
        let fixed = &ours_run.stdout[ours_run.stdout.find("Fixed:").expect("fixed block")..];
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[1..], [
            "  \u{2713} Added orphaned domain \"orphan\" to config",
            "  \u{2713} Created missing JSONL file for domain \"ghost\"",
            "  \u{2713} Removed 4 broken file anchor(s) from d",
            "  \u{2713} Removed 1 broken file anchor(s) from orphan",
        ]);
        // the config gains the adopted domain, in order
        let config = read_store_file(&ours.0, "mulch.config.yaml");
        assert!(
            config.contains("  d: {}\n  ghost: {}\n  orphan: {}\n"),
            "adoption row: {config}"
        );
        // ghost's file exists and is empty; orphan's record survives
        // with its anchors stripped
        assert_eq!(read_store_file(&ours.0, "expertise/ghost.jsonl"), "");
        let orphan = read_store_file(&ours.0, "expertise/orphan.jsonl");
        assert!(
            orphan.contains("\"id\":\"mx-777\""),
            "record kept: {orphan}"
        );
        assert!(
            !orphan.contains("also-gone.txt"),
            "anchors stripped: {orphan}"
        );
        // d's survivor keeps the RESOLVING anchor and its evidence
        // commit, drops the broken ones
        let d = read_store_file(&ours.0, "expertise/d.jsonl");
        assert!(d.contains("\"files\":[\"present.txt\"]"), "kept: {d}");
        assert!(!d.contains("gone.txt"), "stripped: {d}");
        assert!(
            d.contains("\"evidence\":{\"commit\":\"c\"}"),
            "evidence.file dropped, commit kept: {d}"
        );
    }

    /// Several orphan files adopt in the directory's own iteration
    /// order (the `readdir` stream).
    #[test]
    fn doctor_fix_adopts_every_orphan_into_the_config() {
        let ours = TempDir::new("s24-multi");
        write_seed(&ours.0, &[("d", "")]);
        for name in ["zeta", "alpha", "mid"] {
            write_orphan(&ours.0, &format!("{name}.jsonl"), "");
        }

        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
        assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
        let config = read_store_file(&ours.0, "mulch.config.yaml");
        for name in ["zeta", "alpha", "mid"] {
            assert!(
                config.contains(&format!("  {name}: {{}}")),
                "{name} adopted: {config}"
            );
        }
    }

    /// The anchor repair runs AFTER the schema pass: anchors on
    /// schema-invalid records never reach the fix count (the check
    /// still reports them), and the rewrite carries only survivors.
    #[test]
    fn doctor_fix_schema_removal_precedes_anchor_stripping() {
        let ours = TempDir::new("s24-order");
        write_seed(&ours.0, &[(
            "d",
            concat!(
                // schema-invalid: pattern without description
                r#"{"type":"pattern","name":"bad","files":["a-gone.txt","b-gone.txt"],"#,
                r#""classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
                "\n",
                r#"{"type":"pattern","name":"ok","description":"d","files":["c-gone.txt"],"#,
                r#""evidence":{"file":"d-gone.md","commit":"c"},"#,
                r#""classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
                "\n",
            ),
        )]);

        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
        // the check pass counts the invalid record's anchors too
        // (all four: a-gone, b-gone, c-gone, d-gone.md)
        assert!(
            ours_run.stdout.contains("! 4 broken file anchor(s) found"),
            "check pass: {}",
            ours_run.stdout
        );

        let ours_fix = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
        // schema removal and the anchor strip both count as findings
        assert_eq!(ours_fix.code, 1);
        assert!(
            ours_fix
                .stdout
                .contains("Removed 1 invalid record(s) from d"),
            "schema removal: {}",
            ours_fix.stdout
        );
        // only the survivor's two anchors (c-gone.txt, d-gone.md) are
        // stripped — the invalid record's died with it
        assert!(
            ours_fix
                .stdout
                .contains("Removed 2 broken file anchor(s) from d"),
            "anchor strip on survivors only: {}",
            ours_fix.stdout
        );
        let body = read_store_file(&ours.0, "expertise/d.jsonl");
        assert!(!body.contains("a-gone.txt"), "survivors only: {body}");
        assert!(
            body.contains(r#""name":"ok""#),
            "valid record survives: {body}"
        );
        assert!(
            !body.contains("c-gone.txt"),
            "survivor anchors stripped: {body}"
        );
    }
}

// ---- sprint 25 (mulch-cbe2/e2cf/1209/2153/8ca8/5fd8): the record
// flag-path residuals ----

// ---- sprint 36 (mulch-230b): unknown-types doctor surfaces ----

/// The unknown-type store: a string-id unknown record, a schema-valid
/// pattern (full oneOf set — the fixture rule), an id-less unknown
/// record, and an empty-string-id unknown record (the id brackets).
const UNK_TYPES_CORPUS: &str = concat!(
    r#"{"type":"zzzcustom","name":"wtf","id":"mx-bbbb02","content":"x","classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
    '\n',
    r#"{"type":"pattern","name":"ok","description":"d","classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
    '\n',
    r#"{"type":"wtf","content":"noid"}"#,
    '\n',
    r#"{"type":"wtfempty","id":"","content":"x"}"#,
    '\n',
);

#[test]
fn doctor_unknown_types_check_pins_our_lines() {
    let ours = seeded_store("unk230d", None, &[("dev", UNK_TYPES_CORPUS)], &[]);

    // plain: unknown-types fails with the message and details
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    assert_eq!(our.code, 1, "unknown types fail doctor");
    // the law itself: "with unregistered types" wording, typeof-law
    // id brackets (string id, none, empty string -> [])
    assert!(
        our.stdout.contains("✗ 3 record(s) with unregistered types"),
        "message: {}",
        our.stdout
    );
    assert!(
        our.stdout
            .contains("dev:1 [mx-bbbb02] - unknown type \"zzzcustom\""),
        "string id brackets: {}",
        our.stdout
    );
    assert!(
        our.stdout.contains("dev:3 - unknown type \"wtf\""),
        "missing id has no bracket: {}",
        our.stdout
    );
    assert!(
        our.stdout.contains("dev:4 [] - unknown type \"wtfempty\""),
        "empty-string id brackets empty: {}",
        our.stdout
    );
    // unknown-only stores keep schema-validation green
    assert!(
        our.stdout.contains("✓ All records pass schema validation"),
        "schema-validation skips unknown types: {}",
        our.stdout
    );

    // json: the envelope fails the same way
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    assert_eq!(our.code, 1, "json exit");
    let report: serde_json::Value = serde_json::from_str(&our.stdout).expect("doctor json parses");
    assert_eq!(report["success"], serde_json::json!(false));
    assert!(
        report["summary"]["fail"].as_u64().expect("fail count") >= 1,
        "the unknown-types check fails: {}",
        report["summary"]
    );
}

#[test]
fn doctor_ignores_allow_unknown_types_flag() {
    // checkUnknownTypes never consults --allow-unknown-types (the
    // flag is validate's read-through hatch only) — we still exit 1
    // with it.
    let ours = seeded_store("unk230f", None, &[("dev", UNK_TYPES_CORPUS)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "doctor",
        "--allow-unknown-types",
    ]);
    assert_eq!(our.code, 1, "the check ignores the flag");
    assert!(
        our.stdout.contains("✗ 3 record(s) with unregistered types"),
        "the finding stays: {}",
        our.stdout
    );
}
