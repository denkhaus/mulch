//! `mulch restore` parity probes (mulch-7c55, sprint-39 child A):
//! success surfaces (exact/bare/prefix resolution, quiet, json,
//! append-to-live, pre-reason archives, extra fields), the scan
//! error set (not-found hint, cross-domain ambiguity, live
//! conflict, malformed files) — output channels plus the on-disk
//! live and archive bytes.

use std::path::Path;

use super::support::*;

/// The reference banner seeds twin archives identically (each side's
/// own writer banner appears after a rewrite and is normalized for
/// comparison).
const ML_BANNER: &str = "# ARCHIVED — not for active use. Run `ml restore <id>` to revive.";

/// OUR shipped banner (the +3-byte self-reference deviation): seeded
/// archives carry the banner of the binary that owns the twin, so an
/// untouched file compares equal only after `promote_theirs`.
const MULCH_BANNER: &str = "# ARCHIVED — not for active use. Run `mulch restore <id>` to revive.";

/// ALPHA One with the full lifecycle stamp (fixed archived_at).
const STAMPED_ONE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Alpha One\",\"description\":\"first alpha pattern\",\"id\":\"mx-1bb21d\",\"status\":\"archived\",\"archived_at\":\"2026-10-20T01:02:03.004Z\",\"archive_reason\":\"manual: wrong domain\"}";

/// ALPHA One archived WITHOUT the reason stamp (pre-b41a archive:
/// the reason falls back to "manual").
const OLD_ONE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Alpha One\",\"description\":\"first alpha pattern\",\"id\":\"mx-1bb21d\",\"status\":\"archived\"}";

/// A convention whose id shares the `mx-aaaa` prefix with PFX_ARCHED.
const CONV_ARCHED: &str = "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"conv body\",\"id\":\"mx-conv01\",\"status\":\"archived\",\"archived_at\":\"2026-10-20T01:02:03.004Z\",\"archive_reason\":\"manual: x\",\"zzz\":\"keep\"}";

/// Asserts one twin-run pair: exit, stdout, stderr, live bytes, and
/// archive bytes (timestamps masked, `ml` self-references promoted —
/// our hint says `mulch search`, the shipped-strings deviation).
fn assert_restore_parity(ours: &TempDir, our: &Run, theirs: &TempDir, their: &Run, label: &str) {
    assert_eq!(our.code, their.code, "{label} exit");
    assert_eq!(our.stdout, their.stdout, "{label} stdout");
    assert_eq!(promote_theirs(&their.stderr), our.stderr, "{label} stderr");
    assert_eq!(
        read_store_text_or_empty(&ours.0, "expertise/dev.jsonl"),
        read_store_text_or_empty(&theirs.0, "expertise/dev.jsonl"),
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

/// Seeds twin stores: dev+ops config, an archive for each named
/// domain (banner + lines), and an optional live dev file.
fn twin_archived(
    tag: &str,
    archives: &[(&str, &[&str])],
    live_dev: Option<&str>,
) -> (TempDir, TempDir) {
    let (ours, theirs) = twin_seeded(
        tag,
        None,
        &[("dev", live_dev.unwrap_or("")), ("ops", "")],
        &[],
    );
    for (dir, banner) in [(&ours.0, MULCH_BANNER), (&theirs.0, ML_BANNER)] {
        for (domain, lines) in archives {
            std::fs::create_dir_all(dir.join(".mulch/archive")).expect("archive dir");
            let mut body = format!("{banner}\n");
            for line in *lines {
                body.push_str(line);
                body.push('\n');
            }
            std::fs::write(dir.join(format!(".mulch/archive/{domain}.jsonl")), body)
                .expect("archive seed");
        }
    }
    (ours, theirs)
}

#[test]
fn restore_success_surfaces_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // exact id, json, bare hash, prefix, quiet plain, quiet json
    let cases: [&[&str]; 6] = [
        &["restore", "mx-1bb21d"],
        &["--json", "restore", "mx-1bb21d"],
        &["restore", "1bb21d"],
        &["restore", "1bb21"],
        &["--quiet", "restore", "mx-1bb21d"],
        &["--quiet", "--json", "restore", "mx-1bb21d"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let (ours, theirs) = twin_archived(&format!("rs{index}"), &[("dev", &[STAMPED_ONE])], None);
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_restore_parity(&ours, &our, &theirs, &their, &format!("case {index}"));
    }

    // append after an existing live record (canonical rewrite keeps
    // the survivor first, the restored record last)
    let (ours, theirs) = twin_archived(
        "rslive",
        &[("dev", &[STAMPED_ONE])],
        Some(&format!("{ALPHA_TWO}\n")),
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-1bb21d"]);
    let their = run_in(&theirs.0, &ml, &["restore", "mx-1bb21d"]);
    assert_restore_parity(&ours, &our, &theirs, &their, "append");
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        format!("{ALPHA_TWO}\n{ALPHA_ONE}\n")
    );

    // pre-b41a archive: no reason on disk -> "manual" fallback
    let (ours, theirs) = twin_archived("rsold", &[("dev", &[OLD_ONE])], None);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-1bb21d"]);
    let their = run_in(&theirs.0, &ml, &["restore", "mx-1bb21d"]);
    assert_restore_parity(&ours, &our, &theirs, &their, "old archive");
    assert_eq!(
        our.stdout,
        "✓ Restored pattern mx-1bb21d to dev (archived: manual): Alpha One\n"
    );

    // extra fields survive the strip; one archive line stays behind
    let (ours, theirs) = twin_archived("rszzz", &[("dev", &[STAMPED_ONE, CONV_ARCHED])], None);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "restore",
        "mx-conv01",
    ]);
    let their = run_in(&theirs.0, &ml, &["--json", "restore", "mx-conv01"]);
    assert_restore_parity(&ours, &our, &theirs, &their, "extra fields");
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"conv body\",\"id\":\"mx-conv01\",\"zzz\":\"keep\"}\n"
    );
}

#[test]
fn restore_scan_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // not found (hint promotes `ml search` -> `mulch search`), both
    // channels; no archive directory at all behaves the same.
    let (ours, theirs) = twin_seeded("rnf", None, &[("dev", ""), ("ops", "")], &[]);
    for args in [
        (&["restore", "mx-ffffff"][..]),
        (&["--json", "restore", "mx-ffffff"][..]),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code);
        assert_eq!(promote_theirs(&their.stderr), our.stderr, "{args:?}");
    }
    let not_found = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-ffffff"]);
    assert!(
        not_found
            .stderr
            .contains("Run `mulch search --archived <query>` to browse archives.")
    );

    // same prefix archived in TWO domains -> cross-domain ambiguity
    // (exact id included: exact matching is per domain).
    let pfx = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Pfx One\",\"description\":\"p1\",\"id\":\"mx-aaaa11\",\"status\":\"archived\",\"archived_at\":\"2026-10-20T01:02:03.004Z\",\"archive_reason\":\"stale\"}";
    for (args, label) in [
        (&["restore", "mx-aaaa"][..], "prefix"),
        (&["restore", "mx-aaaa11"][..], "exact"),
    ] {
        let (ours, theirs) = twin_archived(
            &format!("ramb-{label}"),
            &[("dev", &[pfx]), ("ops", &[pfx])],
            None,
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_restore_parity(&ours, &our, &theirs, &their, label);
        assert_eq!(our.code, 1);
    }

    // ambiguous inside ONE domain: the resolve error is swallowed —
    // the generic not-found hint renders.
    let pfx_two = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Pfx Two\",\"description\":\"p2\",\"id\":\"mx-aaaa22\",\"status\":\"archived\",\"archived_at\":\"2026-10-20T01:02:03.004Z\",\"archive_reason\":\"stale\"}";
    let (ours, theirs) = twin_archived("ramb1d", &[("dev", &[pfx, pfx_two])], None);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-aaaa"]);
    let their = run_in(&theirs.0, &ml, &["restore", "mx-aaaa"]);
    assert_restore_parity(&ours, &our, &theirs, &their, "one-domain ambiguous");
    assert!(our.stderr.contains("not found in any domain"));

    // live conflict: the archive is untouched
    let (ours, theirs) = twin_archived(
        "rconf",
        &[("dev", &[STAMPED_ONE])],
        Some(&format!("{ALPHA_ONE}\n")),
    );
    for args in [
        (&["restore", "mx-1bb21d"][..]),
        (&["--json", "restore", "mx-1bb21d"][..]),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code);
        assert_eq!(promote_theirs(&their.stderr), our.stderr, "{args:?}");
    }
    assert_eq!(
        read_store_file(&ours.0, "archive/dev.jsonl"),
        format!("{MULCH_BANNER}\n{STAMPED_ONE}\n")
    );

    // malformed archive line: the pinned template, nothing restored
    let (ours, theirs) = twin_archived("rbad", &[("dev", &[MALFORMED_LINE])], None);
    for (dir, program) in [(&ours.0, Path::new(mulch_bin())), (&theirs.0, ml.as_path())] {
        let run = run_in(dir, program, &["restore", "mx-1bb21d"]);
        assert_eq!(run.code, 1);
        let text = normalize_dir(&run.stderr, dir);
        assert!(
            text.starts_with("Error: Malformed JSONL at <DIR>/.mulch/archive/dev.jsonl:2: "),
            "template prefix: {text}"
        );
        assert!(
            text.ends_with(&format!(". Line: {MALFORMED_LINE}\n")),
            "template suffix: {text}"
        );
    }

    // malformed LIVE line: the pre-check read aborts, archive intact
    let (ours, theirs) = twin_archived("rbadl", &[("dev", &[STAMPED_ONE])], Some(MALFORMED_LINE));
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-1bb21d"]);
    let their = run_in(&theirs.0, &ml, &["restore", "mx-1bb21d"]);
    assert_eq!(our.code, their.code);
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        normalize_dir(&their.stderr, &theirs.0)
    );
    assert_eq!(
        read_store_file(&ours.0, "archive/dev.jsonl"),
        format!("{MULCH_BANNER}\n{STAMPED_ONE}\n")
    );
}

#[test]
fn restore_round_trips_the_full_type_set() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let types = [
        "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"conv body\",\"id\":\"mx-conv01\"}",
        "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Pat One\",\"description\":\"pd\",\"id\":\"mx-pat001\"}",
        "{\"type\":\"failure\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.100Z\",\"description\":\"fail desc\",\"resolution\":\"fixed\",\"id\":\"mx-fail01\"}",
        "{\"type\":\"decision\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:58.150Z\",\"title\":\"Decide A\",\"rationale\":\"why\",\"id\":\"mx-dec001\"}",
        "{\"type\":\"reference\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.200Z\",\"name\":\"Ref One\",\"description\":\"rd\",\"id\":\"mx-ref001\"}",
        "{\"type\":\"guide\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:58.250Z\",\"name\":\"Guide One\",\"description\":\"gd\",\"id\":\"mx-guid01\"}",
    ];
    let (ours, theirs) = twin_seeded(
        "rrt",
        None,
        &[("dev", &format!("{}\n", types.join("\n")))],
        &[],
    );
    let ids = [
        "mx-conv01",
        "mx-pat001",
        "mx-fail01",
        "mx-dec001",
        "mx-ref001",
        "mx-guid01",
    ]
    .join(",");
    // archive everything, then restore three of them back
    let archive = [
        "archive".to_string(),
        "dev".to_string(),
        "--records".to_string(),
        ids,
        "--reason".to_string(),
        "decay".to_string(),
    ];
    let archive_args: Vec<&str> = archive.iter().map(String::as_str).collect();
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &archive_args);
    let _ = run_in(&theirs.0, &ml, &archive_args);
    for id in ["mx-fail01", "mx-conv01", "mx-guid01"] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["restore", id]);
        let their = run_in(&theirs.0, &ml, &["restore", id]);
        assert_restore_parity(&ours, &our, &theirs, &their, id);
    }
    // the three restored records are live again (in restore order),
    // the other three stay archived.
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        format!("{}\n{}\n{}\n", types[2], types[0], types[5])
    );
}
