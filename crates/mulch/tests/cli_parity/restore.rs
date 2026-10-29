//! `mulch restore` pins (mulch-7c55, sprint-39 child A): success
//! surfaces (exact/bare/prefix resolution, quiet, json, append-to-
//! live, pre-reason archives, extra fields), the scan error set
//! (not-found hint, cross-domain ambiguity, live conflict, malformed
//! files) — output channels plus the on-disk live and archive bytes
//! (ADR-0008: our byte pins only, literals frozen from the parity
//! era; the reference twin is gone).

use std::path::Path;

use super::support::*;

/// OUR shipped banner (the +3-byte self-reference deviation: the
/// banner says `mulch restore`).
const MULCH_BANNER: &str = "# ARCHIVED — not for active use. Run `mulch restore <id>` to revive.";

/// ALPHA One with the full lifecycle stamp (fixed archived_at).
const STAMPED_ONE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Alpha One\",\"description\":\"first alpha pattern\",\"id\":\"mx-1bb21d\",\"status\":\"archived\",\"archived_at\":\"2026-10-20T01:02:03.004Z\",\"archive_reason\":\"manual: wrong domain\"}";

/// ALPHA One archived WITHOUT the reason stamp (pre-b41a archive:
/// the reason falls back to "manual").
const OLD_ONE: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Alpha One\",\"description\":\"first alpha pattern\",\"id\":\"mx-1bb21d\",\"status\":\"archived\"}";

/// A convention whose id shares the `mx-aaaa` prefix with PFX_ARCHED.
const CONV_ARCHED: &str = "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"conv body\",\"id\":\"mx-conv01\",\"status\":\"archived\",\"archived_at\":\"2026-10-20T01:02:03.004Z\",\"archive_reason\":\"manual: x\",\"zzz\":\"keep\"}";

/// Seeds a store: dev+ops config, an archive for each named domain
/// (OUR banner + lines), and an optional live dev file.
fn archived_store(tag: &str, archives: &[(&str, &[&str])], live_dev: Option<&str>) -> TempDir {
    let ours = seeded_store(
        tag,
        None,
        &[("dev", live_dev.unwrap_or("")), ("ops", "")],
        &[],
    );
    for (domain, lines) in archives {
        std::fs::create_dir_all(ours.0.join(".mulch/archive")).expect("archive dir");
        let mut body = format!("{MULCH_BANNER}\n");
        for line in *lines {
            body.push_str(line);
            body.push('\n');
        }
        std::fs::write(ours.0.join(format!(".mulch/archive/{domain}.jsonl")), body)
            .expect("archive seed");
    }
    ours
}

#[test]
fn restore_success_surfaces() {
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
        let ours = archived_store(&format!("rs{index}"), &[("dev", &[STAMPED_ONE])], None);
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "case {index}: {}", our.stderr);
        if args[0] == "--json" || (args[0] == "--quiet" && args[1] == "--json") {
            assert_eq!(
                our.stdout,
                "{\n  \"success\": true,\n  \"command\": \"restore\",\n  \"domain\": \"dev\",\n  \
                 \"id\": \"mx-1bb21d\",\n  \"type\": \"pattern\",\n  \"summary\": \"Alpha One\",\n  \
                 \"archive_reason\": \"manual: wrong domain\"\n}\n",
                "case {index}"
            );
            assert_eq!(our.stderr, "", "case {index}");
        } else if args[0] == "--quiet" {
            assert_eq!(our.stdout, "", "case {index}: quiet prints nothing");
        } else {
            assert_eq!(
                our.stdout,
                "✓ Restored pattern mx-1bb21d to dev (archived: manual: wrong domain): Alpha \
                 One\n",
                "case {index}"
            );
            assert_eq!(our.stderr, "", "case {index}");
        }
        // the live file gained the record; the archive keeps OUR
        // banner only (the restored line left the file)
        assert_eq!(
            read_store_file(&ours.0, "expertise/dev.jsonl"),
            format!("{ALPHA_ONE}\n"),
            "case {index} live bytes"
        );
        assert_eq!(
            read_store_file(&ours.0, "archive/dev.jsonl"),
            format!("{MULCH_BANNER}\n"),
            "case {index} archive bytes"
        );
    }

    // append after an existing live record (canonical rewrite keeps
    // the survivor first, the restored record last)
    let ours = archived_store(
        "rslive",
        &[("dev", &[STAMPED_ONE])],
        Some(&format!("{ALPHA_TWO}\n")),
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-1bb21d"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "✓ Restored pattern mx-1bb21d to dev (archived: manual: wrong domain): Alpha One\n"
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        format!("{ALPHA_TWO}\n{ALPHA_ONE}\n")
    );

    // pre-b41a archive: no reason on disk -> "manual" fallback
    let ours = archived_store("rsold", &[("dev", &[OLD_ONE])], None);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-1bb21d"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "✓ Restored pattern mx-1bb21d to dev (archived: manual): Alpha One\n"
    );

    // extra fields survive the strip; one archive line stays behind
    let ours = archived_store("rszzz", &[("dev", &[STAMPED_ONE, CONV_ARCHED])], None);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "restore",
        "mx-conv01",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"conv body\",\"id\":\"mx-conv01\",\"zzz\":\"keep\"}\n"
    );
    assert_eq!(
        read_store_file(&ours.0, "archive/dev.jsonl"),
        format!("{MULCH_BANNER}\n{STAMPED_ONE}\n")
    );
}

#[test]
fn restore_scan_errors() {
    // not found (our hint says `mulch search`), both channels; no
    // archive directory at all behaves the same.
    let ours = seeded_store("rnf", None, &[("dev", ""), ("ops", "")], &[]);
    for args in [
        (&["restore", "mx-ffffff"][..]),
        (&["--json", "restore", "mx-ffffff"][..]),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?}");
        assert_eq!(our.stdout, "", "{args:?}");
        if args[0] == "--json" {
            assert_eq!(
                our.stderr,
                error_envelope(
                    "restore",
                    "Archived record \"mx-ffffff\" not found in any domain. Run `mulch search \
                     --archived <query>` to browse archives."
                )
            );
        } else {
            assert_eq!(
                our.stderr,
                "Error: Archived record \"mx-ffffff\" not found in any domain. Run `mulch search \
                 --archived <query>` to browse archives.\n"
            );
        }
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
    for (given, label) in [("mx-aaaa", "prefix"), ("mx-aaaa11", "exact")] {
        let args = ["restore", given];
        let ours = archived_store(
            &format!("ramb-{label}"),
            &[("dev", &[pfx]), ("ops", &[pfx])],
            None,
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1, "{label}");
        assert_eq!(our.stdout, "", "{label}");
        assert_eq!(
            our.stderr,
            format!(
                "Error: Identifier \"{given}\" matches archived records in multiple domains: \
                 dev (mx-aaaa11), ops (mx-aaaa11). Use a longer prefix.\n"
            ),
            "{label}"
        );
    }

    // ambiguous inside ONE domain: the resolve error is swallowed —
    // the generic not-found hint renders.
    let pfx_two = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Pfx Two\",\"description\":\"p2\",\"id\":\"mx-aaaa22\",\"status\":\"archived\",\"archived_at\":\"2026-10-20T01:02:03.004Z\",\"archive_reason\":\"stale\"}";
    let ours = archived_store("ramb1d", &[("dev", &[pfx, pfx_two])], None);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-aaaa"]);
    assert_eq!(our.code, 1);
    assert!(our.stderr.contains("not found in any domain"));

    // live conflict: the archive is untouched
    let ours = archived_store(
        "rconf",
        &[("dev", &[STAMPED_ONE])],
        Some(&format!("{ALPHA_ONE}\n")),
    );
    for args in [
        (&["restore", "mx-1bb21d"][..]),
        (&["--json", "restore", "mx-1bb21d"][..]),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?}");
        if args[0] == "--json" {
            assert_eq!(
                our.stderr,
                error_envelope(
                    "restore",
                    "Live record \"mx-1bb21d\" already exists in domain \"dev\" (classification: \
                     tactical). Delete or rename the live record before restoring, or run \
                     `mulch search --archived mx-1bb21d` to inspect the archived copy."
                )
            );
        } else {
            assert_eq!(
                our.stderr,
                "Error: Live record \"mx-1bb21d\" already exists in domain \"dev\" \
                 (classification: tactical). Delete or rename the live record before \
                 restoring, or run `mulch search --archived mx-1bb21d` to inspect the \
                 archived copy.\n"
            );
        }
    }
    assert_eq!(
        read_store_file(&ours.0, "archive/dev.jsonl"),
        format!("{MULCH_BANNER}\n{STAMPED_ONE}\n")
    );

    // malformed archive line: the pinned template, nothing restored
    let ours = archived_store("rbad", &[("dev", &[MALFORMED_LINE])], None);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-1bb21d"]);
    assert_eq!(run.code, 1);
    let text = normalize_dir(&run.stderr, &ours.0);
    assert!(
        text.starts_with("Error: Malformed JSONL at <DIR>/.mulch/archive/dev.jsonl:2: "),
        "template prefix: {text}"
    );
    assert!(
        text.ends_with(&format!(". Line: {MALFORMED_LINE}\n")),
        "template suffix: {text}"
    );

    // malformed LIVE line: the pre-check read aborts, archive intact
    let ours = archived_store("rbadl", &[("dev", &[STAMPED_ONE])], Some(MALFORMED_LINE));
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["restore", "mx-1bb21d"]);
    assert_eq!(run.code, 1);
    let text = normalize_dir(&run.stderr, &ours.0);
    assert!(
        text.starts_with("Error: Malformed JSONL at <DIR>/.mulch/expertise/dev.jsonl:1: "),
        "template prefix: {text}"
    );
    assert_eq!(
        read_store_file(&ours.0, "archive/dev.jsonl"),
        format!("{MULCH_BANNER}\n{STAMPED_ONE}\n")
    );
}

#[test]
fn restore_round_trips_the_full_type_set() {
    let types = [
        "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"content\":\"conv body\",\"id\":\"mx-conv01\"}",
        "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.050Z\",\"name\":\"Pat One\",\"description\":\"pd\",\"id\":\"mx-pat001\"}",
        "{\"type\":\"failure\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.100Z\",\"description\":\"fail desc\",\"resolution\":\"fixed\",\"id\":\"mx-fail01\"}",
        "{\"type\":\"decision\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:58.150Z\",\"title\":\"Decide A\",\"rationale\":\"why\",\"id\":\"mx-dec001\"}",
        "{\"type\":\"reference\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.200Z\",\"name\":\"Ref One\",\"description\":\"rd\",\"id\":\"mx-ref001\"}",
        "{\"type\":\"guide\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:47:58.250Z\",\"name\":\"Guide One\",\"description\":\"gd\",\"id\":\"mx-guid01\"}",
    ];
    let ours = seeded_store(
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
    let archived = run_in(&ours.0, Path::new(mulch_bin()), &archive_args);
    assert_eq!(archived.code, 0, "{}", archived.stderr);
    for id in ["mx-fail01", "mx-conv01", "mx-guid01"] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["restore", id]);
        assert_eq!(our.code, 0, "{id}: {}", our.stderr);
    }
    // the three restored records are live again (in restore order),
    // the other three stay archived.
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        format!("{}\n{}\n{}\n", types[2], types[0], types[5])
    );
}
