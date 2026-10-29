//! `mulch archive` probes (mulch-7c55, sprint-39 child A) — OUR byte
//! pins for the success surfaces (single/dry-run, file-shape, archive
//! merge), the prologue error set, and the resolve/read error set —
//! output channels plus the on-disk live and archive bytes (ADR-0008:
//! the differential battery is dissolved; our bytes are the
//! regression memory).

use std::path::Path;

use super::support::*;

/// A pre-existing archive line with FIXED stamps (merge input; it
/// passes through byte-stably when the archive is rewritten).
const ARCHIVED_ALPHA_TWO: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Alpha Two\",\"description\":\"second alpha pattern\",\"id\":\"mx-c7129f\",\"status\":\"archived\",\"archived_at\":\"2026-10-20T01:02:03.004Z\",\"archive_reason\":\"stale\"}";

/// The archive banner our writer puts at the top of every archive
/// file (frozen from our bytes).
const ARCHIVE_BANNER: &str = "# ARCHIVED — not for active use. Run `mulch restore <id>` to revive.";

#[test]
fn archive_single_and_dry_run_pin_our_bytes() {
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let ours = seeded_store(
        "archsingle",
        None,
        &[("dev", seed.as_str()), ("ops", "")],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "archive",
        "dev",
        "mx-1bb21d",
        "--reason",
        "wrong domain",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "Archived pattern mx-1bb21d from dev (manual: wrong domain): Alpha One\n"
    );
    // the live file keeps exactly the survivor
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        format!("{ALPHA_TWO}\n")
    );
    // the archive file: banner first, then the stamped record (read
    // RAW — the shared normalizer is byte-wise and mangles the
    // banner's em dash)
    let archive = read_store_file(&ours.0, "archive/dev.jsonl");
    assert!(
        archive.starts_with(&format!("{ARCHIVE_BANNER}\n")),
        "banner leads the archive: {archive}"
    );
    assert!(
        archive.contains(
            "\"name\":\"Alpha One\",\"description\":\"first alpha pattern\",\"id\":\"mx-1bb21d\",\
             \"status\":\"archived\""
        ),
        "stamped record: {archive}"
    );
    assert!(
        archive.contains("\"archived_at\":\""),
        "the fresh stamp rides along: {archive}"
    );
    assert!(
        archive.contains("\"archive_reason\":\"manual: wrong domain\""),
        "reason rides along: {archive}"
    );

    // dry-run: the preview line, and no archive directory at all
    let dry = seeded_store("archdry", None, &[("dev", seed.as_str())], &[]);
    let dry_run = run_in(&dry.0, Path::new(mulch_bin()), &[
        "archive",
        "dev",
        "mx-1bb21d",
        "--reason",
        "x",
        "--dry-run",
    ]);
    assert_eq!(dry_run.code, 0, "{}", dry_run.stderr);
    assert_eq!(
        dry_run.stdout,
        "[DRY RUN] Would archive pattern mx-1bb21d from dev (manual: x): Alpha One\n"
    );
    assert_eq!(read_store_file(&dry.0, "expertise/dev.jsonl"), seed);
    assert!(!dry.0.join(".mulch/archive").exists());
}

/// Merge into a pre-existing archive: the fixed-stamp line passes
/// through byte-stably, the fresh stamp normalizes.
#[test]
fn archive_merge_keeps_preexisting_lines() {
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let ours = seeded_store("archm", None, &[("dev", seed.as_str())], &[]);
    std::fs::create_dir_all(ours.0.join(".mulch/archive")).expect("archive dir");
    std::fs::write(
        ours.0.join(".mulch/archive/dev.jsonl"),
        format!("{ARCHIVE_BANNER}\n{ARCHIVED_ALPHA_TWO}\n"),
    )
    .expect("archive seed");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "archive",
        "dev",
        "mx-1bb21d",
        "--reason",
        "decay",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    let archive = read_store_file(&ours.0, "archive/dev.jsonl");
    assert!(archive.contains(ARCHIVED_ALPHA_TWO), "existing line stable");
    assert!(
        archive.lines().count() == 3,
        "banner + both records: {archive}"
    );
}

#[test]
fn archive_prologue_errors_leave_the_store_untouched() {
    // no store at all: the config-reader wording (frozen)
    let bare = TempDir::new("archnostore");
    let run = run_in(&bare.0, Path::new(mulch_bin()), &[
        "archive", "dev", "mx-1", "--reason", "x",
    ]);
    assert_eq!(run.code, 1);
    assert_eq!(
        run.stderr,
        "Error: No .mulch/ directory found. Run `mulch init` to set up this project.\n"
    );

    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let cases: [&[&str]; 8] = [
        &["archive", "nosuch", "mx-1bb21d", "--reason", "x"],
        &["archive", "dev", "mx-1bb21d", "--reason", "   "],
        &["archive", "dev", "--reason", "x"],
        &["archive", "dev", "--records", " , , ", "--reason", "x"],
        &["archive", "nosuch", "mx-1", "--reason", ""],
        &["--json", "archive", "nosuch", "mx-1bb21d", "--reason", "x"],
        &["--json", "archive", "dev", "mx-1bb21d", "--reason", "   "],
        &["--json", "archive", "dev", "--reason", "x"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let ours = seeded_store(
            &format!("archp{index}"),
            None,
            &[("dev", seed.as_str()), ("ops", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?} exit");
        assert!(
            our.stdout.is_empty(),
            "{args:?} nothing on stdout: {}",
            our.stdout
        );
        assert_eq!(
            read_store_file(&ours.0, "expertise/dev.jsonl"),
            seed,
            "{args:?} must not touch the store"
        );
        assert!(
            !ours.0.join(".mulch/archive").exists(),
            "{args:?} must not create the archive"
        );
    }

    // the empty store renders `(none)` in the available list
    let none = seeded_store("archnone", Some("version: '1'\ndomains: {}\n"), &[], &[]);
    let our = run_in(&none.0, Path::new(mulch_bin()), &[
        "archive", "nosuch", "mx-1", "--reason", "x",
    ]);
    assert_eq!(our.code, 1);
    assert!(our.stderr.contains("Available domains: (none)"));
}

#[test]
fn archive_resolve_and_read_errors_pin_our_bytes() {
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    // resolve errors: unknown id aborts with nothing written
    let ours = seeded_store(
        "archresolve",
        None,
        &[("dev", seed.as_str()), ("ops", "")],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "archive",
        "dev",
        "mx-ffffff",
        "--reason",
        "x",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), seed);
    assert!(!ours.0.join(".mulch/archive").exists());

    // malformed live line: the pinned template aborts before writes
    let bad = format!("{ALPHA_ONE}\n{MALFORMED_LINE}\n");
    let ours = seeded_store("archbad", None, &[("dev", bad.as_str())], &[]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "archive",
        "dev",
        "mx-1bb21d",
        "--reason",
        "x",
    ]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "");
    let text = normalize_dir(&run.stderr, &ours.0);
    assert!(
        text.starts_with("Error: Malformed JSONL at <DIR>/.mulch/expertise/dev.jsonl:2: "),
        "template prefix: {text}"
    );
    assert!(
        text.ends_with(&format!(". Line: {MALFORMED_LINE}\n")),
        "template suffix: {text}"
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), bad);
    assert!(!ours.0.join(".mulch/archive").exists());

    // unregistered type: strict read refuses (byte-pinned text)
    let unknown = format!("{ALPHA_ONE}\n{UNKNOWN_TYPE_LINE}\n");
    let ours = seeded_store("archunk", None, &[("dev", unknown.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "archive",
        "dev",
        "mx-zzz111",
        "--reason",
        "x",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        "Error: Unknown record type \"zzzcustom\" at \
         <DIR>/.mulch/expertise/dev.jsonl:2 (id=mx-zzz111). Register it under \
         custom_types in mulch.config.yaml, remove the record, or pass \
         --allow-unknown-types to bypass.\n"
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), unknown);

    // --allow-unknown-types: the writes land, THEN the summary throws
    let ours = seeded_store("archunk2", None, &[("dev", unknown.as_str())], &[]);
    let args = [
        "--allow-unknown-types",
        "archive",
        "dev",
        "mx-zzz111",
        "--reason",
        "x",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, "Error: Unknown record type: zzzcustom\n");
    // the archive write landed before the summary threw
    let archive = normalize(&read_store_text_or_empty(&ours.0, "archive/dev.jsonl"));
    assert!(
        archive.contains("\"id\":\"mx-zzz111\",\"status\":\"archived\""),
        "archived despite the late throw: {archive}"
    );
    assert!(
        !read_store_file(&ours.0, "expertise/dev.jsonl").contains("mx-zzz111"),
        "removed from the live file"
    );
}

/// The full oneOf set serializes through the archive: a reversed
/// --records list over all six types succeeds and drains the live
/// file.
#[test]
fn archive_full_type_set_survives_the_archive_pass() {
    let type_set: [&str; 6] = [
        "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"conv body\",\"id\":\"mx-conv01\"}",
        "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Pat One\",\"description\":\"pd\",\"id\":\"mx-pat001\"}",
        "{\"type\":\"failure\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.100Z\",\"description\":\"fail desc\",\"resolution\":\"fixed\",\"id\":\"mx-fail01\"}",
        "{\"type\":\"decision\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:58.150Z\",\"title\":\"Decide A\",\"rationale\":\"why\",\"id\":\"mx-dec001\"}",
        "{\"type\":\"reference\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.200Z\",\"name\":\"Ref One\",\"description\":\"rd\",\"id\":\"mx-ref001\"}",
        "{\"type\":\"guide\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:58.250Z\",\"name\":\"Guide One\",\"description\":\"gd\",\"id\":\"mx-guid01\"}",
    ];
    let seed = format!("{}\n", type_set.join("\n"));
    let ours = seeded_store("archts", None, &[("dev", seed.as_str())], &[]);
    let list = [
        "mx-guid01",
        "mx-ref001",
        "mx-dec001",
        "mx-fail01",
        "mx-pat001",
        "mx-conv01",
    ]
    .join(",");
    let owned = [
        "archive".to_string(),
        "dev".to_string(),
        "--records".to_string(),
        list,
        "--reason".to_string(),
        "decay".to_string(),
    ];
    let args: Vec<&str> = owned.iter().map(String::as_str).collect();
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), "");
    let archive = read_store_file(&ours.0, "archive/dev.jsonl");
    for id in [
        "mx-guid01",
        "mx-ref001",
        "mx-dec001",
        "mx-fail01",
        "mx-pat001",
        "mx-conv01",
    ] {
        assert!(archive.contains(id), "{id} archived: {archive}");
    }
}
