//! Battery for `mulch rank` (mulch-16da), ADR-0008 edition: OUR byte
//! pins for the plain table, the strict flag parse matrix, the
//! `--json` payload, and the error surfaces (no-store, unknown
//! domain, strict reads, broken custom_types). The default table and
//! json payload are frozen verbatim; matrix rows keep representative
//! frozen tables plus membership pins.

use std::path::Path;

use serde_json::json;

use super::support::{
    CONFIG_TAIL, TempDir, error_envelope, mulch_bin, normalize_dir, run_in, seeded_store,
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

/// A ranking store (config order zeta, mid, alpha).
fn rank_store(tag: &str) -> TempDir {
    seeded_store(
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

/// The default table (frozen): score-descending order, the padEnd
/// columns, the id-less row without its column, ties in config order.
#[test]
fn rank_defaults_pin_the_table() {
    let ours = rank_store("rank-def");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank"]);
    assert_eq!(our.code, 0);
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

/// The positional domain scope and the flag matrix: limits
/// (including `007` and the huge digit string), min-score bounds,
/// the type filter — representative rows frozen, the rest membership.
#[test]
fn rank_domain_scope_and_flag_matrix() {
    let ours = rank_store("rank-flags");
    // domain scope: zeta only, header names the domain
    let dom = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "zeta"]);
    assert_eq!(dom.code, 0);
    assert_eq!(
        dom.stdout,
        "Top 3 records by confirmation score (zeta)\n\n\
         \x20 mx-aaa001  \u{2605}2    zeta          [pattern]     Z One\n\
         \x20 mx-aaa002  \u{2605}1.5  zeta          [pattern]     Z Two\n\
         \x20 mx-aaa003  \u{2605}0    zeta          [convention]  conv body\n"
    );
    // limit truncates the walk
    let lim2 = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "--limit", "2"]);
    assert_eq!(lim2.code, 0);
    assert_eq!(
        lim2.stdout,
        "Top 2 records by confirmation score (all domains)\n\n\
         \x20 mx-aaa001  \u{2605}2    zeta          [pattern]     Z One\n\
         \x20 mx-aaa002  \u{2605}1.5  zeta          [pattern]     Z Two\n"
    );
    // a huge limit accepts (JS Number law) and keeps the table
    let limbig = run_in(&ours.0, Path::new(mulch_bin()), &[
        "rank",
        "--limit",
        "999999999999999999999999",
    ]);
    assert_eq!(limbig.code, 0);
    assert!(limbig.stdout.starts_with("Top 7 records"));
    // `007` parses as 7
    let lim007 = run_in(&ours.0, Path::new(mulch_bin()), &[
        "rank",
        "--limit",
        "007",
        "--min-score",
        "2",
    ]);
    assert_eq!(lim007.code, 0);
    assert!(lim007.stdout.starts_with("Top 1 record"));
    // min-score bounds: `1.50` canonicalizes like `1.5`
    for raw in ["1.5", "1.50"] {
        let min = run_in(&ours.0, Path::new(mulch_bin()), &[
            "rank",
            "--min-score",
            raw,
        ]);
        assert_eq!(min.code, 0, "{raw}");
        assert!(min.stdout.contains("mx-aaa001"), "{raw}");
        assert!(min.stdout.contains("mx-aaa002"), "{raw}");
        assert!(!min.stdout.contains("mx-aaa003"), "{raw}");
    }
    // min above every score: the fixed empty text
    let minnone = run_in(&ours.0, Path::new(mulch_bin()), &[
        "rank",
        "--min-score",
        "999",
    ]);
    assert_eq!(minnone.code, 0);
    assert_eq!(minnone.stdout, "No records match the ranking criteria.\n");
    // the type filter: patterns only (the id-less row included)
    let typed = run_in(&ours.0, Path::new(mulch_bin()), &[
        "rank", "--type", "pattern",
    ]);
    assert_eq!(typed.code, 0);
    assert!(typed.stdout.starts_with("Top 4 records"));
    assert!(typed.stdout.contains("No Id"));
    assert!(!typed.stdout.contains("conv body"));
    // type + domain + min together
    let combined = run_in(&ours.0, Path::new(mulch_bin()), &[
        "rank",
        "--type",
        "convention",
        "zeta",
    ]);
    assert!(combined.stdout.starts_with("Top 1 record"));
    assert!(combined.stdout.contains("conv body"));
}

/// The strict parse law: limit before min-score, both strict (regex +
/// JS Number), json envelopes on stderr, exit 1.
#[test]
fn rank_flag_parse_errors_pin_the_wording() {
    let ours = rank_store("rank-limerr");
    for raw in ["abc", "0", "-1", "3.7", "10abc", ""] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "--limit", raw]);
        assert_eq!(our.code, 1, "--limit {raw} exits 1");
        assert_eq!(
            our.stderr,
            format!("Error: --limit must be a positive integer (got \"{raw}\").\n")
        );
    }
    for raw in ["abc", "-1", "1e3", ".5", "5."] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &[
            "rank",
            "--min-score",
            raw,
        ]);
        assert_eq!(our.code, 1);
        assert_eq!(
            our.stderr,
            format!("Error: --min-score must be a non-negative number (got \"{raw}\").\n")
        );
    }
    // limit parses before min-score: both bad → the limit error.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "rank",
        "--limit",
        "abc",
        "--min-score",
        "xyz",
    ]);
    assert!(our.stderr.contains("--limit must be"));

    // json variant: the envelope drops the Error: prefix, stderr.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "rank", "--limit", "abc",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        error_envelope("rank", "--limit must be a positive integer (got \"abc\").")
    );
}

/// The `--json` success payload (frozen): envelope shape, score
/// canonicalization (2 and 1.5, never 2.0), the embedded record, the
/// null id of an id-less record, count 0 on an empty store.
#[test]
fn rank_json_payload_pins_the_bytes() {
    let ours = rank_store("rank-json");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "rank", "--limit", "2",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"rank\",\n  \"count\": 2,\n  \"records\": [\
\n    {\n      \"domain\": \"zeta\",\n      \"id\": \"mx-aaa001\",\n      \"type\": \"pattern\",\n      \"score\": 2,\n      \"summary\": \"Z One\",\n      \"record\": {\n        \"type\": \"pattern\",\n        \"classification\": \"tactical\",\n        \"recorded_at\": \"2026-10-03T20:47:57.000Z\",\n        \"name\": \"Z One\",\n        \"description\": \"zeta first\",\n        \"files\": [\n          \"src/a.rs\",\n          \"src/b.rs\"\n        ],\n        \"id\": \"mx-aaa001\",\n        \"outcomes\": [\n          {\n            \"status\": \"success\",\n            \"recorded_at\": \"2026-10-03T21:00:00.000Z\"\n          },\n          {\n            \"status\": \"success\",\n            \"recorded_at\": \"2026-10-03T21:01:00.000Z\"\n          }\n        ]\n      }\n    },\
\n    {\n      \"domain\": \"zeta\",\n      \"id\": \"mx-aaa002\",\n      \"type\": \"pattern\",\n      \"score\": 1.5,\n      \"summary\": \"Z Two\",\n      \"record\": {\n        \"type\": \"pattern\",\n        \"classification\": \"tactical\",\n        \"recorded_at\": \"2026-10-03T20:47:58.000Z\",\n        \"name\": \"Z Two\",\n        \"description\": \"zeta second\",\n        \"id\": \"mx-aaa002\",\n        \"outcomes\": [\n          {\n            \"status\": \"success\",\n            \"recorded_at\": \"2026-10-03T21:02:00.000Z\"\n          },\n          {\n            \"status\": \"partial\",\n            \"recorded_at\": \"2026-10-03T21:03:00.000Z\"\n          }\n        ]\n      }\n    }\n  ]\n}\n"
    );

    // the id-less record: json id null.
    let ours = rank_store("rank-json-noid");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "rank", "alpha",
    ]);
    assert_eq!(our.code, 0);
    assert!(our.stdout.contains("\"id\": null"), "{}", our.stdout);

    // an empty store: count 0.
    let ours = seeded_store("rank-json-empty", None, &[("zeta", "")], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "rank"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"rank\",\n  \"count\": 0,\n  \"records\": []\n}\n"
    );
}

/// Empty and no-store surfaces: the fixed empty text, the no-store
/// config message (plain + json stderr), the unknown domain (two-line
/// plain with the hint, one-line json), the empty-domains `(none)`
/// list, and the registry-gated `--type` choice set.
#[test]
fn rank_empty_no_store_and_unknown_domain() {
    // empty store: fixed text, exit 0 (scoped and unscoped).
    {
        let ours = seeded_store("rank-empty", None, &[("solo", "")], &[]);
        for args in [&["rank"] as &[&str], &["rank", "solo"]] {
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            assert_eq!(our.code, 0);
            assert_eq!(our.stdout, "No records match the ranking criteria.\n");
        }
    }
    // no store: the readConfig wording, plain and json.
    {
        let ours = TempDir::new("rank-nostore");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank"]);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, "");
        assert_eq!(
            our.stderr,
            "Error: No .mulch/ directory found. Run `mulch init` to set up this project.\n"
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "rank"]);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, "");
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
        let ours = rank_store("rank-unkdom");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "nosuch"]);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, "");
        assert_eq!(
            our.stderr,
            "Error: Domain \"nosuch\" not found in config. Available domains: zeta, mid, alpha\n\
             Hint: Run `mulch add nosuch` to create this domain, or check .mulch/mulch.config.yaml\n"
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &[
            "--json", "rank", "nosuch",
        ]);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, "");
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
        let ours = seeded_store("rank-nodomains", Some(&config), &[], &[]);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "x"]);
        assert_eq!(our.code, 1);
        assert!(
            our.stderr.contains("Available domains: (none)"),
            "{}",
            our.stderr
        );
    }
    // `--type` gates on the registry: the choice set and exit code pin
    // (our clap wording is the documented deviation).
    {
        let ours = rank_store("rank-badtype");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &[
            "rank",
            "--type",
            "nosuchtype",
        ]);
        assert_eq!(our.code, 1);
        assert!(
            our.stderr.contains(
                "[possible values: convention, pattern, failure, decision, reference, guide]"
            ),
            "choice set is the registry: {}",
            our.stderr
        );
    }
}

/// The strict-read surfaces: a malformed line aborts (template
/// pinned), an unknown type aborts, and under --allow-unknown-types
/// the summary read throws mid-render (plain: partial stdout then the
/// error; json: bare error envelope).
#[test]
fn rank_strict_read_errors_pin_the_templates() {
    let malformed = format!("{ZETA_SEED}{{\"type\":\"pattern\", BAD\n");
    let ours = seeded_store(
        "rank-malformed",
        None,
        &[("zeta", malformed.as_str()), ("mid", MID_SEED)],
        &[],
    );
    for args in [&["rank"] as &[&str], &["--json", "rank"]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, "");
        let our_stderr = normalize_dir(&our.stderr, &ours.0);
        let head = if args[0] == "--json" {
            format!(
                "{{\n  \"success\": false,\n  \"command\": \"{}\",\n  \"error\": \"Malformed JSONL at <DIR>/.mulch/expertise/zeta.jsonl:4: ",
                args[1]
            )
        } else {
            "Error: Malformed JSONL at <DIR>/.mulch/expertise/zeta.jsonl:4: ".to_string()
        };
        assert!(our_stderr.starts_with(&head), "our template: {our_stderr}");
        if args.first() != Some(&"--json") {
            assert!(our_stderr.ends_with(". Line: {\"type\":\"pattern\", BAD\n"));
        }
    }

    // unknown type: the registry text verbatim (path folded).
    let unknown = format!(
        "{ZETA_SEED}{{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\
\"recorded_at\":\"2026-10-03T20:48:04.000Z\",\"name\":\"Odd\",\"id\":\"mx-zzz001\"}}\n"
    );
    let ours = seeded_store("rank-unktype", None, &[("zeta", unknown.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "zeta"]);
    assert_eq!(our.code, 1);
    assert!(
        normalize_dir(&our.stderr, &ours.0).starts_with(
            "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/zeta.jsonl:4 \
             (id=mx-zzz001)."
        ),
        "{}",
        our.stderr
    );

    // --allow-unknown-types admits the record but the summary read
    // throws (plain: partial table; json: error envelope).
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--allow-unknown-types",
        "rank",
        "zeta",
    ]);
    assert_eq!(our.code, 1);
    // partial table: header (the unknown record still COUNTS) + rows
    // before the throwing record.
    assert!(our.stdout.contains("Top 4 records"));
    assert!(our.stdout.contains("Z One"));
    assert_eq!(our.stderr, "Error: Unknown record type: zzzcustom\n");

    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--allow-unknown-types",
        "--json",
        "rank",
        "zeta",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
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
    let config =
        "version: '1'\ndomains:\n  zeta: {}\ncustom_types:\n  bad:\n    required: [content]\n";
    let ours = seeded_store("rank-broken", Some(config), &[("zeta", ZETA_SEED)], &[]);
    for args in [&["rank"] as &[&str], &["rank", "--limit", "abc"], &[
        "--json", "rank",
    ]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?}");
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
/// row (custom summary template, custom files field) — frozen.
#[test]
fn rank_custom_type_records_rank_and_summarize() {
    let config = "version: '1'\ndomains:\n  ops: {}\ncustom_types:\n  deploy_note:\n    required: [service, action]\n    optional: [files]\n    dedup_key: service\n    summary: \"{service}: {action}\"\n";
    let seed = "{\
\"type\":\"deploy_note\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:49:00.000Z\",\
\"service\":\"api\",\"action\":\"restart\",\"files\":[\"k8s/api.yml\"],\"id\":\"mx-dep001\",\
\"outcomes\":[{\"status\":\"success\",\"recorded_at\":\"2026-10-03T21:06:00.000Z\"}]\
}\n";
    let ours = seeded_store("rank-custom", Some(config), &[("ops", seed)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["rank"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "Top 1 record by confirmation score (all domains)\n\n\
         \x20 mx-dep001  \u{2605}1    ops           [deploy_note] api: restart\n"
    );
    // the type filter routes through the registry row too
    let typed = run_in(&ours.0, Path::new(mulch_bin()), &[
        "rank",
        "--type",
        "deploy_note",
    ]);
    assert_eq!(typed.code, 0);
    assert!(typed.stdout.contains("api: restart"));
    // json: the custom summary and the embedded record with files
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "rank"]);
    assert_eq!(json.code, 0);
    assert!(json.stdout.contains("\"type\": \"deploy_note\""));
    assert!(json.stdout.contains("\"summary\": \"api: restart\""));
    assert!(json.stdout.contains("\"service\": \"api\""));
}

#[test]
fn raw_summary_surfaces_pin_the_render_law() {
    // mulch-f06a: the summary-carrying surfaces embed the RAW summary
    // value — a missing summary field renders `undefined` in plain
    // views and OMITS the json key; a non-string passes through
    // verbatim; the truncating failure summary cuts at 60.
    let ours = TempDir::new("f06a");
    let stale = {
        use jiff::Timestamp;
        let at = Timestamp::now() - jiff::Span::new().hours(48);
        at.to_string()
    };
    let long = "L".repeat(80);
    let body = format!(
        "{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":\"Named\",\"description\":\"d\",\"id\":\"mx-aaa001\"}}\n\
{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"description\":\"d\",\"id\":\"mx-aaa002\"}}\n\
{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"name\":42,\"description\":\"d\",\"id\":\"mx-aaa003\"}}\n\
{{\"type\":\"failure\",\"classification\":\"tactical\",\"recorded_at\":\"{stale}\",\"description\":\"{long}\",\"resolution\":\"r\",\"id\":\"mx-aaa004\"}}\n"
    );
    let config = "version: '1'\ndomains:\n  dev: {}\n";
    super::support::write_seed_with_config(&ours.0, config, &[("dev", body.as_str())]);

    // the rank table (frozen): undefined, the raw 42, and the
    // truncated failure summary
    let rank = run_in(&ours.0, Path::new(mulch_bin()), &["rank", "dev"]);
    assert_eq!(rank.code, 0);
    assert_eq!(
        rank.stdout,
        format!(
            "Top 4 records by confirmation score (dev)\n\n\
             \x20 mx-aaa001  \u{2605}0    dev           [pattern]     Named\n\
             \x20 mx-aaa002  \u{2605}0    dev           [pattern]     undefined\n\
             \x20 mx-aaa003  \u{2605}0    dev           [pattern]     42\n\
             \x20 mx-aaa004  \u{2605}0    dev           [failure]     {}...\n",
            "L".repeat(60)
        )
    );
    // the json payload omits the summary key for the missing field
    // and keeps the raw number
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "rank", "dev"]);
    assert_eq!(json.code, 0);
    let value: serde_json::Value = serde_json::from_str(&json.stdout).expect("json");
    let records = value["records"].as_array().expect("records array");
    let nameless = records
        .iter()
        .find(|record| record["id"] == json!("mx-aaa002"))
        .expect("the name-less record rides along");
    assert!(
        nameless.get("summary").is_none(),
        "missing field omits the json key: {nameless}"
    );
    let numeric = records
        .iter()
        .find(|record| record["id"] == json!("mx-aaa003"))
        .expect("the numeric-name record rides along");
    assert_eq!(numeric["summary"], json!(42));
    assert!(
        json.stdout
            .contains(&format!("\"summary\": \"{}...\"", "L".repeat(60)))
    );
    // the delete json envelope carries the raw summary verbatim (a
    // missing field omits the key; the number passes through)
    let delete = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "delete",
        "dev",
        "--records",
        "mx-aaa002,mx-aaa003",
    ]);
    assert_eq!(delete.code, 0);
    let value: serde_json::Value = serde_json::from_str(&delete.stdout).expect("json");
    let deleted = value["deleted"].as_array().expect("deleted array");
    assert!(
        deleted[0].get("summary").is_none(),
        "missing field omits the key: {}",
        deleted[0]
    );
    assert_eq!(deleted[1]["summary"], json!(42));
}
