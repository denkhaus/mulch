//! The record write model: the dedup lookup (`findDuplicate`), the
//! upsert decision (`UpsertPlan`), outcome-entry building, the compact
//! re-serializing writer (reference `writeExpertiseFile`) and
//! deterministic id assignment — the half of the record semantics the
//! mutating commands share. The read laws live in [`crate::records`].

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::ids::record_id;
use crate::jsval::{js_dedup_eq, parse_non_negative_number, truthy_str};

pub fn find_duplicate<'a, I>(
    registry: &crate::ids::Registry,
    records: I,
    candidate: &Value,
) -> Option<usize>
where
    I: IntoIterator<Item = &'a Value>,
{
    let record_type = candidate.get("type").and_then(Value::as_str)?;
    let row = registry.row(record_type)?;
    // content_hash never matches; the row's dedup is None then
    // (mulch-b4bd).
    let key = row.dedup.as_deref()?;
    let new_value = candidate.get(key);
    records.into_iter().position(|record| {
        record.get("type").and_then(Value::as_str) == Some(record_type)
            && js_dedup_eq(record.get(key), new_value)
    })
}

/// One outcome entry's raw flag strings — the shared owner of the
/// entry key order per command family and the strict duration parse
/// (the three outcome builders in record.ts/edit.ts/outcome.ts plus
/// utils/numeric-flags.ts), and the builders' JS truthiness law
/// (mulch-1209): empty `agent`/`notes`/`test_results` flags are
/// dropped, `duration` is not (`--duration ""` reaches the strict
/// parse and errors). `timestamped` selects the `outcome` command
/// family (status, recorded_at, duration, agent, notes,
/// test_results); the record/edit family writes status, duration,
/// test_results, agent. Plain data: fill the fields, call [`build`].
///
/// [`build`]: OutcomeEntry::build
pub struct OutcomeEntry<'a> {
    /// Whether recorded_at is stamped (the `outcome` command family).
    pub timestamped:   bool,
    /// Outcome verdict (`success` | `failure` | `partial`).
    pub status:        &'a str,
    /// `now` in ISO format — inserted as recorded_at when timestamped.
    pub now:           &'a str,
    /// Raw `--duration`/`--outcome-duration` flag value.
    pub duration:      Option<&'a str>,
    /// The flag name as it appears in the parse error.
    pub duration_flag: &'a str,
    /// Recording agent name.
    pub agent:         Option<&'a str>,
    /// Free-text notes (timestamped family only).
    pub notes:         Option<&'a str>,
    /// Test results summary.
    pub test_results:  Option<&'a str>,
}

impl OutcomeEntry<'_> {
    /// Builds the entry map in the family key order. `Err` carries the
    /// reference's parse-error line WITHOUT the `Error: ` prefix — the
    /// caller renders it through its command-named failure channel.
    pub fn build(&self) -> std::result::Result<Map<String, Value>, String> {
        let mut entry = Map::new();
        entry.insert("status".into(), Value::String(self.status.into()));
        if self.timestamped {
            entry.insert("recorded_at".into(), Value::String(self.now.into()));
        }
        if let Some(raw) = self.duration {
            let Some(value) = parse_non_negative_number(raw) else {
                return Err(format!(
                    "{} must be a non-negative number (got \"{raw}\").",
                    self.duration_flag
                ));
            };
            entry.insert("duration".into(), value);
        }
        // JS truthiness (reference `if (options.agent)` class,
        // mulch-1209): an empty flag value is absent — never stored,
        // never rendered. The jsval seam owns the law.
        let agent = truthy_str(self.agent);
        let notes = truthy_str(self.notes);
        let test_results = truthy_str(self.test_results);
        if self.timestamped {
            if let Some(agent) = agent {
                entry.insert("agent".into(), Value::String(agent.into()));
            }
            if let Some(notes) = notes {
                entry.insert("notes".into(), Value::String(notes.into()));
            }
            if let Some(test_results) = test_results {
                entry.insert("test_results".into(), Value::String(test_results.into()));
            }
        } else {
            if let Some(test_results) = test_results {
                entry.insert("test_results".into(), Value::String(test_results.into()));
            }
            if let Some(agent) = agent {
                entry.insert("agent".into(), Value::String(agent.into()));
            }
        }
        Ok(entry)
    }
}

/// The dedup→update/skip/create decision for one candidate against a
/// working set — the one owner of the registry rule shared by the
/// record flag path and the batch/stdin loop (reference record.ts +
/// `processStdinRecords`): a dedup-field match on a NAMED type upserts
/// with merged outcomes, an anonymous duplicate skips, and `--force`
/// (or no match) appends. The id-placement post-steps stay with the
/// callers (flag: id last; batch: input-id position, generated ids
/// after the merged outcomes).
#[derive(Debug)]
pub enum UpsertPlan {
    /// Append as a new record (no dedup match, or `--force`).
    Create,
    /// Named-type upsert: replace the record at `index` with `merged`
    /// (incoming fields, outcomes merged existing-first).
    Update {
        index:  usize,
        merged: Map<String, Value>,
    },
    /// Anonymous duplicate: keep the existing record at `index`.
    Skip { index: usize },
}

/// Computes the [`UpsertPlan`] for `candidate` against `working`
/// (existing records plus this batch's own appends — within-batch
/// duplicates upsert too; a dry-run pass keeps `working` unmutated so
/// within-batch duplicates count as creates there).
pub fn upsert_plan(
    registry: &crate::ids::Registry,
    working: &[Value],
    candidate: &Value,
    force: bool,
) -> UpsertPlan {
    if !force && let Some(index) = find_duplicate(registry, working, candidate) {
        let named = registry
            .row(
                candidate
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .is_some_and(|row| row.named);
        if named {
            let incoming = candidate.as_object().cloned().unwrap_or_default();
            let merged = merge_outcomes(&working[index], incoming);
            return UpsertPlan::Update { index, merged };
        }
        return UpsertPlan::Skip { index };
    }
    UpsertPlan::Create
}

/// Merges both sides' outcomes into `incoming` (reference
/// `{ ...record, outcomes: merged }`, existing first): replaces the
/// `outcomes` key in place when the incoming record already has one,
/// appends it at the end otherwise. The id stays untouched.
pub fn merge_outcomes(existing: &Value, mut incoming: Map<String, Value>) -> Map<String, Value> {
    let mut merged: Vec<Value> = Vec::new();
    for source in [existing.get("outcomes"), incoming.get("outcomes")] {
        if let Some(outcomes) = source.and_then(Value::as_array) {
            merged.extend(outcomes.iter().cloned());
        }
    }
    if !merged.is_empty() {
        incoming.insert("outcomes".into(), Value::Array(merged));
    }
    incoming
}

/// Writes records compactly (reference `writeExpertiseFile`): missing
/// ids are generated, lines are compact JSON, the file ends with `\n`
/// (0 bytes when empty), written through a temp file + rename.
///
/// # Errors
///
/// [`Error::Write`] on I/O failure.
pub fn write_records(
    registry: &crate::ids::Registry,
    path: &Path,
    records: &[Value],
) -> Result<()> {
    let mut body = String::new();
    for record in records {
        let mut record = record.clone();
        assign_missing_id(registry, &mut record);
        body.push_str(&serde_json::to_string(&record).unwrap_or_default());
        body.push('\n');
    }
    if records.is_empty() {
        body.clear();
    }
    // A unique temp name (the reference uses a random suffix) so two
    // writers never share a scratch file, with best-effort cleanup on a
    // failed rename.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let temp = PathBuf::from(format!("{}.tmp.{unique:x}", path.display()));
    std::fs::write(&temp, body).map_err(|source| Error::Write {
        path: temp.clone(),
        source,
    })?;
    std::fs::rename(&temp, path).map_err(|source| {
        std::fs::remove_file(&temp).ok();
        Error::Write {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// Assigns the deterministic id when a record lacks one.
pub fn assign_missing_id(registry: &crate::ids::Registry, record: &mut Value) {
    let Some(object) = record.as_object_mut() else {
        return;
    };
    if object.get("id").and_then(Value::as_str).is_some() {
        return;
    }
    let kind = crate::ids::effective_type(object.get("type").and_then(Value::as_str)).to_string();
    let key_field = registry.id_key_field(&kind);
    let key_value = object
        .get(key_field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    object.insert("id".into(), Value::String(record_id(&kind, &key_value)));
}

#[cfg(test)]
mod tests {

    /// The builtins-only registry for the dedup-law pins below
    /// (custom-types law pins live in custom_types.rs).
    fn registry() -> crate::ids::Registry {
        crate::ids::Registry::builtins()
    }

    use serde_json::json;

    use super::*;

    #[test]
    fn find_duplicate_matches_on_the_dedup_field_not_the_id() {
        let store = [
            json!({"type": "pattern", "name": "p", "id": "mx-old"}),
            json!({"type": "pattern", "name": "q", "id": "mx-q"}),
        ];
        // same name, different id: still the duplicate
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "pattern", "name": "p"})
            ),
            Some(0)
        );
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "pattern", "name": "zz"})
            ),
            None
        );
        // same dedup VALUE on another type never matches
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "guide", "name": "p"})
            ),
            None
        );
    }

    #[test]
    fn find_duplicate_matches_missing_fields_and_skips_unregistered_types() {
        let bare = [json!({"type": "failure", "resolution": "r"})];
        // both sides missing the dedup field match (undefined === undefined)
        assert_eq!(
            find_duplicate(
                &registry(),
                bare.iter(),
                &json!({"type": "failure", "resolution": "r2"})
            ),
            Some(0)
        );
        let store = [json!({"type": "failure", "description": "d"})];
        // one side missing does NOT match a present value
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "failure", "resolution": "r"})
            ),
            None
        );
        // unregistered types never duplicate (no registry definition)
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "custom", "description": "d"})
            ),
            None
        );
        // a record without a type never duplicates
        assert_eq!(
            find_duplicate(&registry(), store.iter(), &json!({"name": "p"})),
            None
        );
    }

    #[test]
    fn find_duplicate_follows_js_strict_equality() {
        // numbers compare as f64: 1 matches 1.0 (same JS number)
        let store = [json!({"type": "pattern", "name": 1})];
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "pattern", "name": 1.0})
            ),
            Some(0)
        );
        // like-typed only: "1" never matches 1
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "pattern", "name": "1"})
            ),
            None
        );
        // null only matches null, never a present value
        let nulls = [json!({"type": "pattern", "name": null})];
        assert_eq!(
            find_duplicate(
                &registry(),
                nulls.iter(),
                &json!({"type": "pattern", "name": null})
            ),
            Some(0)
        );
        assert_eq!(
            find_duplicate(&registry(), nulls.iter(), &json!({"type": "pattern"})),
            None
        );
        // objects/arrays never match (JS === compares identity)
        let objects = [json!({"type": "pattern", "name": {"a": 1}})];
        assert_eq!(
            find_duplicate(
                &registry(),
                objects.iter(),
                &json!({"type": "pattern", "name": {"a": 1}})
            ),
            None
        );
        let arrays = [json!({"type": "pattern", "name": [1]})];
        assert_eq!(
            find_duplicate(
                &registry(),
                arrays.iter(),
                &json!({"type": "pattern", "name": [1]})
            ),
            None
        );
    }
}

#[cfg(test)]
mod upsert_plan_tests {

    fn registry() -> crate::ids::Registry {
        crate::ids::Registry::builtins()
    }
    use serde_json::json;

    use super::{UpsertPlan, upsert_plan};

    #[test]
    fn plan_upserts_named_skips_anonymous_and_forces_creates() {
        let working = [
            json!({"type": "pattern", "name": "p", "outcomes": [{"status": "success"}]}),
            json!({"type": "convention", "content": "c"}),
        ];
        // named duplicate: Update with outcomes merged existing-first
        let incoming = json!({"type": "pattern", "name": "p", "outcomes": [{"status": "failure"}]});
        match upsert_plan(&registry(), &working, &incoming, false) {
            UpsertPlan::Update { index, merged } => {
                assert_eq!(index, 0);
                assert_eq!(
                    merged.get("outcomes"),
                    Some(&json!([{"status": "success"}, {"status": "failure"}]))
                );
            }
            other => panic!("expected Update, got {other:?}"),
        }
        // anonymous duplicate: Skip
        match upsert_plan(
            &registry(),
            &working,
            &json!({"type": "convention", "content": "c"}),
            false,
        ) {
            UpsertPlan::Skip { index } => assert_eq!(index, 1),
            other => panic!("expected Skip, got {other:?}"),
        }
        // force appends even on a duplicate
        assert!(matches!(
            upsert_plan(
                &registry(),
                &working,
                &json!({"type": "pattern", "name": "p"}),
                true
            ),
            UpsertPlan::Create
        ));
        // no match: Create
        assert!(matches!(
            upsert_plan(
                &registry(),
                &working,
                &json!({"type": "pattern", "name": "q"}),
                false
            ),
            UpsertPlan::Create
        ));
    }
}

#[cfg(test)]
mod outcome_entry_tests {
    use serde_json::{Map, json};

    use super::OutcomeEntry;

    fn key_order(map: &Map<String, serde_json::Value>) -> Vec<&str> {
        map.keys().map(String::as_str).collect()
    }

    #[test]
    fn build_orders_keys_per_family_and_errors_with_flag_name() {
        let timestamped = OutcomeEntry {
            timestamped:   true,
            status:        "success",
            now:           "2026-10-06T19:00:00.000Z",
            duration:      Some("7.25"),
            duration_flag: "--duration",
            agent:         Some("oa"),
            notes:         Some("on"),
            test_results:  Some("ot"),
        }
        .build()
        .expect("valid flags build");
        assert_eq!(key_order(&timestamped), [
            "status",
            "recorded_at",
            "duration",
            "agent",
            "notes",
            "test_results"
        ]);
        assert_eq!(timestamped["duration"], json!(7.25));

        let plain = OutcomeEntry {
            timestamped:   false,
            status:        "success",
            now:           "",
            duration:      Some("42.0"),
            duration_flag: "--outcome-duration",
            agent:         Some("ra"),
            notes:         Some("ignored on this family"),
            test_results:  Some("rt"),
        }
        .build()
        .expect("valid flags build");
        assert_eq!(key_order(&plain), [
            "status",
            "duration",
            "test_results",
            "agent"
        ]);
        // "42.0" canonicalizes to the integer 42
        assert_eq!(plain["duration"], json!(42));

        let error = OutcomeEntry {
            timestamped:   false,
            status:        "success",
            now:           "",
            duration:      Some("abc"),
            duration_flag: "--outcome-duration",
            agent:         None,
            notes:         None,
            test_results:  None,
        }
        .build()
        .expect_err("invalid duration errors");
        assert_eq!(
            error,
            "--outcome-duration must be a non-negative number (got \"abc\")."
        );
    }
}
