//! Differential battery for `mulch rank` (mulch-16da): twin stores,
//! ours vs the reference `ml` — plain table bytes, the strict flag
//! parse matrix, the `--json` payload, and the error surfaces
//! (no-store, unknown domain, strict reads, broken custom_types).
//! Probed live 2026-10-23 (/tmp/opencode/childC).

use std::path::Path;

use super::support::{
    CONFIG_TAIL, TempDir, error_envelope, mulch_bin, normalize_dir, promote_theirs, reference_ml,
    run_in, twin_seeded,
};

/// The ranking fixture: three domains in config order (zeta first —
/// tie order must follow CONFIG order, not alphabetical), records
/// with fixed ids and outcome histories covering 2 / 1.5 / 0.5 / 0
/// scores, an id-less record, and a files-carrying pattern.
const ZETA_SEED: &str = "{\
\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.000Z\",\
\"name\":\"Z One\",\"description\":\"zeta first\",\"files\":[\"src/a.rs\",\"src/b.rs\"],\
\"id\":\"mx-aaa001\",\"outcomes\":[{\"status\":\"success\",\"recorded_at\":\"2026-10-03T21:00:00.000Z\"},{\"status\":\"success\",\"recorded_at\":\"2026-10-03T21:01:00.000Z\"}]\
}\n{\
\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.000Z\",\
\"name\":\"Z Two\",\"description\":\"zeta second\",\"id\":\"mx-aaa002\",\
\"outcomes\":[{\"status\":\"success\",\"recorded_at\":\"2026-10-03T21:02:00.000Z\"},{\"status\":\"partial\",\"recorded_at\":\"2026-10-03T21:03:00.000Z\"}]\
}\n{\
\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:59.000Z\",\
\"content\":\"conv body\",\"id\":\"mx-aaa003\"\
}\n";

const MID_SEED: &str = "{\
\"type\":\"pattern\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T20:48:00.000Z\",\
\"name\":\"M One\",\"description\":\"mid pattern\",\"files\":[\"lib/x.ts\"],\"id\":\"mx-bbb001\",\
\"outcomes\":[{\"status\":\"partial\",\"recorded_at\":\"2026-10-03T21:04:00.000Z\"}]\
}\n{\
\"type\":\"guide\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:01.000Z\",\
\"name\":\"G One\",\"description\":\"guide body\",\"id\":\"mx-bbb002\",\
\"outcomes\":[{\"status\":\"failure\",\"recorded_at\":\"2026-10-03T21:05:00.000Z\"}]\
}\n";

const ALPHA_SEED: &str = "{\
\"type\":\"failure\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:02.000Z\",\
\"description\":\"fail desc\",\"resolution\":\"fix res\",\"id\":\"mx-ccc001\"\
}\n{\
\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:03.000Z\",\
\"name\":\"No Id\",\"description\":\"no id pattern\"\
}\n";

/// Twin ranking stores (config order zeta, mid, alpha).
fn rank_twins(tag: &str) -> (TempDir, TempDir) {
    twin_seeded(
        tag,
        None,
        &[
            ("zeta", ZETA_SEED),
            ("mid", MID_SEED),
            ("alpha", ALPHA_SEED),
        ],
        &[],
    )
}

/// One rank invocation in both twins: asserts channel-split byte
/// equality (stderr path-normalized per side) and returns our run.
fn assert_rank_parity(ml: &Path, tag: &str, args: &[&str]) -> super::support::Run {
    let (ours, theirs) = rank_twins(tag);
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, ml, args);
    assert_eq!(our.code, their.code, "{args:?} exit code");
    assert_eq!(
        our.stdout,
        promote_theirs(&their.stdout),
        "{args:?} stdout (ours promoted)"
    );
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        normalize_dir(&promote_theirs(&their.stderr), &theirs.0),
        "{args:?} stderr"
    );
    our
}

/// The default table: score-descending order, the padEnd columns, the
/// id-less row without its column, ties in config order (zeta's
/// convention, mid's guide, alpha's failure — all score 0).
#[test]
fn rank_defaults_match_the_reference_table() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let our = assert_rank_parity(&ml, "rank-def", &["rank"]);
    assert_eq!(
        our.stdout,
        "Top 7 records by confirmation score (all domains)\n\n\
         \x20 mx-aaa001  \u{2605}2    zeta          [pattern]     Z One\n\
         \x20 mx-aaa002  \u{2605}1.5  zeta          [pattern]     Z Two\n\
         \x20 mx-bbb001  \u{2605}0.5  mid           [pattern]     M One\n\
         \x20 mx-aaa003  \u{2605}0    zeta          [convention]  conv body\n\
         \x20 mx-bbb002  \u{2605}0    mid           [guide]       G One\n\
         \x20 mx-ccc001  \u{2605}0    alpha         [failure]     fail desc\n\
         \x20 \u{2605}0    alpha         [pattern]     No Id\n"
    );
    assert_eq!(our.stderr, "");
}

/// The positional domain scope and the flag matrix: limits (including
/// `007` and the huge digit string), min-score bounds (equal stays,
/// `1.50` and `0.0` canonicalize), the type filter.
#[test]
fn rank_domain_scope_and_flag_matrix_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for (tag, args) in [
        ("dom", vec!["rank", "zeta"]),
        ("dom2", vec!["rank", "mid"]),
        ("lim2", vec!["rank", "--limit", "2"]),
        ("lim007", vec!["rank", "--limit", "007", "--min-score", "2"]),
        ("limbig", vec![
            "rank",
            "--limit",
            "999999999999999999999999",
        ]),
        ("min15", vec!["rank", "--min-score", "1.5"]),
        ("min150", vec!["rank", "--min-score", "1.50"]),
        ("min0", vec!["rank", "--min-score", "0.0", "--limit", "3"]),
        ("minnone", vec!["rank", "--min-score", "999"]),
        ("type", vec!["rank", "--type", "pattern"]),
        ("type2", vec!["rank", "--type", "convention", "zeta"]),
        ("typemin", vec![
            "rank",
            "--type",
            "guide",
            "--min-score",
            "1",
        ]),
    ] {
        assert_rank_parity(&ml, &format!("rank-{tag}"), &args);
    }
}

/// The strict parse law: limit before min-score, both strict (regex +
/// JS Number), json envelopes on stderr, exit 1 — and the parse
/// errors beat the store and domain checks (probed in a no-store
/// dir: the flag error wins).
#[test]
fn rank_flag_parse_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    for raw in ["abc", "0", "-1", "3.7", "10abc", ""] {
        let tag = format!("rank-limerr-{}", raw.replace(['.', '-'], "x"));
        let our = assert_rank_parity(&ml, &tag, &["rank", "--limit", raw]);
        assert_eq!(our.code, 1, "--limit {raw} exits 1");
        assert_eq!(
            our.stderr,
            format!("Error: --limit must be a positive integer (got \"{raw}\").\n")
        );
    }
    for raw in ["abc", "-1", "1e3", ".5", "5."] {
        let tag = format!("rank-minerr-{}", raw.replace(['.', '-'], "x"));
        let our = assert_rank_parity(&ml, &tag, &["rank", "--min-score", raw]);
        assert_eq!(our.code, 1);
        assert_eq!(
            our.stderr,
            format!("Error: --min-score must be a non-negative number (got \"{raw}\").\n")
        );
    }
    // limit parses before min-score: both bad → the limit error.
    let our = assert_rank_parity(&ml, "rank-both-bad", &[
        "rank",
        "--limit",
        "abc",
        "--min-score",
        "xyz",
    ]);
    assert!(our.stderr.contains("--limit must be"));

    // json variant: the envelope drops the Error: prefix, stderr.
    let (ours, theirs) = rank_twins("rank-json-err");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "rank", "--limit", "abc",
    ]);
    let their = run_in(&theirs.0, &ml, &["--json", "rank", "--limit", "abc"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, "");
    assert_eq!(our.stderr, their.stderr);
    assert_eq!(
        our.stderr,
        error_envelope("rank", "--limit must be a positive integer (got \"abc\").")
    );
}

/// The `--json` success payload: envelope shape, score
/// canonicalization (2 and 1.5, never 2.0), the embedded record, the
/// null id of an id-less record, count 0 on an empty store.
#[test]
fn rank_json_payload_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let our = assert_rank_parity(&ml, "rank-json", &["--json", "rank", "--limit", "2"]);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"rank\",\n  \"count\": 2,\n  \"records\": [\
\n    {\n      \"domain\": \"zeta\",\n      \"id\": \"mx-aaa001\",\n      \"type\": \"pattern\",\n      \"score\": 2,\n      \"summary\": \"Z One\",\n      \"record\": {\n        \"type\": \"pattern\",\n        \"classification\": \"tactical\",\n        \"recorded_at\": \"2026-10-03T20:47:57.000Z\",\n        \"name\": \"Z One\",\n        \"description\": \"zeta first\",\n        \"files\": [\n          \"src/a.rs\",\n          \"src/b.rs\"\n        ],\n        \"id\": \"mx-aaa001\",\n        \"outcomes\": [\n          {\n            \"status\": \"success\",\n            \"recorded_at\": \"2026-10-03T21:00:00.000Z\"\n          },\n          {\n            \"status\": \"success\",\n            \"recorded_at\": \"2026-10-03T21:01:00.000Z\"\n          }\n        ]\n      }\n    },\
\n    {\n      \"domain\": \"zeta\",\n      \"id\": \"mx-aaa002\",\n      \"type\": \"pattern\",\n      \"score\": 1.5,\n      \"summary\": \"Z Two\",\n      \"record\": {\n        \"type\": \"pattern\",\n        \"classification\": \"tactical\",\n        \"recorded_at\": \"2026-10-03T20:47:58.000Z\",\n        \"name\": \"Z Two\",\n        \"description\": \"zeta second\",\n        \"id\": \"mx-aaa002\",\n        \"outcomes\": [\n          {\n            \"status\": \"success\",\n            \"recorded_at\": \"2026-10-03T21:02:00.000Z\"\n          },\n          {\n            \"status\": \"partial\",\n            \"recorded_at\": \"2026-10-03T21:03:00.000Z\"\n          }\n        ]\n      }\n    }\n  ]\n}\n"
    );

    // the id-less record: json id null.
    assert_rank_parity(&ml, "rank-json-noid", &["--json", "rank", "alpha"]);

    // an empty store: count 0.
    let (ours, theirs) = twin_seeded("rank-json-empty", None, &[("zeta", "")], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "rank"]);
    let their = run_in(&theirs.0, &ml, &["--json", "rank"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stderr, their.stderr);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"rank\",\n  \"count\": 0,\n  \"records\": []\n}\n"
    );
}

/// Empty and no-store surfaces: the fixed empty text, the no-store
/// config message (plain + json stderr), the unknown domain (two-line
/// plain with the hint, one-line json), the empty-domains `(none)`
/// list, and the registry-gated `--type` choice set (clap wording is
/// the documented deviation — the SET and exit code pin).
#[test]
fn rank_empty_no_store_and_unknown_domain_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // empty store: fixed text, exit 0 (scoped and unscoped).
    {
        let (ours, theirs) = twin_seeded("rank-empty", None, &[("solo", "")], &[]);
        for args in [&["rank"] as &[&str], &["rank", "solo"]] {
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            let their = run_in(&theirs.0, &ml, args);
            assert_eq!(our.code, their.code);
            assert_eq!(our.stdout, "No records match the ranking criteria.\n");
            assert_eq!(our.stdout, their.stdout);
        }
    }
    // no store: the readConfig wording, plain and json.
    {
        let ours = TempDir::new("rank-nostore-o");
        let theirs = TempDir::new("rank-nostore-t");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank"]);
        let their = run_in(&theirs.0, &ml, &["rank"]);
        assert_eq!(our.code, their.code);
        assert_eq!(our.stdout, their.stdout);
        assert_eq!(
            our.stderr,
            "Error: No .mulch/ directory found. Run `mulch init` to set up this project.\n"
        );
        assert_eq!(our.stderr, their.stderr);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "rank"]);
        let their = run_in(&theirs.0, &ml, &["--json", "rank"]);
        assert_eq!(our.code, their.code);
        assert_eq!(our.stdout, "");
        assert_eq!(our.stderr, their.stderr);
        assert_eq!(
            our.stderr,
            error_envelope(
                "rank",
                "No .mulch/ directory found. Run `mulch init` to set up this project."
            )
        );
    }
    // unknown domain: two-line plain (hint names OUR binary), one-line json.
    {
        let (ours, theirs) = rank_twins("rank-unkdom");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "nosuch"]);
        let their = run_in(&theirs.0, &ml, &["rank", "nosuch"]);
        assert_eq!(our.code, their.code);
        assert_eq!(our.stdout, their.stdout);
        assert_eq!(our.stderr, promote_theirs(&their.stderr));
        assert_eq!(
            our.stderr,
            "Error: Domain \"nosuch\" not found in config. Available domains: zeta, mid, alpha\n\
             Hint: Run `mulch add nosuch` to create this domain, or check .mulch/mulch.config.yaml\n"
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &[
            "--json", "rank", "nosuch",
        ]);
        let their = run_in(&theirs.0, &ml, &["--json", "rank", "nosuch"]);
        assert_eq!(our.code, their.code);
        assert_eq!(our.stdout, "");
        assert_eq!(our.stderr, their.stderr);
        assert_eq!(
            our.stderr,
            error_envelope(
                "rank",
                "Domain \"nosuch\" not found in config. Available domains: zeta, mid, alpha"
            )
        );
    }
    // a config with NO domains: the (none) list.
    {
        let config = format!("version: '1'\ndomains: {{}}\n{CONFIG_TAIL}");
        let (ours, theirs) = twin_seeded("rank-nodomains", Some(&config), &[], &[]);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "x"]);
        let their = run_in(&theirs.0, &ml, &["rank", "x"]);
        assert_eq!(our.code, their.code);
        assert!(
            our.stderr.contains("Available domains: (none)"),
            "{}",
            our.stderr
        );
        assert_eq!(our.stderr, promote_theirs(&their.stderr));
    }
    // `--type` gates on the registry: our clap wording is the
    // documented deviation; the choice set and exit code pin.
    {
        let (ours, theirs) = rank_twins("rank-badtype");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &[
            "rank",
            "--type",
            "nosuchtype",
        ]);
        let their = run_in(&theirs.0, &ml, &["rank", "--type", "nosuchtype"]);
        assert_eq!(our.code, their.code);
        assert_eq!(our.code, 1);
        assert!(
            our.stderr.contains(
                "[possible values: convention, pattern, failure, decision, reference, guide]"
            ),
            "choice set is the registry: {}",
            our.stderr
        );
        assert!(their.stderr.contains(
            "Allowed choices are convention, pattern, failure, decision, reference, guide."
        ));
    }
}

/// The strict-read surfaces: a malformed line aborts (template
/// pinned; the engine's reason wording inside it is the documented
/// divergence), an unknown type aborts, and under
/// --allow-unknown-types the summary read throws mid-render (plain:
/// partial stdout then the error; json: bare error envelope).
#[test]
fn rank_strict_read_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let malformed = format!("{ZETA_SEED}{{\"type\":\"pattern\", BAD\n");
    let (ours, theirs) = twin_seeded(
        "rank-malformed",
        None,
        &[("zeta", malformed.as_str()), ("mid", MID_SEED)],
        &[],
    );
    for args in [&["rank"] as &[&str], &["--json", "rank"]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, their.stdout, "{} stdout", args[0]);
        let our_stderr = normalize_dir(&our.stderr, &ours.0);
        let their_stderr = normalize_dir(&their.stderr, &theirs.0);
        // plain carries the Error: line (with the raw preview tail);
        // json the envelope (JSON-escaped preview) — both byte-equal
        // already, so only the template head pins per mode
        let head = if args[0] == "--json" {
            format!(
                "{{\n  \"success\": false,\n  \"command\": \"{}\",\n  \"error\": \"Malformed JSONL at <DIR>/.mulch/expertise/zeta.jsonl:4: ",
                args[1]
            )
        } else {
            "Error: Malformed JSONL at <DIR>/.mulch/expertise/zeta.jsonl:4: ".to_string()
        };
        assert!(our_stderr.starts_with(&head), "our template: {our_stderr}");
        assert!(
            their_stderr.starts_with(&head),
            "their template: {their_stderr}"
        );
        if args.first() != Some(&"--json") {
            assert!(our_stderr.ends_with(". Line: {\"type\":\"pattern\", BAD\n"));
        }
    }

    // unknown type: the registry text verbatim; --allow-unknown-types
    // admits the record but getRecordSummary throws (plain: partial
    // table; json: error envelope).
    let unknown = format!(
        "{ZETA_SEED}{{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\
\"recorded_at\":\"2026-10-03T20:48:04.000Z\",\"name\":\"Odd\",\"id\":\"mx-zzz001\"}}\n"
    );
    let (ours, theirs) = twin_seeded("rank-unktype", None, &[("zeta", unknown.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "zeta"]);
    let their = run_in(&theirs.0, &ml, &["rank", "zeta"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        normalize_dir(&their.stderr, &theirs.0)
    );

    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--allow-unknown-types",
        "rank",
        "zeta",
    ]);
    let their = run_in(&theirs.0, &ml, &["--allow-unknown-types", "rank", "zeta"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 1);
    // partial table: header (the unknown record still COUNTS) + rows
    // before the throwing record.
    assert_eq!(our.stdout, their.stdout);
    assert!(our.stdout.contains("Top 4 records"));
    assert!(our.stdout.contains("Z One"));
    assert_eq!(our.stderr, "Error: Unknown record type: zzzcustom\n");
    assert_eq!(our.stderr, promote_theirs(&their.stderr));

    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--allow-unknown-types",
        "--json",
        "rank",
        "zeta",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "--allow-unknown-types",
        "--json",
        "rank",
        "zeta",
    ]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, "");
    assert_eq!(our.stderr, their.stderr);
    assert_eq!(
        our.stderr,
        error_envelope("rank", "Unknown record type: zzzcustom")
    );
}

/// A broken custom_types block fails every command through the GLOBAL
/// config-error surface before rank's own logic (even before the flag
/// parses — probed).
#[test]
fn rank_broken_custom_types_fails_the_global_surface() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let config =
        "version: '1'\ndomains:\n  zeta: {}\ncustom_types:\n  bad:\n    required: [content]\n";
    let (ours, theirs) = twin_seeded("rank-broken", Some(config), &[("zeta", ZETA_SEED)], &[]);
    for args in [&["rank"] as &[&str], &["rank", "--limit", "abc"], &[
        "--json", "rank",
    ]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{args:?}");
        assert_eq!(our.code, 1);
        assert_eq!(our.stderr, their.stderr, "{args:?}");
        if args.first() == Some(&"--json") {
            assert!(
                our.stderr.contains("\"command\": \"init\""),
                "{}",
                our.stderr
            );
        } else {
            assert!(
                our.stderr
                    .starts_with("Config error: Custom type \"bad\" must set \"dedup_key\""),
                "{}",
                our.stderr
            );
        }
    }
}

/// Registered custom types rank and summarize through their registry
/// row (custom summary template, custom files field).
#[test]
fn rank_custom_type_records_rank_and_summarize() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let config = "version: '1'\ndomains:\n  ops: {}\ncustom_types:\n  deploy_note:\n    required: [service, action]\n    optional: [files]\n    dedup_key: service\n    summary: \"{service}: {action}\"\n";
    let seed = "{\
\"type\":\"deploy_note\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:49:00.000Z\",\
\"service\":\"api\",\"action\":\"restart\",\"files\":[\"k8s/api.yml\"],\"id\":\"mx-dep001\",\
\"outcomes\":[{\"status\":\"success\",\"recorded_at\":\"2026-10-03T21:06:00.000Z\"}]\
}\n";
    let (ours, theirs) = twin_seeded("rank-custom", Some(config), &[("ops", seed)], &[]);
    for args in [&["rank"] as &[&str], &["rank", "--type", "deploy_note"], &[
        "--json", "rank",
    ]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{args:?}");
        assert_eq!(our.stdout, their.stdout, "{args:?}");
        assert_eq!(our.stderr, their.stderr, "{args:?}");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank"]);
    assert!(our.stdout.contains("api: restart"), "{}", our.stdout);
}
