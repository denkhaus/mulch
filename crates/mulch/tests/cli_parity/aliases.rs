//! Canonical-domain alias resolution (ADR-0006, mulch-b0f7) — a
//! port-first extension: the reference has NO alias mechanism and
//! IGNORES the config block (probed: `ml status` and `ml prime` exit 0
//! with `domain_aliases:` present), so every test here is ungated and
//! pins OUR law.

use std::path::Path;

use super::support::*;

/// A twin-less store (no reference side) with `dev`/`rust` domains
/// and the canonical alias fixture: develop -> dev, devloop ->
/// develop (chain), rs -> rust.
fn alias_store(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let config = "version: '1'\n\
domains:\n  dev: {}\n  rust: {}\n\
domain_aliases:\n  develop: dev\n  devloop: develop\n  rs: rust\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";
    write_seed_with_config(&dir.0, config, &[("dev", ""), ("rust", "")]);
    dir
}

/// Address-level resolution end to end: recording through an alias
/// lands in the CANONICAL's file — the write redirect that stops
/// future fragmentation. Catches a resolution that only renames in
/// output (the file would be devloop.jsonl).
#[test]
fn record_through_an_alias_writes_the_canonical_file() {
    let dir = alias_store("alias-record");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "record",
        "devloop",
        "--type",
        "pattern",
        "--name",
        "aliased",
        "--description",
        "recorded through the alias",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        !dir.0.join(".mulch/expertise/devloop.jsonl").exists(),
        "no alias-named file appears"
    );
    let dev = read_store_file(&dir.0, "expertise/dev.jsonl");
    assert!(dev.contains("recorded through the alias"), "{dev}");
}

/// Priming through an alias is priming the canonical: byte-identical
/// output either way. Catches a resolution that skips prime's scoping.
#[test]
fn prime_through_an_alias_primes_the_canonical() {
    let dir = alias_store("alias-prime");
    let body = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-25T10:00:00.000Z\",\"name\":\"p1\",\"description\":\"d\"}\n";
    std::fs::write(dir.0.join(".mulch/expertise/dev.jsonl"), body).expect("seed");
    let canonical = run_in(&dir.0, Path::new(mulch_bin()), &["prime", "dev"]);
    let chained = run_in(&dir.0, Path::new(mulch_bin()), &["prime", "devloop"]);
    assert_eq!(canonical.code, 0, "{}", canonical.stderr);
    assert_eq!(
        canonical.stdout, chained.stdout,
        "alias primes the canonical"
    );
    assert!(canonical.stdout.contains("p1"), "{}", canonical.stdout);
}

/// Two aliases of one domain are the SAME target after resolution:
/// the move guard fires, nothing written. Catches a move that would
/// duplicate the record across alias-named paths.
#[test]
fn move_between_aliases_of_one_domain_is_refused() {
    let dir = alias_store("alias-move");
    let body = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-25T10:00:00.000Z\",\"name\":\"p1\",\"description\":\"d\",\"id\":\"mx-abc123\"}\n";
    std::fs::write(dir.0.join(".mulch/expertise/dev.jsonl"), body).expect("seed");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "move",
        "develop",
        "mx-abc123",
        "devloop",
    ]);
    assert_eq!(run.code, 1);
    assert_eq!(
        run.stderr,
        "Error: Source and target domain are the same — nothing to move.\n"
    );
    assert_eq!(
        read_store_file(&dir.0, "expertise/dev.jsonl"),
        body,
        "nothing written"
    );
}

/// The domain_set family resolves too: query/search/rank/ready take
/// aliases. Query stands in for the set (the same one-line law).
#[test]
fn query_through_an_alias_reads_the_canonical() {
    let dir = alias_store("alias-query");
    let body = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-25T10:00:00.000Z\",\"name\":\"p1\",\"description\":\"d\",\"id\":\"mx-abc123\"}\n";
    std::fs::write(dir.0.join(".mulch/expertise/dev.jsonl"), body).expect("seed");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &["query", "devloop"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.stdout.contains("p1"), "{}", run.stdout);
}

/// An invalid alias block fails LOUDLY at open (the typed
/// DomainAliases error) — never silently dropped.
#[test]
fn invalid_alias_block_fails_loudly() {
    let dir = TempDir::new("alias-invalid");
    let config = "version: '1'\ndomains:\n  dev: {}\ndomain_aliases:\n  x: nope\n";
    write_seed_with_config(&dir.0, config, &[("dev", "")]);
    let run = run_in(&dir.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(run.code, 1);
    assert!(
        run.stderr.contains("invalid domain_aliases"),
        "{}",
        run.stderr
    );
}
