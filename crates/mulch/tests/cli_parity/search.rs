//! Battery for `mulch search` (mulch-16da), ADR-0008 edition: OUR
//! byte pins for the canonical renders (markdown, compact, the json
//! envelope, the count lines, the guard) plus the boost discriminator
//! (relational) and the archived-section law; sweeps that only
//! mirrored the reference are deleted (noted in the sprint ledger).
//!
//! Same fixture discipline as the query battery: hand-written stores
//! with fixed stamps/ids and pinned mtimes. The flip store is the
//! ranking discriminator: its two conventions' pure-BM25 order is
//! OPPOSITE to the boosted order, so `--no-boost` (and a configured
//! factor of 0) visibly re-order against the default run.

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

/// The canonical renders: markdown and compact carry the same match
/// set, the plural count line; one match carries the singular.
#[test]
fn search_ranking_and_counts_pin_the_renders() {
    let ours = seeded_store("s-rank", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let markdown = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "rust", "--format", "markdown",
    ]);
    assert_eq!(markdown.code, 0);
    assert_eq!(markdown.stderr, "");
    assert_eq!(
        markdown.stdout,
        concat!(
            "## dev (4 records, updated 2h ago)\n\n",
            "### Patterns\n",
            "- [mx-aaa001] **rust core**: rust rust rust loader\n",
            "- [mx-aaa002] **rust edge**: rust loader once\n",
            "- [mx-aaa004] **Tagged**: tagged record\n",
            "- [mx-aaa005] **Filed**: filed record (src/rust/mod.rs)\n\n",
            "## ops (1 records, updated 2h ago)\n\n",
            "### Conventions\n",
            "- [mx-bbb001] rust ops convention\n\n",
            "5 matches found.\n",
        )
    );
    // every format family renders the same match set (membership)
    for format in ["compact", "xml", "plain"] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), &[
            "search", "rust", "--format", format,
        ]);
        assert_eq!(run.code, 0, "{format}");
        assert!(run.stdout.contains("mx-aaa001"), "{format}");
        assert!(run.stdout.contains("mx-bbb001"), "{format}");
        assert!(!run.stdout.contains("mx-aaa003"), "{format}");
    }
    // one match: the singular count line
    let one = run_in(&ours.0, Path::new(mulch_bin()), &["search", "Tagged"]);
    assert_eq!(one.code, 0);
    assert!(one.stdout.ends_with("\n1 match found.\n"), "{}", one.stdout);
    // no match: the label line on stdout, rc 0
    let none = run_in(&ours.0, Path::new(mulch_bin()), &["search", "zzzz"]);
    assert_eq!(none.code, 0);
    assert_eq!(none.stdout, "No records matching \"zzzz\" found.\n");
    // no-match filter-only: the "matching filters" label
    let filters = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "--type", "guide",
    ]);
    assert_eq!(filters.code, 0);
    assert_eq!(filters.stdout, "No records matching filters found.\n");
}

/// The boost discriminator (all our-side relational pins): default
/// (boosted) and --no-boost disagree on order, and a configured
/// factor of 0 equals the pure order.
#[test]
fn search_boost_reorders_against_pure_bm25() {
    let ours = seeded_store("s-flip", None, &[], &[]);
    flip_store(&ours.0);
    let args = ["search", "rust", "--format", "compact"];
    let boosted = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(boosted.code, 0);
    assert!(
        first_bullet(&boosted.stdout).contains("mx-flp001"),
        "boosted order: {}",
        boosted.stdout
    );

    let no_boost_args = ["search", "rust", "--no-boost", "--format", "compact"];
    let pure = run_in(&ours.0, Path::new(mulch_bin()), &no_boost_args);
    assert_eq!(pure.code, 0);
    assert!(
        first_bullet(&pure.stdout).contains("mx-flp002"),
        "pure BM25 order: {}",
        pure.stdout
    );
    // the two orders genuinely differ (the flip, not a tie)
    assert_ne!(boosted.stdout, pure.stdout);

    // a configured factor of 0 is the same pure order (config key
    // appended last, the canonical serializer order)
    let zeroed = seeded_store("s-flip0", None, &[], &[]);
    flip_store(&zeroed.0);
    let mut config =
        std::fs::read_to_string(zeroed.0.join(".mulch/mulch.config.yaml")).expect("config");
    config.push_str("search:\n  boost_factor: 0.0\n");
    std::fs::write(zeroed.0.join(".mulch/mulch.config.yaml"), config).expect("config rewrite");
    let zeroed_run = run_in(&zeroed.0, Path::new(mulch_bin()), &args);
    assert_ne!(boosted.stdout, zeroed_run.stdout);
    assert_eq!(zeroed_run.stdout, pure.stdout);
}

/// Filters narrow the corpus (membership pins): the tag filter is a
/// case-insensitive exact element match, the file filter a
/// case-folded substring, outcome statuses any-match, filters AND
/// with the query, and empty-string values are no filter at all.
#[test]
fn search_filters_narrow_the_corpus() {
    let ours = seeded_store("s-flt", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    // tag: case-insensitive exact element match
    let tag = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "--tag", "RUST", "--format", "compact",
    ]);
    assert_eq!(tag.code, 0);
    assert!(tag.stdout.contains("mx-aaa004"), "{}", tag.stdout);
    let tag_miss = run_in(&ours.0, Path::new(mulch_bin()), &["search", "--tag", "us"]);
    assert_eq!(tag_miss.code, 0);
    assert_eq!(tag_miss.stdout, "No records matching filters found.\n");
    // file: case-folded substring
    let file = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "--file", "rust/mod", "--format", "compact",
    ]);
    assert!(file.stdout.contains("mx-aaa005"), "{}", file.stdout);
    // classification
    let foundational = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "--classification",
        "foundational",
        "--format",
        "compact",
    ]);
    assert!(foundational.stdout.contains("mx-aaa006"));
    assert!(!foundational.stdout.contains("mx-aaa001"));
    // outcome status: success and the search-only partial
    for (status, needle, absent) in [
        ("success", "mx-aaa002", "mx-aaa003"),
        ("partial", "mx-aaa006", "mx-aaa001"),
    ] {
        let hit = run_in(&ours.0, Path::new(mulch_bin()), &[
            "search",
            "--outcome-status",
            status,
            "--format",
            "compact",
        ]);
        assert!(hit.stdout.contains(needle), "{status}: {}", hit.stdout);
        assert!(!hit.stdout.contains(absent), "{status}: {}", hit.stdout);
    }
    // filters AND with the query
    let both = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "rust",
        "--type",
        "convention",
        "--format",
        "compact",
    ]);
    assert!(both.stdout.contains("mx-bbb001"));
    assert!(!both.stdout.contains("mx-aaa001"));
    // empty-string values are JS-falsy: no filter at all (guard fires)
    for args in [vec!["search", "--tag", ""], vec![
        "--json", "search", "--tag", "",
    ]] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(run.code, 1, "{args:?}: the guard fires");
        assert!(run.stderr.contains("Provide a search query"), "{args:?}");
    }
    // an empty query with a filter runs the filter path
    let empty_query = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "", "--type", "guide",
    ]);
    assert_eq!(empty_query.code, 0);
    assert_eq!(
        empty_query.stdout, "No records matching filters found.\n",
        "no guide records live in dev/ops"
    );
}

/// Domain scoping: --domain ops hides dev hits; an unknown domain
/// errors with the (none)-less list; an empty --domain is the
/// all-domains walk.
#[test]
fn search_domain_scoping() {
    let ours = seeded_store("s-dom", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let scoped = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "rust", "--domain", "ops", "--format", "compact",
    ]);
    assert_eq!(scoped.code, 0);
    assert!(scoped.stdout.contains("mx-bbb001"));
    assert!(!scoped.stdout.contains("mx-aaa001"));
    // unknown domain: the search hint already says `mulch add`
    let unknown = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "rust", "--domain", "nope",
    ]);
    assert_eq!(unknown.code, 1);
    assert!(unknown.stderr.contains("Available domains: dev, ops"));
    let unknown_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "search", "rust", "--domain", "nope",
    ]);
    assert_eq!(unknown_json.code, 1);
    assert!(
        unknown_json
            .stderr
            .contains("Domain \\\"nope\\\" not found in config. Available domains: dev, ops")
    );
    // an empty --domain is falsy: the all-domains walk (still needs a
    // query or filter to pass the guard)
    let unscoped = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "rust", "--domain", "", "--format", "compact",
    ]);
    assert_eq!(unscoped.code, 0);
    assert!(unscoped.stdout.contains("mx-aaa001"));
    assert!(unscoped.stdout.contains("mx-bbb001"));
}

/// The json envelope: query/total/domains shape; only matching
/// domains appear; filter-only carries query null; no match carries
/// total 0 with empty domains.
#[test]
fn search_json_envelope_pins_the_shape() {
    let ours = seeded_store("s-json", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "search", "rust",
    ]);
    assert_eq!(json.code, 0);
    assert_eq!(json.stderr, "");
    assert!(json.stdout.starts_with(
        "{\n  \"success\": true,\n  \"command\": \"search\",\n  \"query\": \"rust\",\n  \
         \"total\": 5,\n  \"domains\": ["
    ));
    assert!(json.stdout.contains("\"domain\": \"dev\""));
    assert!(json.stdout.contains("\"domain\": \"ops\""));
    // filter-only: query null
    let filtered = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "search", "--type", "pattern",
    ]);
    assert!(filtered.stdout.contains("\"query\": null,\n  \"total\": 5"));
    // no match: total 0, empty domains
    let none = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "search", "zzzz",
    ]);
    assert!(none.stdout.contains("\"total\": 0,\n  \"domains\": []"));
    // --sort-by-score reorders the matches array
    let sorted = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "search",
        "rust",
        "--sort-by-score",
    ]);
    let starred = sorted
        .stdout
        .find("mx-aaa002")
        .expect("the starred record matches");
    let first = sorted
        .stdout
        .find("mx-aaa001")
        .expect("the other record matches");
    assert!(starred < first, "the ★1 record leads: {}", sorted.stdout);
}

/// --sort-by-score: the ★1 record outranks the score-less ones.
#[test]
fn search_sort_by_score() {
    let ours = seeded_store("s-sort", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let sorted = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "rust",
        "--sort-by-score",
        "--format",
        "compact",
    ]);
    assert_eq!(sorted.code, 0);
    assert!(
        first_bullet(&sorted.stdout).contains("mx-aaa002"),
        "{}",
        sorted.stdout
    );
}

/// The guard fires BEFORE the store opens: no-store runs report it,
/// not the no-store error; with a query the no-store wording is
/// readConfig's.
#[test]
fn search_guard_and_no_store() {
    let ours = TempDir::new("s-guard");
    for args in [
        (&["search"][..]),
        (&["--json", "search"][..]),
        (&["search", ""][..]),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?}");
        if args[0] == "--json" {
            assert_eq!(
                our.stderr,
                "{\n  \"success\": false,\n  \"command\": \"search\",\n  \"error\": \"Provide a \
                 search query or use --type, --domain, --tag, --classification, --file, or \
                 --outcome-status to filter.\"\n}\n",
                "{args:?}"
            );
        } else {
            assert_eq!(
                our.stderr,
                "Error: Provide a search query or use --type, --domain, --tag, --classification, \
                 --file, or --outcome-status to filter.\n",
                "{args:?}"
            );
        }
    }
    // json guard: the envelope on stderr
    let json_guard = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "search"]);
    assert_eq!(json_guard.code, 1);
    assert_eq!(
        json_guard.stderr,
        "{\n  \"success\": false,\n  \"command\": \"search\",\n  \"error\": \"Provide a search \
         query or use --type, --domain, --tag, --classification, --file, or --outcome-status to \
         filter.\"\n}\n"
    );
    // with a query the no-store wording is readConfig's
    let with_query = run_in(&ours.0, Path::new(mulch_bin()), &["search", "rust"]);
    assert_eq!(with_query.code, 1);
    assert_eq!(
        with_query.stderr,
        "Error: No .mulch/ directory found. Run `mulch init` to set up this project.\n"
    );
    // filter-only no-store: same readConfig error
    let filtered = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "--type", "pattern",
    ]);
    assert_eq!(filtered.code, 1);
    assert!(
        filtered
            .stderr
            .contains("No .mulch/ directory found. Run `mulch init` to set up this project.")
    );
}

/// The archived sections: live + archived per domain under
/// --archived, archived totals included; without --archived the
/// archive stays invisible.
#[test]
fn search_archived_sections() {
    let ours = seeded_store("s-arch", Some(CORPUS_CONFIG), &[], &[]);
    archive_store(&ours.0);
    // compact over the archive corpus throws on the unknown-type
    // record that matches "rust" through its tags (the registry-gated
    // summary)
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "rust",
        "--archived",
        "--format",
        "compact",
    ]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stderr, "Error: Unknown record type: zzzcustom\n");
    // the reason-less archived record rides along in json (the plain
    // render throws on the unknown-type archive record)
    let reasoned = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "search",
        "archived",
        "--archived",
    ]);
    assert_eq!(reasoned.code, 0);
    assert!(reasoned.stdout.contains("mx-arc002"), "{}", reasoned.stdout);
    // filter-only over the archive corpus
    let conv = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "--archived",
        "--type",
        "convention",
        "--format",
        "compact",
    ]);
    assert!(conv.stdout.contains("mx-arc002"));
    // json: matches/archived keys
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "search",
        "rust",
        "--archived",
    ]);
    assert_eq!(json.code, 0);
    assert!(json.stdout.contains("\"archived\": ["), "{}", json.stdout);
    // without --archived the archive stays invisible
    let live_only = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "search", "rust",
    ]);
    assert!(!live_only.stdout.contains("mx-arc001"));
    let live_plain = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search", "rust", "--format", "compact",
    ]);
    assert!(!live_plain.stdout.contains("mx-arc001"));
}

/// The archived section's summary is registry-gated: an unknown-type
/// archive record that matches through its tags fails the plain
/// render, while json mode never renders a summary and succeeds.
#[test]
fn search_archived_unknown_type_throws_in_plain_only() {
    let ours = seeded_store("s-archz", None, &[], &[]);
    archive_store(&ours.0);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "rust",
        "--archived",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, "Error: Unknown record type: zzzcustom\n");
    // json: the unknown-type record rides along untouched
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "search",
        "rust",
        "--archived",
    ]);
    assert_eq!(json.code, 0);
    assert!(json.stdout.contains("zzzcustom"), "{}", json.stdout);
}

/// --archived with no archive directory at all: an empty corpus, no
/// error (the archive read swallows the missing file).
#[test]
fn search_archived_missing_archive_is_quiet() {
    let ours = seeded_store("s-archm", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "rust",
        "--archived",
        "--format",
        "compact",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "search",
        "rust",
        "--archived",
    ]);
    assert_eq!(json.code, 0);
}

/// `--type`'s choice set is the store's registry: an unregistered
/// name rejects at parse time (rc 1; wording is the parser's).
#[test]
fn search_type_choices_are_registry_gated() {
    let ours = seeded_store("s-type", Some(CORPUS_CONFIG), &[], &[]);
    corpus(&ours.0);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "search",
        "--type",
        "zzzcustom",
        "rust",
    ]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
}
