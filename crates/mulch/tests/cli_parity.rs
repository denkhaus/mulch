#![allow(clippy::print_stderr, reason = "skip notices print to stderr")]

//! Differential battery: our binary vs the reference `ml` 0.10.7
//! (mulch-da8b). Runs the same commands in twin temp stores and
//! compares exit codes, channel-split output, and on-disk state.
//! Skipped with a note when no `ml` is on PATH.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Our binary under test.
fn mulch_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mulch")
}

/// The reference CLI, when installed.
fn reference_ml() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("ml"))
        .find(|ml| ml.is_file())
}

/// One captured run.
#[derive(Debug)]
struct Run {
    code:   i32,
    stdout: String,
    stderr: String,
}

fn run_in(dir: &Path, program: &Path, args: &[&str]) -> Run {
    let output: Output = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn");
    Run {
        code:   output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mulch-parity-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// Replaces volatile ISO timestamps with a stable marker.
fn normalize(text: &str) -> String {
    regex_free_iso(text)
}

/// ISO-8601 timestamps -> `<TS>` without a regex dependency.
fn regex_free_iso(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'2'
            && i + 24 <= bytes.len()
            && bytes[i + 1] == b'0'
            && looks_like_iso(&text[i..i + 24])
        {
            out.push_str("<TS>");
            i += 24;
        } else {
            out.push(char::from(bytes[i]));
            i += 1;
        }
    }
    out
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` shape check.
fn looks_like_iso(window: &str) -> bool {
    let b = window.as_bytes();
    b[4] == b'-'
        && b[7] == b'-'
        && (b[10] == b'T' || b[10] == b't')
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'.'
        && b[23] == b'Z'
}

/// Read a store file's bytes as a lossy string.
fn read_store_file(dir: &Path, relative: &str) -> String {
    String::from_utf8_lossy(
        &std::fs::read(dir.join(".mulch").join(relative)).expect("store file exists"),
    )
    .into_owned()
}

#[test]
fn init_matches_reference_on_disk_and_output() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("init-ours");
    let theirs = TempDir::new("init-theirs");

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let theirs_run = run_in(&theirs.0, &ml, &["init"]);

    assert_eq!(ours_run.code, theirs_run.code, "exit codes differ");
    assert_eq!(
        ours_run.stdout,
        theirs_run.stdout.replace(
            &theirs.0.display().to_string(),
            &ours.0.display().to_string()
        ),
        "init stdout differs"
    );
    assert_eq!(
        read_store_file(&ours.0, "README.md"),
        read_store_file(&theirs.0, "README.md")
    );
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        read_store_file(&theirs.0, "mulch.config.yaml")
    );
    assert!(ours.0.join(".mulch/expertise").is_dir());
}

#[test]
fn init_reinit_message_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("reinit-ours");
    let theirs = TempDir::new("reinit-theirs");
    run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    run_in(&theirs.0, &ml, &["init"]);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let theirs_run = run_in(&theirs.0, &ml, &["init"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
}

#[test]
fn status_no_store_error_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("nostore-ours");
    let theirs = TempDir::new("nostore-theirs");

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_run = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["status", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(ours_json.stdout, theirs_json.stdout);
    assert_eq!(ours_json.stderr, theirs_json.stderr);
}

#[test]
fn status_fresh_store_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("fresh-ours");
    let theirs = TempDir::new("fresh-theirs");
    run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    run_in(&theirs.0, &ml, &["init"]);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_run = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["status", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));
}

#[test]
fn status_with_records_matches_structurally() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("recs-ours");
    let theirs = TempDir::new("recs-theirs");
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
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "convention",
            "--content",
            "c1",
        ]);
    }

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["status", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));

    // plain lines: counts and suffix shape (relative times stay volatile)
    let ours_plain = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_plain = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_plain.code, theirs_plain.code);
    assert_eq!(
        ours_plain.stdout.lines().count(),
        theirs_plain.stdout.lines().count()
    );
    assert!(ours_plain.stdout.contains("dev: 2 records"));
}

#[test]
fn validate_clean_and_broken_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("val-ours");
    let theirs = TempDir::new("val-theirs");
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

    // clean
    let ours_clean = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_clean = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_clean.code, theirs_clean.code);
    assert_eq!(ours_clean.stdout, theirs_clean.stdout);

    // broken: convention record missing --content + garbage line
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "convention",
            "--name",
            "t1",
            "--description",
            "d",
        ]);
        let file = dir.join(".mulch/expertise/dev.jsonl");
        let mut text = std::fs::read_to_string(&file).expect("domain file");
        // remove the just-added invalid line? no — the reference wrote it and
        // kept it; we only append garbage below (probe scenario B)
        text.push_str("this line is not json\n");
        std::fs::write(&file, text).expect("append garbage");
    }

    let ours_broken = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_broken = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_broken.code, theirs_broken.code);
    assert_eq!(ours_broken.stdout, theirs_broken.stdout);
    assert_eq!(ours_broken.stderr, theirs_broken.stderr);

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["validate", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["validate", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));
}

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

/// Parses a doctor JSON report, neutralizes the environment-dependent
/// upgrade check and the summary counts, and re-serializes
/// deterministically for comparison.
fn normalize_doctor_json(text: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(text).expect("doctor json parses");
    let Some(checks) = value.get_mut("checks").and_then(|c| c.as_array_mut()) else {
        return text.into();
    };
    for check in checks {
        if check.get("name").and_then(|n| n.as_str()) == Some("upgrade") {
            check["message"] = serde_json::Value::String("<UPGRADE>".into());
            check["details"] = serde_json::Value::Array(Vec::new());
            check["status"] = serde_json::Value::String("<STATUS>".into());
        }
    }
    if let Some(object) = value.as_object_mut() {
        object.remove("summary");
    }
    serde_json::to_string(&value).expect("doctor json serializes")
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

/// The `recorded_at` value inside a freshly written domain file.
fn recent_timestamp(file: &Path) -> String {
    let text = std::fs::read_to_string(file).expect("domain file");
    let start = text
        .find("\"recorded_at\":\"")
        .expect("recorded_at present")
        + 15;
    let rest = &text[start..];
    rest[..24].to_string()
}

// ---- sprint 15 (mulch-7ac9): doctor duplicates + divergent ids ----

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

// ---- spec-review round 2: parse layer, schema-invalid strings, no-store
// doctor, --fix --json, quiet matrix, blank-line numbering ----

#[test]
fn parse_layer_contract() {
    let ours = TempDir::new("parse");
    let bin = Path::new(mulch_bin());

    // no args: help on stderr, exit 1, stdout empty
    let run = run_in(&ours.0, bin, &[]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "");
    assert!(run.stderr.starts_with("mulch"), "help goes to stderr");

    // -v and --version: bare version, exit 0
    for flag in ["-v", "--version"] {
        let run = run_in(&ours.0, bin, &[flag]);
        assert_eq!(run.code, 0);
        assert_eq!(run.stdout, concat!(env!("CARGO_PKG_VERSION"), "\n"));
    }

    // unknown command: exact two-line stderr, no timing noise
    let run = run_in(&ours.0, bin, &["nonsense", "--timing"]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        "Unknown command: nonsense\nRun 'mulch --help' for usage.\n"
    );
}

#[test]
fn schema_invalid_strings_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("schemainv-ours");
    let theirs = TempDir::new("schemainv-theirs");
    let record = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\",\"name\":\"x\"}";
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let file = dir.join(".mulch/expertise/dev.jsonl");
        std::fs::write(file, format!("{record}\n")).expect("write record");
        let cfg = dir.join(".mulch/mulch.config.yaml");
        let text = std::fs::read_to_string(&cfg).expect("config");
        std::fs::write(cfg, text.replace("domains: {}", "domains:\n  dev: {}"))
            .expect("register domain");
    }

    // validate plain: summary on stdout, multi-line details on stderr
    let ours_plain = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_plain = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_plain.code, theirs_plain.code);
    assert_eq!(ours_plain.stdout, theirs_plain.stdout);
    assert_eq!(ours_plain.stderr, theirs_plain.stderr);

    // validate json: envelope on stdout, message strings pinned
    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["validate", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["validate", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));

    // doctor plain: the schema-validation detail line matches modulo
    // the upgrade check
    let ours_doc = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    let theirs_doc = run_in(&theirs.0, &ml, &["doctor"]);
    assert_eq!(ours_doc.code, theirs_doc.code);
    assert!(
        ours_doc
            .stdout
            .contains("must have required property 'description'")
    );
    assert_eq!(
        ours_doc
            .stdout
            .lines()
            .filter(|l| l.contains("must have required property"))
            .collect::<Vec<_>>(),
        theirs_doc
            .stdout
            .lines()
            .filter(|l| l.contains("must have required property"))
            .collect::<Vec<_>>()
    );
}

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
fn quiet_matrix_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("quiet-ours");
    let theirs = TempDir::new("quiet-theirs");
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

    // validate --quiet still prints the summary
    let ours_val = run_in(&ours.0, Path::new(mulch_bin()), &["validate", "--quiet"]);
    let theirs_val = run_in(&theirs.0, &ml, &["validate", "--quiet"]);
    assert_eq!(ours_val.code, theirs_val.code);
    assert_eq!(ours_val.stdout, theirs_val.stdout);

    // doctor --quiet prints NOTHING (plain mode)
    let ours_doc = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--quiet"]);
    let theirs_doc = run_in(&theirs.0, &ml, &["doctor", "--quiet"]);
    assert_eq!(ours_doc.code, theirs_doc.code);
    assert_eq!(ours_doc.stdout, theirs_doc.stdout);
    assert_eq!(ours_doc.stdout, "");
}

#[test]
fn blank_lines_keep_physical_line_numbers() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("blank-ours");
    let theirs = TempDir::new("blank-theirs");
    let valid = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\",\"name\":\"p\",\"description\":\"d\"}";
    let invalid = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\",\"name\":\"x\"}";
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let file = dir.join(".mulch/expertise/dev.jsonl");
        std::fs::write(file, format!("\n{valid}\n{invalid}\nnot json\n")).expect("write lines");
        let cfg = dir.join(".mulch/mulch.config.yaml");
        let text = std::fs::read_to_string(&cfg).expect("config");
        std::fs::write(cfg, text.replace("domains: {}", "domains:\n  dev: {}"))
            .expect("register domain");
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_run = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
    // physical addressing: findings at lines 3 and 4, not 2 and 3
    assert!(
        ours_run
            .stderr
            .contains("dev:3 - Schema validation failed:")
    );
    assert!(ours_run.stderr.contains("dev:4 - Invalid JSON"));
}

/// The reference record id of pattern name "p" (sha256 rule).
fn pattern_p_id() -> String {
    use std::fmt::Write as _;

    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(b"pattern:p");
    let mut hex = String::new();
    for byte in digest.iter().take(3) {
        let _ = write!(hex, "{byte:02x}");
    }
    format!("mx-{hex}")
}

// ---- sprint 2 (mulch-fdd0): add / record / edit / outcome ----

/// Normalizes recorded_at timestamps for line comparison.
fn normalize_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' && text[i..].starts_with("\"recorded_at\":\"") {
            out.push_str("\"recorded_at\":\"<TS>\"");
            let mut j = i + 15;
            while j < bytes.len() && bytes[j] != b'"' {
                j += 1;
            }
            i = j + 1;
        } else {
            out.push(char::from(bytes[i]));
            i += 1;
        }
    }
    out
}

#[test]
fn record_all_types_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let cases: [(&str, &[&str]); 6] = [
        ("convention", &["--content", "cc"]),
        ("pattern", &["--name", "nn", "--description", "dd"]),
        ("failure", &["--description", "dd", "--resolution", "rr"]),
        ("decision", &["--title", "tt", "--rationale", "rr"]),
        ("reference", &["--name", "nn", "--description", "dd"]),
        ("guide", &["--name", "nn", "--description", "dd"]),
    ];
    for (index, (kind, payload)) in cases.iter().enumerate() {
        let ours = TempDir::new(format!("rec{index}-ours").as_str());
        let theirs = TempDir::new(format!("rec{index}-theirs").as_str());
        for dir in [&ours.0, &theirs.0] {
            let _ = run_in(dir, &ml, &["init"]);
        }
        let mut args = vec!["record", "d", "--type", kind];
        args.extend_from_slice(payload);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, theirs_run.code, "{kind} exit");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{kind} stdout");
        assert_eq!(
            normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
            normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl")),
            "{kind} jsonl"
        );
        // domain registered in config on both sides
        assert!(read_store_file(&ours.0, "mulch.config.yaml").contains("d: {}"));
    }
}

#[test]
fn record_optional_flags_line_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("optf-ours");
    let theirs = TempDir::new("optf-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
    }
    let args = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "optflag-pat",
        "--description",
        "optflag-desc",
        "--classification",
        "foundational",
        "--tags",
        "a,b",
        "--files",
        "x.rs,y.rs",
        "--dir-anchor",
        "src/",
        "--relates-to",
        "mx-aaaa01",
        "--supersedes",
        "mx-bbbb02",
        "--evidence-commit",
        "abc123",
        "--evidence-seeds",
        "mulch-1",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );
}

#[test]
fn add_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("add-ours");
    let theirs = TempDir::new("add-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
    }
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["add", "cli"]);
    let theirs_run = run_in(&theirs.0, &ml, &["add", "cli"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        read_store_file(&theirs.0, "mulch.config.yaml")
    );
    assert_eq!(read_store_file(&ours.0, "expertise/cli.jsonl"), "");

    // existing domain
    let ours_dup = run_in(&ours.0, Path::new(mulch_bin()), &["add", "cli"]);
    let theirs_dup = run_in(&theirs.0, &ml, &["add", "cli"]);
    assert_eq!(ours_dup.code, theirs_dup.code);
    assert_eq!(ours_dup.stdout, theirs_dup.stdout);
    assert_eq!(ours_dup.stderr, theirs_dup.stderr);
}

#[test]
fn record_auto_create_failure_side_effects_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("autofail-ours");
    let theirs = TempDir::new("autofail-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
    }
    let args = [
        "record",
        "devfail",
        "--type",
        "convention",
        "--name",
        "t1",
        "--description",
        "d",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
    // the domain side effect persists on both sides
    assert!(read_store_file(&ours.0, "mulch.config.yaml").contains("devfail: {}"));
    assert!(read_store_file(&theirs.0, "mulch.config.yaml").contains("devfail: {}"));
}

#[test]
fn duplicate_and_force_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("dup-ours");
    let theirs = TempDir::new("dup-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "d",
            "--type",
            "convention",
            "--content",
            "c1",
        ]);
    }
    let dup = ["record", "d", "--type", "convention", "--content", "c1"];
    let ours_dup = run_in(&ours.0, Path::new(mulch_bin()), &dup);
    let theirs_dup = run_in(&theirs.0, &ml, &dup);
    assert_eq!(ours_dup.code, theirs_dup.code);
    assert_eq!(ours_dup.stdout, theirs_dup.stdout);

    let forced = [
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "c1",
        "--force",
    ];
    let ours_force = run_in(&ours.0, Path::new(mulch_bin()), &forced);
    let theirs_force = run_in(&theirs.0, &ml, &forced);
    assert_eq!(ours_force.code, theirs_force.code);
    assert_eq!(ours_force.stdout, theirs_force.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl")
            .lines()
            .count(),
        2
    );
}

#[test]
fn record_ids_match_known_reference_values() {
    // Pinned from the reference source algorithm + probe table.
    let cases: [(&str, &str, &str); 4] = [
        ("convention", "c1", "mx-1d1926"),
        ("reference", "N1", "mx-b9079b"),
        ("pattern", "N1", "mx-a6adb8"),
        ("guide", "N1", "mx-ab7ce3"),
    ];
    for (kind, key, expected) in cases {
        assert_eq!(mulch::record_id(kind, key), expected);
    }
}

#[test]
fn record_dry_run_leaves_store_unchanged() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("dry-ours");
    let _ = run_in(&ours.0, &ml, &["init"]);
    let _ = run_in(&ours.0, &ml, &["add", "d"]);
    let before = read_store_file(&ours.0, "expertise/d.jsonl");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "x",
        "--dry-run",
    ]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.contains("Dry-run: Would create convention in d"));
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), before);
}

#[test]
fn record_stdin_matches_reference() {
    use std::io::Write as _;
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("stdin-ours");
    let theirs = TempDir::new("stdin-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
    let payload = r#"{"type":"convention","content":"stdin-conv-1"}"#;
    for (dir, program) in [(&ours.0, Path::new(mulch_bin())), (&theirs.0, &ml)] {
        let mut child = std::process::Command::new(program)
            .args(["record", "d", "--stdin"])
            .current_dir(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("write");
        let output = child.wait_with_output().expect("wait");
        assert!(output.status.success());
    }
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );
    // input key order preserved: type before content before recorded_at
    let line = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(line.starts_with(r#"{"type":"convention","content":"stdin-conv-1","recorded_at""#));
}

#[test]
fn edit_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("edit-ours");
    let theirs = TempDir::new("edit-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "p",
            "--description",
            "old",
        ]);
    }
    let id = pattern_p_id();
    let edit = ["edit", "d", &id, "--description", "new"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &edit);
    let theirs_run = run_in(&theirs.0, &ml, &edit);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // unknown id
    let missing = ["edit", "d", "mx-deadbe", "--description", "x"];
    let ours_missing = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    let theirs_missing = run_in(&theirs.0, &ml, &missing);
    assert_eq!(ours_missing.code, theirs_missing.code);
    assert_eq!(ours_missing.stdout, theirs_missing.stdout);
    assert_eq!(ours_missing.stderr, theirs_missing.stderr);

    // idKey edit does not recompute the id
    let rename = ["edit", "d", &id, "--name", "p2"];
    let ours_rename = run_in(&ours.0, Path::new(mulch_bin()), &rename);
    assert_eq!(ours_rename.code, 0);
    assert!(read_store_file(&ours.0, "expertise/d.jsonl").contains(&id));
}

#[test]
fn outcome_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("outc-ours");
    let theirs = TempDir::new("outc-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "p",
            "--description",
            "d",
        ]);
    }
    let id = pattern_p_id();
    let outcome = [
        "outcome",
        "d",
        &id,
        "--status",
        "success",
        "--agent",
        "probe-agent",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &outcome);
    let theirs_run = run_in(&theirs.0, &ml, &outcome);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // second outcome appends; no-agent form omits the parens
    let second = ["outcome", "d", &id, "--status", "partial"];
    let ours_second = run_in(&ours.0, Path::new(mulch_bin()), &second);
    let theirs_second = run_in(&theirs.0, &ml, &second);
    assert_eq!(ours_second.code, theirs_second.code);
    assert_eq!(ours_second.stdout, theirs_second.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // unknown id
    let missing = ["outcome", "d", "mx-deadbe", "--status", "success"];
    let ours_missing = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    let theirs_missing = run_in(&theirs.0, &ml, &missing);
    assert_eq!(ours_missing.code, theirs_missing.code);
    assert_eq!(ours_missing.stderr, theirs_missing.stderr);
}

// ---- sprint 5 (mulch-b88b): outcome without --status is read-only ----

#[test]
fn outcome_read_only_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("outc-ro-ours");
    let theirs = TempDir::new("outc-ro-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "p",
            "--description",
            "d",
        ]);
    }
    let id = pattern_p_id();

    // empty listing: same notice on both sides, store untouched
    let ours_before = read_store_file(&ours.0, "expertise/d.jsonl");
    let theirs_before = read_store_file(&theirs.0, "expertise/d.jsonl");
    let listing = ["outcome", "d", &id];
    let ours_empty = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    let theirs_empty = run_in(&theirs.0, &ml, &listing);
    assert_eq!(ours_empty.code, theirs_empty.code);
    assert_eq!(ours_empty.stdout, theirs_empty.stdout);
    assert_eq!(ours_empty.stderr, theirs_empty.stderr);
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        ours_before,
        "read-only listing must not write"
    );
    assert_eq!(
        read_store_file(&theirs.0, "expertise/d.jsonl"),
        theirs_before
    );

    // json envelope of the empty listing: outcomes stays []
    let json_empty = ["--json", "outcome", "d", &id];
    let ours_json_empty = run_in(&ours.0, Path::new(mulch_bin()), &json_empty);
    let theirs_json_empty = run_in(&theirs.0, &ml, &json_empty);
    assert_eq!(ours_json_empty.code, theirs_json_empty.code);
    assert_eq!(ours_json_empty.stdout, theirs_json_empty.stdout);

    // two outcomes with different field shapes, then the populated listing
    for outcome in [
        vec![
            "outcome",
            "d",
            &id,
            "--status",
            "success",
            "--agent",
            "probe-agent",
            "--duration",
            "42",
            "--notes",
            "went fine",
            "--test-results",
            "3 passed",
        ],
        vec![
            "outcome", "d", &id, "--status", "failure", "--notes", "second",
        ],
    ] {
        let ours_add = run_in(&ours.0, Path::new(mulch_bin()), &outcome);
        let theirs_add = run_in(&theirs.0, &ml, &outcome);
        assert_eq!(ours_add.code, theirs_add.code);
        assert_eq!(ours_add.stdout, theirs_add.stdout);
    }
    let ours_before = read_store_file(&ours.0, "expertise/d.jsonl");
    let ours_list = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    let theirs_list = run_in(&theirs.0, &ml, &listing);
    assert_eq!(ours_list.code, theirs_list.code);
    assert_eq!(
        normalize(&ours_list.stdout),
        normalize(&theirs_list.stdout),
        "populated listing must render every detail line"
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        ours_before,
        "populated listing must not write"
    );

    // json listing carries the raw outcomes array (key order preserved)
    let json = ["--json", "outcome", "d", &id];
    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &json);
    let theirs_json = run_in(&theirs.0, &ml, &json);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));

    // quiet suppresses the plain listing entirely
    let quiet = ["--quiet", "outcome", "d", &id];
    let ours_quiet = run_in(&ours.0, Path::new(mulch_bin()), &quiet);
    let theirs_quiet = run_in(&theirs.0, &ml, &quiet);
    assert_eq!(ours_quiet.code, theirs_quiet.code);
    assert_eq!(ours_quiet.stdout, theirs_quiet.stdout);
    assert_eq!(ours_quiet.stdout, "");

    // crafted stores pin the odd read-only surfaces: a legacy singular
    // `outcome` object (reader normalization) and a hand-corrupted
    // non-array `outcomes` value (header + engine error, raw json)
    let legacy = serde_json::json!({
        "type": "pattern",
        "classification": "tactical",
        "recorded_at": "2026-10-04T08:00:00.000Z",
        "name": "p",
        "description": "d",
        "id": id.clone(),
        "outcome": {"status": "success", "agent": "legacy"}
    });
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d.jsonl"),
            format!("{legacy}\n"),
        )
        .expect("crafted store writable");
    }
    let ours_legacy = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    let theirs_legacy = run_in(&theirs.0, &ml, &listing);
    assert_eq!(ours_legacy.code, theirs_legacy.code);
    assert_eq!(ours_legacy.stdout, theirs_legacy.stdout);
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        format!("{legacy}\n"),
        "legacy listing must not write"
    );

    let corrupt = serde_json::json!({
        "type": "pattern",
        "classification": "tactical",
        "recorded_at": "2026-10-04T08:00:00.000Z",
        "name": "p",
        "description": "d",
        "id": id.clone(),
        "outcomes": "not-an-array"
    });
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d.jsonl"),
            format!("{corrupt}\n"),
        )
        .expect("crafted store writable");
    }
    let ours_corrupt = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    let theirs_corrupt = run_in(&theirs.0, &ml, &listing);
    assert_eq!(ours_corrupt.code, theirs_corrupt.code);
    assert_eq!(ours_corrupt.stdout, theirs_corrupt.stdout);
    assert_eq!(ours_corrupt.stderr, theirs_corrupt.stderr);
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        format!("{corrupt}\n"),
        "corrupted-store listing must not write"
    );
    let corrupt_json = ["--json", "outcome", "d", &id];
    let ours_corrupt_json = run_in(&ours.0, Path::new(mulch_bin()), &corrupt_json);
    let theirs_corrupt_json = run_in(&theirs.0, &ml, &corrupt_json);
    assert_eq!(ours_corrupt_json.code, theirs_corrupt_json.code);
    assert_eq!(ours_corrupt_json.stdout, theirs_corrupt_json.stdout);
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        format!("{corrupt}\n"),
        "corrupted-store json listing must not write"
    );

    // unknown id stays an error in read-only mode
    let missing = ["outcome", "d", "mx-deadbe"];
    let ours_missing = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    let theirs_missing = run_in(&theirs.0, &ml, &missing);
    assert_eq!(ours_missing.code, theirs_missing.code);
    assert_eq!(ours_missing.stderr, theirs_missing.stderr);
}

// ---- sprint 8 (mulch-00aa): status reads strictly, lenient seam ----

#[test]
fn status_reader_surfaces_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("stat-rd-ours");
    let theirs = TempDir::new("stat-rd-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d1"]);
        let _ = run_in(dir, &ml, &["add", "d2"]);
    }
    // fixed, far-apart recorded_at stamps keep the "Xd ago" buckets
    // stable for the test duration AND cross the observational shelf
    // life (rotting); the array line counts, the numeric type line
    // pins the integer-first distribution key order.
    let corpus = "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-08-26T08:00:00.000Z\",\"content\":\"old\",\"id\":\"mx-bbbbbb\"}\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-08-27T08:00:00.000Z\",\"content\":\"new\",\"id\":\"mx-cccccc\"}\n[1,2,3]\n{\"type\":5,\"classification\":\"tactical\",\"recorded_at\":\"2026-08-26T08:00:00.000Z\",\"content\":\"numtype\",\"id\":\"mx-333333\"}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            corpus,
        )
        .expect("corpus writable");
        // d2 keeps its empty file; a file-less domain renders "never"
        std::fs::remove_file(dir.join(".mulch").join("expertise").join("d2.jsonl"))
            .expect("d2 file removable");
    }

    // plain: count includes the array line, the recorded range renders,
    // the file-less domain says "never"
    let ours_plain = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_plain = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_plain.code, theirs_plain.code);
    assert_eq!(ours_plain.stdout, theirs_plain.stdout);

    // json: same story (timestamps normalized)
    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "status"]);
    let theirs_json = run_in(&theirs.0, &ml, &["--json", "status"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));

    // scalar line: both exit 1; the reference stack-traces (DEVIATIONS:
    // ours is a clean error), so only the code is comparable
    let scalar = "5\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            scalar,
        )
        .expect("scalar writable");
    }
    let ours_scalar = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_scalar = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_scalar.code, theirs_scalar.code);
    assert_eq!(ours_scalar.code, 1);
    assert!(
        ours_scalar.stderr.contains("non-object record at"),
        "clean scalar error, got: {}",
        ours_scalar.stderr
    );

    // malformed line: same class (reference stack-traces)
    let malformed = "{\"type\":\"convention\" ...\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            malformed,
        )
        .expect("malformed writable");
    }
    let ours_bad = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_bad = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_bad.code, theirs_bad.code);
    assert_eq!(ours_bad.code, 1);
    assert!(
        ours_bad.stderr.contains("Malformed JSONL at"),
        "clean malformed error, got: {}",
        ours_bad.stderr
    );

    // unknown type: both exit 1 (reference stack-traces; ours clean)
    let unknown = "{\"type\":\"wat\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"x\",\"id\":\"mx-dddddd\"}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            unknown,
        )
        .expect("unknown writable");
    }
    let ours_unknown = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_unknown = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_unknown.code, theirs_unknown.code);
    assert_eq!(ours_unknown.code, 1);

    // comment lines are findings for the raw-loop readers (validate,
    // doctor jsonl-integrity) — only the strict reader skips them
    // ...plus a legacy singular-outcome record: warning, not error
    let commented = "# ARCHIVED — not for active use.\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"ok\",\"id\":\"mx-aaaaaa\"}\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"legacy\",\"id\":\"mx-444444\",\"outcome\":{\"status\":\"success\"}}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            commented,
        )
        .expect("commented writable");
    }
    let ours_validate = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_validate = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_validate.code, theirs_validate.code);
    assert_eq!(ours_validate.stdout, theirs_validate.stdout);
    assert_eq!(ours_validate.stderr, theirs_validate.stderr);

    // --allow-unknown-types keeps status alive (reference flag too)
    let ours_flag = run_in(&ours.0, Path::new(mulch_bin()), &[
        "status",
        "--allow-unknown-types",
    ]);
    let theirs_flag = run_in(&theirs.0, &ml, &["status", "--allow-unknown-types"]);
    assert_eq!(ours_flag.code, theirs_flag.code);
    assert_eq!(ours_flag.stdout, theirs_flag.stdout);
}

// ---- sprint 6 (mulch-ccf6): record upsert semantics ----

#[test]
fn record_upsert_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("ccf6-ours");
    let theirs = TempDir::new("ccf6-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "p",
            "--description",
            "v1",
        ]);
    }
    let id = pattern_p_id();
    let compare = |ours: (&Run, String), theirs: (&Run, String)| {
        assert_eq!(ours.0.code, theirs.0.code);
        assert_eq!(normalize(&ours.0.stdout), normalize(&theirs.0.stdout));
        assert_eq!(normalize(&ours.0.stderr), normalize(&theirs.0.stderr));
        assert_eq!(
            normalize_line(&ours.1),
            normalize_line(&theirs.1),
            "store diverged"
        );
    };

    // named duplicate upserts in place
    let up = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v2",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &up),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &up),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // json updated envelope + outcomes carried into the upsert
    let up_outcome = [
        "--json",
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v3",
        "--outcome-status",
        "success",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &up_outcome),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &up_outcome),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // second outcome merges after the existing one
    let up_outcome2 = [
        "--json",
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v4",
        "--outcome-status",
        "failure",
        "--outcome-agent",
        "second-agent",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &up_outcome2),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &up_outcome2),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // anonymous duplicate skips with the advisory
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
    ]);
    let _ = run_in(&theirs.0, &ml, &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
    ]);
    let anon = ["record", "d", "--type", "convention", "--content", "same"];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &anon),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &anon),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // json skipped envelope carries the record number
    let anon_json = [
        "--json",
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &anon_json),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &anon_json),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // --force appends anyway
    let force = [
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
        "--force",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &force),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &force),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // dry-run mirrors the decision without writing (plain + json)
    let dry_named = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v9",
        "--dry-run",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &dry_named),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &dry_named),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );
    let dry_json = [
        "--json",
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v9",
        "--dry-run",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &dry_json),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &dry_json),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // a renamed record still dedupes (dedup field, not id) and the
    // upsert corrects the stale id in place
    let rename = ["edit", "d", &id, "--name", "p2"];
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &rename);
    let _ = run_in(&theirs.0, &ml, &rename);
    let renamed_up = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p2",
        "--description",
        "v10",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &renamed_up),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &renamed_up),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // batch: one new guide + one named duplicate -> created 1, updated 1
    let batch = serde_json::json!([
        {"type": "guide", "name": "g1", "description": "gd"},
        {"type": "pattern", "name": "p2", "description": "batch-v11"}
    ]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch.json"), format!("{batch}")).expect("batch file");
    }
    let run_batch = ["record", "d", "--batch", "batch.json", "--json"];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &run_batch),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &run_batch),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // within-batch duplicate in dry-run counts as another create
    // (dedupe runs against the store only, never the batch)
    let batch2 = serde_json::json!([
        {"type": "pattern", "name": "q", "description": "q1"},
        {"type": "pattern", "name": "q", "description": "q2"}
    ]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch2.json"), format!("{batch2}")).expect("batch file");
    }
    let run_batch2 = [
        "record",
        "d",
        "--batch",
        "batch2.json",
        "--json",
        "--dry-run",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &run_batch2),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &run_batch2),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // non-dry within-batch duplicate upserts the batch's own record
    let run_batch3 = ["record", "d", "--batch", "batch2.json", "--json"];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &run_batch3),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &run_batch3),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // an explicit batch id survives upsert AND create (generated ids
    // only fill gaps — spec-review blocker F1)
    let batch3 = serde_json::json!([
        {"type": "pattern", "name": "p2", "description": "keep-id", "id": "mx-abcd1234"},
        {"type": "guide", "name": "g2", "description": "gd2", "id": "mx-feed1234"}
    ]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch3.json"), format!("{batch3}")).expect("batch file");
    }
    let run_batch4 = ["record", "d", "--batch", "batch3.json", "--json"];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &run_batch4),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &run_batch4),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // flag-path key order: outcomes precede dir_anchors on the
    // rewritten line (spec-review minor F2)
    let anchored = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p2",
        "--description",
        "v12",
        "--dir-anchor",
        "src/lib",
        "--outcome-status",
        "partial",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &anchored),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &anchored),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );
}

// ---- sprint 2 review round: json error channels, stdin/batch matrix ----

#[test]
fn edit_outcome_flags_and_list_replace_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("eo-ours");
    let theirs = TempDir::new("eo-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "p",
            "--description",
            "d1",
            "--files",
            "f1.rs",
        ]);
    }
    let id = pattern_p_id();

    // edit --outcome-status uses the --outcome-* longs and appends
    let edit = [
        "edit",
        "d",
        &id,
        "--outcome-status",
        "success",
        "--outcome-agent",
        "ag",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &edit);
    let theirs_run = run_in(&theirs.0, &ml, &edit);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // edit --files REPLACES the list
    let replace = ["edit", "d", &id, "--files", "g1.rs,g2.rs"];
    let ours_rep = run_in(&ours.0, Path::new(mulch_bin()), &replace);
    let theirs_rep = run_in(&theirs.0, &ml, &replace);
    assert_eq!(ours_rep.code, theirs_rep.code);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );
    assert!(read_store_file(&ours.0, "expertise/d.jsonl").contains("g1.rs"));
    assert!(!read_store_file(&ours.0, "expertise/d.jsonl").contains("f1.rs"));
}

#[test]
fn edit_unknown_id_json_envelope_on_stderr() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("ejs-ours");
    let theirs = TempDir::new("ejs-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
    let missing = ["edit", "d", "mx-deadbe", "--description", "x", "--json"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    let theirs_run = run_in(&theirs.0, &ml, &missing);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
}

#[test]
fn record_invalid_ref_matches_reference_blob() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("iref-ours");
    let theirs = TempDir::new("iref-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
    }
    let args = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "x",
        "--relates-to",
        "mx-abc",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
}

#[test]
fn record_missing_flags_json_error_text() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("mfj-ours");
    let theirs = TempDir::new("mfj-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
    let args = [
        "record",
        "d",
        "--type",
        "convention",
        "--name",
        "t1",
        "--description",
        "d",
        "--json",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
}

#[test]
fn stdin_invalid_record_envelope_and_untouched_store() {
    use std::io::Write as _;
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("sinv-ours");
    let theirs = TempDir::new("sinv-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
    let payload = r#"{"type":"pattern","name":"pn"}"#;
    for (dir, program) in [(&ours.0, Path::new(mulch_bin())), (&theirs.0, &ml)] {
        let mut child = std::process::Command::new(program)
            .args(["record", "d", "--stdin", "--json"])
            .current_dir(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("write");
        let output = child.wait_with_output().expect("wait");
        assert_eq!(output.status.code(), Some(1));
    }
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
    assert_eq!(read_store_file(&theirs.0, "expertise/d.jsonl"), "");
}

#[test]
fn batch_dry_run_writes_nothing() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("bdry-ours");
    let theirs = TempDir::new("bdry-theirs");
    let batch = r#"[{"type":"convention","content":"b1"},{"type":"convention","content":"b2"}]"#;
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
        std::fs::write(dir.join("b.json"), batch).expect("batch file");
    }
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "b.json",
        "--dry-run",
    ]);
    let theirs_run = run_in(&theirs.0, &ml, &[
        "record",
        "d",
        "--batch",
        "b.json",
        "--dry-run",
    ]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
}

#[test]
fn classification_choices_rejected() {
    let ours = TempDir::new("cls-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "c",
        "--classification",
        "bogus",
    ]);
    // clap rejects the value before the store is touched (wording is a
    // documented deviation; exit 1 + untouched store is the contract)
    assert_eq!(run.code, 1);
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
}

// ---- sprint 3 (mulch-dc67): delete / delete-domain / move ----

/// A deterministic store written by hand (fixed timestamps) so byte
/// comparisons are meaningful.
fn write_seed(dir: &Path, domains: &[(&str, &str)]) {
    use std::fmt::Write as _;
    let store = dir.join(".mulch");
    std::fs::create_dir_all(store.join("expertise")).expect("store dirs");
    let mut config = String::from("version: '1'\ndomains:\n");
    for (domain, _) in domains {
        let _ = writeln!(config, "  {domain}: {{}}");
    }
    config.push_str(CONFIG_TAIL);
    std::fs::write(store.join("mulch.config.yaml"), config).expect("config");
    for (domain, body) in domains {
        std::fs::write(
            store.join("expertise").join(format!("{domain}.jsonl")),
            body,
        )
        .expect("domain file");
    }
}

const ALPHA_ONE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Alpha One\",\"description\":\"first alpha pattern\",\"id\":\"mx-1bb21d\"}";
const ALPHA_TWO: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Alpha Two\",\"description\":\"second alpha pattern\",\"id\":\"mx-c7129f\"}";

#[test]
fn delete_single_bulk_and_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let cases: [&[&str]; 6] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete", "alpha", "mx-c7129f"],
        &["delete", "alpha", "--records", "mx-1bb21d,mx-c7129f"],
        &["delete", "alpha", "--all-except", "mx-1bb21d"],
        &["delete", "alpha", "--dry-run"],
        &["delete", "alpha", "--records", "mx-1bb21d", "--json"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let ours = TempDir::new(format!("del{index}-o").as_str());
        let theirs = TempDir::new(format!("del{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", seed.as_str()), ("beta", "")]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{args:?} stdout");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
        assert_eq!(
            read_store_file(&ours.0, "expertise/alpha.jsonl"),
            read_store_file(&theirs.0, "expertise/alpha.jsonl"),
            "{args:?} file"
        );
    }

    // Error surfaces
    let errors: [&[&str]; 5] = [
        &["delete", "alpha"],
        &["delete", "alpha", "mx-1bb21d", "--records", "mx-c7129f"],
        &["delete", "alpha", "--records", ""],
        &["delete", "alpha", "--all-except", ""],
        &["delete", "alpha", "mx-ffffff"],
    ];
    for (index, args) in errors.iter().enumerate() {
        let ours = TempDir::new(format!("dele{index}-o").as_str());
        let theirs = TempDir::new(format!("dele{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", seed.as_str())]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
        assert_eq!(
            read_store_file(&ours.0, "expertise/alpha.jsonl"),
            seed,
            "{args:?} must not touch the store"
        );
    }
}

#[test]
fn delete_junk_rewrite_and_last_record_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let junk = format!(
        "# comment\n{{\"id\":\"mx-aaaaaa\",\"type\":\"pattern\",\"name\":\"Weird Order\",\"zzz_custom\":\"keepme\",\"description\":\"weird\",\"classification\":\"tactical\",\"recorded_at\":\"2026-01-01T00:00:00.000Z\"}}\n\n{ALPHA_ONE}\n"
    );
    let ours = TempDir::new("djunk-o");
    let theirs = TempDir::new("djunk-t");
    for dir in [&ours.0, &theirs.0] {
        write_seed(dir, &[("alpha", junk.as_str())]);
    }
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "delete",
        "alpha",
        "mx-1bb21d",
    ]);
    let theirs_run = run_in(&theirs.0, &ml, &["delete", "alpha", "mx-1bb21d"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    // the junk is dropped, the survivor stays byte-identical
    assert_eq!(
        read_store_file(&ours.0, "expertise/alpha.jsonl"),
        read_store_file(&theirs.0, "expertise/alpha.jsonl")
    );
    assert!(!read_store_file(&ours.0, "expertise/alpha.jsonl").contains("comment"));

    // deleting the last record leaves a 0-byte file (never removes it)
    let ours_last = TempDir::new("dlast-o");
    let theirs_last = TempDir::new("dlast-t");
    for dir in [&ours_last.0, &theirs_last.0] {
        write_seed(dir, &[("beta", &format!("{ALPHA_ONE}\n"))]);
    }
    let _ = run_in(&ours_last.0, Path::new(mulch_bin()), &[
        "delete",
        "beta",
        "mx-1bb21d",
    ]);
    let _ = run_in(&theirs_last.0, &ml, &["delete", "beta", "mx-1bb21d"]);
    assert_eq!(read_store_file(&ours_last.0, "expertise/beta.jsonl"), "");
    assert_eq!(read_store_file(&theirs_last.0, "expertise/beta.jsonl"), "");
    assert!(ours_last.0.join(".mulch/expertise/beta.jsonl").is_file());
}

#[test]
fn delete_domain_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    // --yes, --json (prompt skipped) and --dry-run
    let cases: [&[&str]; 3] = [
        &["delete-domain", "alpha", "--yes"],
        &["delete-domain", "alpha", "--json"],
        &["delete-domain", "alpha", "--yes", "--dry-run"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let ours = TempDir::new(format!("dd{index}-o").as_str());
        let theirs = TempDir::new(format!("dd{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", seed.as_str()), ("beta", "")]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{args:?} stdout");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
        assert_eq!(
            read_store_file(&ours.0, "mulch.config.yaml"),
            read_store_file(&theirs.0, "mulch.config.yaml"),
            "{args:?} config"
        );
        assert_eq!(
            ours.0.join(".mulch/expertise/alpha.jsonl").exists(),
            theirs.0.join(".mulch/expertise/alpha.jsonl").exists(),
            "{args:?} file presence"
        );
    }

    // unknown domain: plain carries the add-hint, json the domain list
    for json in [false, true] {
        let ours = TempDir::new(format!("ddu-{}", if json { "j" } else { "p" }).as_str());
        let theirs = TempDir::new(format!("ddut-{}", if json { "j" } else { "p" }).as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("beta", "")]);
        }
        let mut args = vec!["delete-domain", "nope", "--yes"];
        if json {
            args.push("--json");
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, theirs_run.code);
        assert_eq!(ours_run.stdout, theirs_run.stdout);
        assert_eq!(ours_run.stderr, theirs_run.stderr);
    }
}

#[test]
fn move_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let alpha = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let beta = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.427Z\",\"name\":\"Beta One\",\"description\":\"first beta pattern\",\"id\":\"mx-3242b5\"}\n";
    let cases: [&[&str]; 3] = [
        &["move", "alpha", "mx-1bb21d", "beta"],
        &["move", "alpha", "mx-1bb21d", "beta", "--json"],
        &["move", "alpha", "mx-1bb21d", "beta", "--dry-run"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let ours = TempDir::new(format!("mv{index}-o").as_str());
        let theirs = TempDir::new(format!("mv{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", alpha.as_str()), ("beta", beta)]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{args:?} stdout");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
        assert_eq!(
            read_store_file(&ours.0, "expertise/beta.jsonl"),
            read_store_file(&theirs.0, "expertise/beta.jsonl"),
            "{args:?} target"
        );
        assert_eq!(
            read_store_file(&ours.0, "expertise/alpha.jsonl"),
            read_store_file(&theirs.0, "expertise/alpha.jsonl"),
            "{args:?} source"
        );
    }

    // errors: same domain, unknown target, schema-invalid record
    let invalid = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Broken\",\"id\":\"mx-f16294\"}\n";
    let error_cases: [(&[&str], &str); 3] = [
        (&["move", "alpha", "mx-1bb21d", "alpha"], alpha.as_str()),
        (&["move", "alpha", "mx-1bb21d", "zeta"], alpha.as_str()),
        (&["move", "alpha", "mx-f16294", "beta"], invalid),
    ];
    for (index, (args, body)) in error_cases.iter().enumerate() {
        let ours = TempDir::new(format!("mve{index}-o").as_str());
        let theirs = TempDir::new(format!("mve{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", body), ("beta", beta)]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
    }
}

#[test]
fn move_incoming_references_and_allowed_types() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // gamma refers to the moved id -> incomingReferences entry
    let gamma = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:00.000Z\",\"name\":\"Gamma One\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"],\"id\":\"mx-da09e8\"}\n";
    let alpha = format!("{ALPHA_ONE}\n");
    let beta = format!("{ALPHA_TWO}\n");
    let ours = TempDir::new("mvr-o");
    let theirs = TempDir::new("mvr-t");
    for dir in [&ours.0, &theirs.0] {
        write_seed(dir, &[
            ("alpha", alpha.as_str()),
            ("beta", beta.as_str()),
            ("gamma", gamma),
        ]);
    }
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    if ours_run.stdout.contains("incomingReferences") {
        assert!(ours_run.stdout.contains("mx-da09e8"), "referrer reported");
    }
}

// ---- sprint 3 (mulch-dc67) hardening: strict reads, identifier
// resolution, config order and the move gates ----

/// The reference's canonical config tail (governance + shelf life).
/// Every mutating config rewrite backfills it, in this byte order.
const CONFIG_TAIL: &str = "governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\nclassification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";

/// A second beta record (fixed timestamp) for cross-domain cases.
const BETA_ONE_LINE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.427Z\",\"name\":\"Beta One\",\"description\":\"first beta pattern\",\"id\":\"mx-3242b5\"}";

/// A physical line that is not valid JSON.
const MALFORMED_LINE: &str = "{\"type\":\"pattern\", BAD";

/// A record whose type is not in the payload registry.
const UNKNOWN_TYPE_LINE: &str = "{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Odd\",\"id\":\"mx-zzz111\"}";

/// An archived variant of [`ALPHA_ONE`] (same id).
const ARCHIVED_ALPHA_ONE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"status\":\"archived\",\"name\":\"Old Alpha\",\"description\":\"archived alpha pattern\",\"id\":\"mx-1bb21d\"}";

/// A target domain that demands a field the alpha record lacks.
const REQUIRED_FIELDS_CONFIG: &str =
    "version: '1'\ndomains:\n  alpha: {}\n  beta:\n    required_fields:\n      - owner\n";

/// A target domain that admits only convention records.
const ALLOWED_TYPES_CONFIG: &str =
    "version: '1'\ndomains:\n  alpha: {}\n  beta:\n    allowed_types:\n      - convention\n";

/// A convention record whose `content` is 80 chars (summary window 60).
fn long_convention_line() -> String {
    format!(
        "{{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"{}\",\"id\":\"mx-1bb21d\"}}",
        "C".repeat(80)
    )
}

/// The truncated form of [`long_convention_line`]'s summary.
fn truncated_summary() -> String {
    format!("{}...", "C".repeat(60))
}

/// [`write_seed`] with a caller-supplied config body.
fn write_seed_with_config(dir: &Path, config: &str, domains: &[(&str, &str)]) {
    let store = dir.join(".mulch");
    std::fs::create_dir_all(store.join("expertise")).expect("store dirs");
    std::fs::write(store.join("mulch.config.yaml"), config).expect("config");
    for (domain, body) in domains {
        std::fs::write(
            store.join("expertise").join(format!("{domain}.jsonl")),
            body,
        )
        .expect("domain file");
    }
}

/// Writes an expertise file the config does not register (orphan).
fn write_orphan(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(".mulch").join("expertise").join(name), body).expect("orphan file");
}

/// Twin stores seeded identically (`None` = the default seed config).
fn twin_seeded(
    tag: &str,
    config: Option<&str>,
    domains: &[(&str, &str)],
    orphans: &[(&str, &str)],
) -> (TempDir, TempDir) {
    let ours = TempDir::new(format!("{tag}-o").as_str());
    let theirs = TempDir::new(format!("{tag}-t").as_str());
    for dir in [&ours.0, &theirs.0] {
        match config {
            Some(config) => write_seed_with_config(dir, config, domains),
            None => write_seed(dir, domains),
        }
        for (name, body) in orphans {
            write_orphan(dir, name, body);
        }
    }
    (ours, theirs)
}

/// Folds the temp root so messages carrying absolute paths compare.
fn normalize_dir(text: &str, dir: &Path) -> String {
    text.replace(&dir.display().to_string(), "<DIR>")
}

/// Asserts both stderr streams equal the expected reference text.
fn assert_same_stderr(ours: &TempDir, our: &Run, theirs: &TempDir, their: &Run, expected: &str) {
    assert_eq!(normalize_dir(&our.stderr, &ours.0), expected, "our stderr");
    assert_eq!(
        normalize_dir(&their.stderr, &theirs.0),
        expected,
        "reference stderr"
    );
}

/// Asserts both stores hold the same bytes for a store-relative path.
fn assert_same_file(ours: &TempDir, theirs: &TempDir, relative: &str) {
    assert_eq!(
        read_store_file(&ours.0, relative),
        read_store_file(&theirs.0, relative),
        "{relative} differs"
    );
}

/// The reference's pretty JSON error envelope (stderr channel).
fn error_envelope(command: &str, error: &str) -> String {
    format!(
        "{{\n  \"success\": false,\n  \"command\": \"{command}\",\n  \"error\": \"{}\"\n}}\n",
        error.replace('"', "\\\"")
    )
}

#[test]
fn delete_and_move_abort_on_malformed_jsonl_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{MALFORMED_LINE}\n{ALPHA_TWO}\n");
    let beta = format!("{BETA_ONE_LINE}\n");
    let commands: [&[&str]; 4] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete", "alpha", "--records", "mx-1bb21d"],
        &["delete-domain", "alpha", "--yes"],
        &["move", "alpha", "mx-1bb21d", "beta"],
    ];
    for (index, args) in commands.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("mal{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", beta.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(their.code, 1, "{args:?} reference aborts");
        assert_eq!(our.stdout, "", "{args:?} prints nothing to stdout");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        // The template is pinned; the parser's own reason wording
        // (serde vs JSON.parse) inside it is not (see the reply/dossier).
        let prefix = "Error: Malformed JSONL at <DIR>/.mulch/expertise/alpha.jsonl:2: ";
        let suffix = format!(". Line: {MALFORMED_LINE}\n");
        let our_stderr = normalize_dir(&our.stderr, &ours.0);
        let their_stderr = normalize_dir(&their.stderr, &theirs.0);
        for text in [&our_stderr, &their_stderr] {
            assert!(text.starts_with(prefix), "template prefix: {text}");
            assert!(text.ends_with(&suffix), "template suffix: {text}");
        }
        // Nothing was written.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&theirs.0, "expertise/alpha.jsonl"), seed);
        assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
        assert_same_file(&ours, &theirs, "mulch.config.yaml");
    }

    // The preview is the trimmed line, cut at 77 chars plus "...".
    let long_line = format!("{{\"type\":\"convention\",\"name\":\"{}\"", "x".repeat(120));
    let preview = format!("{}...", &long_line[..77]);
    let (ours, theirs) = twin_seeded(
        "mallong",
        None,
        &[("alpha", format!("{long_line}\n").as_str())],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "delete",
        "alpha",
        "mx-1bb21d",
    ]);
    let their = run_in(&theirs.0, &ml, &["delete", "alpha", "mx-1bb21d"]);
    assert_eq!(our.code, 1);
    assert_eq!(their.code, 1);
    let suffix = format!(". Line: {preview}\n");
    assert!(normalize_dir(&our.stderr, &ours.0).ends_with(&suffix));
    assert!(normalize_dir(&their.stderr, &theirs.0).ends_with(&suffix));

    // `move` reads only the SOURCE strictly: a malformed line in the
    // target file neither blocks the move nor changes the append.
    let target = format!("{BETA_ONE_LINE}\n{MALFORMED_LINE}\n");
    for (index, extra) in [&[] as &[&str], &["--json"]].into_iter().enumerate() {
        let mut args = vec!["move", "alpha", "mx-1bb21d", "beta"];
        args.extend_from_slice(extra);
        let (ours, theirs) = twin_seeded(
            &format!("maltgt{index}"),
            None,
            &[
                ("alpha", format!("{ALPHA_ONE}\n").as_str()),
                ("beta", target.as_str()),
            ],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "{args:?} exit");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
        assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
        assert!(read_store_file(&ours.0, "expertise/beta.jsonl").contains(MALFORMED_LINE));
    }
}

#[test]
fn unknown_record_type_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{UNKNOWN_TYPE_LINE}\n");
    let expected = "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/alpha.jsonl:2 (id=mx-zzz111). Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.\n";
    let commands: [&[&str]; 3] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete-domain", "alpha", "--yes"],
        &["move", "alpha", "mx-1bb21d", "beta"],
    ];
    for (index, args) in commands.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("unk{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, expected);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&theirs.0, "expertise/alpha.jsonl"), seed);
    }

    // Without an id the `(id=…)` part is omitted.
    let id_less = "{\"type\":\"zzzcustom\",\"name\":\"Odd\"}";
    let expected = "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/alpha.jsonl:1. Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.\n";
    let (ours, theirs) = twin_seeded("unknoid", None, &[("alpha", id_less)], &[]);
    let args = ["delete", "alpha", "mx-1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(&ours, &our, &theirs, &their, expected);

    // `--allow-unknown-types` reads through the unknown line; a KNOWN
    // record can then be removed while the unknown record survives.
    for (index, args) in [
        &["delete", "alpha", "mx-1bb21d", "--allow-unknown-types"] as &[&str],
        &[
            "move",
            "alpha",
            "mx-1bb21d",
            "beta",
            "--allow-unknown-types",
        ],
        &[
            "delete",
            "alpha",
            "--all-except",
            "mx-zzz111",
            "--allow-unknown-types",
        ],
    ]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("unkallow{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{args:?} exit");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
        assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
    }
}

#[test]
fn delete_all_except_unknown_id_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let expected = "Error: Record \"mx-ffffff\" not found. Run `mulch query` to see record IDs.\n";
    let cases: [(&[&str], &str); 2] = [
        (&["delete", "alpha", "--all-except", "mx-ffffff"], expected),
        (
            &["delete", "alpha", "--all-except", "mx-ffffff", "--json"],
            "{\n  \"success\": false,\n  \"command\": \"delete\",\n  \"error\": \"Record \\\"mx-ffffff\\\" not found. Run `mulch query` to see record IDs.\"\n}\n",
        ),
    ];
    for (index, (args, expected)) in cases.into_iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("except{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, expected);
        // The keep-list is resolved before anything is written.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&theirs.0, "expertise/alpha.jsonl"), seed);
    }
}

#[test]
fn identifier_resolution_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let survivor = format!("{ALPHA_TWO}\n");
    // Exact id, bare hash and a unique prefix all resolve; delete and
    // move use the same rule.
    let accepted: [&[&str]; 4] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete", "alpha", "1bb21d"],
        &["delete", "alpha", "mx-1b"],
        &["delete", "alpha", "1bb21"],
    ];
    for (index, args) in accepted.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("res{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(
            our.stdout, "✓ Deleted pattern mx-1bb21d from alpha: Alpha One\n",
            "{args:?} stdout"
        );
        assert_eq!(our.stdout, their.stdout, "{args:?} reference stdout");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), survivor);
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    }

    let moved: [&str; 3] = ["mx-1bb21d", "1bb21d", "mx-1b"];
    for (index, id) in moved.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("resmv{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let args = ["move", "alpha", id, "beta"];
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(
            our.stdout,
            "✓ Moved pattern mx-1bb21d from alpha → beta: Alpha One\n"
        );
        assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
    }

    // Two live records share the `mx-` prefix: ambiguous, nothing is
    // written, in both plain and json mode.
    let ambiguous = "Ambiguous identifier \"mx-\" matches 2 records: mx-1bb21d, mx-c7129f. Use more characters to disambiguate.";
    let expected = format!("Error: {ambiguous}\n");
    for (index, args) in [&["delete", "alpha", "mx-"] as &[&str], &[
        "move", "alpha", "mx-", "beta",
    ]]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("amb{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, &expected);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&theirs.0, "expertise/alpha.jsonl"), seed);
    }

    let (ours, theirs) = twin_seeded(
        "ambjson",
        None,
        &[
            ("alpha", seed.as_str()),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
        ],
        &[],
    );
    let args = ["move", "alpha", "mx-", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("move", ambiguous),
    );
}

// ---- sprint 10 (mulch-5f8a): required common fields + batch enrichment ----

#[test]
fn required_common_fields_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Hand-written stores (the `ml record` fixture would always carry
    // classification + recorded_at, which hid this gap for 9 sprints).
    let stores: [(&str, &str); 4] = [
        (
            "missing-both",
            "{\"type\":\"convention\",\"content\":\"c\",\"id\":\"mx-aaaaaa\"}\n",
        ),
        (
            "missing-recorded",
            "{\"type\":\"pattern\",\"name\":\"n\",\"description\":\"d\",\"classification\":\"tactical\",\"id\":\"mx-bbbbbb\"}\n",
        ),
        (
            "const-mix",
            "{\"type\":\"convention\",\"name\":\"n\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"id\":\"mx-cccccc\"}\n",
        ),
        (
            "no-type",
            "{\"name\":\"n\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"id\":\"mx-dddddd\"}\n",
        ),
    ];
    for (index, (tag, corpus)) in stores.iter().enumerate() {
        // Plain `doctor` also runs the network update check (a known
        // deviation), so it is compared by exit code only; the json
        // shape is byte-comparable.
        let (ours, theirs) = twin_seeded(&format!("req{index}"), None, &[("alpha", *corpus)], &[]);
        for args in [
            &["validate"] as &[&str],
            &["--json", "validate"],
            &["--json", "doctor"],
            &["doctor"],
        ] {
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            let their = run_in(&theirs.0, &ml, args);
            assert_eq!(our.code, their.code, "{tag} {args:?} exit code");
            assert_eq!(our.code, 1, "{tag} {args:?} must fail like the reference");
            if args.contains(&"doctor") && !args.contains(&"--json") {
                // doctor also runs the network update check (known
                // deviation): compare only the failure line
                assert!(
                    our.stdout.contains("failed schema validation"),
                    "{tag} plain doctor reports the failure"
                );
            } else if args.contains(&"doctor") {
                // json doctor: compare the schema-validation check and
                // the fail count (the upgrade check is network-bound)
                let ours_json: serde_json::Value =
                    serde_json::from_str(&our.stdout).expect("our json");
                let theirs_json: serde_json::Value =
                    serde_json::from_str(&their.stdout).expect("their json");
                let pick = |value: &serde_json::Value| {
                    value["checks"]
                        .as_array()
                        .expect("checks")
                        .iter()
                        .find(|check| check["name"] == "schema-validation")
                        .cloned()
                        .expect("schema-validation check")
                };
                assert_eq!(
                    pick(&ours_json),
                    pick(&theirs_json),
                    "{tag} schema-validation check"
                );
                assert_eq!(
                    ours_json["summary"]["fail"], theirs_json["summary"]["fail"],
                    "{tag} fail count"
                );
            } else {
                assert_eq!(
                    normalize_dir(&our.stdout, &ours.0),
                    normalize_dir(&their.stdout, &theirs.0),
                    "{tag} {args:?} stdout"
                );
                // plain validate renders the details on STDERR — the
                // gap that hid the const-mix spacing regression
                assert_eq!(
                    normalize_dir(&our.stderr, &ours.0),
                    normalize_dir(&their.stderr, &theirs.0),
                    "{tag} {args:?} stderr"
                );
            }
        }
    }

    // A batch record without the common fields is ENRICHED before
    // validation (reference `processStdinRecords`), so it is accepted.
    let batch = "[{\"type\":\"guide\",\"name\":\"g\",\"description\":\"gd\"}]";
    let (ours, theirs) = twin_seeded("reqbatch", None, &[("alpha", "")], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch.json"), batch).expect("batch file");
    }
    let args = ["record", "alpha", "--batch", "batch.json", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(normalize(&our.stdout), normalize(&their.stdout));
    let ours_store = read_store_file(&ours.0, "expertise/alpha.jsonl");
    let theirs_store = read_store_file(&theirs.0, "expertise/alpha.jsonl");
    assert_eq!(normalize_line(&ours_store), normalize_line(&theirs_store));
    assert!(
        ours_store.contains("\"classification\":\"tactical\""),
        "batch enrichment fills classification: {ours_store}"
    );

    // an explicit recorded_at/classification survive the enrichment
    // (`if (!("recorded_at" in record))` — the values must not be
    // overwritten; timestamps are masked by normalize, so assert the
    // literal string survives on BOTH sides)
    let fixed = "2019-01-02T03:04:05.678Z";
    let batch_kept = format!(
        "[{{\"type\":\"guide\",\"name\":\"g2\",\"description\":\"gd2\",\"recorded_at\":\"{fixed}\",\"classification\":\"foundational\"}}]"
    );
    let (ours, theirs) = twin_seeded("reqbatch2", None, &[("alpha", "")], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch.json"), &batch_kept).expect("batch file");
    }
    let args = ["record", "alpha", "--batch", "batch.json", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(normalize(&our.stdout), normalize(&their.stdout));
    let ours_store = read_store_file(&ours.0, "expertise/alpha.jsonl");
    let theirs_store = read_store_file(&theirs.0, "expertise/alpha.jsonl");
    assert!(
        ours_store.contains(fixed),
        "our enrichment must keep the explicit recorded_at: {ours_store}"
    );
    assert!(
        theirs_store.contains(fixed),
        "reference keeps the explicit recorded_at: {theirs_store}"
    );
    assert!(
        ours_store.contains("\"classification\":\"foundational\""),
        "our enrichment must keep the explicit classification: {ours_store}"
    );
    assert_eq!(normalize_line(&ours_store), normalize_line(&theirs_store));

    // Batch FAILURE surfaces: the entry carries the sub-errors without
    // a prefix, a type hint only for registered types, and non-object
    // items read `must be object`.
    let failing = [
        ("notype", "[{\"name\":\"n\",\"description\":\"d\"}]"),
        ("typed", "[{\"type\":\"pattern\",\"name\":\"n\"}]"),
        ("notobj", "[\"oops\"]"),
    ];
    for (index, (tag, batch)) in failing.iter().enumerate() {
        let (ours, theirs) = twin_seeded(&format!("reqfail{index}"), None, &[("alpha", "")], &[]);
        for dir in [&ours.0, &theirs.0] {
            std::fs::write(dir.join("batch.json"), batch).expect("batch file");
        }
        for args in [&["record", "alpha", "--batch", "batch.json"] as &[&str], &[
            "--json",
            "record",
            "alpha",
            "--batch",
            "batch.json",
        ]] {
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            let their = run_in(&theirs.0, &ml, args);
            assert_eq!(our.code, their.code, "{tag} {args:?} exit");
            assert_eq!(
                normalize_dir(&our.stdout, &ours.0),
                normalize_dir(&their.stdout, &theirs.0),
                "{tag} {args:?} stdout"
            );
            assert_eq!(
                normalize_dir(&our.stderr, &ours.0),
                normalize_dir(&their.stderr, &theirs.0),
                "{tag} {args:?} stderr"
            );
        }
    }

    // `--files` on a type that does not declare it is DROPPED (the
    // reference collects required ∪ optional fields only)
    for (index, kind) in ["guide", "failure", "decision"].iter().enumerate() {
        let (ours, theirs) = twin_seeded(&format!("b8cafiles{index}"), None, &[("alpha", "")], &[]);
        let payload = match *kind {
            "guide" => vec!["guide", "--name", "g", "--description", "d"],
            "failure" => vec!["failure", "--description", "d", "--resolution", "r"],
            _ => vec!["decision", "--title", "t", "--rationale", "r"],
        };
        let mut args = vec!["record", "alpha", "--type"];
        args.extend(payload);
        args.extend(["--files", "a.ts"]);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "{kind} --files exit");
        assert_eq!(our.code, 0, "{kind} --files must succeed");
        assert_eq!(our.stdout, their.stdout, "{kind} --files stdout");
        let ours_store = read_store_file(&ours.0, "expertise/alpha.jsonl");
        let theirs_store = read_store_file(&theirs.0, "expertise/alpha.jsonl");
        assert!(
            !ours_store.contains("files"),
            "{kind} must drop --files: {ours_store}"
        );
        assert_eq!(normalize_line(&ours_store), normalize_line(&theirs_store));
    }

    // Flag-path json failure: the envelope carries the validate-style
    // text (not the plain rendering).
    let (ours, theirs) = twin_seeded("reqflagjson", None, &[("alpha", "")], &[]);
    let args = [
        "--json",
        "record",
        "alpha",
        "--type",
        "pattern",
        "--name",
        "n",
        "--description",
        "d",
        "--relates-to",
        "bogus",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stderr, their.stderr, "flag-path json envelope");
}

// ---- sprint 14 (mulch-b8ca): full subschema model ----

#[test]
fn schema_suberrors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Each case was probe-fitted against ml 0.10.7: the branch reports
    // its FIRST failure in required -> additionalProperties ->
    // properties(declaration order) order.
    let cases: [(&str, &str); 9] = [
        (
            "additional",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"bogus\":1}\n",
        ),
        (
            "files-on-guide",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"files\":[]}\n",
        ),
        (
            "classification-enum",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"weird\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\"}\n",
        ),
        (
            "description-type",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":123,\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\"}\n",
        ),
        (
            "name-null",
            "{\"type\":\"guide\",\"name\":null,\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\"}\n",
        ),
        (
            "id-pattern",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"id\":\"XX-bad\"}\n",
        ),
        (
            "tags-type",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"tags\":\"nope\"}\n",
        ),
        (
            "relates-to-item",
            "{\"type\":\"pattern\",\"name\":\"p\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"relates_to\":[\"ok:mx-abcd12\",\"bad\"]}\n",
        ),
        (
            "status-enum",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"status\":\"weird\"}\n",
        ),
    ];
    for (index, (tag, corpus)) in cases.iter().enumerate() {
        for args in [&["validate"] as &[&str], &["--json", "validate"]] {
            let (ours, theirs) = twin_seeded(&format!("b8ca{index}"), None, &[("d", *corpus)], &[]);
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            let their = run_in(&theirs.0, &ml, args);
            assert_eq!(our.code, their.code, "{tag} {args:?} exit");
            assert_eq!(our.code, 1, "{tag} {args:?} must fail like the reference");
            assert_eq!(
                normalize_dir(&our.stdout, &ours.0),
                normalize_dir(&their.stdout, &theirs.0),
                "{tag} {args:?} stdout"
            );
            assert_eq!(
                normalize_dir(&our.stderr, &ours.0),
                normalize_dir(&their.stderr, &theirs.0),
                "{tag} {args:?} stderr"
            );
        }
    }
}

// ---- sprint 11 (mulch-ca49): batch summary surfaces + compact rewrite ----

#[test]
fn batch_summary_surfaces_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // canonical store: a pattern (named-dup target) + a convention
    // (anon-dup target)
    let seed = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"name\":\"p\",\"description\":\"d\",\"id\":\"mx-f16294\"}\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"same\",\"id\":\"mx-ef8fb3\"}\n";
    // batch: 2 creates + 1 named dup (update) + 1 anon dup (skip)
    let mixed = "[{\"type\":\"guide\",\"name\":\"g1\",\"description\":\"gd\"},{\"type\":\"failure\",\"description\":\"f1\",\"resolution\":\"r1\"},{\"type\":\"pattern\",\"name\":\"p\",\"description\":\"up\"},{\"type\":\"convention\",\"content\":\"same\"}]";
    let empty = "[]";

    // dry-run plain: Would process counts created+updated; Create/
    // Update/Skip lines only when non-zero
    let (ours, theirs) = twin_seeded("bs-dry", None, &[("d", seed)], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("mixed.json"), mixed).expect("batch file");
    }
    let args = ["record", "d", "--batch", "mixed.json", "--dry-run"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout, "dry-run summary lines");
    assert_eq!(
        our.stdout,
        "✓ Dry-run complete. Would process 3 record(s) in d:\n  Create: 2\n  Update: 1\n  Skip: 1\n  Run without --dry-run to apply changes.\n"
    );

    // normal plain: created/updated/skipped lines, store identical
    let (ours, theirs) = twin_seeded("bs-run", None, &[("d", seed)], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("mixed.json"), mixed).expect("batch file");
    }
    let args = ["record", "d", "--batch", "mixed.json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout, "normal summary lines");
    assert_eq!(
        our.stdout,
        "✓ Created 2 record(s) in d\n✓ Updated 1 record(s) in d\nSkipped 1 duplicate(s) in d\n"
    );
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // all-duplicates dry-run: Would process 0 with only a Skip line
    let (ours, theirs) = twin_seeded("bs-alldup", None, &[("d", seed)], &[]);
    let alldup = "[{\"type\":\"convention\",\"content\":\"same\"},{\"type\":\"pattern\",\"name\":\"p\",\"description\":\"again\"}]";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("alldup.json"), alldup).expect("batch file");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "alldup.json",
        "--dry-run",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "record",
        "d",
        "--batch",
        "alldup.json",
        "--dry-run",
    ]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout, "all-duplicates dry-run");
    assert_eq!(
        our.stdout,
        "✓ Dry-run complete. Would process 1 record(s) in d:\n  Update: 1\n  Skip: 1\n  Run without --dry-run to apply changes.\n"
    );

    // empty batch: nothing in normal mode, the notice in dry-run
    let (ours, theirs) = twin_seeded("bs-empty", None, &[("d", seed)], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("empty.json"), empty).expect("batch file");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "empty.json",
    ]);
    let their = run_in(&theirs.0, &ml, &["record", "d", "--batch", "empty.json"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stdout, "", "zero batch prints nothing");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "empty.json",
        "--dry-run",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "record",
        "d",
        "--batch",
        "empty.json",
        "--dry-run",
    ]);
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stdout, "No records would be processed.\n");

    let commented = format!("# banner\n\n{seed}\n\n");

    // skip-only batch: the reference does NOT write (guard
    // `created > 0 || updated > 0`), so comments survive untouched
    let (ours, theirs) = twin_seeded("bs-skiponly", None, &[("d", &commented)], &[]);
    let skiponly = "[{\"type\":\"convention\",\"content\":\"same\"}]";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("skiponly.json"), skiponly).expect("batch file");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "skiponly.json",
    ]);
    let their = run_in(&theirs.0, &ml, &["record", "d", "--batch", "skiponly.json"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout, "skip-only summary");
    assert_eq!(our.stdout, "Skipped 1 duplicate(s) in d\n");
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        commented,
        "skip-only batch must not write"
    );

    // create-only batch on a commented store rewrites compactly on
    // both sides (comments and blank lines drop)
    let (ours, theirs) = twin_seeded("bs-compact", None, &[("d", &commented)], &[]);
    let one = "[{\"type\":\"guide\",\"name\":\"g2\",\"description\":\"gd2\"}]";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("one.json"), one).expect("batch file");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "d", "--batch", "one.json",
    ]);
    let their = run_in(&theirs.0, &ml, &["record", "d", "--batch", "one.json"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout);
    let ours_store = read_store_file(&ours.0, "expertise/d.jsonl");
    let theirs_store = read_store_file(&theirs.0, "expertise/d.jsonl");
    assert!(!ours_store.contains("# banner"), "comments drop");
    assert_eq!(
        normalize_line(&ours_store),
        normalize_line(&theirs_store),
        "compact rewrite identical"
    );
}

// ---- sprint 9 (mulch-351d): edit/outcome identifier resolution ----

#[test]
fn edit_outcome_identifier_resolution_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");

    // edit and outcome resolve bare hashes and unique prefixes; their
    // output surfaces carry the RESOLVED id, not the input.
    for (index, id) in ["1bb21d", "mx-1b", "1bb21"].iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("eoed{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let args = ["edit", "alpha", id, "--description", "touched"];
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(our.stdout, "✓ Updated pattern mx-1bb21d in alpha\n");
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    }
    for (index, id) in ["1bb21d", "mx-1b"].iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("eoout{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let args = ["outcome", "alpha", id, "--status", "success"];
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(our.stdout, "✓ Outcome recorded: success on mx-1bb21d\n");
        let ours_file = read_store_file(&ours.0, "expertise/alpha.jsonl");
        let theirs_file = read_store_file(&theirs.0, "expertise/alpha.jsonl");
        assert_eq!(normalize_line(&ours_file), normalize_line(&theirs_file));
    }

    // Ambiguous prefix: the shared text, nothing written, plain + json.
    let ambiguous = "Ambiguous identifier \"mx-\" matches 2 records: mx-1bb21d, mx-c7129f. Use more characters to disambiguate.";
    let expected = format!("Error: {ambiguous}\n");
    for (index, args) in [
        &["edit", "alpha", "mx-", "--description", "x"] as &[&str],
        &["outcome", "alpha", "mx-", "--status", "success"],
    ]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("eoamb{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_same_stderr(&ours, &our, &theirs, &their, &expected);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }
    let (ours, theirs) = twin_seeded("eoambj", None, &[("alpha", seed.as_str())], &[]);
    let args = ["edit", "alpha", "mx-", "--description", "x", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("edit", ambiguous),
    );

    // outcome LISTING (no --status) resolves prefixes too and prints
    // the resolved id in its header
    let (ours, theirs) = twin_seeded(
        "eolist",
        None,
        &[(
            "alpha",
            format!("{ALPHA_ONE}\nmx-ab1111-placeholder\n").as_str(),
        )],
        &[],
    );
    // craft a store where the first record carries an outcome array
    let with_outcome = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"name\":\"Alpha One\",\"description\":\"d\",\"id\":\"mx-1bb21d\",\"outcomes\":[{\"status\":\"success\"}]}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("alpha.jsonl"),
            with_outcome,
        )
        .expect("store writable");
    }
    let list_prefix = ["outcome", "alpha", "1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &list_prefix);
    let their = run_in(&theirs.0, &ml, &list_prefix);
    assert_eq!(our.code, their.code, "listing resolves the prefix");
    assert_eq!(our.stdout, their.stdout, "listing stdout");
    assert_eq!(our.stdout, "Outcomes for mx-1bb21d (1):\n  1. success\n");

    // a non-object record under a prefix input: both sides report the
    // non-object clean error (reference crashes — DEVIATIONS)
    let scalar_store = "5\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("alpha.jsonl"),
            scalar_store,
        )
        .expect("scalar store writable");
    }
    let our_scalar = run_in(&ours.0, Path::new(mulch_bin()), &[
        "edit",
        "alpha",
        "1bb",
        "--description",
        "x",
    ]);
    assert_eq!(our_scalar.code, 1);
    assert!(
        our_scalar.stderr.contains("non-object record at"),
        "clean scalar error, got: {}",
        our_scalar.stderr
    );

    // json surfaces carry the resolved id too: edit success, outcome
    // append success, outcome listing, and the ambiguous envelope
    let (ours, theirs) = twin_seeded("eojson", None, &[("alpha", seed.as_str())], &[]);
    let edit_json = [
        "--json",
        "edit",
        "alpha",
        "1bb21d",
        "--description",
        "touched",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &edit_json);
    let their = run_in(&theirs.0, &ml, &edit_json);
    assert_eq!(our.code, their.code);
    assert_eq!(
        normalize(&our.stdout),
        normalize(&their.stdout),
        "edit json"
    );
    assert!(
        our.stdout.contains("\"id\": \"mx-1bb21d\""),
        "{}",
        our.stdout
    );

    let (ours, theirs) = twin_seeded("eojson2", None, &[("alpha", seed.as_str())], &[]);
    let out_json = [
        "--json", "outcome", "alpha", "1bb21d", "--status", "success",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &out_json);
    let their = run_in(&theirs.0, &ml, &out_json);
    assert_eq!(our.code, their.code);
    assert_eq!(
        normalize(&our.stdout),
        normalize(&their.stdout),
        "outcome json"
    );

    let listing_json = ["--json", "outcome", "alpha", "mx-1b"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &listing_json);
    let their = run_in(&theirs.0, &ml, &listing_json);
    assert_eq!(our.code, their.code);
    assert_eq!(
        normalize(&our.stdout),
        normalize(&their.stdout),
        "outcome listing json"
    );

    let (ours, theirs) = twin_seeded("eoambout", None, &[("alpha", seed.as_str())], &[]);
    let amb_out_json = ["--json", "outcome", "alpha", "mx-", "--status", "success"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &amb_out_json);
    let their = run_in(&theirs.0, &ml, &amb_out_json);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("outcome", ambiguous),
    );

    // Unknown identifier keeps the not-found text.
    for (index, args) in [
        &["edit", "alpha", "zzz", "--description", "x"] as &[&str],
        &["outcome", "alpha", "zzz", "--status", "success"],
    ]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("eomiss{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1);
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }
}

#[test]
fn delete_domain_preserves_remaining_domain_order() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Removing the FIRST of three domains must not shuffle the rest.
    let (ours, theirs) = twin_seeded(
        "ddorder",
        None,
        &[
            ("alpha", format!("{ALPHA_ONE}\n").as_str()),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ("gamma", ""),
        ],
        &[],
    );
    let args = ["delete-domain", "alpha", "--yes"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0, "delete-domain succeeds");
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stderr, their.stderr);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
    let config = read_store_file(&ours.0, "mulch.config.yaml");
    let expected = format!("version: '1'\ndomains:\n  beta: {{}}\n  gamma: {{}}\n{CONFIG_TAIL}");
    assert_eq!(config, expected, "remaining domains keep their order");
    assert!(!config.contains("alpha"), "removed domain is gone");
    assert!(!ours.0.join(".mulch/expertise/alpha.jsonl").exists());
    assert!(!theirs.0.join(".mulch/expertise/alpha.jsonl").exists());

    // Removing the MIDDLE domain keeps the outer order too.
    let (ours, theirs) = twin_seeded(
        "ddorder2",
        None,
        &[
            ("alpha", ""),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ("gamma", ""),
        ],
        &[],
    );
    let args = ["delete-domain", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
    let config = read_store_file(&ours.0, "mulch.config.yaml");
    let expected = format!("version: '1'\ndomains:\n  alpha: {{}}\n  gamma: {{}}\n{CONFIG_TAIL}");
    assert_eq!(config, expected);

    // A registered domain whose file is already gone still deletes.
    let (ours, theirs) = twin_seeded(
        "ddgone",
        None,
        &[
            ("alpha", ""),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
        ],
        &[],
    );
    for dir in [&ours.0, &theirs.0] {
        std::fs::remove_file(dir.join(".mulch/expertise/alpha.jsonl")).expect("seed file");
    }
    let args = ["delete-domain", "alpha", "--yes"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0, "missing file is tolerated");
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stderr, their.stderr);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
}

#[test]
fn delete_domain_backfills_missing_config_blocks() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // The minimal config (version + domains only) gains the full
    // governance and shelf-life blocks on rewrite.
    let minimal = "version: '1'\ndomains:\n  alpha: {}\n  beta: {}\n";
    let (ours, theirs) = twin_seeded(
        "ddmin",
        Some(minimal),
        &[("alpha", format!("{ALPHA_ONE}\n").as_str()), ("beta", "")],
        &[],
    );
    let args = ["delete-domain", "alpha", "--yes"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
    let config = read_store_file(&ours.0, "mulch.config.yaml");
    let expected = format!("version: '1'\ndomains:\n  beta: {{}}\n{CONFIG_TAIL}");
    assert_eq!(config, expected, "governance and shelf life are backfilled");
    for needle in [
        "max_entries: 100",
        "warn_entries: 150",
        "hard_limit: 200",
        "classification_defaults:",
        "shelf_life:",
        "tactical: 14",
        "observational: 30",
    ] {
        assert!(config.contains(needle), "missing {needle} in {config}");
    }

    // Same minimal config, middle domain removed.
    let (ours, theirs) = twin_seeded(
        "ddmin2",
        Some("version: '1'\ndomains:\n  alpha: {}\n  beta: {}\n  gamma: {}\n"),
        &[("alpha", ""), ("beta", ""), ("gamma", "")],
        &[],
    );
    let args = ["delete-domain", "beta", "--yes"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        format!("version: '1'\ndomains:\n  alpha: {{}}\n  gamma: {{}}\n{CONFIG_TAIL}")
    );
}

#[test]
fn delete_domain_dry_run_json_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let expected = "{\n  \"success\": true,\n  \"command\": \"delete-domain\",\n  \"domain\": \"alpha\",\n  \"dryRun\": true,\n  \"recordCount\": 2\n}\n";
    let cases: [&[&str]; 2] = [&["delete-domain", "alpha", "--dry-run", "--json"], &[
        "delete-domain",
        "alpha",
        "--dry-run",
        "--json",
        "--yes",
    ]];
    for (index, args) in cases.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("dddry{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 0, "{args:?} succeeds");
        assert_eq!(our.stdout, expected, "{args:?} is the json envelope");
        assert_eq!(our.stdout, their.stdout, "{args:?} reference stdout");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        // Dry run: file and config stay untouched.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_same_file(&ours, &theirs, "mulch.config.yaml");
        assert!(ours.0.join(".mulch/expertise/alpha.jsonl").is_file());
    }
}

#[test]
fn delete_mode_and_unknown_domain_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let cases: [(&[&str], String); 8] = [
        (
            &["delete", "alpha"],
            "Error: must provide a record ID, --records, or --all-except.\n".to_string(),
        ),
        (
            &["delete", "alpha", "--json"],
            error_envelope("delete", "Must provide a record ID, --records, or --all-except."),
        ),
        (
            &["delete", "alpha", "--records", ""],
            "Error: --records requires at least one ID.\n".to_string(),
        ),
        (
            &["delete", "alpha", "--records", "", "--json"],
            error_envelope("delete", "--records requires at least one ID."),
        ),
        (
            &["delete", "alpha", "--all-except", ""],
            "Error: --all-except requires at least one ID to keep.\n".to_string(),
        ),
        (
            &["delete", "alpha", "mx-1bb21d", "--records", "mx-c7129f"],
            "Error: cannot combine a record ID with --records or --all-except. Use only one mode.\n"
                .to_string(),
        ),
        (
            &["delete", "nope", "mx-1bb21d"],
            "Error: domain \"nope\" not found in config.\nAvailable domains: alpha, beta\n"
                .to_string(),
        ),
        (
            &["delete", "nope", "mx-1bb21d", "--json"],
            error_envelope(
                "delete",
                "Domain \"nope\" not found in config. Available domains: alpha, beta",
            ),
        ),
    ];
    for (index, (args, expected)) in cases.into_iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("mode{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, &expected);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }

    // `delete-domain` plain carries the add-hint, json the domain list.
    let domain_cases: [(&[&str], String); 2] = [
        (
            &["delete-domain", "nope", "--yes"],
            "Error: domain \"nope\" not found in config.\nHint: Run `mulch add nope` to create it, or check `mulch status` for existing domains.\n"
                .to_string(),
        ),
        (
            &["delete-domain", "nope", "--yes", "--json"],
            error_envelope(
                "delete-domain",
                "Domain \"nope\" not found in config. Available domains: alpha, beta",
            ),
        ),
    ];
    for (index, (args, expected)) in domain_cases.into_iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("movedom{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_same_stderr(&ours, &our, &theirs, &their, &expected);
    }

    // No domains at all renders `(none)`; the list joins with ", ".
    let (ours, theirs) = twin_seeded("modenone", None, &[], &[]);
    let args = ["delete", "nope", "mx-1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        "Error: domain \"nope\" not found in config.\nAvailable domains: (none)\n",
    );

    let (ours, theirs) = twin_seeded(
        "modemulti",
        None,
        &[("alpha", ""), ("beta", ""), ("gamma", "")],
        &[],
    );
    let args = ["delete", "nope", "mx-1bb21d", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope(
            "delete",
            "Domain \"nope\" not found in config. Available domains: alpha, beta, gamma",
        ),
    );
}

#[test]
fn move_same_domain_check_precedes_domain_lookup() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let message = "Error: Source and target domain are the same — nothing to move.\n";
    // Both domains unknown, both known, and only the source known: the
    // same-domain check always wins.
    let cases: [&[&str]; 3] = [
        &["move", "nope", "mx-1bb21d", "nope"],
        &["move", "alpha", "mx-1bb21d", "alpha"],
        &["move", "zeta", "mx-1bb21d", "zeta"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("samedom{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, message);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }

    let (ours, theirs) = twin_seeded(
        "samedomjson",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "nope", "mx-1bb21d", "nope", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope(
            "move",
            "Source and target domain are the same — nothing to move.",
        ),
    );

    // Two DIFFERENT unknown domains fall through to the domain lookup.
    let (ours, theirs) = twin_seeded(
        "unkdoms",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "zeta", "mx-1bb21d", "nope"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        "Error: Domain \"zeta\" not found in config. Available domains: alpha, beta\n",
    );
}

#[test]
fn move_refuses_archived_records_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ARCHIVED_ALPHA_ONE}\n");
    let message = "Error: Record mx-1bb21d is archived. Run `ml restore mx-1bb21d` first.\n";
    for (index, args) in [
        &["move", "alpha", "mx-1bb21d", "beta"] as &[&str],
        &["move", "alpha", "mx-1bb21d", "beta", "--force"],
        &["move", "alpha", "1bb21d", "beta"],
    ]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("arch{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, message);
        // The archived record stays put, the target stays empty.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");
    }

    let (ours, theirs) = twin_seeded(
        "archjson",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope(
            "move",
            "Record mx-1bb21d is archived. Run `ml restore mx-1bb21d` first.",
        ),
    );
}

#[test]
fn move_required_fields_gate_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let message = "Record is missing field(s) required by target domain \"beta\": \"owner\". Edit the record (`ml edit mx-1bb21d`) before moving.";
    // `--force` bypasses allowed_types only; required_fields always holds.
    for (index, args) in [
        &["move", "alpha", "mx-1bb21d", "beta"] as &[&str],
        &["move", "alpha", "mx-1bb21d", "beta", "--force"],
        &["move", "alpha", "1bb21d", "beta"],
    ]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("req{index}"),
            Some(REQUIRED_FIELDS_CONFIG),
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        // Reference plain text carries the `Error: ` prefix (the json
        // field drops it — see the envelope case below).
        assert_same_stderr(&ours, &our, &theirs, &their, &format!("Error: {message}\n"));
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");
    }

    let (ours, theirs) = twin_seeded(
        "reqjson",
        Some(REQUIRED_FIELDS_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("move", message),
    );

    // A target that does not require the field moves normally.
    let (ours, theirs) = twin_seeded("reqok", None, &[("alpha", seed.as_str()), ("beta", "")], &[
    ]);
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
}

#[test]
fn move_allowed_types_gate_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let message = "Type \"pattern\" is not in target domain \"beta\" allowed_types (convention). Pass --force to override, or adjust mulch.config.yaml.";
    let (ours, theirs) = twin_seeded(
        "allowtype",
        Some(ALLOWED_TYPES_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1, "the gate refuses");
    assert_eq!(our.stdout, "", "nothing on stdout");
    assert_same_stderr(&ours, &our, &theirs, &their, &format!("Error: {message}\n"));
    assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");

    let (ours, theirs) = twin_seeded(
        "allowtypejson",
        Some(ALLOWED_TYPES_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("move", message),
    );

    // `--force` overrides the allowed_types gate.
    let (ours, theirs) = twin_seeded(
        "allowtypeforce",
        Some(ALLOWED_TYPES_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--force"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0, "--force overrides allowed_types");
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stderr, their.stderr);
    assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
}

#[test]
fn move_incoming_references_scan_orphan_files_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // The source and the target are skipped, an ORPHAN file (present on
    // disk, absent from the config) is scanned. One referrer has an id,
    // one has none.
    let ref_source = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:00.000Z\",\"name\":\"In Alpha\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"],\"id\":\"mx-aaa111\"}";
    let ref_target = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:00.500Z\",\"name\":\"In Beta\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"],\"id\":\"mx-bbb111\"}";
    let ref_orphan = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:01.000Z\",\"name\":\"In Delta\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"],\"id\":\"mx-ddd222\"}";
    let ref_orphan_no_id = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:02.000Z\",\"name\":\"No Id\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"]}";
    let domains = [
        ("alpha", format!("{ALPHA_ONE}\n{ref_source}\n")),
        ("beta", format!("{BETA_ONE_LINE}\n{ref_target}\n")),
    ];
    let orphans = [("delta.jsonl", format!("{ref_orphan}\n{ref_orphan_no_id}\n"))];
    let seeds: Vec<(&str, &str)> = domains
        .iter()
        .map(|(domain, body)| (*domain, body.as_str()))
        .collect();
    let orphan_seeds: Vec<(&str, &str)> = orphans
        .iter()
        .map(|(name, body)| (*name, body.as_str()))
        .collect();

    let expected_json = "{\n  \"success\": true,\n  \"command\": \"move\",\n  \"sourceDomain\": \"alpha\",\n  \"targetDomain\": \"beta\",\n  \"record\": {\n    \"id\": \"mx-1bb21d\",\n    \"type\": \"pattern\",\n    \"summary\": \"Alpha One\"\n  },\n  \"incomingReferences\": [\n    {\n      \"domain\": \"delta\",\n      \"id\": \"mx-ddd222\",\n      \"field\": \"relates_to\"\n    },\n    {\n      \"domain\": \"delta\",\n      \"id\": null,\n      \"field\": \"relates_to\"\n    }\n  ]\n}\n";
    let expected_plain = "✓ Moved pattern mx-1bb21d from alpha → beta: Alpha One\n  2 inbound reference(s) found; ID preserved so existing links still resolve:\n    delta/mx-ddd222 via relates_to\n    delta/(no id) via relates_to\n";
    for (index, (extra, expected)) in [(&[][..], expected_plain), (&["--json"][..], expected_json)]
        .into_iter()
        .enumerate()
    {
        let tag = format!("incref{index}");
        let (ours, theirs) = twin_seeded(&tag, None, &seeds, &orphan_seeds);
        let mut args = vec!["move", "alpha", "mx-1bb21d", "beta"];
        args.extend_from_slice(extra);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, 0, "{args:?} succeeds");
        assert_eq!(
            our.stdout, expected,
            "{args:?} reports the orphan referrers"
        );
        assert_eq!(our.stdout, their.stdout, "{args:?} reference stdout");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        // Referrers living in the source or target file are not listed.
        assert!(!our.stdout.contains("mx-aaa111"), "source domain skipped");
        assert!(!our.stdout.contains("mx-bbb111"), "target domain skipped");
        // The orphan file itself is never rewritten.
        assert_eq!(
            read_store_file(&ours.0, "expertise/delta.jsonl"),
            format!("{ref_orphan}\n{ref_orphan_no_id}\n")
        );
    }
}

#[test]
fn delete_all_except_keeping_all_prints_nothing_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let expected = "{\n  \"success\": true,\n  \"command\": \"delete\",\n  \"domain\": \"alpha\",\n  \"dryRun\": false,\n  \"deleted\": [],\n  \"kept\": 1\n}\n";
    let cases: [(&[&str], &str); 2] = [
        (&["delete", "alpha", "--all-except", "mx-1bb21d"], ""),
        (
            &["delete", "alpha", "--all-except", "mx-1bb21d", "--json"],
            expected,
        ),
    ];
    for (index, (args, expected)) in cases.into_iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("keepall{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 0, "{args:?} succeeds");
        // Zero deletions: plain mode prints NOTHING (not an empty line).
        assert_eq!(our.stdout, expected, "{args:?} stdout");
        assert_eq!(our.stdout, their.stdout, "{args:?} reference stdout");
        assert_eq!(our.stderr, "", "{args:?} no stderr");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    }
}

#[test]
fn delete_and_move_truncate_convention_summary_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{}\n", long_convention_line());
    let (ours, theirs) = twin_seeded("trunc", None, &[("alpha", seed.as_str())], &[]);
    let args = ["delete", "alpha", "mx-1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        format!(
            "✓ Deleted convention mx-1bb21d from alpha: {}\n",
            truncated_summary()
        )
    );
    assert_eq!(our.stdout, their.stdout, "reference truncates the same way");

    let (ours, theirs) = twin_seeded(
        "truncmv",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        format!(
            "✓ Moved convention mx-1bb21d from alpha → beta: {}\n",
            truncated_summary()
        )
    );
    assert_eq!(our.stdout, their.stdout);
}

#[test]
fn delete_reserializes_survivors_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // A survivor written with spaces must come back compact, and a
    // survivor without an id gains a generated one.
    let spaced = "{ \"type\": \"pattern\" ,  \"classification\":\"tactical\", \"recorded_at\":\"2026-10-03T20:47:57.690Z\", \"name\":\"Spaced\", \"description\":\"d\", \"id\":\"mx-1bb21d\" }";
    let id_less = "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.000Z\",\"content\":\"no id here\"}";
    let seed = format!("{spaced}\n{ALPHA_TWO}\n{id_less}\n");
    let (ours, theirs) = twin_seeded("reser", None, &[("alpha", seed.as_str())], &[]);
    let args = ["delete", "alpha", "mx-c7129f"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    let file = read_store_file(&ours.0, "expertise/alpha.jsonl");
    let lines: Vec<&str> = file.lines().collect();
    assert_eq!(lines.len(), 2, "two survivors, one record per line");
    assert!(
        lines[0].starts_with("{\"type\":\"pattern\""),
        "compacted survivor: {}",
        lines[0]
    );
    assert!(
        !file.contains("\"type\": \""),
        "spaces are stripped: {file}"
    );
    assert!(
        lines[1].starts_with("{\"type\":\"convention\""),
        "compacted survivor: {}",
        lines[1]
    );
    assert!(
        lines[1].ends_with("\"id\":\"mx-5670d0\"}"),
        "generated id for the id-less survivor: {}",
        lines[1]
    );
    assert!(file.ends_with('\n'), "the file keeps its trailing newline");

    // `move` compacts the target append and the source survivors alike.
    let (ours, theirs) = twin_seeded(
        "resermv",
        None,
        &[
            ("alpha", format!("{spaced}\n{id_less}\n").as_str()),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
        ],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
    let target = read_store_file(&ours.0, "expertise/beta.jsonl");
    assert_eq!(
        target.lines().count(),
        2,
        "beta keeps its line and gains one"
    );
    assert!(target.ends_with('\n'));
    let compact_moved = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Spaced\",\"description\":\"d\",\"id\":\"mx-1bb21d\"}";
    assert_eq!(
        target.lines().nth(1),
        Some(compact_moved),
        "the moved record is appended compact"
    );
    let source = read_store_file(&ours.0, "expertise/alpha.jsonl");
    assert_eq!(source.lines().count(), 1);
    assert!(source.contains("\"id\":\"mx-5670d0\"}"), "id generated");
}

#[test]
fn delete_duplicate_records_delete_once_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let survivor = format!("{ALPHA_TWO}\n");
    let (ours, theirs) = twin_seeded("dup", None, &[("alpha", seed.as_str())], &[]);
    let args = ["delete", "alpha", "--records", "mx-1bb21d,mx-1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    // One deletion line, no bulk tally.
    assert_eq!(
        our.stdout, "Deleted pattern mx-1bb21d from alpha: Alpha One\n",
        "a repeated id deletes once"
    );
    assert_eq!(our.stdout, their.stdout);
    assert!(
        !our.stdout.contains("✓ Deleted 2 records"),
        "no phantom second deletion: {}",
        our.stdout
    );
    assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), survivor);
    assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");

    let (ours, theirs) = twin_seeded("dupjson", None, &[("alpha", seed.as_str())], &[]);
    let args = [
        "delete",
        "alpha",
        "--records",
        "mx-1bb21d,mx-1bb21d",
        "--json",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"delete\",\n  \"domain\": \"alpha\",\n  \"dryRun\": false,\n  \"deleted\": [\n    {\n      \"id\": \"mx-1bb21d\",\n      \"type\": \"pattern\",\n      \"summary\": \"Alpha One\"\n    }\n  ],\n  \"kept\": 1\n}\n"
    );
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
}
