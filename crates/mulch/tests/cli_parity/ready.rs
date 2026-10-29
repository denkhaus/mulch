//! Battery for `mulch ready` (mulch-16da), ADR-0008 edition: OUR
//! byte pins for the plain table (relative ages, the padEnd columns,
//! config-order ties), the flag matrix's literal surfaces, the
//! `--json` envelope (key order, null id, omitted recorded_at), and
//! the error surfaces (no store, strict flags, unknown domain, strict
//! reads, unknown / missing / non-string types, broken custom_types,
//! invalid configured domain names).
//!
//! Two probed quirks are documented rather than pinned, because they
//! live outside this surface: the Array#sort order for NaN dates at
//! n >= 5 and the `--domain constructor` prototype-chain divergence.
//! (Historical probes: /tmp/sprint35b/ready.)

use std::path::Path;

use super::support::{
    CONFIG_TAIL, TempDir, error_envelope, mulch_bin, normalize_dir, run_in, seeded_store,
};

/// An RFC 3339 timestamp `hours` before now. Every fixture offset sits
/// mid-bucket (4h clear of the next day boundary), so the age label
/// stays stable for the test's duration.
fn at(hours: i64) -> String {
    (jiff::Timestamp::now() - jiff::Span::new().hours(hours)).to_string()
}

/// One `pattern` line recorded `hours` before now.
fn pattern_line(name: &str, id: &str, hours: i64) -> String {
    let at = at(hours);
    format!(
        "{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{at}\",\
\"name\":\"{name}\",\"description\":\"d\",\"id\":\"{id}\"}}\n"
    )
}

/// A ready store: config order zeta, mid, alpha (tie order must
/// follow CONFIG order) and ages 4d / 1d / 2d / 20h / 5h.
fn ready_store(tag: &str) -> TempDir {
    let zeta = format!(
        "{}{}",
        pattern_line("A", "mx-aaaa01", 100),
        pattern_line("B", "mx-aaaa02", 28)
    );
    let mid = format!(
        "{}{}",
        pattern_line("C", "mx-bbbb01", 51),
        pattern_line("D", "mx-bbbb02", 20)
    );
    let alpha = pattern_line("E", "mx-cccc01", 5);
    seeded_store(
        tag,
        None,
        &[
            ("zeta", zeta.as_str()),
            ("mid", mid.as_str()),
            ("alpha", alpha.as_str()),
        ],
        &[],
    )
}

/// The default table (frozen): recorded_at DESC, the `  <id>  ` column
/// only when the id is JS-truthy, padEnd 10 / 14 / 14, and the `1
/// records` header (no pluralization).
#[test]
fn ready_defaults_pin_the_table() {
    let ours = ready_store("ready-def");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "Recent Expertise (last 5 records)\n\n\
         \x20 mx-cccc01  5h ago    alpha         [pattern]     E\n\
         \x20 mx-bbbb02  20h ago   mid           [pattern]     D\n\
         \x20 mx-aaaa02  1d ago    zeta          [pattern]     B\n\
         \x20 mx-bbbb01  2d ago    mid           [pattern]     C\n\
         \x20 mx-aaaa01  4d ago    zeta          [pattern]     A\n"
    );
    assert_eq!(our.stderr, "");
}

/// The flag matrix: limits (including `007` and the huge digit run),
/// the domain scope, the `--since` windows, the json variants, and
/// `-q` (which does NOT suppress ready — it prints through
/// `console.log`).
#[test]
fn ready_flag_matrix_pins_the_law() {
    let ours = ready_store("ready-flags");
    for (args, header) in [
        (
            vec!["ready", "--limit", "2"],
            "Recent Expertise (last 2 records)",
        ),
        (
            vec!["ready", "--limit", "1"],
            "Recent Expertise (last 1 records)",
        ),
        (
            vec!["ready", "--limit", "999999999999999999999999"],
            "Recent Expertise (last 5 records)",
        ),
        (
            vec!["ready", "--domain", "mid"],
            "Recent Expertise (last 2 records)",
        ),
        (
            vec!["ready", "--domain", "alpha", "--limit", "1"],
            "Recent Expertise (last 1 records)",
        ),
        (
            vec!["ready", "--since", "0007d"],
            "Recent Expertise (last 0007d)",
        ),
        (vec!["ready", "--since", "2w"], "Recent Expertise (last 2w)"),
        (
            vec!["ready", "--since", "0h"],
            "No recent expertise records found.",
        ),
        // JS truthiness: an empty flag value is an absent flag.
        (
            vec!["ready", "--domain", ""],
            "Recent Expertise (last 5 records)",
        ),
        (
            vec!["ready", "--since", ""],
            "Recent Expertise (last 5 records)",
        ),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?}: {}", our.stderr);
        assert_eq!(our.stdout.lines().next(), Some(header), "{args:?} header");
    }
    // `--limit 007` parses as 7 (the full table renders)
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "ready", "--limit", "007",
    ]);
    assert!(
        our.stdout.starts_with("Recent Expertise (last 5 records)"),
        "{}",
        our.stdout
    );

    // `--since` echoes the RAW string in the header (no duration echo).
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "ready", "--since", "0007d",
    ]);
    assert_eq!(
        our.stdout.lines().next(),
        Some("Recent Expertise (last 0007d)")
    );
    // 24h keeps only the two records inside the window; the header
    // echoes the raw `--since` string, not the count.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "ready", "--since", "24h",
    ]);
    assert_eq!(
        our.stdout.lines().next(),
        Some("Recent Expertise (last 24h)")
    );
    assert!(!our.stdout.contains("mx-aaaa01"), "{}", our.stdout);
    assert!(our.stdout.contains("mx-cccc01"), "{}", our.stdout);
    // `-q` does not suppress the table.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["-q", "ready"]);
    assert!(our.stdout.starts_with("Recent Expertise (last 5 records)"));
    // json rides the envelope in both quiet and plain
    for args in [
        vec!["--json", "ready"],
        vec!["-q", "--json", "ready"],
        vec!["--json", "ready", "--since", "24h"],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?}");
        assert!(our.stdout.contains("\"command\": \"ready\""), "{args:?}");
    }
}

/// A fixed-date store for the `--json` byte pin (no ages in the
/// payload, so fixed timestamps are stable across days).
const FIXED_READY_SEED: &str = "{\
\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.000Z\",\
\"name\":\"Alpha One\",\"description\":\"first alpha pattern\",\"id\":\"mx-1bb21d\"\
}\n{\
\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.000Z\",\
\"content\":\"conv body\",\"id\":\"mx-2bb21d\"\
}\n";

/// The `--json` success payload (frozen): envelope shape, the entry
/// key order `domain, id, type, recorded_at, summary, record`, the
/// embedded record, the null id of an id-less record, and the omitted
/// `recorded_at` of a record that never had one.
#[test]
fn ready_json_payload_pins_the_bytes() {
    let ours = seeded_store("ready-json", None, &[("d", FIXED_READY_SEED)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"ready\",\n  \"count\": 2,\n  \"entries\": [\
\n    {\n      \"domain\": \"d\",\n      \"id\": \"mx-2bb21d\",\n      \"type\": \"convention\",\
\n      \"recorded_at\": \"2026-10-03T20:47:58.000Z\",\n      \"summary\": \"conv body\",\
\n      \"record\": {\n        \"type\": \"convention\",\n        \"classification\": \"tactical\",\
\n        \"recorded_at\": \"2026-10-03T20:47:58.000Z\",\n        \"content\": \"conv body\",\
\n        \"id\": \"mx-2bb21d\"\n      }\n    },\
\n    {\n      \"domain\": \"d\",\n      \"id\": \"mx-1bb21d\",\n      \"type\": \"pattern\",\
\n      \"recorded_at\": \"2026-10-03T20:47:57.000Z\",\n      \"summary\": \"Alpha One\",\
\n      \"record\": {\n        \"type\": \"pattern\",\n        \"classification\": \"tactical\",\
\n        \"recorded_at\": \"2026-10-03T20:47:57.000Z\",\n        \"name\": \"Alpha One\",\
\n        \"description\": \"first alpha pattern\",\n        \"id\": \"mx-1bb21d\"\
\n      }\n    }\n  ]\n}\n"
    );
    assert_eq!(our.stderr, "");

    // An id-less record: `id: null` (nullish coalescing).
    let seed = "{\"type\":\"pattern\",\"classification\":\"tactical\",\
\"recorded_at\":\"2026-10-03T20:47:57.000Z\",\"name\":\"No Id\",\"description\":\"x\"}\n";
    let ours = seeded_store("ready-json-noid", None, &[("d", seed)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
    assert!(our.stdout.contains("\"id\": null"), "{}", our.stdout);

    // A record with no recorded_at: the entry OMITS the key and the
    // plain row renders `NaNd ago`.
    let seed = "{\"type\":\"pattern\",\"classification\":\"tactical\",\
\"name\":\"No Date\",\"description\":\"x\",\"id\":\"mx-3bb21d\"}\n";
    let ours = seeded_store("ready-json-nodate", None, &[("d", seed)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
    assert!(!our.stdout.contains("recorded_at"), "{}", our.stdout);
    assert!(
        our.stdout.contains("\"summary\": \"No Date\""),
        "{}",
        our.stdout
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert!(our.stdout.contains("NaNd ago"), "{}", our.stdout);
    assert_eq!(
        our.stdout.lines().next(),
        Some("Recent Expertise (last 1 records)")
    );
}

/// The strict `--limit` law (regex + JS Number): every rejection is
/// the one-line ready wording, plain on stderr and inside the json
/// envelope.
#[test]
fn ready_limit_parse_errors_pin_the_wording() {
    let ours = ready_store("ready-limerr");
    // Both spellings reach the command's own parse: `--limit=<raw>`
    // and the space form.
    for raw in ["abc", "0", "-1", "3.7", "10abc", "", "+5", " 5", ".5", "5."] {
        let arg = format!("--limit={raw}");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready", &arg]);
        assert_eq!(our.code, 1, "--limit={raw} exits 1");
        assert_eq!(
            our.stderr,
            format!("Error: --limit must be a positive integer (got \"{raw}\").\n")
        );
        if raw.starts_with('-') {
            let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready", "--limit", raw]);
            assert_eq!(our.code, 1, "--limit {raw} exits 1");
        }
    }
    // json: the envelope drops the `Error: ` prefix, still stderr.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "ready",
        "--limit=abc",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        error_envelope("ready", "--limit must be a positive integer (got \"abc\").")
    );
    // limit parses BEFORE the domain check and the since parse.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "ready",
        "--limit=abc",
        "--domain",
        "nope",
    ]);
    assert!(our.stderr.contains("--limit must be"));
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "ready",
        "--limit=abc",
        "--since=bogus",
    ]);
    assert!(our.stderr.contains("--limit must be"));
}

/// The `--since` duration law `^(\d+)(h|d|w)$`: rejections carry the
/// one-line wording; the domain check runs BEFORE the since parse.
#[test]
fn ready_since_errors_pin_the_wording() {
    let ours = ready_store("ready-sinceerr");
    for raw in [
        "24", "1x", "7D", " 7d", "-1d", "1.5d", "h", "7d ", "é", "abc",
    ] {
        let arg = format!("--since={raw}");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready", &arg]);
        assert_eq!(our.code, 1, "--since={raw} exits 1");
        assert_eq!(
            our.stderr,
            format!(
                "Error: Invalid duration: \"{raw}\". Use format like \"24h\", \"7d\", \"2w\".\n"
            )
        );
    }
    // json envelope.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "ready",
        "--since=bogus",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        error_envelope(
            "ready",
            "Invalid duration: \"bogus\". Use format like \"24h\", \"7d\", \"2w\"."
        )
    );
    // The domain check beats the since parse.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "ready",
        "--domain",
        "nope",
        "--since=bogus",
    ]);
    assert!(our.stderr.contains("not found"), "{}", our.stderr);
}

/// The unknown-domain surface: ready's own single-line plain wording
/// (no `in config`, no `domains`, no hint), the one-line json shape,
/// and the empty config's trailing `Available: ` with nothing after
/// it.
#[test]
fn ready_unknown_domain_pins_its_wording() {
    let ours = ready_store("ready-unkdom");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "ready", "--domain", "nosuch",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        "Error: domain \"nosuch\" not found. Available: zeta, mid, alpha\n"
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "ready", "--domain", "nosuch",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        error_envelope(
            "ready",
            "Domain \"nosuch\" not found in config. Available domains: zeta, mid, alpha"
        )
    );
    // `--domain=-x` (hyphen value through the `=` form).
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready", "--domain=-x"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Error: domain \"-x\" not found. Available: zeta, mid, alpha\n"
    );

    // A config with NO domains: the list is empty, the trailing space
    // stays.
    let config = format!("version: '1'\ndomains: {{}}\n{CONFIG_TAIL}");
    let ours = seeded_store("ready-nodomains", Some(&config), &[], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready", "--domain", "x"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, "Error: domain \"x\" not found. Available: \n");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "ready", "--domain", "x",
    ]);
    assert_eq!(
        our.stderr,
        error_envelope(
            "ready",
            "Domain \"x\" not found in config. Available domains: "
        )
    );
}

/// The summary-carrying surfaces embed the RAW summary value: a
/// missing non-truncating field renders `undefined` in plain and
/// OMITS the json key, a non-string passes through verbatim, and the
/// truncating laws (`convention`/`failure`) abort with the TypeError
/// — plain streams the earlier rows first, json prints the bare
/// envelope.
#[test]
fn ready_summary_raw_values_pin_the_law() {
    let at = at(3);
    let noname = format!(
        "{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{at}\",\"description\":\"d\",\"id\":\"mx-dddd01\"}}\n"
    );
    let numeric = format!(
        "{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{at}\",\"name\":5,\"id\":\"mx-dddd02\"}}\n"
    );
    let nocontent = format!(
        "{{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"{at}\",\"id\":\"mx-dddd03\"}}\n"
    );
    let nullcontent = format!(
        "{{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"{at}\",\"content\":null,\"id\":\"mx-dddd04\"}}\n"
    );
    let arraycontent = format!(
        "{{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"{at}\",\"content\":[1,2],\"id\":\"mx-dddd05\"}}\n"
    );
    let numcontent = format!(
        "{{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"{at}\",\"content\":5,\"id\":\"mx-dddd06\"}}\n"
    );

    // Pins: plain renders the word, json omits the key / keeps the
    // raw number, and the truncating law aborts with the TypeError.
    let ours = seeded_store("ready-sum-pin1", None, &[("zeta", noname.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert_eq!(our.code, 0);
    assert!(our.stdout.ends_with("undefined\n"), "{}", our.stdout);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
    assert!(!our.stdout.contains("\"summary\""), "{}", our.stdout);
    let ours = seeded_store("ready-sum-pin2", None, &[("zeta", numeric.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert!(our.stdout.ends_with("5\n"), "{}", our.stdout);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
    assert!(our.stdout.contains("\"summary\": 5"), "{}", our.stdout);
    let ours = seeded_store("ready-sum-pin3", None, &[("zeta", nocontent.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Error: undefined is not an object (evaluating 'text.length')\n"
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        error_envelope(
            "ready",
            "undefined is not an object (evaluating 'text.length')"
        )
    );

    // The truncating law's other three TypeError shapes: `null` fails
    // at `.length`, every other length-less value at `.search`, and an
    // array of at most 60 elements passes through — plain renders
    // `String(array)`, json carries the raw array.
    let ours = seeded_store(
        "ready-sum-pin4",
        None,
        &[("zeta", nullcontent.as_str())],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Error: null is not an object (evaluating 'text.length')\n"
    );
    let ours = seeded_store("ready-sum-pin5", None, &[("zeta", numcontent.as_str())], &[
    ]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Error: text.search is not a function. (In 'text.search(/[.!?]\\s/)', 'text.search' is undefined)\n"
    );
    let ours = seeded_store(
        "ready-sum-pin6",
        None,
        &[("zeta", arraycontent.as_str())],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert_eq!(our.code, 0);
    assert!(our.stdout.ends_with("1,2\n"), "{}", our.stdout);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
    assert!(our.stdout.contains("\"summary\": ["), "{}", our.stdout);
}

/// No `.mulch/` at all: the readConfig wording, and the config read
/// runs BEFORE every flag parse (probed: `--limit abc` in a no-store
/// dir reports the config message — the inverse of rank).
#[test]
fn ready_no_store_pins_the_wording() {
    let ours = TempDir::new("ready-nostore");
    let message = "No .mulch/ directory found. Run `mulch init` to set up this project.";
    for args in [
        &["ready"] as &[&str],
        &["ready", "--limit=abc"],
        &["ready", "--domain", "x"],
        &["ready", "--since=bogus"],
        &["--json", "ready"],
        &["--json", "ready", "--limit=abc"],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?}");
        assert_eq!(our.stdout, "");
        if args.first() == Some(&"--json") {
            assert_eq!(our.stderr, error_envelope("ready", message), "{args:?}");
        } else {
            assert_eq!(our.stderr, format!("Error: {message}\n"), "{args:?}");
        }
    }
}

/// An empty store: the fixed plain text (no header) and the json
/// `count: 0` envelope; `--since` changes neither.
#[test]
fn ready_empty_store_pins_its_texts() {
    let ours = seeded_store("ready-empty", None, &[("solo", "")], &[]);
    for args in [&["ready"] as &[&str], &["ready", "--domain", "solo"], &[
        "ready", "--since", "24h",
    ]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{args:?}");
        assert_eq!(our.stdout, "No recent expertise records found.\n");
        assert_eq!(our.stderr, "");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"ready\",\n  \"count\": 0,\n  \"entries\": []\n}\n"
    );
}

/// The strict-read surfaces: a malformed line aborts the whole command
/// (the engine's reason wording inside the template is the documented
/// repo-wide divergence, so only the template head/tail pin), an
/// unknown string type aborts the read, and `--allow-unknown-types`
/// admits it only for the summary read to throw (plain: partial stdout
/// then the error; json: bare error envelope).
#[test]
fn ready_strict_read_errors_pin_the_templates() {
    let malformed = format!(
        "{}{}",
        pattern_line("Ok", "mx-m00001", 5),
        "{\"type\":\"pattern\", BAD\n"
    );
    let ours = seeded_store("ready-malformed", None, &[("d", malformed.as_str())], &[]);
    for args in [&["ready"] as &[&str], &["--json", "ready"]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, "");
        let our_stderr = normalize_dir(&our.stderr, &ours.0);
        let head = if args.first() == Some(&"--json") {
            "{\n  \"success\": false,\n  \"command\": \"ready\",\n  \"error\": \"Malformed JSONL at <DIR>/.mulch/expertise/d.jsonl:2: "
                .to_string()
        } else {
            "Error: Malformed JSONL at <DIR>/.mulch/expertise/d.jsonl:2: ".to_string()
        };
        assert!(our_stderr.starts_with(&head), "{our_stderr}");
        if args.first() != Some(&"--json") {
            assert!(
                our_stderr.ends_with(". Line: {\"type\":\"pattern\", BAD\n"),
                "{our_stderr}"
            );
        }
    }

    // Unknown string type: the registry text verbatim (path folded).
    let unknown = format!(
        "{}{}",
        pattern_line("Ok", "mx-m00001", 5),
        "{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.000Z\",\
\"name\":\"Odd\",\"description\":\"x\",\"id\":\"mx-m00002\"}\n"
    );
    let ours = seeded_store("ready-unktype", None, &[("d", unknown.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert!(
        normalize_dir(&our.stderr, &ours.0).starts_with(
            "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/d.jsonl:2 (id=mx-m00002)."
        ),
        "{}",
        our.stderr
    );

    // --allow-unknown-types: the walk succeeds, the summary throws on
    // the OLDEST row, so the newer row already printed.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--allow-unknown-types",
        "ready",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stdout,
        "Recent Expertise (last 2 records)\n\n\
         \x20 mx-m00001  5h ago    d             [pattern]     Ok\n"
    );
    assert_eq!(our.stderr, "Error: Unknown record type: zzzcustom\n");

    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--allow-unknown-types",
        "--json",
        "ready",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        error_envelope("ready", "Unknown record type: zzzcustom")
    );
}

/// A record with no `type` and one with a NON-STRING `type`: the
/// strict read only rejects unregistered STRING types, so both reach
/// the summary throw — `Unknown record type: undefined` / `… : 5`.
#[test]
fn ready_missing_and_non_string_type_pin_the_wording() {
    for (tag, prefix, expected) in [
        ("ready-notype", "", "undefined"),
        ("ready-numtype", "\"type\":5,", "5"),
    ] {
        let older = at(20);
        let seed = format!(
            "{}{{{prefix}\"classification\":\"tactical\",\
\"recorded_at\":\"{older}\",\"name\":\"X\",\"description\":\"x\",\
\"id\":\"mx-n00002\"}}\n",
            pattern_line("New", "mx-n00001", 5)
        );
        let ours = seeded_store(tag, None, &[("d", seed.as_str())], &[]);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
        assert_eq!(our.code, 1, "{tag}");
        assert_eq!(
            our.stdout,
            "Recent Expertise (last 2 records)\n\n\
             \x20 mx-n00001  5h ago    d             [pattern]     New\n",
            "{tag}"
        );
        assert_eq!(
            our.stderr,
            format!("Error: Unknown record type: {expected}\n"),
            "{tag}"
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
        assert_eq!(
            our.stderr,
            error_envelope("ready", &format!("Unknown record type: {expected}")),
            "{tag}"
        );
    }
}

/// `getExpertisePath` validates each configured name in config order:
/// an invalid name aborts the walk before any output, while scoping
/// to a VALID domain sidesteps the bad one entirely.
#[test]
fn ready_invalid_configured_domain_name() {
    let config = format!("version: '1'\ndomains:\n  good: {{}}\n  bad.name: {{}}\n{CONFIG_TAIL}");
    let seed = pattern_line("G", "mx-g00001", 5);
    let ours = seeded_store("ready-invdom", Some(&config), &[("good", &seed)], &[]);
    for args in [&["ready"] as &[&str], &["--json", "ready"]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?}");
        assert_eq!(our.stdout, "");
        // plain carries the quotes verbatim; the json envelope escapes
        // them.
        let expected = if args.first() == Some(&"--json") {
            "Invalid domain name: \\\"bad.name\\\"."
        } else {
            "Invalid domain name: \"bad.name\"."
        };
        assert!(our.stderr.contains(expected), "{}", our.stderr);
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "ready", "--domain", "good",
    ]);
    assert_eq!(our.code, 0);
    assert!(our.stdout.contains("mx-g00001"), "{}", our.stdout);
}

/// A broken `custom_types` block fails every ready invocation through
/// the GLOBAL config-error surface — before the flag parses.
#[test]
fn ready_broken_custom_types() {
    let config =
        "version: '1'\ndomains:\n  d: {}\ncustom_types:\n  bad:\n    required: [content]\n";
    let seed = pattern_line("G", "mx-g00001", 5);
    let ours = seeded_store("ready-broken", Some(config), &[("d", &seed)], &[]);
    for args in [&["ready"] as &[&str], &["ready", "--limit=abc"], &[
        "--json", "ready",
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

/// A registered custom type summarizes through its registry template
/// (plain and json), and its `files` field stays in the embedded
/// record.
#[test]
fn ready_custom_type_summary() {
    let config = "version: '1'\ndomains:\n  ops: {}\ncustom_types:\n  deploy_note:\n    required: [service, action]\n    optional: [files]\n    dedup_key: service\n    summary: \"{service}: {action}\"\n";
    let seed = "{\
\"type\":\"deploy_note\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:49:00.000Z\",\
\"service\":\"api\",\"action\":\"restart\",\"files\":[\"k8s/api.yml\"],\"id\":\"mx-dep001\"\
}\n";
    let ours = seeded_store("ready-custom", Some(config), &[("ops", seed)], &[]);
    for args in [&["ready"] as &[&str], &["--json", "ready"]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{args:?}: {}", our.stderr);
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert!(our.stdout.contains("api: restart"), "{}", our.stdout);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "ready"]);
    assert!(
        our.stdout.contains("\"summary\": \"api: restart\""),
        "{}",
        our.stdout
    );
    assert!(our.stdout.contains("k8s/api.yml"), "{}", our.stdout);
}

/// NaN dates: rendered `NaNd ago`, never pass the `--since` cutoff,
/// and keep insertion order in this small fixture (the sort order for
/// NaN diverges only from five rows on — documented, not pinned).
#[test]
fn ready_nan_dates_pin_the_rendering() {
    let seed = format!(
        "{}{}{}",
        pattern_line("Fresh", "mx-a00001", 5),
        "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"not-a-date\",\
\"name\":\"Broken\",\"description\":\"d\",\"id\":\"mx-a00002\"}\n",
        pattern_line("Older", "mx-a00003", 20)
    );
    let ours = seeded_store("ready-nan", None, &[("d", seed.as_str())], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert_eq!(
        our.stdout,
        "Recent Expertise (last 3 records)\n\n\
         \x20 mx-a00001  5h ago    d             [pattern]     Fresh\n\
         \x20 mx-a00002  NaNd ago  d             [pattern]     Broken\n\
         \x20 mx-a00003  20h ago   d             [pattern]     Older\n"
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "ready", "--since", "24h",
    ]);
    assert_eq!(
        our.stdout,
        "Recent Expertise (last 24h)\n\n\
         \x20 mx-a00001  5h ago    d             [pattern]     Fresh\n\
         \x20 mx-a00003  20h ago   d             [pattern]     Older\n"
    );
}

/// Equal `recorded_at` values keep the domain-walk order (config
/// order, not alphabetical): the sort is stable.
#[test]
fn ready_identical_timestamps_keep_config_order() {
    // ONE shared stamp: three separate now() calls straddle a
    // millisecond boundary on ~1% of runs, breaking the ms-granular
    // tie this test exists to pin.
    let tie = at(30);
    let line = |name: &str, id: &str| {
        format!(
            "{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"{tie}\",\
\"name\":\"{name}\",\"description\":\"d\",\"id\":\"{id}\"}}\n"
        )
    };
    let (zeta, mid, alpha) = (
        line("Z", "mx-t10001"),
        line("M", "mx-t10002"),
        line("A", "mx-t10003"),
    );
    let ours = seeded_store(
        "ready-tie",
        None,
        &[
            ("zeta", zeta.as_str()),
            ("mid", mid.as_str()),
            ("alpha", alpha.as_str()),
        ],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["ready"]);
    assert_eq!(
        our.stdout,
        "Recent Expertise (last 3 records)\n\n\
         \x20 mx-t10001  1d ago    zeta          [pattern]     Z\n\
         \x20 mx-t10002  1d ago    mid           [pattern]     M\n\
         \x20 mx-t10003  1d ago    alpha         [pattern]     A\n"
    );
}
