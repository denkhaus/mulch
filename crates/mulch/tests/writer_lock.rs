//! Writer-lock behavior (mulch-228d) at the CLI level: parallel
//! writers keep every record, a contended writer fails fast naming
//! the lock file, a writer survives another holder's death without
//! wedging, and read commands never create `.mulch/.lock`.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// One fresh initialized store per test (`mulch init` writes the
/// byte-pinned config, so the domain list starts empty — the parallel
/// writers exercise the auto-create race under the lock).
fn temp_store(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mulch-writer-lock-{tag}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp project dir");
    let output = run(&dir, &["init"]);
    assert!(output.status.success(), "init failed: {output:?}");
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mulch"))
        .args(args)
        .current_dir(dir)
        .env_remove("RUST_BACKTRACE")
        .output()
        .expect("binary runs")
}

/// The record arguments for `name`: the CONTENT rides along so every
/// writer's dedup key is unique (identical content would upsert into
/// one record — correct dedup, wrong test).
fn record_args(name: &str) -> Vec<String> {
    [
        "record",
        "rust",
        "--type",
        "convention",
        "--name",
        name,
        "--content",
        &format!("writer-lock probe: {name}"),
    ]
    .map(String::from)
    .into_iter()
    .collect::<Vec<_>>()
}

/// Eight writers race on one store (each auto-creating the domain);
/// every record must survive with a unique id.
#[test]
fn parallel_records_keep_every_record() {
    let dir = temp_store("parallel");
    let mut children: Vec<Child> = (0..8)
        .map(|i| {
            let args = record_args(&format!("racer {i}"));
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            Command::new(env!("CARGO_BIN_EXE_mulch"))
                .args(refs)
                .current_dir(&dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("writer spawns")
        })
        .collect();
    for child in &mut children {
        let status = child.wait().expect("writer finishes");
        assert!(status.success(), "writer failed: {status:?}");
    }

    let body =
        fs::read_to_string(dir.join(".mulch/expertise/rust.jsonl")).expect("domain file exists");
    let ids: Vec<String> = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("record line parses")
                .get("id")
                .and_then(serde_json::Value::as_str)
                .expect("record carries an id")
                .to_string()
        })
        .collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(ids.len(), 8, "lost records: {body}");
    assert_eq!(unique.len(), ids.len(), "duplicate ids: {body}");
}

/// Read commands stay lock-free: they never create `.mulch/.lock`;
/// a mutating command does.
#[test]
fn reads_never_create_the_lock_file() {
    let dir = temp_store("read-clean");
    fs::remove_file(dir.join(".mulch/.lock")).expect("init's inert lock removed");
    let dry_run = record_args("dry");
    let mut dry_args: Vec<&str> = dry_run.iter().map(String::as_str).collect();
    dry_args.push("--dry-run");
    for args in [
        &["status"][..],
        &["validate"][..],
        &["doctor"][..],
        &dry_args,
    ] {
        let output = run(&dir, args);
        assert!(output.status.success(), "{args:?} failed: {output:?}");
    }
    assert!(
        !dir.join(".mulch/.lock").exists(),
        "a read command created the lock file"
    );

    let real = record_args("writes");
    let refs: Vec<&str> = real.iter().map(String::as_str).collect();
    let output = run(&dir, &refs);
    assert!(output.status.success(), "record failed: {output:?}");
    assert!(
        dir.join(".mulch/.lock").exists(),
        "record left no lock file behind"
    );
}

/// A writer whose competitor keeps the lock past the wait budget
/// fails fast naming the lock file, then succeeds once it is free.
#[test]
fn record_fails_fast_while_lock_held() {
    let dir = temp_store("contended");
    let mut holder = spawn_holder(&dir);

    let started = Instant::now();
    let args = record_args("blocked");
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run(&dir, &refs);
    let elapsed = started.elapsed();
    assert!(
        !output.status.success(),
        "record must not succeed under a held lock: {output:?}"
    );
    assert!(
        elapsed >= Duration::from_secs(4),
        "record failed too fast, it never waited: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(8),
        "record waited past its 5s budget: {elapsed:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(
        stderr.contains(".mulch/.lock") && stderr.contains("writer lock"),
        "error must name the writer lock: {stderr}"
    );

    // The json arm of the same surface: failure envelope, exit 1.
    let mut json_args = record_args("blocked");
    json_args.push("--json".to_string());
    let json_refs: Vec<&str> = json_args.iter().map(String::as_str).collect();
    let json_run = run(&dir, &json_refs);
    assert!(
        !json_run.status.success(),
        "--json record must fail under a held lock: {json_run:?}"
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&json_run.stdout).expect("json envelope on stdout");
    assert_eq!(envelope["success"], serde_json::Value::Bool(false));
    assert_eq!(envelope["command"], "record");
    assert!(
        envelope["error"]
            .as_str()
            .unwrap_or_default()
            .contains(".mulch/.lock"),
        "envelope error must name the lock file: {envelope}"
    );

    kill_holder(&mut holder);

    let retry = run(&dir, &refs);
    assert!(
        retry.status.success(),
        "record must succeed after release: {retry:?}"
    );
}

/// A writer whose predecessor dies mid-hold proceeds immediately:
/// the kernel released the dead holder's lock.
#[test]
fn record_proceeds_after_holder_dies() {
    let dir = temp_store("crash-release");
    let mut holder = spawn_holder(&dir);

    let started = Instant::now();
    let args = record_args("after crash");
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut writer = Command::new(env!("CARGO_BIN_EXE_mulch"))
        .args(refs)
        .current_dir(&dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("writer spawns");
    std::thread::sleep(Duration::from_millis(200));
    kill_holder(&mut holder);

    let status = writer.wait().expect("writer finishes");
    assert!(
        status.success(),
        "writer failed after holder death: {status:?}"
    );
    assert!(
        started.elapsed() >= Duration::from_millis(150),
        "writer did not wait for the held lock"
    );

    let body =
        fs::read_to_string(dir.join(".mulch/expertise/rust.jsonl")).expect("domain file exists");
    assert_eq!(
        body.lines().filter(|l| !l.trim().is_empty()).count(),
        1,
        "record missing: {body}"
    );
}

/// Spawns the helper process holding `.mulch/.lock` and waits for its
/// `held` announcement.
fn spawn_holder(dir: &Path) -> Child {
    let mut holder = Command::new(std::env::current_exe().expect("test exe"))
        .args(["--exact", "writer_lock_hold_helper", "--nocapture"])
        .env("MULCH_TEST_LOCK_DIR", dir)
        .stdout(Stdio::piped())
        .spawn()
        .expect("holder spawns");
    let mut stdout = holder.stdout.take().expect("holder stdout");
    let mut held = false;
    for line in BufReader::new(&mut stdout).lines() {
        if line.expect("holder line") == "held" {
            held = true;
            break;
        }
    }
    drop(stdout);
    assert!(held, "holder never announced the lock");
    holder
}

fn kill_holder(holder: &mut Child) {
    holder.kill().expect("holder killed");
    holder.wait().expect("holder reaped");
}

/// Helper process for the contention tests: holds an exclusive lock
/// on `.mulch/.lock` until killed. A plain suite run (no
/// `MULCH_TEST_LOCK_DIR`) returns immediately.
#[test]
#[expect(
    clippy::print_stdout,
    reason = "the held marker is the parent test's synchronization signal"
)]
fn writer_lock_hold_helper() {
    let Some(dir) = std::env::var_os("MULCH_TEST_LOCK_DIR") else {
        return;
    };
    let path = Path::new(&dir).join(".mulch/.lock");
    // create + !truncate: never clobber an existing lock file.
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .expect("lock file opens");
    file.try_lock().expect("helper acquires");
    println!("held");
    std::thread::sleep(Duration::from_secs(30));
}
