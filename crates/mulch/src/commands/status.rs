//! `mulch status` — domain statistics, plain and `--json`.

use std::fmt::Write as _;

use jiff::Timestamp;
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::stale::StaleRule;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Record types in the reference's fixed distribution order.
const TYPES: [&str; 6] = [
    "convention",
    "pattern",
    "failure",
    "decision",
    "reference",
    "guide",
];

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
        let status = DomainStatus::compute(&domain, &records, mtime, now, &rule);

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
    type_counts:           Map<String, Value>,
    classification_counts: Map<String, Value>,
}

impl DomainStatus {
    fn compute(
        domain: &str,
        records: &[Value],
        mtime: Option<Timestamp>,
        now: Timestamp,
        rule: &StaleRule,
    ) -> Self {
        // Ordered distributions with the reference's JS key semantics:
        // seeded known keys first, then extras in first-seen order; a
        // non-string (undefined) field lands under the literal key
        // "undefined", and classification counters beyond the seeded
        // three are NaN in the reference — JSON `null`.
        let mut type_counts = Map::new();
        for kind in TYPES {
            type_counts.insert((*kind).into(), json_num(0));
        }
        let mut classification_counts = Map::new();
        for class in CLASSIFICATIONS {
            classification_counts.insert((*class).into(), json_num(0));
        }
        let mut oldest_recorded = None;
        let mut newest_recorded = None;
        let mut stale_count = 0;

        for record in records {
            let field = |name: &str| record.get(name).and_then(Value::as_str);
            let type_key = field("type").unwrap_or("undefined");
            let counted = match type_counts.get(type_key) {
                Some(Value::Number(number)) => number.as_u64().map_or(1, |counted| counted + 1),
                _ => 1,
            };
            type_counts.insert(type_key.into(), json_num(counted));
            let class_key = field("classification").unwrap_or("undefined");
            let next_class = if CLASSIFICATIONS.contains(&class_key) {
                match classification_counts.get(class_key) {
                    Some(Value::Number(number)) => {
                        number.as_u64().map_or(Value::Null, |n| json_num(n + 1))
                    }
                    _ => Value::Null,
                }
            } else {
                // NaN -> null in the reference's JSON
                Value::Null
            };
            classification_counts.insert(class_key.into(), next_class);
            if let Ok(Some(recorded)) = parse_timestamp(field("recorded_at")) {
                if oldest_recorded.is_none_or(|o| recorded < o) {
                    oldest_recorded = Some(recorded);
                }
                if newest_recorded.is_none_or(|n| recorded > n) {
                    newest_recorded = Some(recorded);
                }
                if rule.is_stale(classification_of(record), recorded, now) {
                    stale_count += 1;
                }
            }
        }

        Self {
            domain: domain.into(),
            count: records.len(),
            last_updated: mtime,
            oldest_recorded,
            newest_recorded,
            stale_count,
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
        line
    }

    /// The `--json` domain object (health block included).
    fn into_json(self) -> Value {
        let mut health = Map::new();
        health.insert("governance_utilization".into(), json_num(self.count as u64));
        health.insert("stale_count".into(), json_num(self.stale_count as u64));
        health.insert("type_distribution".into(), Value::Object(self.type_counts));
        health.insert(
            "classification_distribution".into(),
            Value::Object(self.classification_counts),
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
        body.insert("rotting".into(), Value::Bool(self.stale_count > 0));
        body.insert("rotting_days".into(), match self.stale_count {
            0 => Value::Null,
            _ => Value::from(0),
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

/// The record's classification, defaulting like the reference writer.
fn classification_of(record: &Value) -> &str {
    record
        .get("classification")
        .and_then(Value::as_str)
        .unwrap_or("tactical")
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
fn json_num(value: u64) -> Value {
    Value::from(value)
}
