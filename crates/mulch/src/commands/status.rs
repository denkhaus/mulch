//! `mulch status` — domain statistics, plain and `--json`.

use std::cmp::Ordering;
use std::fmt::Write as _;

use jiff::Timestamp;
use mulch::stale::{StaleRule, StaleVerdict};
use mulch::{CLASSIFICATIONS, Counter, js_round_u64, truthy};
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Record types in the reference's fixed distribution order (the
/// registry's order; mulch-a3de).
const TYPES: [&str; 6] = mulch::PAYLOAD_TYPES;

/// Runs `status` against the store at the current directory.
pub(super) fn run(opts: &GlobalOpts) -> Result<(), Failure> {
    let store = crate::commands::store::open_store("status", false)?;

    let governance = store.config().effective_governance();
    let shelf_life = store.config().effective_shelf_life();
    let now = Timestamp::now();
    let rule = StaleRule::from_shelf_life(&shelf_life);

    let mut domain_lines = Vec::new();
    let mut domains_json = Vec::new();
    for domain in store.domains() {
        let (records, mtime) = read_domain(opts, &store, &domain)?;
        let status = DomainStatus::compute(
            &domain,
            &records,
            mtime,
            now,
            &rule,
            &governance,
            &shelf_life,
        );

        domain_lines.push(status.plain_line(now));
        domains_json.push(status.into_json());
    }

    if opts.json {
        let mut fields = Map::new();
        fields.insert("domains".into(), Value::Array(domains_json));
        fields.insert("governance".into(), governance_json(&governance));
        fields.insert("shelf_life".into(), shelf_life_json(&shelf_life));
        print_json(&success_envelope("status", fields), false);
    } else {
        let mut text = String::from("Mulch Status\n============\n\n");
        if domain_lines.is_empty() {
            text.push_str("No domains configured. Run `mulch add <domain>` to get started.");
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
    oldest:                StampSlot,
    newest:                StampSlot,
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
        governance: &mulch::Governance,
        shelf: &mulch::ShelfLife,
    ) -> Self {
        // Distributions count in ordered usize maps; the JSON shape
        // (seeded keys first, extras in first-seen order, the JS
        // "undefined"-key and NaN->null rules) lives in one boundary
        // helper, `to_distribution`.
        let mut type_counts = Counter::seeded(&TYPES);
        let mut classification_counts = Counter::seeded(&CLASSIFICATIONS);
        let mut oldest = StampSlot::default();
        let mut newest = StampSlot::default();
        let mut stale_count = 0;

        for record in records {
            let field = |name: &str| record.get(name);
            type_counts.bump_field(field("type"));
            classification_counts.bump_known_field(field("classification"), &CLASSIFICATIONS);
            // The oldest/newest selection compares the RAW recorded_at
            // values (JS relational semantics over whatever JSON type
            // the field carries) and only coerces the winner through
            // `new Date(...)` at render time — a strict-ISO selection
            // diverged on every non-ISO-but-JS-parseable stamp
            // (mulch-4d25: date-only, space separator, zone-less
            // civil time, and the null/number coercions).
            let raw = field("recorded_at");
            oldest = oldest.older(raw);
            newest = newest.newer(raw);
            // The verdict owns the decay rule (reference isRecordStale:
            // unknown/missing -> never stale, and the (shelf+1)-day
            // boundary); it parses recorded_at itself.
            if matches!(rule.verdict(record, now), StaleVerdict::Stale { .. }) {
                stale_count += 1;
            }
        }

        // Rotting is the NEWEST record's age vs the observational
        // shelf life (reference status.ts), not the stale count:
        // `new Date(newest)` — truthiness-gated — then Math.floor of
        // the day quotient, strictly above the shelf. An Invalid Date
        // (truthy but unparsable) floors to NaN and never rots.
        let now_ms = mulch::timestamp_ms(now);
        let (rotting, rotting_days) = match newest.as_date() {
            DateOf::Ms(newest_ms) => {
                let age_days = ((now_ms - newest_ms) / 86_400_000.0).floor();
                if age_days > shelf.observational as f64 {
                    (true, Some(mulch::jsval::days_u64(age_days)))
                } else {
                    (false, None)
                }
            }
            _ => (false, None),
        };

        Self {
            domain: domain.into(),
            max_entries: governance.max_entries,
            warn_entries: governance.warn_entries,
            hard_limit: governance.hard_limit,
            count: records.len(),
            last_updated: mtime,
            oldest,
            newest,
            stale_count,
            rotting,
            rotting_days,
            type_counts,
            classification_counts,
        }
    }

    /// The plain one-line summary (relative times like the reference).
    fn plain_line(&self, now: Timestamp) -> String {
        let now_ms = mulch::timestamp_ms(now);
        let mut line = format!(
            "  {}: {} records (updated {})",
            self.domain,
            self.count,
            self.last_updated
                .map_or_else(|| "never".into(), |t| relative(t, now))
        );
        // The reference gates the range on the DATES' truthiness
        // (an Invalid Date is truthy — it renders as `NaNd ago`);
        // only a falsy/absent winner skips the range.
        let range = match (&self.oldest, &self.newest) {
            (StampSlot::Won(oldest), StampSlot::Won(newest))
                if truthy(oldest) && truthy(newest) =>
            {
                let oldest_ago = mulch::time_ago(mulch::js_date_ms(oldest), now_ms);
                let newest_ago = mulch::time_ago(mulch::js_date_ms(newest), now_ms);
                if oldest_ago == newest_ago {
                    format!(" — recorded {oldest_ago}")
                } else {
                    format!(" — recorded {oldest_ago} → {newest_ago}")
                }
            }
            _ => String::new(),
        };
        let _ = write!(line, "{range}");
        // Governance thresholds, then rotting (reference order). The
        // ladder verdict is the lib's (ADR-0005); the wording stays.
        let ladder = mulch::Governance {
            max_entries:  self.max_entries,
            warn_entries: self.warn_entries,
            hard_limit:   self.hard_limit,
        };
        match ladder.tier(count64(self.count)) {
            mulch::Tier::Hard => {
                let _ = write!(line, " ⚠ OVER HARD LIMIT — must decompose");
            }
            mulch::Tier::Warn => {
                let _ = write!(line, " ⚠ consider splitting domain");
            }
            mulch::Tier::Approaching => {
                let _ = write!(line, " — approaching limit");
            }
            mulch::Tier::Ok => {}
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
            json_num(js_round_u64(utilization.round())),
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
        // The health timestamps carry the RAW winners: the reference's
        // selection compares and keeps the field value verbatim (a
        // date-only stamp renders "2020-01-01"); an all-undefined
        // store DROPS the keys (JSON.stringify omits undefined
        // properties), an empty one renders null.
        if let Some(value) = self.oldest.health_json() {
            health.insert("oldest_timestamp".into(), value);
        }
        if let Some(value) = self.newest.health_json() {
            health.insert("newest_timestamp".into(), value);
        }

        let mut body = Map::new();
        body.insert("domain".into(), Value::String(self.domain));
        body.insert("count".into(), json_num(self.count as u64));
        body.insert("lastUpdated".into(), timestamp_json(self.last_updated));
        body.insert("oldest_recorded".into(), iso_json(&self.oldest.as_date()));
        body.insert("newest_recorded".into(), iso_json(&self.newest.as_date()));
        body.insert("rotting".into(), Value::Bool(self.rotting));
        body.insert("rotting_days".into(), match self.rotting_days {
            Some(days) => Value::from(days),
            None => Value::Null,
        });
        body.insert("health".into(), Value::Object(health));
        Value::Object(body)
    }
}

/// Reads a domain's records for reporting through the shared strict
/// reader seam (mulch-a8f4; the reference status uses
/// `readExpertiseFile`): arrays count as records, scalar/null lines
/// are a clean error where the reference crashes, unknown types error
/// unless allowed. The seam supplies the modification time.
fn read_domain(
    opts: &GlobalOpts,
    store: &mulch::StoreFiles,
    domain: &str,
) -> Result<(Vec<Value>, Option<Timestamp>), Failure> {
    let records = crate::commands::store::read_domain_records(
        "status",
        store,
        domain,
        opts.allow_unknown_types,
    )?
    .into_iter()
    .map(|line| line.record)
    .collect();
    let mtime = store
        .domain_modified(domain)
        .map_err(|source| Failure::handled("status", crate::output::chain_message(&source)))?
        .and_then(|time| Timestamp::try_from(time).ok());
    Ok((records, mtime))
}

/// The raw `recorded_at` winner of the health selection. The
/// reference carries the RAW field value through the loop (JS
/// relational comparisons, strict `<`/`>` keeping the first record on
/// ties) and only coerces it through `new Date(...)` when rendering —
/// probed 2026-10-24 (mulch-4d25): a space-separated stamp wins
/// "oldest" over a lexicographically-later but temporally-earlier
/// T-separated one, and the health block echoes the raw strings.
#[derive(Default)]
enum StampSlot {
    /// No record seen (the loop's `null` seed): health renders null.
    #[default]
    Empty,
    /// The winning record had NO recorded_at (JS `undefined`): the
    /// health JSON key DROPS (JSON.stringify omits undefined
    /// properties); every date consumer treats it as falsy.
    Missing,
    /// The winning raw value, whatever JSON type it carries.
    Won(Value),
}

impl StampSlot {
    /// `!winner || candidate < winner` — the oldest slot's law.
    fn older(self, candidate: Option<&Value>) -> Self {
        self.displaced(candidate, Ordering::Less)
    }

    /// `!winner || candidate > winner` — the newest slot's law.
    fn newer(self, candidate: Option<&Value>) -> Self {
        self.displaced(candidate, Ordering::Greater)
    }

    /// The shared displacement law: a falsy live winner (or none yet)
    /// takes any candidate unconditionally (`!oldestTimestamp`); a
    /// truthy winner keeps the slot unless the strict JS comparison
    /// displaces it — ties keep the FIRST record, and a `undefined`
    /// candidate's NaN never displaces.
    fn displaced(self, candidate: Option<&Value>, direction: Ordering) -> Self {
        match self {
            Self::Won(ref winner) if truthy(winner) => match candidate {
                Some(value) if mulch::jsval::js_rel(value, winner) == Some(direction) => {
                    Self::Won(value.clone())
                }
                _ => self,
            },
            _ => candidate.map_or(Self::Missing, |value| Self::Won(value.clone())),
        }
    }

    /// The health block's value: the raw JSON value, `null` for the
    /// empty seed, and NOTHING for the undefined winner (the key
    /// drops).
    fn health_json(&self) -> Option<Value> {
        match self {
            Self::Empty => Some(Value::Null),
            Self::Missing => None,
            Self::Won(value) => Some(value.clone()),
        }
    }

    /// The truthiness-gated `new Date(winner)`: [`DateOf::None`] —
    /// no Date at all (a falsy or absent winner; the plain range and
    /// rotting skip); [`DateOf::Invalid`] — a truthy winner that is
    /// an Invalid Date (the plain range still renders, as `NaNd
    /// ago`, and rotting stays false); [`DateOf::Ms`] — the
    /// constructor-coerced epoch milliseconds.
    fn as_date(&self) -> DateOf {
        match self {
            Self::Won(value) if truthy(value) => match mulch::js_date_ms(value) {
                Some(ms) => DateOf::Ms(ms),
                None => DateOf::Invalid,
            },
            _ => DateOf::None,
        }
    }
}

/// `new Date(winner)` behind the reference's `winner ? … : null`
/// truthiness gate (status.ts) — the three-way outcome the plain,
/// JSON, and rotting consumers each read differently.
enum DateOf {
    /// A falsy or absent winner: no Date object was constructed.
    None,
    /// A truthy winner whose coercion is the JS Invalid Date.
    Invalid,
    /// The coerced epoch milliseconds.
    Ms(f64),
}

/// `new Date(winner)?.toISOString() ?? null` — the body block's ISO
/// rendering of the coerced winner. The reference THROWS (RangeError)
/// on a truthy Invalid Date in JSON mode; we render null — the
/// documented clean-deviation class for reference crashes (the plain
/// channel renders `NaNd ago` and exits 0 there). A numeric stamp
/// inside TimeClip but beyond jiff's year-9999 range also renders
/// null.
fn iso_json(date: &DateOf) -> Value {
    let DateOf::Ms(ms) = *date else {
        return Value::Null;
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "TimeClip bounds |ms| within i64"
    )]
    let ms = ms as i64;
    timestamp_json(Timestamp::from_millisecond(ms).ok())
}

/// The record count as the thresholds' u64 shape.
fn count64(count: usize) -> u64 {
    u64::try_from(count).unwrap_or(0)
}

/// Relative-time rendering ("just now", "2m ago", "3h ago", "4d ago")
/// — the shared lib law (the reference `formatTimeAgo`). The old
/// seconds-trunc buckets were outcome-equivalent for every whole-second
/// stamp; the delegation additionally floors the sub-second mtimes
/// exactly like the reference's `Math.floor` on milliseconds.
fn relative(then: Timestamp, now: Timestamp) -> String {
    mulch::time_ago_ts(then, now)
}

/// Formats a timestamp as ISO-8601 with milliseconds and `Z`.
fn timestamp_json(time: Option<Timestamp>) -> Value {
    match time {
        None => Value::Null,
        Some(t) => Value::String(t.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()),
    }
}

/// Governance block (the effective thresholds).
fn governance_json(governance: &mulch::Governance) -> Value {
    let mut map = Map::new();
    let (max, warn, hard) = (
        governance.max_entries,
        governance.warn_entries,
        governance.hard_limit,
    );
    map.insert("max_entries".into(), json_num(max));
    map.insert("warn_entries".into(), json_num(warn));
    map.insert("hard_limit".into(), json_num(hard));
    Value::Object(map)
}

/// Shelf-life block (the effective thresholds).
fn shelf_life_json(shelf: &mulch::ShelfLife) -> Value {
    let mut map = Map::new();
    let (tactical, observational) = (shelf.tactical, shelf.observational);
    map.insert("tactical".into(), json_num(tactical));
    map.insert("observational".into(), json_num(observational));
    Value::Object(map)
}

/// JSON number helper.
fn json_num(value: u64) -> Value {
    Value::from(value)
}
