//! `mulch archive` parity probes (mulch-7c55, sprint-39 child A):
//! success surfaces (single/bulk/dry-run/quiet/json, file-order
//! output, dedupe, archive merge), the prologue error set, and the
//! resolve/read error set — output channels plus the on-disk live
//! and archive bytes.

use std::path::Path;

use super::support::*;

/// A pre-existing archive line with FIXED stamps (merge input; it
/// passes through byte-stably when the archive is rewritten).
const ARCHIVED_ALPHA_TWO: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Alpha Two\",\"description\":\"second alpha pattern\",\"id\":\"mx-c7129f\",\"status\":\"archived\",\"archived_at\":\"2026-10-20T01:02:03.004Z\",\"archive_reason\":\"stale\"}";

/// Two records whose ids share the prefix `mx-aaaa`.
const PFX_ONE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Pfx One\",\"description\":\"p1\",\"id\":\"mx-aaaa11\"}";
const PFX_TWO: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Pfx Two\",\"description\":\"p2\",\"id\":\"mx-aaaa22\"}";

/// The ml banner seeds pre-existing archives in BOTH twin stores
/// (only a comment to the readers; each side's own writer banner
/// appears after a rewrite and is normalized for comparison).
const ML_BANNER: &str = "# ARCHIVED — not for active use. Run `ml restore <id>` to revive.";

/// One registry record per type (the full oneOf set), fixed stamps,
/// tactical/foundational mix.
const TYPE_SET: [&str; 6] = [
    "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"conv body\",\"id\":\"mx-conv01\"}",
    "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Pat One\",\"description\":\"pd\",\"id\":\"mx-pat001\"}",
    "{\"type\":\"failure\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.100Z\",\"description\":\"fail desc\",\"resolution\":\"fixed\",\"id\":\"mx-fail01\"}",
    "{\"type\":\"decision\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:58.150Z\",\"title\":\"Decide A\",\"rationale\":\"why\",\"id\":\"mx-dec001\"}",
    "{\"type\":\"reference\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.200Z\",\"name\":\"Ref One\",\"description\":\"rd\",\"id\":\"mx-ref001\"}",
    "{\"type\":\"guide\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:58.250Z\",\"name\":\"Guide One\",\"description\":\"gd\",\"id\":\"mx-guid01\"}",
];

/// Asserts one twin-run pair: exit, stdout, stderr, live bytes, and
/// archive bytes (timestamps masked, the reference's `ml`
/// self-references promoted — our banner says `mulch restore`, the
/// +3-byte shipped-strings deviation).
fn assert_archive_parity(ours: &TempDir, our: &Run, theirs: &TempDir, their: &Run, label: &str) {
    assert_eq!(our.code, their.code, "{label} exit");
    assert_eq!(our.stdout, their.stdout, "{label} stdout");
    assert_eq!(our.stderr, their.stderr, "{label} stderr");
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        read_store_file(&theirs.0, "expertise/dev.jsonl"),
        "{label} live bytes"
    );
    assert_eq!(
        normalize(&read_store_text_or_empty(&ours.0, "archive/dev.jsonl")),
        promote_theirs(&normalize(&read_store_text_or_empty(
            &theirs.0,
            "archive/dev.jsonl"
        ))),
        "{label} archive bytes"
    );
}

#[test]
fn archive_success_surfaces_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let cases: [&[&str]; 9] = [
        &["archive", "dev", "mx-1bb21d", "--reason", "wrong domain"],
        &[
            "--json",
            "archive",
            "dev",
            "mx-1bb21d",
            "--reason",
            "wrong domain",
        ],
        &[
            "archive",
            "dev",
            "mx-1bb21d",
            "--reason",
            "cleanup",
            "--dry-run",
        ],
        &[
            "--json",
            "archive",
            "dev",
            "mx-1bb21d",
            "--reason",
            "cleanup",
            "--dry-run",
        ],
        &[
            "archive",
            "dev",
            "--records",
            "mx-1bb21d,mx-c7129f",
            "--reason",
            "decay",
        ],
        &[
            "--json",
            "archive",
            "dev",
            "--records",
            "mx-1bb21d,mx-c7129f",
            "--reason",
            "decay",
        ],
        // reversed list: output and archive follow FILE order
        &[
            "archive",
            "dev",
            "--records",
            "mx-c7129f,mx-1bb21d",
            "--reason",
            "decay",
        ],
        // duplicate raw id: the index set dedupes (no bulk line)
        &[
            "archive",
            "dev",
            "--records",
            "mx-1bb21d,mx-1bb21d",
            "--reason",
            "decay",
        ],
        &[
            "--quiet",
            "archive",
            "dev",
            "mx-1bb21d",
            "--reason",
            "decay",
        ],
    ];
    for (index, args) in cases.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("archs{index}"),
            None,
            &[("dev", seed.as_str()), ("ops", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_archive_parity(&ours, &our, &theirs, &their, &format!("case {index}"));
    }
    // dry-run leaves no archive directory at all
    let dry_ours = TempDir::new("archdryo");
    let dry_theirs = TempDir::new("archdryt");
    for dir in [&dry_ours.0, &dry_theirs.0] {
        write_seed(dir, &[("dev", seed.as_str())]);
    }
    let _ = run_in(&dry_ours.0, Path::new(mulch_bin()), &[
        "archive",
        "dev",
        "mx-1bb21d",
        "--reason",
        "x",
        "--dry-run",
    ]);
    assert!(!dry_ours.0.join(".mulch/archive").exists());

    // merge into a pre-existing archive: the fixed-stamp line passes
    // through byte-stably, the fresh stamp normalizes.
    let (ours, theirs) = twin_seeded("archm", None, &[("dev", seed.as_str())], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::create_dir_all(dir.join(".mulch/archive")).expect("archive dir");
        std::fs::write(
            dir.join(".mulch/archive/dev.jsonl"),
            format!("{ML_BANNER}\n{ARCHIVED_ALPHA_TWO}\n"),
        )
        .expect("archive seed");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "archive",
        "dev",
        "mx-1bb21d",
        "--reason",
        "decay",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "archive",
        "dev",
        "mx-1bb21d",
        "--reason",
        "decay",
    ]);
    assert_archive_parity(&ours, &our, &theirs, &their, "merge");
    assert!(
        read_store_file(&ours.0, "archive/dev.jsonl").contains(ARCHIVED_ALPHA_TWO),
        "existing line stable"
    );
}

#[test]
fn archive_prologue_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // no store at all: the config-reader wording, both channels
    let bare: [&[&str]; 2] = [&["archive", "dev", "mx-1", "--reason", "x"], &[
        "--json", "archive", "dev", "mx-1", "--reason", "x",
    ]];
    for (index, args) in bare.iter().enumerate() {
        let ours = TempDir::new(&format!("archb{index}"));
        let theirs = TempDir::new(&format!("archbt{index}"));
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{args:?} exit");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
    }

    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let cases: [&[&str]; 8] = [
        &["archive", "nosuch", "mx-1bb21d", "--reason", "x"],
        &["--json", "archive", "nosuch", "mx-1bb21d", "--reason", "x"],
        &["archive", "dev", "mx-1bb21d", "--reason", "   "],
        &["--json", "archive", "dev", "mx-1bb21d", "--reason", "   "],
        &["archive", "dev", "--reason", "x"],
        &["--json", "archive", "dev", "--reason", "x"],
        &[
            "archive",
            "dev",
            "mx-1bb21d",
            "--records",
            "mx-c7129f",
            "--reason",
            "x",
        ],
        &[
            "--json",
            "archive",
            "dev",
            "mx-1bb21d",
            "--records",
            "mx-c7129f",
            "--reason",
            "x",
        ],
    ];
    for (index, args) in cases.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("archp{index}"),
            None,
            &[("dev", seed.as_str()), ("ops", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{args:?} exit");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
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

    // junk list, and the domain-before-reason order
    let order: [&[&str]; 4] = [
        &["archive", "dev", "--records", " , , ", "--reason", "x"],
        &[
            "--json",
            "archive",
            "dev",
            "--records",
            " , , ",
            "--reason",
            "x",
        ],
        &["archive", "nosuch", "mx-1", "--reason", ""],
        &["--json", "archive", "nosuch", "mx-1", "--reason", ""],
    ];
    for (index, args) in order.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("archo{index}"),
            None,
            &[("dev", seed.as_str()), ("ops", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{args:?} exit");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
    }

    // the empty store renders `(none)` in the available list
    let (ours, theirs) = twin_seeded("archnone", Some("version: '1'\ndomains: {}\n"), &[], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "archive", "nosuch", "mx-1", "--reason", "x",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "archive", "nosuch", "mx-1", "--reason", "x",
    ]);
    assert_eq!(our.stderr, their.stderr);
    assert!(our.stderr.contains("Available domains: (none)"));
}

#[test]
fn archive_resolve_and_read_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    // resolve errors (both channels) and two resolve successes
    let cases: [(&str, &[&str]); 6] = [
        ("seed", &["archive", "dev", "mx-ffffff", "--reason", "x"]),
        ("seed", &[
            "--json",
            "archive",
            "dev",
            "mx-ffffff",
            "--reason",
            "x",
        ]),
        ("pfx", &["archive", "dev", "mx-aaaa", "--reason", "x"]),
        ("pfx", &[
            "--json", "archive", "dev", "mx-aaaa", "--reason", "x",
        ]),
        // bare hash resolves the exact id
        ("seed", &["archive", "dev", "1bb21d", "--reason", "x"]),
        ("pfx", &[
            "--quiet",
            "archive",
            "dev",
            "mx-aaaa11",
            "--reason",
            "x",
        ]),
    ];
    for (index, (fixture, args)) in cases.iter().enumerate() {
        let body = if *fixture == "pfx" {
            format!("{PFX_ONE}\n{PFX_TWO}\n")
        } else {
            seed.clone()
        };
        let (ours, theirs) = twin_seeded(
            &format!("archr{index}"),
            None,
            &[("dev", body.as_str()), ("ops", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_archive_parity(&ours, &our, &theirs, &their, &format!("resolve {index}"));
    }

    // malformed live line: the pinned template aborts before writes
    let bad = format!("{ALPHA_ONE}\n{MALFORMED_LINE}\n");
    let (ours, theirs) = twin_seeded("archbad", None, &[("dev", bad.as_str())], &[]);
    for (dir, program) in [(&ours.0, Path::new(mulch_bin())), (&theirs.0, ml.as_path())] {
        let run = run_in(dir, program, &[
            "archive",
            "dev",
            "mx-1bb21d",
            "--reason",
            "x",
        ]);
        assert_eq!(run.code, 1);
        let text = normalize_dir(&run.stderr, dir);
        assert!(
            text.starts_with("Error: Malformed JSONL at <DIR>/.mulch/expertise/dev.jsonl:2: "),
            "template prefix: {text}"
        );
        assert!(
            text.ends_with(&format!(". Line: {MALFORMED_LINE}\n")),
            "template suffix: {text}"
        );
    }
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), bad);
    assert!(!ours.0.join(".mulch/archive").exists());

    // unregistered type: strict read refuses (byte-pinned text)
    let unknown = format!("{ALPHA_ONE}\n{UNKNOWN_TYPE_LINE}\n");
    let (ours, theirs) = twin_seeded("archunk", None, &[("dev", unknown.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "archive",
        "dev",
        "mx-zzz111",
        "--reason",
        "x",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "archive",
        "dev",
        "mx-zzz111",
        "--reason",
        "x",
    ]);
    assert_eq!(our.code, their.code);
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        normalize_dir(&their.stderr, &theirs.0)
    );
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), unknown);

    // --allow-unknown-types: the writes land, THEN the summary throws
    let (ours, theirs) = twin_seeded("archunk2", None, &[("dev", unknown.as_str())], &[]);
    let args = [
        "--allow-unknown-types",
        "archive",
        "dev",
        "mx-zzz111",
        "--reason",
        "x",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stderr, their.stderr);
    assert_eq!(our.stderr, "Error: Unknown record type: zzzcustom\n");
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        read_store_file(&theirs.0, "expertise/dev.jsonl")
    );
    assert_eq!(
        normalize(&read_store_text_or_empty(&ours.0, "archive/dev.jsonl")),
        promote_theirs(&normalize(&read_store_text_or_empty(
            &theirs.0,
            "archive/dev.jsonl"
        )))
    );
}

#[test]
fn archive_full_type_set_stamps_and_summaries_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{}\n", TYPE_SET.join("\n"));
    let (ours, theirs) = twin_seeded("archts", None, &[("dev", seed.as_str())], &[]);
    // reversed --records: per-type summaries in FILE order plus the
    // bulk line; every type stamps and serializes through the archive.
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
    let their = run_in(&theirs.0, &ml, &args);
    assert_archive_parity(&ours, &our, &theirs, &their, "type set");
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), "");
}
