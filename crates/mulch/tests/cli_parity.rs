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
