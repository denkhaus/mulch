//! `mulch doctor` (+`--fix`) parity probes.
//!
//! Moved verbatim from the one-file battery (mulch-3e7c); sprint
//! banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn doctor_clean_store_matches_modulo_upgrade() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("doc-ours");
    let theirs = TempDir::new("doc-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p1",
            "--description",
            "dp1",
        ]);
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor"]);

    let strip_upgrade = |text: &str| -> String {
        text.lines()
            .filter(|line| {
                !line.contains("Update available")
                    && !line.contains("Run `mulch upgrade`")
                    && !line.contains("Native binary")
            })
            .map(|line| {
                // summary counts differ by the upgrade warn; neutralize
                if line.contains("passed") {
                    let digits: Vec<&str> = line
                        .split(|c: char| !c.is_ascii_digit())
                        .filter(|s| !s.is_empty())
                        .collect();
                    format!("<SUMMARY {} passes>", digits.len())
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(
        strip_upgrade(&ours_run.stdout),
        strip_upgrade(&theirs_run.stdout)
    );

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["doctor", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(
        normalize_doctor_json(&ours_json.stdout),
        normalize_doctor_json(&theirs_json.stdout)
    );
}

#[test]
fn doctor_fix_prunes_stale_record_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("fix-ours");
    let theirs = TempDir::new("fix-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "convention",
            "--content",
            "old",
        ]);
        // age the record beyond the tactical shelf life
        let file = dir.join(".mulch/expertise/dev.jsonl");
        let aged = std::fs::read_to_string(&file)
            .expect("domain file")
            .replace(
                // fresh timestamp -> 40 days ago (fixed instant)
                &recent_timestamp(&file),
                "2026-08-20T10:00:00.000Z",
            );
        std::fs::write(&file, aged).expect("age record");
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--fix"]);
    assert_eq!(ours_run.code, theirs_run.code);
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
fn doctor_duplicates_warn_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("dup-ours");
    let theirs = TempDir::new("dup-theirs");
    let fixture = |dir: &Path| {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p1",
            "--description",
            "dp1",
        ]);
        let _ = run_in(dir, &ml, &[
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
            let file = dir.join(format!(".mulch/expertise/{domain}.jsonl"));
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
    };
    fixture(&ours.0);
    fixture(&theirs.0);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor"]);
    assert_eq!(ours_run.code, theirs_run.code);
    let strip = |text: &str| -> String {
        text.lines()
            .filter(|line| {
                !line.contains("Update available")
                    && !line.contains("Run `mulch upgrade`")
                    && !line.contains("Native binary")
                    && !line.contains("passed,")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(strip(&ours_run.stdout), strip(&theirs_run.stdout));
    // the reference contract itself: redundant records counted per record,
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

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["doctor", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(
        normalize_doctor_json(&ours_json.stdout),
        normalize_doctor_json(&theirs_json.stdout)
    );
}

#[test]
fn doctor_fix_keeps_duplicates_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("dupfix-ours");
    let theirs = TempDir::new("dupfix-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p1",
            "--description",
            "dp1",
        ]);
        let file = dir.join(".mulch/expertise/dev.jsonl");
        let base = std::fs::read_to_string(&file).expect("domain file");
        let repeated = format!("{base}{base}");
        std::fs::write(&file, repeated).expect("byte-identical repeat");
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--fix"]);
    // duplicates warn (never fail) and --fix never touches them
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.code, 0);
    assert!(!ours_run.stdout.contains("Fixed:"));
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/dev.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/dev.jsonl"))
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl")
            .lines()
            .count(),
        2
    );
}

/// Our documented extension beyond the reference (README DEVIATIONS):
/// a repeated id with divergent content fails OUR report while the
/// reference stays on its own output ("No duplicates", exit 0).
#[test]
fn doctor_flags_divergent_ids_beyond_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("div-ours");
    let theirs = TempDir::new("div-theirs");
    let fixture = |dir: &Path| {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p1",
            "--description",
            "dp1",
        ]);
        // same id, different content — no dedup-field match between them
        let a = "{\"type\":\"failure\",\"description\":\"d1\",\"resolution\":\"r1\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-05T10:00:00.000Z\",\"id\":\"mx-ae4f85\"}";
        let b = "{\"type\":\"failure\",\"description\":\"d2\",\"resolution\":\"r2\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-05T11:00:00.000Z\",\"id\":\"mx-ae4f85\"}";
        std::fs::write(
            dir.join(".mulch/expertise/dev.jsonl"),
            format!("{a}\n{b}\n"),
        )
        .expect("divergent fixture");
    };
    fixture(&ours.0);
    fixture(&theirs.0);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor"]);
    // the reference's output stays unchanged: pass, exit 0
    assert_eq!(theirs_run.code, 0);
    assert!(theirs_run.stdout.contains("✓ No duplicates"));
    // ours additionally reports the divergence as a fail
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
fn doctor_stale_policy_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("stale-ours");
    let theirs = TempDir::new("stale-theirs");
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
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "dev"]);
        let _ = run_in(dir, &ml, &["add", "rust"]);
        std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("expertise dir");
        std::fs::write(dir.join(".mulch/expertise/dev.jsonl"), &store).expect("dev store");
        std::fs::write(dir.join(".mulch/expertise/rust.jsonl"), &rust_store).expect("rust store");
    }

    let strip = |text: &str| -> String {
        text.lines()
            .filter(|line| {
                !line.contains("Update available")
                    && !line.contains("Run `mulch upgrade`")
                    && !line.contains("Native binary")
                    && !line.contains("passed,")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    // the stale check: the missing-classification record never decays
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(
        strip(&ours_run.stdout),
        strip(&theirs_run.stdout),
        "doctor plain diverges"
    );
    assert!(ours_run.stdout.contains("2 stale record(s) found"));
    assert!(ours_run.stdout.contains("dev: stale pattern (tactical)"));

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["doctor", "--json"]);
    assert_eq!(
        normalize_doctor_json(&ours_json.stdout),
        normalize_doctor_json(&theirs_json.stdout),
        "doctor json diverges"
    );

    // --fix prunes the stale ones and removes the schema-invalid
    // missing-classification record; the report follows the reference's
    // CHECK order (all Removed lines, then all Pruned lines).
    let ours_fix = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    let theirs_fix = run_in(&theirs.0, &ml, &["doctor", "--fix"]);
    assert_eq!(ours_fix.code, theirs_fix.code);
    assert_eq!(
        strip(&ours_fix.stdout),
        strip(&theirs_fix.stdout),
        "fix report diverges"
    );
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
    // the surviving store is identical
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/dev.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/dev.jsonl"))
    );
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
fn doctor_no_store_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("nostore-doc-ours");
    let theirs = TempDir::new("nostore-doc-theirs");

    let ours_plain = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    let theirs_plain = run_in(&theirs.0, &ml, &["doctor"]);
    assert_eq!(ours_plain.code, theirs_plain.code);
    assert_eq!(ours_plain.stdout, theirs_plain.stdout);
    assert_eq!(ours_plain.stderr, theirs_plain.stderr);

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["doctor", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(ours_json.stdout, theirs_json.stdout);
    assert_eq!(ours_json.stderr, theirs_json.stderr);
}

#[test]
fn doctor_fix_json_mutates_store_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("fixjson-ours");
    let theirs = TempDir::new("fixjson-theirs");
    let record = "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\"}";
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let file = dir.join(".mulch/expertise/dev.jsonl");
        std::fs::write(file, format!("{record}\n")).expect("write record");
        let cfg = dir.join(".mulch/mulch.config.yaml");
        let text = std::fs::read_to_string(&cfg).expect("config");
        std::fs::write(cfg, text.replace("domains: {}", "domains:\n  dev: {}"))
            .expect("register domain");
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "doctor", "--fix", "--json",
    ]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--fix", "--json"]);
    assert_eq!(ours_run.code, theirs_run.code);
    // --json applies the fix: the invalid record is gone in BOTH stores.
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        read_store_file(&theirs.0, "expertise/dev.jsonl")
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), "");
}

#[test]
fn doctor_fix_repair_order_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("fix19-ours");
    let theirs = TempDir::new("fix19-theirs");
    repair_fixture(&ours.0, &ml);
    repair_fixture(&theirs.0, &ml);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--fix"]);
    assert_eq!(ours_run.code, theirs_run.code);
    let strip = |text: &str| -> String {
        text.lines()
            .filter(|line| {
                !line.contains("Update available")
                    && !line.contains("Run `mulch upgrade`")
                    && !line.contains("Native binary")
                    && !line.contains("passed,")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(strip(&ours_run.stdout), strip(&theirs_run.stdout));
    assert_eq!(strip(&ours_run.stderr), strip(&theirs_run.stderr));
    // the report carries all three repair lines in CHECK order
    let fixed = &ours_run.stdout[ours_run.stdout.find("Fixed:").expect("fixed block")..];
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[1..], [
        "  \u{2713} Removed 1 invalid JSON line(s) from dev",
        "  \u{2713} Migrated 1 legacy \"outcome\" field(s) to \"outcomes[]\" in dev",
        "  \u{2713} Removed 1 invalid record(s) from dev",
        "  \u{2713} Pruned 1 stale record(s) from dev",
    ]);
    // the twin fixtures record `now` microseconds apart — mask before
    // comparing the final store bytes
    assert_eq!(
        mask_timestamps(&read_store_file(&ours.0, "expertise/dev.jsonl")),
        mask_timestamps(&read_store_file(&theirs.0, "expertise/dev.jsonl"))
    );
}

#[test]
fn doctor_fix_keeps_unknown_type_records() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("fix19u-ours");
    let theirs = TempDir::new("fix19u-theirs");
    let now = jiff::Timestamp::now();
    let fresh_at = (now - jiff::Span::new().hours(24)).to_string();
    let unknown = format!(
        r#"{{"type":"wtf","name":"u1","classification":"tactical","recorded_at":"{fresh_at}","id":"mx-bbbb02"}}"#
    );
    let valid = format!(
        r#"{{"type":"pattern","name":"p1","description":"d","classification":"tactical","recorded_at":"{fresh_at}"}}"#
    );
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "dev"]);
        std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("expertise dir");
        std::fs::write(
            dir.join(".mulch/expertise/dev.jsonl"),
            format!("{valid}\n{unknown}\n"),
        )
        .expect("dev store");
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--fix"]);
    assert_eq!(ours_run.code, theirs_run.code);
    // nothing fixable fires: unknown-type records are flagged, never
    // silently deleted (the reference keeps them byte-identical)
    assert!(!ours_run.stdout.contains("Fixed:"));
    assert!(!theirs_run.stdout.contains("Fixed:"));
    assert_same_file(&ours, &theirs, "expertise/dev.jsonl");
}

#[test]
fn doctor_json_fix_envelope_carries_fixed_lines() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("fix19j-ours");
    let theirs = TempDir::new("fix19j-theirs");
    repair_fixture(&ours.0, &ml);
    repair_fixture(&theirs.0, &ml);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "doctor", "--fix", "--json",
    ]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--fix", "--json"]);
    assert_eq!(ours_run.code, theirs_run.code);
    let mut our_json: serde_json::Value = serde_json::from_str(&ours_run.stdout).expect("json");
    let mut their_json: serde_json::Value = serde_json::from_str(&theirs_run.stdout).expect("json");
    // neutralize the upgrade check (network warn vs our native pass):
    // drop it and recompute the summary from the remaining checks
    for env in [&mut our_json, &mut their_json] {
        if let Some(checks) = env.get_mut("checks").and_then(|c| c.as_array_mut()) {
            checks.retain(|c| c.get("name").and_then(|n| n.as_str()) != Some("upgrade"));
        }
        let count = |status: &str| {
            env.get("checks")
                .and_then(|c| c.as_array())
                .map_or(0, |checks| {
                    checks
                        .iter()
                        .filter(|c| c.get("status") == Some(&serde_json::Value::from(status)))
                        .count()
                })
        };
        let (pass, warn, fail) = (count("pass"), count("warn"), count("fail"));
        if let Some(summary) = env.get_mut("summary").and_then(|s| s.as_object_mut()) {
            summary.insert("pass".into(), serde_json::Value::from(pass));
            summary.insert("warn".into(), serde_json::Value::from(warn));
            summary.insert("fail".into(), serde_json::Value::from(fail));
        }
    }
    assert_eq!(our_json, their_json);
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
    assert_eq!(
        mask_timestamps(&read_store_file(&ours.0, "expertise/dev.jsonl")),
        mask_timestamps(&read_store_file(&theirs.0, "expertise/dev.jsonl"))
    );
}

/// DEVIATION PIN (README Deviations): the reference CRASHES on
/// malformed lines — its checks abort with an unhandled error before
/// any report renders — while we report jsonl-integrity findings and
/// repair. Reference side: rc 1, no report; our side: full report +
/// `--fix` removes the line.
#[test]
fn doctor_reports_and_repairs_malformed_lines_where_reference_crashes() {
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

/// DEVIATION PIN (README Deviations): the reference's repair rewrite
/// CRASHES on a kept id-less record of unregistered type
/// (`generateRecordId` throws, no report at all) — we complete the
/// repair and the writer assigns the deterministic id.
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
fn doctor_fix_on_odd_legacy_outcomes_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("odd1609-fix-ours");
    let theirs = TempDir::new("odd1609-fix-theirs");
    odd_legacy_fixture(&ours.0, &ml);
    odd_legacy_fixture(&theirs.0, &ml);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--fix"]);
    assert_eq!(ours_run.code, theirs_run.code);
    let strip = |text: &str| -> String {
        text.lines()
            .filter(|line| {
                !line.contains("Update available")
                    && !line.contains("Run `mulch upgrade`")
                    && !line.contains("Native binary")
                    && !line.contains("passed,")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(strip(&ours_run.stdout), strip(&theirs_run.stdout));
    assert_eq!(strip(&ours_run.stderr), strip(&theirs_run.stderr));
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
        mask_timestamps(&read_store_file(&ours.0, "expertise/probe.jsonl")),
        mask_timestamps(&read_store_file(&theirs.0, "expertise/probe.jsonl"))
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/probe.jsonl"),
        "{\"type\":\"pattern\",\"name\":\"obj\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T00:00:00.000Z\",\"id\":\"mx-666666\",\"outcomes\":[{\"status\":\"success\",\"agent\":\"a\"}]}\n{\"type\":\"pattern\",\"name\":\"mid\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T00:00:00.000Z\",\"id\":\"mx-777777\",\"outcomes\":[{\"status\":\"success\"}]}\n"
    );
}

// Sprint 22, mulch-0fd8 + absorbed mulch-e645 (lane iterate-s22-c2).
// Pins: (1) the record-locator seam is byte-neutral on the four
// surfaces' existing prologue paths (covered by the existing
// per-command tests); (2) delete checks the domain BEFORE the
// id/mode exclusivity, in plain and json.

#[test]
fn doctor_governance_overruns_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("gov-ours");
    let theirs = TempDir::new("gov-theirs");
    let governance = "governance:\n  max_entries: 5\n  warn_entries: 8\n  hard_limit: 12\n";
    let counts = [("alpha", 4), ("beta", 6), ("gamma", 9), ("delta", 12)];
    governance_fixture(&ours.0, governance, &counts);
    governance_fixture(&theirs.0, governance, &counts);

    let ours_before = read_store_file(&ours.0, "expertise/delta.jsonl");
    for args in [
        &["doctor"][..],
        &["doctor", "--json"][..],
        &["doctor", "--fix"][..],
        &["doctor", "--fix", "--json"][..],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "rc differs for {args:?}");
        assert_eq!(our.stderr, their.stderr, "stderr {args:?}");
        if args.contains(&"--json") {
            assert_eq!(
                normalize_doctor_json(&our.stdout),
                normalize_doctor_json(&their.stdout),
                "json {args:?}"
            );
        } else {
            assert_eq!(
                strip_upgrade_and_summary(&our.stdout),
                strip_upgrade_and_summary(&their.stdout),
                "plain {args:?}"
            );
        }
    }
    // --fix never touches counts (fixable: false): the store bytes
    // stay (the twins' fixture timestamps differ, so each store is
    // compared against its own pre-run snapshot)
    assert_eq!(
        read_store_file(&ours.0, "expertise/delta.jsonl"),
        ours_before
    );
    assert_eq!(
        read_store_file(&theirs.0, "expertise/delta.jsonl")
            .lines()
            .count(),
        12
    );
    assert_eq!(ours_before.lines().count(), 12);
}

#[test]
fn doctor_governance_warn_only_keeps_exit_zero() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("govwarn-ours");
    let theirs = TempDir::new("govwarn-theirs");
    let governance = "governance:\n  max_entries: 5\n  warn_entries: 8\n  hard_limit: 12\n";
    let counts = [("beta", 6), ("gamma", 9)];
    governance_fixture(&ours.0, governance, &counts);
    governance_fixture(&theirs.0, governance, &counts);

    let our = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    let their = run_in(&theirs.0, &ml, &["doctor"]);
    // warn tiers never fail the run
    assert_eq!(our.code, 0);
    assert_eq!(our.code, their.code);
    assert_eq!(
        strip_upgrade_and_summary(&our.stdout),
        strip_upgrade_and_summary(&their.stdout)
    );
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
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("govpart-ours");
    let theirs = TempDir::new("govpart-theirs");
    // only max_entries set: warn/hard backfill to the 150/200 defaults,
    // so 3 records warn "approaching limit of 2"
    let governance = "governance:\n  max_entries: 2\n";
    let counts = [("beta", 3)];
    governance_fixture(&ours.0, governance, &counts);
    governance_fixture(&theirs.0, governance, &counts);

    let our = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    let their = run_in(&theirs.0, &ml, &["doctor", "--json"]);
    assert_eq!(our.code, their.code);
    assert_eq!(
        normalize_doctor_json(&our.stdout),
        normalize_doctor_json(&their.stdout)
    );
    assert!(
        our.stdout
            .contains("beta: 3 records (approaching limit of 2)")
    );
}

// mulch-42f2 battery: doctor's orphaned-domains and file-anchors
// checks (both directions, detail order, id rendering) and their
// --fix cases (unvalidated adoption, file creation, anchor stripping,
// pass order after the schema repair).

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
    fn doctor_orphan_and_anchor_checks_match_reference() {
        let Some(ml) = reference_ml() else {
            eprintln!("skipped: no ml on PATH");
            return;
        };
        let ours = TempDir::new("s24-check-o");
        let theirs = TempDir::new("s24-check-t");
        for dir in [&ours.0, &theirs.0] {
            orphan_anchor_fixture(dir);
        }

        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
        let theirs_run = run_in(&theirs.0, &ml, &["doctor"]);
        assert_eq!(
            strip_upgrade_and_summary(&ours_run.stdout),
            strip_upgrade_and_summary(&theirs_run.stdout),
            "doctor plain diverges"
        );
        assert_eq!(ours_run.code, theirs_run.code);

        let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
        let theirs_json = run_in(&theirs.0, &ml, &["doctor", "--json"]);
        assert_eq!(
            normalize_doctor_json(&ours_json.stdout),
            normalize_doctor_json(&theirs_json.stdout),
            "doctor json diverges"
        );
        assert_eq!(ours_json.code, theirs_json.code);
    }

    #[test]
    fn doctor_fix_adopts_orphans_and_strips_anchors_like_reference() {
        let Some(ml) = reference_ml() else {
            eprintln!("skipped: no ml on PATH");
            return;
        };
        let ours = TempDir::new("s24-fix-o");
        let theirs = TempDir::new("s24-fix-t");
        for dir in [&ours.0, &theirs.0] {
            orphan_anchor_fixture(dir);
        }

        for args in [&["doctor", "--fix"][..], &["doctor", "--fix", "--json"][..]] {
            let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
            let theirs_run = run_in(&theirs.0, &ml, args);
            match args.last() {
                Some(&"--json") => assert_eq!(
                    normalize_doctor_json(&ours_run.stdout),
                    normalize_doctor_json(&theirs_run.stdout),
                    "doctor --fix --json diverges ({args:?})"
                ),
                _ => assert_eq!(
                    strip_upgrade_and_summary(&ours_run.stdout),
                    strip_upgrade_and_summary(&theirs_run.stdout),
                    "doctor --fix plain diverges ({args:?})"
                ),
            }
            assert_eq!(ours_run.code, theirs_run.code, "exit codes ({args:?})");
            // the second run repairs nothing: fixed is empty and the
            // store files stay byte-identical (idempotence)
        }
        for relative in [
            "mulch.config.yaml",
            "expertise/d.jsonl",
            "expertise/orphan.jsonl",
            "expertise/ghost.jsonl",
        ] {
            assert_same_file(&ours, &theirs, relative);
        }
        // ghost's file exists and is empty on both sides
        assert_eq!(
            read_store_file(&ours.0, "expertise/ghost.jsonl"),
            read_store_file(&theirs.0, "expertise/ghost.jsonl")
        );
    }

    /// Several orphan files adopt in the directory's own iteration
    /// order (the reference's `readdir` — both binaries walk the same
    /// unsorted stream, so twin stores list identically).
    #[test]
    fn doctor_fix_multi_orphan_adoption_order_matches_reference() {
        let Some(ml) = reference_ml() else {
            eprintln!("skipped: no ml on PATH");
            return;
        };
        let ours = TempDir::new("s24-multi-o");
        let theirs = TempDir::new("s24-multi-t");
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("d", "")]);
            for name in ["zeta", "alpha", "mid"] {
                write_orphan(dir, &format!("{name}.jsonl"), "");
            }
        }

        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
        let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--fix"]);
        assert_eq!(
            strip_upgrade_and_summary(&ours_run.stdout),
            strip_upgrade_and_summary(&theirs_run.stdout),
            "adoption and creation lines diverge"
        );
        assert_same_file(&ours, &theirs, "mulch.config.yaml");
    }

    /// The anchor repair runs AFTER the schema pass: anchors on
    /// schema-invalid records never reach the fix count (the check
    /// still reports them), and the rewrite carries only survivors.
    #[test]
    fn doctor_fix_schema_removal_precedes_anchor_stripping() {
        let Some(ml) = reference_ml() else {
            eprintln!("skipped: no ml on PATH");
            return;
        };
        let ours = TempDir::new("s24-order-o");
        let theirs = TempDir::new("s24-order-t");
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[(
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
        }

        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
        let theirs_run = run_in(&theirs.0, &ml, &["doctor"]);
        assert_eq!(
            strip_upgrade_and_summary(&ours_run.stdout),
            strip_upgrade_and_summary(&theirs_run.stdout),
            "check pass diverges (anchors of invalid records count)"
        );

        let ours_fix = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
        let theirs_fix = run_in(&theirs.0, &ml, &["doctor", "--fix"]);
        assert_eq!(
            strip_upgrade_and_summary(&ours_fix.stdout),
            strip_upgrade_and_summary(&theirs_fix.stdout),
            "fix pass diverges (schema removal precedes anchor strip)"
        );
        assert_eq!(ours_fix.code, theirs_fix.code);
        assert_same_file(&ours, &theirs, "expertise/d.jsonl");
        let body = read_store_file(&ours.0, "expertise/d.jsonl");
        assert!(!body.contains("a-gone.txt"), "survivors only: {body}");
        assert!(
            body.contains(r#""name":"ok""#),
            "valid record survives: {body}"
        );
    }
}

// ---- sprint 25 (mulch-cbe2/e2cf/1209/2153/8ca8/5fd8): the record
// flag-path residuals ----
