//! `mulch status` — domain statistics, plain and `--json`.

use std::fmt::Write as _;

use jiff::Timestamp;
use mulch::stale::{StaleRule, StaleVerdict};
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Record types in the reference's fixed distribution order (the
/// registry's order; mulch-a3de).
const TYPES: [&str; 6] = mulch::PAYLOAD_TYPES;

/// Classifications in the reference's fixed distribution order.
const CLASSIFICATIONS: [&str; 3] = ["foundational", "tactical", "observational"];

/// Runs `status` against the store at the current directory.
pub(super) fn run(opts: &GlobalOpts) -> Result<(), Failure> {
    let store = crate::commands::open_store("status", false)?;

    let governance = store.config().governance().ok().flatten();
    let shelf_life = store.config().shelf_life().ok().flatten();
    let now = Timestamp::now();
    let rule = StaleRule::from_config(shelf_life.as_ref());

    let mut domain_lines = Vec::new();
    let mut domains_json = Vec::new();
    for domain in store.domains() {
        let (records, mtime) = read_domain(opts, &store, &domain)?;
        let observational_days = shelf_life.as_ref().map_or(30, |life| life.observational);
        let (max_entries, warn_entries, hard_limit) =
            governance.as_ref().map_or((100, 150, 200), |gov| {
                (gov.max_entries, gov.warn_entries, gov.hard_limit)
            });
        let status = DomainStatus::compute(
            &domain,
            &records,
            mtime,
            now,
            &rule,
            max_entries,
            warn_entries,
            hard_limit,
            observational_days,
        );

        domain_lines.push(status.plain_line(now));
        domains_json.push(status.into_json());
    }

    if opts.json {
        let mut fields = Map::new();
        fields.insert("domains".into(), Value::Array(domains_json));
        fields.insert("governance".into(), governance_json(governance.as_ref()));
        fields.insert("shelf_life".into(), shelf_life_json(shelf_life.as_ref()));
        print_json(&success_envelope("status", fields), false);
    } else {
        let mut text = String::from("Mulch Status\n============\n\n");
        if domain_lines.is_empty() {
            text.push_str("No domains configured. Run `ml add <domain>` to get started.");
        } else {
            text.push_str(&domain_lines.join("\n"));
        }
        // `--quiet` does NOT suppress status output (reference quirk).
        let _ = opts.quiet;
        print_line(false, &text);
    }
    Ok(())
}

/// Per-domain statistics (mirrors the reference JSON health block).
struct DomainStatus {
    domain:                String,
    count:                 usize,
    last_updated:          Option<Timestamp>,
    oldest_recorded:       Option<Timestamp>,
    newest_recorded:       Option<Timestamp>,
    stale_count:           usize,
    rotting:               bool,
    rotting_days:          Option<u64>,
    max_entries:           u64,
    warn_entries:          u64,
    hard_limit:            u64,
    type_counts:           Counter,
    classification_counts: Counter,
}

impl DomainStatus {
    fn compute(
        domain: &str,
        records: &[Value],
        mtime: Option<Timestamp>,
        now: Timestamp,
        rule: &StaleRule,
        max_entries: u64,
        warn_entries: u64,
        hard_limit: u64,
        observational_days: u64,
    ) -> Self {
        // Distributions count in ordered usize maps; the JSON shape
        // (seeded keys first, extras in first-seen order, the JS
        // "undefined"-key and NaN->null rules) lives in one boundary
        // helper, `to_distribution`.
        let mut type_counts = Counter::seeded(&TYPES);
        let mut classification_counts = Counter::seeded(&CLASSIFICATIONS);
        let mut oldest_recorded = None;
        let mut newest_recorded = None;
        let mut stale_count = 0;

        for record in records {
            let field = |name: &str| record.get(name);
            type_counts.bump_field(field("type"));
            classification_counts.bump_known_field(field("classification"), &CLASSIFICATIONS);
            if let Ok(Some(recorded)) =
                parse_timestamp(field("recorded_at").and_then(Value::as_str))
            {
                if oldest_recorded.is_none_or(|o| recorded < o) {
                    oldest_recorded = Some(recorded);
                }
                if newest_recorded.is_none_or(|n| recorded > n) {
                    newest_recorded = Some(recorded);
                }
            }
            // The verdict owns the decay rule (reference isRecordStale:
            // unknown/missing -> never stale, and the (shelf+1)-day
            // boundary); it parses recorded_at itself.
            if rule.verdict(record, now) == StaleVerdict::Stale {
                stale_count += 1;
            }
        }

        // Rotting is the NEWEST record's age vs the observational
        // shelf life (reference status.ts), not the stale count.
        let (rotting, rotting_days) = match newest_recorded {
            Some(newest) => {
                // Math.floor of the total-hours quotient (reference
                // age-in-days); the span is finite and non-negative
                let age_days =
                    f64_to_u64(((now - newest).total(jiff::Unit::Hour)).unwrap_or(0.0) / 24.0);
                if age_days > observational_days {
                    (true, Some(age_days))
                } else {
                    (false, None)
                }
            }
            None => (false, None),
        };

        Self {
            domain: domain.into(),
            max_entries,
            warn_entries,
            hard_limit,
            count: records.len(),
            last_updated: mtime,
            oldest_recorded,
            newest_recorded,
            stale_count,
            rotting,
            rotting_days,
            type_counts,
            classification_counts,
        }
    }

    /// The plain one-line summary (relative times like the reference).
    fn plain_line(&self, now: Timestamp) -> String {
        let mut line = format!(
            "  {}: {} records (updated {})",
            self.domain,
            self.count,
            self.last_updated
                .map_or_else(|| "never".into(), |t| relative(t, now))
        );
        if let (Some(oldest), Some(newest)) = (self.oldest_recorded, self.newest_recorded) {
            let oldest_ago = relative(oldest, now);
            let newest_ago = relative(newest, now);
            if oldest_ago == newest_ago {
                let _ = write!(line, " — recorded {oldest_ago}");
            } else {
                let _ = write!(line, " — recorded {oldest_ago} → {newest_ago}");
            }
        }
        // Governance thresholds, then rotting (reference order)
        if count64(self.count) >= self.hard_limit {
            let _ = write!(line, " ⚠ OVER HARD LIMIT — must decompose");
        } else if count64(self.count) >= self.warn_entries {
            let _ = write!(line, " ⚠ consider splitting domain");
        } else if count64(self.count) >= self.max_entries {
            let _ = write!(line, " — approaching limit");
        }
        if self.rotting {
            match self.rotting_days {
                Some(days) => {
                    let _ = write!(line, " ⚠ ROTTING (no writes in {days}d)");
                }
                None => {
                    let _ = write!(line, " ⚠ ROTTING");
                }
            }
        }
        line
    }

    /// The `--json` domain object (health block included).
    fn into_json(self) -> Value {
        let mut health = Map::new();
        // Reference: Math.round(count / max_entries * 100)
        let utilization = (self.count as f64 / self.max_entries as f64) * 100.0;
        health.insert(
            "governance_utilization".into(),
            json_num(f64_to_u64(utilization.round())),
        );
        health.insert("stale_count".into(), json_num(self.stale_count as u64));
        health.insert(
            "type_distribution".into(),
            self.type_counts.to_distribution(),
        );
        health.insert(
            "classification_distribution".into(),
            self.classification_counts.to_distribution(),
        );
        health.insert(
            "oldest_timestamp".into(),
            timestamp_json(self.oldest_recorded),
        );
        health.insert(
            "newest_timestamp".into(),
            timestamp_json(self.newest_recorded),
        );

        let mut body = Map::new();
        body.insert("domain".into(), Value::String(self.domain));
        body.insert("count".into(), json_num(self.count as u64));
        body.insert("lastUpdated".into(), timestamp_json(self.last_updated));
        body.insert(
            "oldest_recorded".into(),
            timestamp_json(self.oldest_recorded),
        );
        body.insert(
            "newest_recorded".into(),
            timestamp_json(self.newest_recorded),
        );
        body.insert("rotting".into(), Value::Bool(self.rotting));
        body.insert("rotting_days".into(), match self.rotting_days {
            Some(days) => Value::from(days),
            None => Value::Null,
        });
        body.insert("health".into(), Value::Object(health));
        Value::Object(body)
    }
}

/// Reads a domain's records for reporting through the strict reader
/// (the reference status uses `readExpertiseFile`): arrays count as
/// records, scalar/null lines are a clean error where the reference
/// crashes, unknown types error unless allowed. The seam supplies the
/// modification time.
fn read_domain(
    opts: &GlobalOpts,
    store: &mulch::StoreFiles,
    domain: &str,
) -> Result<(Vec<Value>, Option<Timestamp>), Failure> {
    let records = store
        .read_records(domain, opts.allow_unknown_types)
        .map_err(|source| {
            Failure::handled_on_stderr("status", crate::commands::render_core_error(&source))
        })?
        .into_iter()
        .map(|line| line.record)
        .collect();
    let mtime = store
        .domain_modified(domain)
        .map_err(|source| Failure::handled("status", crate::output::chain_message(&source)))?
        .and_then(|time| Timestamp::try_from(time).ok());
    Ok((records, mtime))
}

/// Parses an ISO-8601 timestamp with `Z` offset.
fn parse_timestamp(raw: Option<&str>) -> Result<Option<Timestamp>, jiff::Error> {
    match raw {
        None => Ok(None),
        Some(text) => text.parse::<Timestamp>().map(Some),
    }
}

/// The record count as the thresholds' u64 shape.
fn count64(count: usize) -> u64 {
    u64::try_from(count).unwrap_or(0)
}

/// An ordered distribution counter with the reference's JS semantics
/// at the JSON boundary: seeded keys first, extras in first-seen
/// order; a missing/non-string field counts under the literal key
/// `"undefined"`.
#[derive(Debug)]
struct Counter {
    counts: Vec<(String, Option<u64>)>,
}

impl Counter {
    fn seeded(keys: &[&str]) -> Self {
        Self {
            counts: keys.iter().map(|k| ((*k).to_owned(), Some(0))).collect(),
        }
    }

    /// Counts the field under its JS property key (`String(key)`
    /// coercion — `5` becomes `"5"`, `null` becomes `"null"`,
    /// objects `[object Object]`; a missing field is `"undefined"`).
    fn bump_field(&mut self, field: Option<&Value>) {
        let key = field.map_or_else(|| "undefined".into(), mulch::value_text);
        self.bump_key(&key, false);
    }

    /// Counts the field only when its key is one of `known`; anything
    /// else lands under its key as `None` (the reference's counter
    /// goes NaN there, JSON `null`).
    fn bump_known_field(&mut self, field: Option<&Value>, known: &[&str]) {
        let key = field.map_or_else(|| "undefined".into(), mulch::value_text);
        self.bump_key(&key, !known.contains(&key.as_str()));
    }

    fn bump_key(&mut self, key: &str, force_null: bool) {
        if let Some(slot) = self.counts.iter_mut().find(|(k, _)| k == key) {
            if force_null {
                slot.1 = None;
            } else {
                slot.1 = Some(slot.1.unwrap_or(0) + 1);
            }
            return;
        }
        let value = if force_null { None } else { Some(1) };
        self.counts.push((key.to_owned(), value));
    }

    /// The JSON object: counts, `null` for the NaN-emulating slots.
    /// Key order follows JS object enumeration: integer-like keys
    /// ascending first, then string keys in insertion order (probe:
    /// `{"type": 5}` yields `"5"` before the seeded names).
    fn to_distribution(&self) -> Value {
        let mut integer_keys: Vec<&(String, Option<u64>)> = self
            .counts
            .iter()
            .filter(|(key, _)| key.parse::<u64>().is_ok())
            .collect();
        integer_keys.sort_by_key(|(key, _)| key.parse::<u64>().unwrap_or(0));
        let mut map = Map::new();
        for (key, count) in integer_keys.into_iter().chain(
            self.counts
                .iter()
                .filter(|(key, _)| key.parse::<u64>().is_err()),
        ) {
            let value = match count {
                Some(count) => json_num(*count),
                None => Value::Null,
            };
            map.insert(key.clone(), value);
        }
        Value::Object(map)
    }
}

/// Relative-time rendering ("just now", "2m ago", "3h ago", "4d ago").
fn relative(then: Timestamp, now: Timestamp) -> String {
    let seconds = now.as_second() - then.as_second();
    if seconds < 60 {
        "just now".into()
    } else if seconds < 3_600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h ago", seconds / 3_600)
    } else {
        format!("{}d ago", seconds / 86_400)
    }
}

/// Formats a timestamp as ISO-8601 with milliseconds and `Z`.
fn timestamp_json(time: Option<Timestamp>) -> Value {
    match time {
        None => Value::Null,
        Some(t) => Value::String(t.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()),
    }
}

/// Governance defaults block (absent config → reference defaults).
fn governance_json(governance: Option<&mulch::Governance>) -> Value {
    let mut map = Map::new();
    let (max, warn, hard) = match governance {
        Some(g) => (g.max_entries, g.warn_entries, g.hard_limit),
        None => (100, 150, 200),
    };
    map.insert("max_entries".into(), json_num(max));
    map.insert("warn_entries".into(), json_num(warn));
    map.insert("hard_limit".into(), json_num(hard));
    Value::Object(map)
}

/// Shelf-life defaults block (absent config → reference defaults).
fn shelf_life_json(shelf: Option<&mulch::ShelfLife>) -> Value {
    let mut map = Map::new();
    let (tactical, observational) = match shelf {
        Some(s) => (s.tactical, s.observational),
        None => (14, 30),
    };
    map.insert("tactical".into(), json_num(tactical));
    map.insert("observational".into(), json_num(observational));
    Value::Object(map)
}

/// JSON number helper.
/// Clamps a non-negative reference computation into u64 (the
/// Math.round contract on finite, non-negative inputs).
fn f64_to_u64(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        // the reference's Math.round output (finite, non-negative);
        // truncation cannot lose data past .round()
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "guarded and rounded; mirrors Math.round"
        )]
        {
            value as u64
        }
    } else {
        0
    }
}

fn json_num(value: u64) -> Value {
    Value::from(value)
}
