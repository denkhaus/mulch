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

/// The write-time collision guard (mulch-8deb): registering an alias
/// name is refused and the store still opens. Catches the guard being
/// placed after the config write (the store would already be bricked)
/// or missing altogether (the reference happily writes the name).
#[test]
fn add_of_an_alias_name_is_refused_and_the_store_stays_open() {
    let dir = alias_store("alias-add-refuse");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &["add", "develop"]);
    assert_eq!(run.code, 1);
    assert!(
        run.stderr
            .contains("cannot register \"develop\": it is an alias of \"dev\""),
        "{}",
        run.stderr
    );
    assert!(
        !dir.0.join(".mulch/expertise/develop.jsonl").exists(),
        "the guard fires before any file write"
    );
    let status = run_in(&dir.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(status.code, 0, "store still opens: {}", status.stderr);
}

/// Deleting a canonical that live aliases (transitively) resolve to
/// is refused (mulch-8deb): the dangling aliases would fail every
/// later open. The names are sorted, so the message is deterministic.
#[test]
fn delete_domain_of_an_aliased_canonical_is_refused() {
    let dir = alias_store("alias-delete-refuse");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "delete-domain",
        "dev",
        "--yes",
    ]);
    assert_eq!(run.code, 1);
    assert!(
        run.stderr
            .contains("still has live aliases (develop, devloop)"),
        "{}",
        run.stderr
    );
    assert!(
        dir.0.join(".mulch/expertise/dev.jsonl").exists(),
        "the domain file is kept"
    );
    let status = run_in(&dir.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(status.code, 0, "store still opens: {}", status.stderr);
}

/// `doctor --fix` leaves alias-named orphans alone (mulch-8deb):
/// adopting one would turn the very fragmentation ADR-0006 targets
/// into an unopenable store. The orphan warning survives the run —
/// nothing was adopted — and the store stays openable.
#[test]
fn doctor_fix_skips_alias_named_orphans() {
    let dir = alias_store("alias-doctor-skip");
    let body = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-25T10:00:00.000Z\",\"name\":\"stray\",\"description\":\"d\"}\n";
    std::fs::write(dir.0.join(".mulch/expertise/develop.jsonl"), body).expect("stray file");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("Skipped orphaned domain \"develop\": the name is an alias of \"dev\""),
        "{}",
        run.stdout
    );
    let after = run_in(&dir.0, Path::new(mulch_bin()), &["doctor"]);
    assert!(
        after.stdout.contains("develop.jsonl"),
        "still an orphan: {}",
        after.stdout
    );
    let status = run_in(&dir.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(status.code, 0, "store still opens: {}", status.stderr);
}

/// A seeded canonical record for the entry-side batteries: `mx-abc123`
/// in dev's file (the alias fixtures carry empty files).
fn seed_canonical_record(dir: &Path, name: &str) {
    let body = format!(
        "{{\"type\":\"pattern\",\"classification\":\"tactical\",\
         \"recorded_at\":\"2026-10-25T10:00:00.000Z\",\"name\":\"{name}\",\
         \"description\":\"d\",\"id\":\"mx-abc123\"}}\n"
    );
    std::fs::write(dir.join(".mulch/expertise/dev.jsonl"), body).expect("seed");
}

/// Archiving through an alias archives FROM the canonical: the record
/// lands in `archive/dev.jsonl`, `dev.jsonl` empties, and no
/// `archive/develop.jsonl` appears. Catches an archive entry that
/// resolves the live file but not the archive path.
#[test]
fn archive_through_an_alias_uses_the_canonical_archive() {
    let dir = alias_store("alias-archive");
    seed_canonical_record(&dir.0, "archived");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "archive",
        "develop",
        "mx-abc123",
        "--reason",
        "alias battery",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        !dir.0.join(".mulch/archive/develop.jsonl").exists(),
        "no alias-named archive"
    );
    let archive = read_store_file(&dir.0, "archive/dev.jsonl");
    assert!(archive.contains("alias battery"), "{archive}");
    let live = read_store_file(&dir.0, "expertise/dev.jsonl");
    assert!(!live.contains("mx-abc123"), "{live}");
}

/// Deleting through an alias empties the CANONICAL file and leaves no
/// alias-named residue. Catches a delete that addresses the alias
/// path (which would fail — no such file — or create one).
#[test]
fn delete_through_an_alias_empties_the_canonical_file() {
    let dir = alias_store("alias-delete");
    seed_canonical_record(&dir.0, "deleted");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "delete",
        "develop",
        "mx-abc123",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let live = read_store_file(&dir.0, "expertise/dev.jsonl");
    assert!(!live.contains("mx-abc123"), "{live}");
    assert!(
        !dir.0.join(".mulch/expertise/develop.jsonl").exists(),
        "no alias-named file appears"
    );
}

/// Editing through an alias updates the CANONICAL record — the write
/// half of the resolution law. Catches an edit that reads through the
/// alias and writes through the raw argument name.
#[test]
fn edit_through_an_alias_updates_the_canonical_record() {
    let dir = alias_store("alias-edit");
    seed_canonical_record(&dir.0, "before");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "edit",
        "develop",
        "mx-abc123",
        "--description",
        "edited through the alias",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    // The success line names the CANONICAL (the alias is an address,
    // not a domain) - the same law as the file it wrote.
    assert!(
        run.stdout.contains("in dev"),
        "names the canonical: {}",
        run.stdout
    );
    let live = read_store_file(&dir.0, "expertise/dev.jsonl");
    assert!(live.contains("edited through the alias"), "{live}");
    assert!(
        !dir.0.join(".mulch/expertise/develop.jsonl").exists(),
        "no alias-named file appears"
    );
}

/// Appending an outcome through an alias writes the CANONICAL file —
/// the second caller of the record-locator seam (mulch-8452), pinned
/// separately so a regression in one entry cannot hide behind the
/// edit battery.
#[test]
fn outcome_through_an_alias_writes_the_canonical_file() {
    let dir = alias_store("alias-outcome");
    seed_canonical_record(&dir.0, "outcome");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "outcome",
        "develop",
        "mx-abc123",
        "--status",
        "success",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let live = read_store_file(&dir.0, "expertise/dev.jsonl");
    assert!(live.contains("\"success\""), "{live}");
    assert!(
        !dir.0.join(".mulch/expertise/develop.jsonl").exists(),
        "no alias-named file appears"
    );
}

/// `compact --apply` resolves its address ONCE and writes through the
/// canonical (mulch-8452 review fold): the pre-fold body discarded the
/// table's answer and rewrote the raw argument's path, so an alias
/// input would read an empty file and archive into `develop.jsonl`.
#[test]
fn compact_apply_through_an_alias_writes_the_canonical_files() {
    let dir = alias_store("alias-compact");
    let body = "{\"type\":\"convention\",\"classification\":\"tactical\",\
\"recorded_at\":\"2026-10-25T10:00:00.000Z\",\"content\":\"one\",\
\"id\":\"mx-c0de01\"}\n\
{\"type\":\"convention\",\"classification\":\"tactical\",\
\"recorded_at\":\"2026-10-25T10:01:00.000Z\",\"content\":\"two\",\
\"id\":\"mx-c0de02\"}\n";
    std::fs::write(dir.0.join(".mulch/expertise/dev.jsonl"), body).expect("seed");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "compact",
        "develop",
        "--apply",
        "--records",
        "mx-c0de01,mx-c0de02",
        "--type",
        "convention",
        "--content",
        "merged through the alias",
        "--yes",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let live = read_store_file(&dir.0, "expertise/dev.jsonl");
    assert!(live.contains("merged through the alias"), "{live}");
    let archive = read_store_file(&dir.0, "archive/dev.jsonl");
    assert!(archive.contains("mx-c0de01"), "{archive}");
    assert!(
        !dir.0.join(".mulch/expertise/develop.jsonl").exists(),
        "no alias-named live file appears"
    );
    assert!(
        !dir.0.join(".mulch/archive/develop.jsonl").exists(),
        "no alias-named archive appears"
    );
}
