//! Differential battery for `mulch query` (mulch-16da).
//!
//! Twin stores are hand-written (fixed timestamps and ids), so byte
//! comparisons need no normalization; domain-file mtimes are pinned
//! to a fixed age (`age_domain_files`) so the `updated …` suffix is
//! deterministic. The malformed-line reason wording carries serde's
//! phrasing (documented deviation) — those asserts pin the template
//! prefix/suffix like the delete battery does.

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
/// the `2h ago` bucket for the test's duration (support.rs is
/// frozen, so this helper is local; noted for host-folds).
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

/// Runs one invocation against both twins and asserts the full
/// channel-split picture.
fn assert_same(ours: &TempDir, theirs: &TempDir, ml: &Path, args: &[&str]) {
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, ml, args);
    assert_eq!(our.code, their.code, "rc differs for {args:?}");
    // ours promotes THIS binary's name (user directive 2026-10-13);
    // promote theirs for the byte comparison.
    assert_eq!(
        our.stdout,
        promote_theirs(&their.stdout),
        "stdout differs for {args:?}"
    );
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        normalize_dir(&promote_theirs(&their.stderr), &theirs.0),
        "stderr differs for {args:?}"
    );
}

#[test]
fn query_default_listing_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("q-def", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    assert_same(&ours, &theirs, &ml, &["query", "dev"]);
    assert_same(&ours, &theirs, &ml, &["query", "empty"]);
}

#[test]
fn query_format_families_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("q-fmt", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    // subcommand position and global position both route through the
    // four-choice global option (the ids declaration is dead code)
    for format in ["compact", "xml", "plain"] {
        assert_same(&ours, &theirs, &ml, &["query", "dev", "--format", format]);
        assert_same(&ours, &theirs, &ml, &["--format", format, "query", "dev"]);
    }
}

#[test]
fn query_all_walks_config_order_and_empty_domains() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("q-all", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    assert_same(&ours, &theirs, &ml, &["query", "--all"]);
    assert_same(&ours, &theirs, &ml, &[
        "query", "--all", "--format", "compact",
    ]);
    // --all wins over the positional (query.ts checks options.all
    // first); the ops record proves the walk left the single domain
    assert_same(&ours, &theirs, &ml, &["query", "--all", "ops"]);
    // a missing domain file drops the updated suffix entirely
    for dir in [&ours.0, &theirs.0] {
        std::fs::remove_file(dir.join(".mulch/expertise/empty.jsonl")).expect("file removable");
    }
    assert_same(&ours, &theirs, &ml, &[
        "query", "--all", "--format", "compact",
    ]);
}

#[test]
fn query_filters_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("q-flt", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    // --type is a free string: exact case matches, wrong case is the
    // empty listing, and an empty value is JS-falsy (no filter)
    assert_same(&ours, &theirs, &ml, &["query", "dev", "--type", "pattern"]);
    assert_same(&ours, &theirs, &ml, &["query", "dev", "--type", "Pattern"]);
    assert_same(&ours, &theirs, &ml, &["query", "dev", "--type", ""]);
    assert_same(&ours, &theirs, &ml, &[
        "query",
        "dev",
        "--classification",
        "tactical",
    ]);
    assert_same(&ours, &theirs, &ml, &[
        "query",
        "dev",
        "--classification",
        "foundational",
    ]);
    // file filter: case-folded substring over the files array
    assert_same(&ours, &theirs, &ml, &["query", "dev", "--file", "LIB.RS"]);
    assert_same(&ours, &theirs, &ml, &["query", "--all", "--file", "ops.sh"]);
    assert_same(&ours, &theirs, &ml, &["query", "dev", "--file", "nomatch"]);
    // outcome status: any-match over entries (query gates to
    // success|failure at the CLI)
    assert_same(&ours, &theirs, &ml, &[
        "query",
        "dev",
        "--outcome-status",
        "success",
    ]);
    assert_same(&ours, &theirs, &ml, &[
        "query",
        "dev",
        "--outcome-status",
        "failure",
    ]);
}

#[test]
fn query_sort_by_score_orders_records() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("q-sort", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    // compact makes the order visible: ★2 pattern, then the
    // score-less convention and guide keep store order
    assert_same(&ours, &theirs, &ml, &[
        "query",
        "dev",
        "--sort-by-score",
        "--format",
        "compact",
    ]);
    assert_same(&ours, &theirs, &ml, &[
        "--json",
        "query",
        "dev",
        "--sort-by-score",
    ]);
}

#[test]
fn query_json_envelope_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("q-json", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    // fixed stamps and ids: the envelope is byte-identical (record
    // key order follows the disk, the outcomes sit at the end)
    assert_same(&ours, &theirs, &ml, &["--json", "query", "dev"]);
    assert_same(&ours, &theirs, &ml, &["--json", "query", "--all"]);
    assert_same(&ours, &theirs, &ml, &[
        "--json", "query", "dev", "--type", "pattern",
    ]);
    assert_same(&ours, &theirs, &ml, &["--json", "query", "empty"]);
}

/// The legacy singular `outcome` key normalizes away on read: the
/// json record carries only the `outcomes` array (an undefined value
/// is omitted by JSON.stringify), exactly like the strict reader.
#[test]
fn query_json_normalizes_legacy_outcome() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let legacy = "{\"type\":\"pattern\",\"name\":\"Legacy\",\"description\":\"has legacy outcome\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T10:00:00.000Z\",\"id\":\"mx-ccc001\",\"outcome\":{\"status\":\"success\",\"duration\":5}}\n";
    let config = "version: '1'\ndomains:\n  dev: {}\n";
    let (ours, theirs) = twin_seeded("q-leg", Some(config), &[("dev", legacy)], &[]);
    assert_same(&ours, &theirs, &ml, &["--json", "query", "dev"]);
    assert_same(&ours, &theirs, &ml, &[
        "query", "dev", "--format", "compact",
    ]);
}

#[test]
fn query_domain_failures_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("q-dom", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    // our hint promotes THIS binary (pinned below); the comparison
    // reverses it
    assert_same(&ours, &theirs, &ml, &["query", "nope"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["query", "nope"]);
    assert!(our.stderr.contains("Hint: Run `mulch add nope`"));
    assert_same(&ours, &theirs, &ml, &["--json", "query", "nope"]);
    // no domain and no --all: the usage error, plain and json
    assert_same(&ours, &theirs, &ml, &["query"]);
    assert_same(&ours, &theirs, &ml, &["--json", "query"]);
    // an empty positional is JS-falsy: the same usage error
    assert_same(&ours, &theirs, &ml, &["query", ""]);
    // an empty store lists (none)
    let ours = TempDir::new("q-dom-init-o");
    let theirs = TempDir::new("q-dom-init-t");
    for dir in [&ours.0, &theirs.0] {
        write_seed_with_config(dir, "version: '1'\ndomains: {}\n", &[]);
    }
    assert_same(&ours, &theirs, &ml, &["query", "dev"]);
    assert_same(&ours, &theirs, &ml, &["--json", "query", "dev"]);
}

#[test]
fn query_all_empty_store_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("q-ae-o");
    let theirs = TempDir::new("q-ae-t");
    for dir in [&ours.0, &theirs.0] {
        write_seed_with_config(dir, "version: '1'\ndomains: {}\n", &[]);
    }
    // the no-domains line (status's bytes) on stdout, rc 0 — and the
    // empty json envelope
    assert_same(&ours, &theirs, &ml, &["query", "--all"]);
    assert_same(&ours, &theirs, &ml, &["--json", "query", "--all"]);
}

#[test]
fn query_no_store_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("q-ns-o");
    let theirs = TempDir::new("q-ns-t");
    // the readConfig wording ("to set up this project"), not the
    // ENOENT branch's "Run `ml init` first."
    assert_same(&ours, &theirs, &ml, &["query", "dev"]);
    assert_same(&ours, &theirs, &ml, &["--json", "query", "dev"]);
    assert_same(&ours, &theirs, &ml, &["query", "--all"]);
    // no domain: the store resolves first, so the no-store wording
    // wins over the please-specify error
    assert_same(&ours, &theirs, &ml, &["query"]);
}

/// Reader failures: the unknown-type and malformed templates (the
/// reason wording inside the malformed template is serde's — a
/// documented deviation — so the assert pins prefix and suffix).
#[test]
fn query_reader_failures_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let body = concat!(
        "{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T10:00:00.000Z\",\"name\":\"Odd\",\"id\":\"mx-zzz111\"}\n",
        "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T11:00:00.000Z\",\"name\":\"Ok\",\"description\":\"fine\",\"id\":\"mx-zzz222\"}\n",
    );
    let config = "version: '1'\ndomains:\n  dev: {}\n";
    let (ours, theirs) = twin_seeded("q-rd", Some(config), &[("dev", body)], &[]);

    // unknown type: exact bytes (path folded), plain and json
    assert_same(&ours, &theirs, &ml, &["query", "dev"]);
    assert_same(&ours, &theirs, &ml, &["--json", "query", "dev"]);
    // the global flag lifts only the unknown-type failure; the custom
    // stays invisible in compact (registry miss) but counted
    assert_same(&ours, &theirs, &ml, &[
        "query",
        "dev",
        "--allow-unknown-types",
    ]);
    assert_same(&ours, &theirs, &ml, &[
        "query",
        "dev",
        "--allow-unknown-types",
        "--format",
        "compact",
    ]);
    assert_same(&ours, &theirs, &ml, &[
        "--json",
        "query",
        "dev",
        "--allow-unknown-types",
    ]);

    // malformed: template prefix/suffix per the documented deviation
    let malformed = "{\"type\":\"pattern\" BAD";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch/expertise/dev.jsonl"),
            format!("{malformed}\n"),
        )
        .expect("malformed writable");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["query", "dev"]);
    let their = run_in(&theirs.0, &ml, &["query", "dev"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 1);
    let prefix = "Error: Malformed JSONL at <DIR>/.mulch/expertise/dev.jsonl:1: ";
    let suffix = format!(". Line: {malformed}\n");
    for text in [
        normalize_dir(&our.stderr, &ours.0).as_str(),
        normalize_dir(&their.stderr, &theirs.0).as_str(),
    ] {
        assert!(text.starts_with(prefix), "template prefix: {text}");
        assert!(text.ends_with(&suffix), "template suffix: {text}");
    }
    // json carries the same template on the error channel
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "query", "dev"]);
    let their = run_in(&theirs.0, &ml, &["--json", "query", "dev"]);
    assert_eq!(our.code, their.code);
    assert!(normalize_dir(&our.stderr, &ours.0).contains("Malformed JSONL at"));
    assert_eq!(our.stdout, "");
    assert_eq!(their.stdout, "");
}

/// `--format ids` rejects in both flag positions: the command's own
/// five-choice declaration is dead — commander matches the token
/// against the GLOBAL four-choice option (probed 2026-10-23), so the
/// ids render path is unreachable. Wording is clap's vs commander's
/// (documented family); the exit-1 contract is the pin.
#[test]
fn query_format_ids_rejects_everywhere() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("q-ids", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    for args in [vec!["query", "dev", "--format", "ids"], vec![
        "--format", "ids", "query", "dev",
    ]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "rc for {args:?}");
        assert_eq!(our.code, 1, "rc for {args:?}");
        assert!(
            our.stderr.starts_with("error:"),
            "our stderr: {}",
            our.stderr
        );
        assert!(
            their.stderr.starts_with("error:"),
            "their stderr: {}",
            their.stderr
        );
        assert_eq!(our.stdout, "");
        assert_eq!(their.stdout, "");
    }
}
