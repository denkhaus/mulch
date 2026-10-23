//! Differential battery for `mulch search` (mulch-16da).
//!
//! Same fixture discipline as the query battery: hand-written twin
//! stores with fixed stamps/ids and pinned mtimes (the helpers are
//! duplicated from the query module — support.rs is frozen; noted
//! for host-folds). The flip store is the ranking discriminator: its
//! two conventions' pure-BM25 order is OPPOSITE to the boosted
//! order, so `--no-boost` (and a configured factor of 0) visibly
//! re-order against the default run.

use std::path::Path;

use super::support::*;

const CORPUS_CONFIG: &str = "version: '1'\ndomains:\n  dev: {}\n  ops: {}\n";

const DEV_LINES: &str = concat!(
    "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T10:00:00.000Z\",\"name\":\"rust core\",\"description\":\"rust rust rust loader\",\"id\":\"mx-aaa001\"}\n",
    "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T11:00:00.000Z\",\"name\":\"rust edge\",\"description\":\"rust loader once\",\"id\":\"mx-aaa002\",\"outcomes\":[{\"status\":\"success\",\"recorded_at\":\"2026-10-02T11:00:00.000Z\"}]}\n",
    "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T12:00:00.000Z\",\"name\":\"plain\",\"description\":\"nothing here\",\"id\":\"mx-aaa003\"}\n",
    "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T13:00:00.000Z\",\"name\":\"Tagged\",\"description\":\"tagged record\",\"tags\":[\"Rust\",\"ops\"],\"id\":\"mx-aaa004\"}\n",
    "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T14:00:00.000Z\",\"name\":\"Filed\",\"description\":\"filed record\",\"files\":[\"src/rust/mod.rs\"],\"id\":\"mx-aaa005\"}\n",
    "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-01T15:00:00.000Z\",\"content\":\"partial convention body\",\"id\":\"mx-aaa006\",\"outcomes\":[{\"status\":\"partial\",\"recorded_at\":\"2026-10-02T15:00:00.000Z\"}]}\n",
);

const OPS_LINES: &str = "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T16:00:00.000Z\",\"content\":\"rust ops convention\",\"id\":\"mx-bbb001\"}\n";

/// Seeds the corpus store and pins the domain files' mtimes.
fn corpus(dir: &Path) {
    write_seed_with_config(dir, CORPUS_CONFIG, &[
        ("dev", DEV_LINES),
        ("ops", OPS_LINES),
    ]);
    age_domain_files(dir, &["dev", "ops"]);
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

/// Runs one invocation against both twins and asserts the full
/// channel-split picture.
fn assert_same(ours: &TempDir, theirs: &TempDir, ml: &Path, args: &[&str]) {
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, ml, args);
    assert_eq!(our.code, their.code, "rc differs for {args:?}");
    assert_eq!(our.stdout, their.stdout, "stdout differs for {args:?}");
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        normalize_dir(&their.stderr, &theirs.0),
        "stderr differs for {args:?}"
    );
}

/// The first bullet line of a rendered section (the ranking pin for
/// the flip store).
fn first_bullet(stdout: &str) -> &str {
    stdout
        .lines()
        .find(|line| line.starts_with("- "))
        .unwrap_or("")
}

/// The flip store: the short tf=1 record carries ★5 (outcomes), the
/// longer tf=2 record none — pure BM25 ranks the tf=2 record first,
/// the boost flips it. Fixed ids keep the orders assertable.
fn flip_store(dir: &Path) {
    let body = concat!(
        "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T10:00:00.000Z\",\"content\":\"rust x\",\"id\":\"mx-flp001\",\"outcomes\":[{\"status\":\"success\"},{\"status\":\"success\"},{\"status\":\"success\"},{\"status\":\"success\"},{\"status\":\"success\"}]}\n",
        "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-01T11:00:00.000Z\",\"content\":\"rust rust y z\",\"id\":\"mx-flp002\"}\n",
    );
    write_seed_with_config(dir, "version: '1'\ndomains:\n  dev: {}\n", &[("dev", body)]);
    age_domain_files(dir, &["dev"]);
}

/// The archive store: live dev corpus plus an archive with two
/// records (reasoned and reason-less) and one unknown-type record
/// that only matches through its tags.
fn archive_store(dir: &Path) {
    corpus(dir);
    std::fs::create_dir_all(dir.join(".mulch/archive")).expect("archive dir");
    let archive = concat!(
        "# ARCHIVED — not for active use. Run `ml restore <id>` to revive.\n",
        "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-09-01T10:00:00.000Z\",\"status\":\"archived\",\"archived_at\":\"2026-09-02T08:30:00.000Z\",\"archive_reason\":\"stale\",\"name\":\"rust legacy\",\"description\":\"archived rust pattern\",\"id\":\"mx-arc001\"}\n",
        "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-08-01T10:00:00.000Z\",\"archived_at\":\"2026-08-02T08:30:00.000Z\",\"content\":\"archived convention\",\"id\":\"mx-arc002\"}\n",
        "{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-07-01T10:00:00.000Z\",\"archived_at\":\"2026-07-02T08:30:00.000Z\",\"archive_reason\":\"odd\",\"content\":\"body\",\"tags\":[\"rust\"],\"id\":\"mx-arc003\"}\n",
    );
    std::fs::write(dir.join(".mulch/archive/dev.jsonl"), archive).expect("archive file");
}

#[test]
fn search_ranking_and_counts_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-rank", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    // every format family renders the same match set; the count line
    // is plural here
    for format in ["markdown", "compact", "xml", "plain"] {
        assert_same(&ours, &theirs, &ml, &["search", "rust", "--format", format]);
    }
    // one match: the singular count line
    assert_same(&ours, &theirs, &ml, &["search", "Tagged"]);
    // no match: the label line on stdout, rc 0
    assert_same(&ours, &theirs, &ml, &["search", "zzzz"]);
    // no-match filter-only: the "matching filters" label
    assert_same(&ours, &theirs, &ml, &["search", "--type", "guide"]);
}

/// The boost discriminator: default (boosted) and --no-boost runs
/// each match the reference AND disagree on order.
#[test]
fn search_boost_reorders_against_pure_bm25() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-flip", None, &[], &[]);
    flip_store(&ours.0);
    flip_store(&theirs.0);
    let args = ["search", "rust", "--format", "compact"];
    assert_same(&ours, &theirs, &ml, &args);
    let boosted = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert!(
        first_bullet(&boosted.stdout).contains("mx-flp001"),
        "boosted order: {}",
        boosted.stdout
    );

    let no_boost_args = ["search", "rust", "--no-boost", "--format", "compact"];
    assert_same(&ours, &theirs, &ml, &no_boost_args);
    let pure = run_in(&ours.0, Path::new(mulch_bin()), &no_boost_args);
    assert!(
        first_bullet(&pure.stdout).contains("mx-flp002"),
        "pure BM25 order: {}",
        pure.stdout
    );
    // the two orders genuinely differ (the flip, not a tie)
    assert_ne!(boosted.stdout, pure.stdout);

    // a configured factor of 0 is the same pure order (config key
    // appended last, like the reference's serializer)
    let (ours, theirs) = twin_seeded("s-flip0", None, &[], &[]);
    for dir in [&ours.0, &theirs.0] {
        flip_store(dir);
        let mut config =
            std::fs::read_to_string(dir.join(".mulch/mulch.config.yaml")).expect("config");
        config.push_str("search:\n  boost_factor: 0.0\n");
        std::fs::write(dir.join(".mulch/mulch.config.yaml"), config).expect("config rewrite");
    }
    assert_same(&ours, &theirs, &ml, &args);
    let zeroed = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_ne!(boosted.stdout, zeroed.stdout);
    assert_eq!(zeroed.stdout, pure.stdout);
}

#[test]
fn search_filters_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-flt", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    // filter-only searches (no query): every record of the filtered
    // set, "matching filters" label, per-domain sections
    assert_same(&ours, &theirs, &ml, &[
        "search", "--type", "pattern", "--format", "compact",
    ]);
    assert_same(&ours, &theirs, &ml, &["search", "--type", "convention"]);
    // tag: case-insensitive exact element match
    assert_same(&ours, &theirs, &ml, &[
        "search", "--tag", "RUST", "--format", "compact",
    ]);
    assert_same(&ours, &theirs, &ml, &["search", "--tag", "us"]);
    // file: case-folded substring
    assert_same(&ours, &theirs, &ml, &[
        "search", "--file", "rust/mod", "--format", "compact",
    ]);
    // classification
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "--classification",
        "foundational",
        "--format",
        "compact",
    ]);
    // outcome status: success and the search-only partial
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "--outcome-status",
        "success",
        "--format",
        "compact",
    ]);
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "--outcome-status",
        "partial",
        "--format",
        "compact",
    ]);
    // filters AND with the query
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "rust",
        "--type",
        "convention",
        "--format",
        "compact",
    ]);
    // empty-string values are JS-falsy: no filter at all (guard fires)
    assert_same(&ours, &theirs, &ml, &["search", "--tag", ""]);
    assert_same(&ours, &theirs, &ml, &["--json", "search", "--tag", ""]);
    // an empty query with a filter runs the filter path
    assert_same(&ours, &theirs, &ml, &["search", "", "--type", "guide"]);
}

#[test]
fn search_domain_scoping_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-dom", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    assert_same(&ours, &theirs, &ml, &[
        "search", "rust", "--domain", "ops", "--format", "compact",
    ]);
    // unknown domain: the search hint already says `mulch add`, and
    // the available list has NO (none) fallback
    assert_same(&ours, &theirs, &ml, &["search", "rust", "--domain", "nope"]);
    assert_same(&ours, &theirs, &ml, &[
        "--json", "search", "rust", "--domain", "nope",
    ]);
    // an empty --domain is falsy: the all-domains walk (still needs a
    // query or filter to pass the guard)
    assert_same(&ours, &theirs, &ml, &[
        "search", "rust", "--domain", "", "--format", "compact",
    ]);
}

#[test]
fn search_json_envelopes_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-json", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    // query/total/domains shape; only matching domains appear
    assert_same(&ours, &theirs, &ml, &["--json", "search", "rust"]);
    // filter-only: query null
    assert_same(&ours, &theirs, &ml, &[
        "--json", "search", "--type", "pattern",
    ]);
    // no match: total 0, empty domains
    assert_same(&ours, &theirs, &ml, &["--json", "search", "zzzz"]);
    // --sort-by-score reorders the matches array
    assert_same(&ours, &theirs, &ml, &[
        "--json",
        "search",
        "rust",
        "--sort-by-score",
    ]);
}

#[test]
fn search_sort_by_score_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-sort", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    // the ★1 record outranks the score-less ones; ties keep order
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "rust",
        "--sort-by-score",
        "--format",
        "compact",
    ]);
}

#[test]
fn search_guard_and_no_store_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("s-guard-o");
    let theirs = TempDir::new("s-guard-t");
    // the guard fires BEFORE the store opens: no-store runs report
    // it, not the no-store error
    assert_same(&ours, &theirs, &ml, &["search"]);
    assert_same(&ours, &theirs, &ml, &["--json", "search"]);
    // an empty query is falsy: the guard again
    assert_same(&ours, &theirs, &ml, &["search", ""]);
    // with a query the no-store wording is readConfig's
    assert_same(&ours, &theirs, &ml, &["search", "rust"]);
    assert_same(&ours, &theirs, &ml, &["--json", "search", "rust"]);
    // filter-only no-store: same readConfig error
    assert_same(&ours, &theirs, &ml, &["search", "--type", "pattern"]);
}

#[test]
fn search_archived_sections_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-arch", Some(CORPUS_CONFIG), &[], &[]);
    for dir in [&ours.0, &theirs.0] {
        archive_store(dir);
    }
    // live + archived sections per domain, archived totals included
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "rust",
        "--archived",
        "--format",
        "compact",
    ]);
    assert_same(&ours, &theirs, &ml, &["search", "archived", "--archived"]);
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "convention",
        "--archived",
        "--format",
        "compact",
    ]);
    // filter-only over the archive corpus
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "--archived",
        "--type",
        "convention",
        "--format",
        "compact",
    ]);
    // json: matches/archived keys, [] where a domain had none
    assert_same(&ours, &theirs, &ml, &[
        "--json",
        "search",
        "rust",
        "--archived",
    ]);
    assert_same(&ours, &theirs, &ml, &[
        "--json",
        "search",
        "archived",
        "--archived",
    ]);
    // without --archived the archive stays invisible
    assert_same(&ours, &theirs, &ml, &["--json", "search", "rust"]);
    assert_same(&ours, &theirs, &ml, &[
        "search", "rust", "--format", "compact",
    ]);
}

/// The archived section's summary is registry-gated: an unknown-type
/// archive record that matches through its tags THROWS in the
/// reference's plain render (getRecordSummary), while json mode
/// never renders a summary and succeeds.
#[test]
fn search_archived_unknown_type_throws_in_plain_only() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-archz", None, &[], &[]);
    for dir in [&ours.0, &theirs.0] {
        archive_store(dir);
    }
    // plain: both fail with the same clean message
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "rust",
        "--archived",
        "--format",
        "plain",
    ]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "rust",
        "--archived",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, "Error: Unknown record type: zzzcustom\n");
    // json: the unknown-type record rides along untouched
    assert_same(&ours, &theirs, &ml, &[
        "--json",
        "search",
        "rust",
        "--archived",
    ]);
}

/// --archived with no archive directory at all: an empty corpus, no
/// error (readArchiveFile swallows the missing file).
#[test]
fn search_archived_missing_archive_is_quiet() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-archm", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    assert_same(&ours, &theirs, &ml, &[
        "search",
        "rust",
        "--archived",
        "--format",
        "compact",
    ]);
    assert_same(&ours, &theirs, &ml, &[
        "--json",
        "search",
        "rust",
        "--archived",
    ]);
}

/// `--type`'s choice set is the store's registry (search.ts:72): an
/// unregistered name rejects at parse time in both binaries (rc 1;
/// wording is the clap/commander family, not byte-compared).
#[test]
fn search_type_choices_are_registry_gated() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("s-type", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    corpus(&theirs.0);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "--type",
        "zzzcustom",
        "rust",
    ]);
    let their = run_in(&theirs.0, &ml, &["search", "--type", "zzzcustom", "rust"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stdout, "");
}
