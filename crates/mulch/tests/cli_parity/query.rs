//! Battery for `mulch query` (mulch-16da), ADR-0008 edition: OUR
//! byte pins for the canonical renders (default listing, the format
//! families, the --all walk, the json envelope) plus the error
//! surfaces; sweeps that only mirrored the reference are deleted
//! (noted in the sprint ledger) — the render laws live on in the
//! frozen literals below.
//!
//! The corpus keeps fixed timestamps and ids, so byte comparisons
//! need no normalization; domain-file mtimes are pinned to a fixed
//! age (`age_domain_files`) so the `updated …` suffix is
//! deterministic.

use std::path::Path;

use super::support::*;

/// The corpus config: three domains in config order (dev, ops, empty).
const CORPUS_CONFIG: &str = "version: '1'\ndomains:\n  dev: {}\n  ops: {}\n  empty: {}\n";

/// dev: one record per family — ★2 pattern (files + tags), a failed
/// convention, an outcome-less guide. Fixed stamps keep ages and ids
/// deterministic.
const DEV_LINES: &str = concat!(
    "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T10:00:00.000Z\",\"name\":\"Alpha One\",\"description\":\"first alpha pattern\",\"files\":[\"src/lib.rs\"],\"tags\":[\"Rust\"],\"id\":\"mx-aaa001\",\"outcomes\":[{\"status\":\"success\",\"recorded_at\":\"2026-10-02T10:00:00.000Z\"},{\"status\":\"success\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\"}]}\n",
    "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-01T11:00:00.000Z\",\"content\":\"always use rustfmt\",\"id\":\"mx-aaa002\",\"outcomes\":[{\"status\":\"failure\",\"recorded_at\":\"2026-10-02T11:00:00.000Z\"}]}\n",
    "{\"type\":\"guide\",\"classification\":\"observational\",\"recorded_at\":\"2026-10-01T12:00:00.000Z\",\"name\":\"Guido\",\"description\":\"beta guide\",\"id\":\"mx-aaa003\"}\n",
);

/// ops: one pattern tagged for the tag/file filters.
const OPS_LINES: &str = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T13:00:00.000Z\",\"name\":\"Ops Pat\",\"description\":\"gamma ops pattern\",\"files\":[\"deploy/ops.sh\"],\"tags\":[\"ops\"],\"id\":\"mx-bbb001\"}\n";

/// Seeds the corpus store and pins the domain files' mtimes.
fn corpus(dir: &Path) {
    write_seed_with_config(dir, CORPUS_CONFIG, &[
        ("dev", DEV_LINES),
        ("ops", OPS_LINES),
        ("empty", ""),
    ]);
    age_domain_files(dir, &["dev", "ops", "empty"]);
}

/// Pins every domain file's mtime 2.5h into the past — safely inside
/// the `2h ago` bucket for the test's duration.
fn age_domain_files(dir: &Path, domains: &[&str]) {
    let past = std::time::SystemTime::now() - std::time::Duration::from_mins(150);
    for domain in domains {
        let file = std::fs::File::options()
            .append(true)
            .open(dir.join(".mulch/expertise").join(format!("{domain}.jsonl")))
            .expect("domain file exists");
        file.set_modified(past).expect("mtime settable");
    }
}

#[test]
fn query_default_listing_pins_the_markdown_law() {
    let ours = seeded_store("q-def", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["query", "dev"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stderr, "");
    assert_eq!(
        our.stdout,
        "## dev (3 records, updated 2h ago)\n\n### Conventions\n- [mx-aaa002] always use \
         rustfmt\n\n### Patterns\n- [mx-aaa001] **Alpha One**: first alpha pattern \
         (src/lib.rs)\n\n### Guides\n- [mx-aaa003] **Guido**: beta guide\n"
    );
    // an empty domain renders its header only
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["query", "empty"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "## empty (0 records, updated 2h ago)\n\n\n");
}

/// The format families, byte-frozen; the subcommand-position and
/// global-position routing is pinned relationally (both positions
/// must produce the same bytes).
#[test]
fn query_format_families_pin_their_renders() {
    let ours = seeded_store("q-fmt", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let compact = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "dev", "--format", "compact",
    ]);
    assert_eq!(compact.code, 0);
    assert_eq!(
        compact.stdout,
        concat!(
            "## dev (3 records, updated 2h ago)\n",
            "- [pattern] Alpha One: first alpha pattern (src/lib.rs) (mx-aaa001, tactical 28d ago, ★2) [✓ (2x)]\n",
            "- [convention] always use rustfmt (mx-aaa002, foundational) [✗]\n",
            "- [guide] Guido: beta guide (mx-aaa003, observational 28d ago)\n",
        )
    );
    let xml = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "dev", "--format", "xml",
    ]);
    assert_eq!(xml.code, 0);
    assert_eq!(
        xml.stdout,
        concat!(
            "<domain name=\"dev\" entries=\"3\" updated=\"2h ago\">\n",
            "  <pattern id=\"mx-aaa001\" classification=\"tactical\">\n",
            "    <name>Alpha One</name>\n",
            "    <description>first alpha pattern</description>\n",
            "    <files>src/lib.rs</files>\n",
            "    <tags>Rust</tags>\n",
            "    <outcome status=\"success\"></outcome>\n",
            "    <outcome status=\"success\"></outcome>\n",
            "  </pattern>\n",
            "  <convention id=\"mx-aaa002\" classification=\"foundational\">\n",
            "    always use rustfmt\n",
            "    <outcome status=\"failure\"></outcome>\n",
            "  </convention>\n",
            "  <guide id=\"mx-aaa003\" classification=\"observational\">\n",
            "    <name>Guido</name>\n",
            "    <description>beta guide</description>\n",
            "  </guide>\n",
            "</domain>\n",
        )
    );
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "dev", "--format", "plain",
    ]);
    assert_eq!(plain.code, 0);
    assert_eq!(
        plain.stdout,
        concat!(
            "[dev] 3 records (updated 2h ago)\n\n",
            "Conventions:\n",
            "  - [mx-aaa002] always use rustfmt\n\n",
            "Patterns:\n",
            "  - [mx-aaa001] Alpha One: first alpha pattern (src/lib.rs)\n\n",
            "Guides:\n",
            "  - [mx-aaa003] Guido: beta guide\n",
        )
    );
    // the global flag position routes through the same option
    for format in ["compact", "xml", "plain"] {
        let global = run_in(&ours.0, Path::new(mulch_bin()), &[
            "--format", format, "query", "dev",
        ]);
        assert_eq!(
            global.stdout,
            run_in(&ours.0, Path::new(mulch_bin()), &[
                "query", "dev", "--format", format
            ])
            .stdout,
            "{format}: both flag positions agree"
        );
    }
}

/// --all walks config order including empty domains; --all wins over
/// the positional; a missing domain file drops the updated suffix.
#[test]
fn query_all_walks_config_order_and_empty_domains() {
    let ours = seeded_store("q-all", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let all = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "--all", "--format", "compact",
    ]);
    assert_eq!(all.code, 0);
    assert!(
        all.stdout
            .starts_with("## dev (3 records, updated 2h ago)\n")
    );
    assert!(
        all.stdout
            .contains("\n\n## ops (1 records, updated 2h ago)\n")
    );
    assert!(
        all.stdout
            .ends_with("\n\n## empty (0 records, updated 2h ago)\n")
    );
    // --all wins over the positional (the ops record proves the walk
    // left the single domain)
    let positional = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "--all", "ops", "--format", "compact",
    ]);
    assert_eq!(positional.stdout, all.stdout, "--all beats the positional");
    // a missing domain file drops the updated suffix entirely
    std::fs::remove_file(ours.0.join(".mulch/expertise/empty.jsonl")).expect("file removable");
    let after = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "--all", "--format", "compact",
    ]);
    assert!(
        after.stdout.ends_with("\n\n## empty (0 records)\n"),
        "{}",
        after.stdout
    );
}

/// Filters narrow the corpus (membership pins; the wrong-case type
/// filter renders the empty listing; an empty --type value is no
/// filter at all).
#[test]
fn query_filters_narrow_the_corpus() {
    let ours = seeded_store("q-flt", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    // exact-case type filter keeps the patterns only
    let patterns = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "dev", "--type", "pattern",
    ]);
    assert_eq!(patterns.code, 0);
    assert!(patterns.stdout.contains("Alpha One"));
    assert!(!patterns.stdout.contains("Guido"));
    // wrong case is the empty listing (0 records)
    let wrong_case = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "dev", "--type", "Pattern",
    ]);
    assert!(
        wrong_case.stdout.contains("(0 records,"),
        "{}",
        wrong_case.stdout
    );
    // an empty value is JS-falsy (no filter)
    let no_filter = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "dev", "--type", "",
    ]);
    assert_eq!(
        no_filter.stdout,
        run_in(&ours.0, Path::new(mulch_bin()), &["query", "dev"]).stdout
    );
    // classification filter
    let foundational = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query",
        "dev",
        "--classification",
        "foundational",
    ]);
    assert!(foundational.stdout.contains("rustfmt"));
    assert!(!foundational.stdout.contains("Alpha One"));
    // file filter: case-folded substring over the files array
    let file_hit = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "dev", "--file", "LIB.RS",
    ]);
    assert!(file_hit.stdout.contains("Alpha One"));
    let file_all = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "--all", "--file", "ops.sh",
    ]);
    assert!(file_all.stdout.contains("Ops Pat"));
    let file_miss = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "dev", "--file", "nomatch",
    ]);
    assert!(
        file_miss.stdout.contains("(0 records,"),
        "{}",
        file_miss.stdout
    );
    // outcome status: any-match over entries
    for (status, needle) in [("success", "Alpha One"), ("failure", "rustfmt")] {
        let hit = run_in(&ours.0, Path::new(mulch_bin()), &[
            "query",
            "dev",
            "--outcome-status",
            status,
        ]);
        assert!(hit.stdout.contains(needle), "{status}: {}", hit.stdout);
    }
}

/// --sort-by-score: the ★2 pattern leads the compact order.
#[test]
fn query_sort_by_score_orders_records() {
    let ours = seeded_store("q-sort", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let sorted = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query",
        "dev",
        "--sort-by-score",
        "--format",
        "compact",
    ]);
    assert_eq!(sorted.code, 0);
    let first = sorted
        .stdout
        .lines()
        .find(|line| line.starts_with("- "))
        .expect("a bullet");
    assert!(first.contains("mx-aaa001"), "{first}");
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "query",
        "dev",
        "--sort-by-score",
    ]);
    assert_eq!(json.code, 0);
    assert!(json.stdout.contains("\"id\": \"mx-aaa001\""));
}

/// The json envelope: record key order follows the disk, the
/// outcomes sit at the end (byte-frozen).
#[test]
fn query_json_envelope_pins_the_payload() {
    let ours = seeded_store("q-json", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "query", "dev"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stderr, "");
    let first_record = our
        .stdout
        .split("\"records\": [")
        .nth(1)
        .expect("records array");
    assert!(
        first_record.starts_with(
            "\n        {\n          \"type\": \"pattern\",\n          \"classification\": \
             \"tactical\",\n          \"recorded_at\": \"2026-10-01T10:00:00.000Z\",\n          \
             \"name\": \"Alpha One\",\n          \"description\": \"first alpha pattern\",\n          \
             \"files\": [\n            \"src/lib.rs\"\n          ],\n          \"tags\": [\n            \
             \"Rust\"\n          ],\n          \"id\": \"mx-aaa001\",\n          \"outcomes\": [\n            \
             {\n              \"status\": \"success\",\n              \"recorded_at\": \
             \"2026-10-02T10:00:00.000Z\"\n            }"
        ),
        "key order follows the disk, outcomes last: {first_record}"
    );
    // --all appends the ops domain and the empty domain (records [])
    let all = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "query", "--all",
    ]);
    assert!(all.stdout.contains("\"domain\": \"ops\""));
    assert!(
        all.stdout
            .contains("\"domain\": \"empty\",\n      \"records\": []")
    );
    // the empty domain alone: an empty records array
    let empty = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "query", "empty",
    ]);
    assert!(empty.stdout.contains("\"records\": []"));
}

/// The legacy singular `outcome` key normalizes away on read: the
/// json record carries only the `outcomes` array.
#[test]
fn query_json_normalizes_legacy_outcome() {
    let legacy = "{\"type\":\"pattern\",\"name\":\"Legacy\",\"description\":\"has legacy outcome\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T10:00:00.000Z\",\"id\":\"mx-ccc001\",\"outcome\":{\"status\":\"success\",\"duration\":5}}\n";
    let config = "version: '1'\ndomains:\n  dev: {}\n";
    let ours = seeded_store("q-leg", Some(config), &[("dev", legacy)], &[]);
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "query", "dev"]);
    assert_eq!(json.code, 0);
    assert!(
        json.stdout
            .contains("\"id\": \"mx-ccc001\",\n          \"outcomes\": ["),
        "legacy outcome key normalized away: {}",
        json.stdout
    );
    assert!(!json.stdout.contains("\"outcome\":"), "{}", json.stdout);
    let compact = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query", "dev", "--format", "compact",
    ]);
    assert!(compact.stdout.contains("mx-ccc001"));
}

/// Domain failures: the unknown-domain hint (two lines, plain; one
/// sentence, json), the usage error, and the empty-store listing.
#[test]
fn query_domain_failures_pin_their_texts() {
    let ours = seeded_store("q-dom", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["query", "nope"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        "Error: Domain \"nope\" not found in config. Available domains: dev, ops, empty\nHint: \
         Run `mulch add nope` to create this domain, or check .mulch/mulch.config.yaml\n"
    );
    assert!(our.stderr.contains("Hint: Run `mulch add nope`"));
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "query", "nope",
    ]);
    assert_eq!(json.code, 1);
    assert_eq!(
        json.stderr,
        error_envelope(
            "query",
            "Domain \"nope\" not found in config. Available domains: dev, ops, empty"
        )
    );
    // no domain and no --all: the usage error, plain and json; an
    // empty positional is JS-falsy: the same usage error
    for args in [&["query"][..], &["--json", "query"][..], &["query", ""][..]] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 1, "{args:?}");
        if args[0] == "--json" {
            assert_eq!(
                run.stderr,
                error_envelope(
                    "query",
                    "Please specify a domain or use --all to query all domains."
                )
            );
        } else {
            assert_eq!(
                run.stderr,
                "Error: Please specify a domain or use --all to query all domains.\n"
            );
        }
    }
    // an empty store lists (none)
    let empty_store = seeded_store("q-dom-init", Some("version: '1'\ndomains: {}\n"), &[], &[]);
    let plain = run_in(&empty_store.0, Path::new(mulch_bin()), &["query", "dev"]);
    assert_eq!(plain.code, 1);
    assert!(plain.stderr.contains("not found in config"));
    let json = run_in(&empty_store.0, Path::new(mulch_bin()), &[
        "--json", "query", "dev",
    ]);
    assert_eq!(json.code, 1);
    assert!(json.stderr.contains("not found in config"));
}

#[test]
fn query_all_empty_store_pins_the_notice() {
    let ours = seeded_store("q-ae", Some("version: '1'\ndomains: {}\n"), &[], &[]);
    // the no-domains line (status's bytes) on stdout, rc 0 — and the
    // empty json envelope
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["query", "--all"]);
    assert_eq!(plain.code, 0);
    assert_eq!(
        plain.stdout,
        "No domains configured. Run `mulch add <domain>` to get started.\n"
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "query", "--all",
    ]);
    assert_eq!(json.code, 0);
    assert_eq!(
        json.stdout,
        "{\n  \"success\": true,\n  \"command\": \"query\",\n  \"domains\": []\n}\n"
    );
}

#[test]
fn query_no_store_pins_the_read_config_wording() {
    let ours = TempDir::new("q-ns");
    // the readConfig wording ("to set up this project"), not the
    // ENOENT branch's "Run `mulch init` first."
    for args in [&["query", "dev"][..], &["--json", "query", "dev"][..]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?}");
        if args[0] == "--json" {
            assert_eq!(
                our.stderr,
                error_envelope(
                    "query",
                    "No .mulch/ directory found. Run `mulch init` to set up this project."
                )
            );
        } else {
            assert_eq!(
                our.stderr,
                "Error: No .mulch/ directory found. Run `mulch init` to set up this project.\n"
            );
        }
    }
    // no domain: the store resolves first, so the no-store wording
    // wins over the please-specify error
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["query"]);
    assert!(
        our.stderr
            .contains("No .mulch/ directory found. Run `mulch init` to set up this project.")
    );
}

/// Reader failures: the unknown-type and malformed templates (the
/// reason wording inside the malformed template is the engine's — a
/// documented deviation — so the assert pins prefix and suffix).
#[test]
fn query_reader_failures_pin_the_templates() {
    let body = concat!(
        "{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T10:00:00.000Z\",\"name\":\"Odd\",\"id\":\"mx-zzz111\"}\n",
        "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T11:00:00.000Z\",\"name\":\"Ok\",\"description\":\"fine\",\"id\":\"mx-zzz222\"}\n",
    );
    let config = "version: '1'\ndomains:\n  dev: {}\n";
    let ours = seeded_store("q-rd", Some(config), &[("dev", body)], &[]);

    // unknown type: exact bytes (path folded), plain and json
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["query", "dev"]);
    assert_eq!(plain.code, 1);
    assert_eq!(
        normalize_dir(&plain.stderr, &ours.0),
        "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/dev.jsonl:1 \
         (id=mx-zzz111). Register it under custom_types in mulch.config.yaml, remove the \
         record, or pass --allow-unknown-types to bypass.\n"
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "query", "dev"]);
    assert_eq!(json.code, 1);
    assert!(normalize_dir(&json.stderr, &ours.0).contains("Unknown record type \\\"zzzcustom\\\""));
    // the global flag lifts only the unknown-type failure; the custom
    // stays invisible in compact (registry miss) but counted
    let lifted = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query",
        "dev",
        "--allow-unknown-types",
    ]);
    assert_eq!(lifted.code, 0, "{}", lifted.stderr);
    assert!(!lifted.stdout.contains("mx-zzz111"));
    let lifted_compact = run_in(&ours.0, Path::new(mulch_bin()), &[
        "query",
        "dev",
        "--allow-unknown-types",
        "--format",
        "compact",
    ]);
    assert_eq!(lifted_compact.code, 0);
    assert!(!lifted_compact.stdout.contains("mx-zzz111"));

    // malformed: template prefix/suffix per the documented deviation
    let malformed = "{\"type\":\"pattern\" BAD";
    std::fs::write(
        ours.0.join(".mulch/expertise/dev.jsonl"),
        format!("{malformed}\n"),
    )
    .expect("malformed writable");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["query", "dev"]);
    assert_eq!(our.code, 1);
    let text = normalize_dir(&our.stderr, &ours.0);
    assert!(
        text.starts_with("Error: Malformed JSONL at <DIR>/.mulch/expertise/dev.jsonl:1: "),
        "template prefix: {text}"
    );
    assert!(
        text.ends_with(&format!(". Line: {malformed}\n")),
        "template suffix: {text}"
    );
    // json carries the same template on the error channel
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "query", "dev"]);
    assert_eq!(our.code, 1);
    assert!(normalize_dir(&our.stderr, &ours.0).contains("Malformed JSONL at"));
    assert_eq!(our.stdout, "");
}

/// `--format ids` rejects in both flag positions: the command's own
/// five-choice declaration is dead — the parser matches the token
/// against the GLOBAL four-choice option, so the ids render path is
/// unreachable. The exit-1 contract is the pin.
#[test]
fn query_format_ids_rejects_everywhere() {
    let ours = seeded_store("q-ids", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    for args in [vec!["query", "dev", "--format", "ids"], vec![
        "--format", "ids", "query", "dev",
    ]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1, "rc for {args:?}");
        assert!(
            our.stderr.starts_with("error:"),
            "our stderr: {}",
            our.stderr
        );
        assert_eq!(our.stdout, "");
    }
}
