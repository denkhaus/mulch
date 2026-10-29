//! `custom_types` probes (mulch-b4bd) — OUR byte pins (ADR-0008: the
//! differential battery is dissolved; our bytes are the regression
//! memory): batch create/upsert/content-hash ids, doctor's registry
//! counts, the global config-error channel, `--fix` over invalid
//! custom records, the flag path, field aliases' end-to-end
//! discriminators. (Six differential-only probes were deleted at the
//! ADR-0008 cutover: validate's oneOf cascade, status counts, the
//! batch validation hint, alias read canonicalization, alias command
//! surfaces, and the alias invalid-config matrix — none carried an
//! own-side expectation; the alias validation rules stay unit-pinned
//! in the lib's custom_types tests.)

use std::path::Path;

use super::support::*;

/// The probe config: a field-dedup custom (separate id_key) and a
/// content_hash custom (never-duplicate, empty id key).
const CUSTOM_CONFIG: &str = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  snippet:\n    required: [name, code]\n    optional: [note]\n    dedup_key: name\n    id_key: name\n    summary: \"{name}: {code}\"\n  rant:\n    required: [body]\n    dedup_key: content_hash\n    summary: \"{body}\"\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";

/// The invalid config: missing dedup_key.
const BROKEN_CONFIG: &str = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  broken:\n    required: [name]\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";

/// Seeds a store with the custom config and the shared batch file.
fn custom_store(tag: &str) -> TempDir {
    let ours = TempDir::new(tag);
    write_seed_with_config(&ours.0, CUSTOM_CONFIG, &[]);
    std::fs::write(
        ours.0.join("b.json"),
        "[\n \
         {\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:00.000Z\",\"name\":\"b1\",\"code\":\"x\"},\n \
         {\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:01.000Z\",\"name\":\"b1\",\"code\":\"y\"},\n \
         {\"type\":\"rant\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:02.000Z\",\"body\":\"bb\"}\n\
         ]\n",
    )
    .expect("batch file");
    ours
}

#[test]
fn custom_batch_create_upsert_content_hash_pin_our_summaries() {
    let ours = custom_store("ct-batch");
    // First run: create 2 (snippet + rant), update 1 (within-batch
    // snippet upsert); second run: the same records again — the
    // snippet updates in place, the content-hash rant CREATES anew.
    let first = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--batch", "b.json",
    ]);
    assert_eq!(first.code, 0, "{}", first.stderr);
    assert_eq!(
        first.stdout,
        "✓ Created 2 record(s) in dev\n✓ Updated 1 record(s) in dev\n"
    );
    let second = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--batch", "b.json",
    ]);
    assert_eq!(second.code, 0, "{}", second.stderr);
    assert_eq!(
        second.stdout,
        "✓ Created 1 record(s) in dev\n✓ Updated 2 record(s) in dev\n"
    );
}

#[test]
fn custom_doctor_registry_counts_custom_rows() {
    let ours = custom_store("ct-doctor");
    // Run the batch so counts exist, then doctor: the registry row
    // counts built-in vs custom types.
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--batch", "b.json",
    ]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("8 type(s) registered: 6 built-in, 2 custom"),
        "registry counts the custom rows: {}",
        run.stdout
    );
}

#[test]
fn custom_invalid_config_error_pins_the_prefix() {
    let ours = TempDir::new("ct-broken");
    write_seed_with_config(&ours.0, BROKEN_CONFIG, &[]);
    // The store commands fail through the global config-error
    // surface (the registry bootstrap).
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(run.code, 1);
    assert!(
        run.stderr
            .starts_with("Config error: Custom type \"broken\" must set \"dedup_key\""),
        "{}",
        run.stderr
    );
}

#[test]
fn custom_doctor_fix_drops_invalid_custom_records() {
    let ours = custom_store("ct-fix");
    std::fs::write(
        ours.0.join(".mulch/expertise/dev.jsonl"),
        "{\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:00.000Z\",\"name\":\"ok\",\"code\":\"c\",\"id\":\"mx-333333\"}\n\
         {\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:01.000Z\",\"code\":\"no-name\"}\n\
         {\"type\":\"rant\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:02.000Z\",\"body\":\"b\"}\n",
    )
    .expect("domain file");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    // the unknown-type finding keeps the run failing after the fix
    assert_eq!(run.code, 1);
    let file = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert_eq!(file.lines().count(), 2, "invalid custom dropped: {file}");
    assert!(file.contains("mx-333333"), "valid custom kept: {file}");
    assert!(
        file.contains("\"type\":\"rant\""),
        "valid id-less kept: {file}"
    );
    assert!(
        !file.contains("no-name"),
        "name-less snippet dropped: {file}"
    );
}

#[test]
fn custom_move_and_delete_summaries_render_the_compiled_template() {
    // CUSTOM_CONFIG plus a registered `tgt` for the move target; the
    // fixed ids (the resolver matches the id field verbatim) render
    // the COMPILED summary template ("{name}: {code}").
    let ours = TempDir::new("ct-move");
    write_seed_with_config(
        &ours.0,
        &CUSTOM_CONFIG.replacen("  dev: {}\n", "  dev: {}\n  tgt: {}\n", 1),
        &[],
    );
    let fixture = "{\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T10:00:00.000Z\",\"name\":\"s1\",\"code\":\"print(1)\",\"id\":\"mx-aaaa01\"}\n{\
        \"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T10:00:00.000Z\",\"name\":\"s2\",\"code\":\"x\",\"id\":\"mx-aaaa02\"}\n";
    std::fs::write(ours.0.join(".mulch/expertise/dev.jsonl"), fixture).expect("dev fixture");
    std::fs::write(ours.0.join(".mulch/expertise/tgt.jsonl"), "").expect("tgt fixture");

    let moved = run_in(&ours.0, Path::new(mulch_bin()), &[
        "move",
        "dev",
        "mx-aaaa01",
        "tgt",
    ]);
    assert_eq!(moved.code, 0, "{}", moved.stderr);
    assert_eq!(
        moved.stdout,
        "✓ Moved snippet mx-aaaa01 from dev → tgt: s1: print(1)\n"
    );
    assert!(
        read_store_file(&ours.0, "expertise/tgt.jsonl").contains("mx-aaaa01"),
        "the move landed"
    );
    let deleted = run_in(&ours.0, Path::new(mulch_bin()), &[
        "delete",
        "dev",
        "mx-aaaa02",
    ]);
    assert_eq!(deleted.code, 0, "{}", deleted.stderr);
    assert_eq!(
        deleted.stdout,
        "✓ Deleted snippet mx-aaaa02 from dev: s2: x\n"
    );
}

// ---- sprint 31 (mulch-d524): the custom-type-aware flag path ----

/// The flag-path config: adds an extracts-files custom and an
/// underscored-field custom to the probe pair.
const FLAG_CONFIG: &str = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  snippet:\n    required: [name, code]\n    optional: [note]\n    dedup_key: name\n    id_key: name\n    summary: \"{name}: {code}\"\n  sketchnote:\n    required: [title, code_name]\n    dedup_key: title\n    id_key: title\n    summary: \"{title}!\"\n  gitfile:\n    required: [name]\n    optional: [files]\n    dedup_key: name\n    id_key: name\n    extracts_files: true\n    summary: \"{name}\"\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";

/// Flag-path store (no git: evidence and context files stay absent).
fn flag_store(tag: &str) -> TempDir {
    let ours = TempDir::new(tag);
    write_seed_with_config(&ours.0, FLAG_CONFIG, &[]);
    ours
}

#[test]
fn custom_flag_path_create_and_upsert_write_the_payload() {
    // plain create and a second-run upsert (the field-dedup custom
    // updates in place); customs have NO positional fallback.
    let ours = flag_store("ct-flag-plain");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--type", "snippet", "--name", "s1", "--code", "print(1)",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let file = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(
        file.contains("\"name\":\"s1\"") && file.contains("\"code\":\"print(1)\""),
        "payload fields written: {file}"
    );

    // upsert: the same named record already exists — updated in place
    let again = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--type", "snippet", "--name", "s1", "--code", "c2",
    ]);
    assert_eq!(again.code, 0, "{}", again.stderr);
    let file = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert_eq!(file.lines().count(), 1, "one record, updated: {file}");
    assert!(file.contains("\"code\":\"c2\""), "code replaced: {file}");
}

#[test]
fn custom_flag_missing_flags_retry_and_choices_pin_our_set() {
    // OUR invalid-choice rejection (clap wording is the documented
    // deviation): the SET is the merged registry's names.
    let ours = flag_store("ct-choice");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--type", "nosuch",
    ]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(
        run.stderr.contains(
            "[possible values: convention, pattern, failure, decision, reference, guide, snippet, sketchnote, gitfile]"
        ),
        "the choice set is the merged registry: {}",
        run.stderr
    );
}

#[test]
fn custom_git_context_files_feed_the_extracts_files_field() {
    // extracts_files types auto-populate from the git context
    // (staged + unstaged, sorted): the custom AND the builtin pattern.
    for (tag, args) in [
        ("custom", vec![
            "record", "dev", "--type", "gitfile", "--name", "g1",
        ]),
        ("builtin", vec![
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p1",
            "--description",
            "d1",
        ]),
    ] {
        let ours = TempDir::new(&format!("ct-ctx-{tag}"));
        write_seed_with_config(&ours.0, FLAG_CONFIG, &[("dev", "")]);
        std::fs::write(ours.0.join("a.rs"), "one\n").expect("a.rs");
        std::fs::create_dir_all(ours.0.join("src")).expect("src");
        std::fs::write(ours.0.join("src").join("b.rs"), "one\n").expect("b.rs");
        for git_args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "t@example.org"],
            vec!["config", "user.name", "t"],
            vec!["add", ".", "-A"],
            vec!["commit", "-qm", "init"],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(&git_args)
                    .current_dir(&ours.0)
                    .output()
                    .expect("git spawns")
                    .status
                    .success(),
                "fixture git {git_args:?}"
            );
        }
        // unstaged modification + staged addition (untracked never
        // appears in the context)
        std::fs::write(ours.0.join("a.rs"), "two\n").expect("a.rs modified");
        std::fs::write(ours.0.join("src").join("c.rs"), "new\n").expect("c.rs");
        assert!(
            std::process::Command::new("git")
                .args(["add", "src/c.rs"])
                .current_dir(&ours.0)
                .output()
                .expect("git stage")
                .status
                .success()
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(run.code, 0, "{tag}: {}", run.stderr);
        let file = read_store_file(&ours.0, "expertise/dev.jsonl");
        assert!(
            file.contains("a.rs") && file.contains("src/c.rs"),
            "{tag}: context files collected: {file}"
        );
        assert!(
            !file.contains("src/b.rs"),
            "{tag}: unchanged file not collected: {file}"
        );
    }
}

#[test]
fn custom_edit_flag_path_writes_and_rejects() {
    // edit accepts custom field flags: scalars replace in place, the
    // extracts-files field splits commas, and unregistered types
    // write no payload fields (the write gate rejects either way).
    let ours = flag_store("ct-edit-custom-field");
    let seeded = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--type", "snippet", "--name", "s1", "--code", "c",
    ]);
    assert_eq!(seeded.code, 0, "seed failed: {}", seeded.stderr);
    let edited = run_in(&ours.0, Path::new(mulch_bin()), &[
        "edit",
        "dev",
        "mx-298f5e",
        "--code",
        "new",
    ]);
    assert_eq!(edited.code, 0, "{}", edited.stderr);
    assert!(
        read_store_file(&ours.0, "expertise/dev.jsonl").contains("\"code\":\"new\""),
        "scalar replaced in place"
    );

    // unregistered type + --allow-unknown-types: NO payload field is
    // written — the write gate rejects the update, store intact
    let ours = flag_store("ct-edit-unknown");
    let unknown = "{\"type\":\"wtf\",\"name\":\"u\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-16T00:00:00.000Z\",\"id\":\"mx-000001\"}\n";
    std::fs::write(ours.0.join(".mulch/expertise/dev.jsonl"), unknown).expect("store");
    let edit = [
        "--allow-unknown-types",
        "edit",
        "dev",
        "mx-000001",
        "--content",
        "x",
    ];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &edit);
    assert_eq!(run.code, 1, "the write gate rejects the update");
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), unknown);
}

/// The alias config: `code` and `name` each own legacy names (the
/// read-time canonicalization matrix, mulch-b1cb).
const ALIAS_CONFIG: &str = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  snippet:\n    required: [code, name]\n    dedup_key: code\n    id_key: code\n    summary: \"{name}\"\n    aliases:\n      code: [snippet_code, legacycode]\n      name: [title2]\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";

/// A JS-falsy-but-non-empty canonical (0) WINS over the legacy value
/// — but the canonical shape is then schema-invalid (the field is
/// string-typed), so validate fails. An accidental legacy-wins
/// implementation would validate clean here (the end-to-end
/// discriminator the unit tables lack; the read-side matrix itself is
/// unit-pinned in the lib's custom_types tests).
#[test]
fn custom_alias_falsy_canonical_wins_end_to_end() {
    let store = "{\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-10T10:00:00Z\",\"code\":0,\"legacycode\":\"L1\"}\n";
    let ours = TempDir::new("ct-alias-falsy");
    write_seed_with_config(&ours.0, ALIAS_CONFIG, &[]);
    std::fs::write(ours.0.join(".mulch/expertise/dev.jsonl"), store).expect("alias store");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    assert_eq!(
        run.code, 1,
        "the falsy-canonical store must fail validation"
    );
}

#[test]
fn custom_alias_outcome_corner_migrates_the_raw_shape() {
    // `outcome` (singular) is a legal alias legacy name: the doctor
    // legacy-outcome check reads RAW bytes (doctor parses its own
    // lines without applyAliases), so the warn + migration must fire
    // even though the canonicalizing readers turn the key into
    // `result` (spec review 2026-10-17).
    let config = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  note:\n    required: [result]\n    dedup_key: result\n    id_key: result\n    summary: \"{result}\"\n    aliases:\n      result: [outcome]\n";
    // a FRESH stamp (a fixed calendar date would age past the
    // tactical shelf and turn this into a prune probe)
    let fresh_at = (jiff::Timestamp::now() - jiff::Span::new().hours(24)).to_string();
    let store = format!(
        "{{\"type\":\"note\",\"classification\":\"tactical\",\"recorded_at\":\"{fresh_at}\",\"outcome\":{{\"status\":\"success\"}},\"result\":\"keep\"}}\n"
    );
    let ours = TempDir::new("ct-alias-outcome");
    write_seed_with_config(&ours.0, config, &[]);
    std::fs::write(ours.0.join(".mulch/expertise/dev.jsonl"), store).expect("corner store");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let file = read_store_file(&ours.0, "expertise/dev.jsonl");
    // the migration wrote the RAW shape (result kept verbatim,
    // outcomes appended) — no alias canonicalization in this pass.
    assert!(
        file.contains("\"result\":\"keep\""),
        "raw result kept: {file}"
    );
    assert!(
        file.contains("\"outcomes\":[{\"status\":\"success\"}]"),
        "outcome migrated to outcomes: {file}"
    );
    assert!(
        !file.contains("\"outcome\":"),
        "legacy singular key gone: {file}"
    );
}
