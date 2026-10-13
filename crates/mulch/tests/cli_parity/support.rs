//! The battery's shared harness (mulch-3e7c): temp dirs, twin-store
//! runs, reference discovery, and the cross-surface assertion and
//! normalization helpers — every helper lived in one flat file before;
//! they moved here once, verbatim, with `pub(crate)` visibility.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Our binary under test.
pub(crate) fn mulch_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mulch")
}

/// The reference CLI, when installed.
pub(crate) fn reference_ml() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("ml"))
        .find(|ml| ml.is_file())
}

/// One captured run.
#[derive(Debug)]
pub(crate) struct Run {
    pub(crate) code:   i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

pub(crate) fn run_in(dir: &Path, program: &Path, args: &[&str]) -> Run {
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

pub(crate) struct TempDir(pub(crate) PathBuf);

impl TempDir {
    pub(crate) fn new(tag: &str) -> Self {
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
pub(crate) fn normalize(text: &str) -> String {
    regex_free_iso(text)
}

/// The Retry/Example echo promotes THIS binary (user directive
/// 2026-10-13: no ml references in the shipped binary — README
/// DEVIATIONS): ours says `mulch record` where the reference echoes
/// its own name. Byte comparisons normalize the binary name; the
/// deviation itself is pinned by `record_echo_promotes_mulch`.
pub(crate) fn echo_ours_to_reference(text: &str) -> String {
    text.replace("mulch record", "ml record")
}

/// ISO-8601 timestamps -> `<TS>` without a regex dependency.
pub(crate) fn regex_free_iso(text: &str) -> String {
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
pub(crate) fn looks_like_iso(window: &str) -> bool {
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
pub(crate) fn read_store_file(dir: &Path, relative: &str) -> String {
    String::from_utf8_lossy(
        &std::fs::read(dir.join(".mulch").join(relative)).expect("store file exists"),
    )
    .into_owned()
}

/// Parses a doctor JSON report, neutralizes the environment-dependent
/// upgrade check and the summary counts, and re-serializes
/// deterministically for comparison.
pub(crate) fn normalize_doctor_json(text: &str) -> String {
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

/// The `recorded_at` value inside a freshly written domain file.
pub(crate) fn recent_timestamp(file: &Path) -> String {
    let text = std::fs::read_to_string(file).expect("domain file");
    let start = text
        .find("\"recorded_at\":\"")
        .expect("recorded_at present")
        + 15;
    let rest = &text[start..];
    rest[..24].to_string()
}

// ---- sprint 15 (mulch-7ac9): doctor duplicates + divergent ids ----

/// Shared twin-store fixture: `ml init` + one base pattern p1.
pub(crate) fn duration_fixture(dir: &Path, ml: &Path) {
    let _ = run_in(dir, ml, &["init"]);
    let _ = run_in(dir, ml, &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p1",
        "--description",
        "d1",
    ]);
}

/// The reference record id of pattern name "p" (sha256 rule).
pub(crate) fn pattern_p_id() -> String {
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
pub(crate) fn normalize_line(text: &str) -> String {
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

/// A deterministic store written by hand (fixed timestamps) so byte
/// comparisons are meaningful.
pub(crate) fn write_seed(dir: &Path, domains: &[(&str, &str)]) {
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

pub(crate) const ALPHA_ONE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Alpha One\",\"description\":\"first alpha pattern\",\"id\":\"mx-1bb21d\"}";

pub(crate) const ALPHA_TWO: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Alpha Two\",\"description\":\"second alpha pattern\",\"id\":\"mx-c7129f\"}";

/// The reference's canonical config tail (governance + shelf life).
/// Every mutating config rewrite backfills it, in this byte order.
pub(crate) const CONFIG_TAIL: &str = "governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\nclassification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";

/// A second beta record (fixed timestamp) for cross-domain cases.
pub(crate) const BETA_ONE_LINE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.427Z\",\"name\":\"Beta One\",\"description\":\"first beta pattern\",\"id\":\"mx-3242b5\"}";

/// A physical line that is not valid JSON.
pub(crate) const MALFORMED_LINE: &str = "{\"type\":\"pattern\", BAD";

/// A record whose type is not in the payload registry.
pub(crate) const UNKNOWN_TYPE_LINE: &str = "{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Odd\",\"id\":\"mx-zzz111\"}";

/// An archived variant of [`ALPHA_ONE`] (same id).
pub(crate) const ARCHIVED_ALPHA_ONE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"status\":\"archived\",\"name\":\"Old Alpha\",\"description\":\"archived alpha pattern\",\"id\":\"mx-1bb21d\"}";

/// A target domain that demands a field the alpha record lacks.
pub(crate) const REQUIRED_FIELDS_CONFIG: &str =
    "version: '1'\ndomains:\n  alpha: {}\n  beta:\n    required_fields:\n      - owner\n";

/// A target domain that admits only convention records.
pub(crate) const ALLOWED_TYPES_CONFIG: &str =
    "version: '1'\ndomains:\n  alpha: {}\n  beta:\n    allowed_types:\n      - convention\n";

/// A convention record whose `content` is 80 chars (summary window 60).
pub(crate) fn long_convention_line() -> String {
    format!(
        "{{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"{}\",\"id\":\"mx-1bb21d\"}}",
        "C".repeat(80)
    )
}

/// The truncated form of [`long_convention_line`]'s summary.
pub(crate) fn truncated_summary() -> String {
    format!("{}...", "C".repeat(60))
}

/// [`write_seed`] with a caller-supplied config body.
pub(crate) fn write_seed_with_config(dir: &Path, config: &str, domains: &[(&str, &str)]) {
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
pub(crate) fn write_orphan(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(".mulch").join("expertise").join(name), body).expect("orphan file");
}

/// Twin stores seeded identically (`None` = the default seed config).
pub(crate) fn twin_seeded(
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
pub(crate) fn normalize_dir(text: &str, dir: &Path) -> String {
    text.replace(&dir.display().to_string(), "<DIR>")
}

/// Asserts both stderr streams equal the expected reference text.
pub(crate) fn assert_same_stderr(
    ours: &TempDir,
    our: &Run,
    theirs: &TempDir,
    their: &Run,
    expected: &str,
) {
    assert_eq!(normalize_dir(&our.stderr, &ours.0), expected, "our stderr");
    assert_eq!(
        normalize_dir(&their.stderr, &theirs.0),
        expected,
        "reference stderr"
    );
}

/// Asserts both stores hold the same bytes for a store-relative path.
pub(crate) fn assert_same_file(ours: &TempDir, theirs: &TempDir, relative: &str) {
    assert_eq!(
        read_store_file(&ours.0, relative),
        read_store_file(&theirs.0, relative),
        "{relative} differs"
    );
}

/// The reference's pretty JSON error envelope (stderr channel).
pub(crate) fn error_envelope(command: &str, error: &str) -> String {
    format!(
        "{{\n  \"success\": false,\n  \"command\": \"{command}\",\n  \"error\": \"{}\"\n}}\n",
        error.replace('"', "\\\"")
    )
}

/// Masks `recorded_at` values: twin fixtures record `now` microseconds
/// apart, and the final stores otherwise match byte-for-byte.
pub(crate) fn mask_timestamps(text: &str) -> String {
    const KEY: &str = "\"recorded_at\":\"";
    let mut out = String::new();
    let mut rest = text;
    while let Some(pos) = rest.find(KEY) {
        let value_start = pos + KEY.len();
        out.push_str(&rest[..value_start]);
        rest = &rest[value_start..];
        let end = rest.find('"').expect("timestamp end");
        out.push_str("<TS>");
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The sprint-19 pathological fixture: a comment line
/// (jsonl-integrity), a legacy singular `outcome`, a stale-valid
/// record, a stale AND schema-invalid record, and a fresh valid one —
/// all four repair passes fire in check order. Timestamps derive from
/// the runtime clock (stale a full day past the 14-day tactical
/// shelf; fresh a full day inside it).
pub(crate) fn repair_fixture(dir: &Path, ml: &Path) {
    let now = jiff::Timestamp::now();
    let stale_at = (now - jiff::Span::new().hours(40 * 24)).to_string();
    let fresh_at = (now - jiff::Span::new().hours(24)).to_string();
    let _ = run_in(dir, ml, &["init"]);
    let _ = run_in(dir, ml, &["add", "dev"]);
    let store = [
        "# archived banner".to_string(),
        format!(
            r#"{{"type":"pattern","name":"stale-valid","description":"d","classification":"tactical","recorded_at":"{stale_at}"}}"#
        ),
        // stale AND invalid (no payload fields): the schema pass must
        // claim it (Removed), never the stale pass (Pruned).
        format!(
            r#"{{"type":"pattern","classification":"tactical","recorded_at":"{stale_at}"}}"#
        ),
        format!(
            r#"{{"type":"pattern","name":"legacy","description":"d","classification":"tactical","recorded_at":"{fresh_at}","outcome":{{"status":"success","duration":5}}}}"#
        ),
    ]
    .join("\n")
        + "\n";
    std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(dir.join(".mulch/expertise/dev.jsonl"), store).expect("dev store");
}

/// A crafted store with one record per legacy `outcome` value shape:
/// number, string, null, array, empty object, proper object (control)
/// — plus one mid-object `outcome` key to pin the trailing position
/// of the migrated `outcomes` array through the strict-read rewrite.
pub(crate) fn odd_legacy_fixture(dir: &Path, ml: &Path) {
    let _ = run_in(dir, ml, &["init"]);
    let _ = run_in(dir, ml, &["add", "probe"]);
    let base = r#"{"type":"pattern","name":"{name}","description":"d","classification":"tactical","recorded_at":"2026-10-01T00:00:00.000Z","id":"{id}","outcome":{outcome}}"#;
    let rows = [
        ("num", "mx-111111", "5"),
        ("str", "mx-222222", "\"x\""),
        ("nul", "mx-333333", "null"),
        ("arr", "mx-444444", "[1]"),
        ("emp", "mx-555555", "{}"),
        (
            "obj",
            "mx-666666",
            "{\"status\":\"success\",\"agent\":\"a\"}",
        ),
    ];
    let mut store = rows
        .iter()
        .map(|(name, id, outcome)| {
            base.replace("{name}", name)
                .replace("{id}", id)
                .replace("{outcome}", outcome)
        })
        .collect::<Vec<_>>();
    // mid-object outcome: the rewrite must append outcomes at the END
    store.push(
        r#"{"type":"pattern","outcome":{"status":"success"},"name":"mid","description":"d","classification":"tactical","recorded_at":"2026-10-01T00:00:00.000Z","id":"mx-777777"}"#
            .to_string(),
    );
    std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(
        dir.join(".mulch/expertise/probe.jsonl"),
        store.join("\n") + "\n",
    )
    .expect("probe store");
}

/// One crafted store shared by the gate probes: five convention
/// records (one valid, one content-less, one with an extra field,
/// one with a wrong-typed payload, one with a legacy singular
/// outcome) plus a pattern. Returns marker → id (ids are content
/// hashes, identical in both stores) captured BEFORE the markers
/// are rewritten away.
pub(crate) fn craft_gate_store(dir: &Path, ml: &Path) -> std::collections::HashMap<String, String> {
    let _ = run_in(dir, ml, &["init"]);
    let _ = run_in(dir, ml, &["add", "d"]);
    let _ = run_in(dir, ml, &["add", "d2"]);
    let _ = run_in(dir, ml, &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "ok body",
    ]);
    let _ = run_in(dir, ml, &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "contentless body",
    ]);
    let _ = run_in(dir, ml, &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "extra body",
    ]);
    let _ = run_in(dir, ml, &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "wrongtyped body",
    ]);
    let _ = run_in(dir, ml, &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "legacy body",
    ]);
    let _ = run_in(dir, ml, &[
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "pd",
    ]);
    // Capture the ids by content marker first (the markers are
    // rewritten away next); ids are content hashes, so both stores
    // agree.
    let path = dir.join(".mulch/expertise/d.jsonl");
    let text = std::fs::read_to_string(&path).expect("store file");
    let mut ids = std::collections::HashMap::new();
    for line in text.lines() {
        let record: serde_json::Value = serde_json::from_str(line).expect("json");
        if let Some(marker) = record["content"].as_str() {
            ids.insert(
                marker.to_string(),
                record["id"].as_str().expect("id").to_string(),
            );
        }
    }
    let mut lines = Vec::new();
    for line in text.lines() {
        let mut record: serde_json::Value = serde_json::from_str(line).expect("json");
        match record["content"].as_str() {
            Some("contentless body") => {
                record.as_object_mut().expect("obj").remove("content");
            }
            Some("extra body") => {
                record["bogus"] = serde_json::json!(1);
            }
            Some("wrongtyped body") => {
                record["content"] = serde_json::json!(5);
            }
            Some("legacy body") => {
                record["outcome"] = serde_json::json!({"status": "success"});
            }
            _ => {}
        }
        lines.push(serde_json::to_string(&record).expect("ser"));
    }
    std::fs::write(&path, lines.join("\n") + "\n").expect("write");
    ids
}

/// Writes a store whose config carries a custom governance block and
/// one domain per record count. Records are fresh foundational
/// patterns with distinct names (no stale/duplicates noise).
pub(crate) fn governance_fixture(dir: &Path, governance: &str, counts: &[(&str, usize)]) {
    use std::fmt::Write as _;

    let mut config = String::from("version: '1'\ndomains:\n");
    for (domain, _) in counts {
        let _ = writeln!(config, "  {domain}: {{}}");
    }
    config.push_str(governance);
    std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(dir.join(".mulch/mulch.config.yaml"), &config).expect("config");
    let now = jiff::Timestamp::now().to_string();
    for (domain, count) in counts {
        let lines: Vec<String> = (0..*count)
            .map(|i| {
                format!(
                    r#"{{"type":"pattern","name":"{domain}-p{i}","description":"d","classification":"foundational","recorded_at":"{now}"}}"#
                )
            })
            .collect();
        std::fs::write(
            dir.join(format!(".mulch/expertise/{domain}.jsonl")),
            lines.join("\n") + "\n",
        )
        .expect("domain store");
    }
}

/// Plain doctor output modulo the documented upgrade-check deviation
/// (its lines drop; the summary counts neutralize — the reference
/// warns "Update available", we pass "Native binary").
pub(crate) fn strip_upgrade_and_summary(text: &str) -> String {
    text.lines()
        .filter(|line| {
            !line.contains("Update available")
                && !line.contains("Run `mulch upgrade`")
                && !line.contains("Native binary")
        })
        .map(|line| {
            if line.contains("passed") {
                "<SUMMARY>"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The config file's text, for domain side-effect pins.
pub(crate) fn config_text(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(".mulch").join("mulch.config.yaml"))
        .expect("init wrote a config")
}

/// Whether the store registered a domain (its `domains:` mapping
/// carries a bare `{}` row).
pub(crate) fn has_domain(dir: &Path, domain: &str) -> bool {
    config_text(dir).contains(&format!("\n  {domain}: {{}}"))
}

/// One captured run with piped stdin (the reference's `--stdin`
/// surface).
pub(crate) fn run_stdin(dir: &Path, program: &Path, args: &[&str], input: &str) -> Run {
    use std::io::Write as _;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn stdin run");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(input.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait stdin run");
    Run {
        code:   output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Reads a store file when it exists, empty text otherwise (failed
/// record paths never write).
pub(crate) fn read_store_text_or_empty(dir: &Path, relative: &str) -> String {
    std::fs::read(dir.join(".mulch").join(relative)).map_or_else(
        |_| String::new(),
        |bytes| String::from_utf8_lossy(&bytes).into_owned(),
    )
}
