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
