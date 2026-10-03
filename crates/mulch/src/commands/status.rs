//! `mulch status` — domain statistics, plain and `--json`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use jiff::Timestamp;
use mulch::Record;
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::stale::StaleRule;
use crate::commands::{NO_CONFIG_MESSAGE, NO_STORE_MESSAGE, StoreLocation, domain_file, locate};
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
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("status", format!("resolving cwd: {source}")))?;
    let store = match locate(&cwd) {
        Ok(StoreLocation::Missing) => {
            return Err(no_store("status", opts.json));
        }
        Ok(StoreLocation::NoConfig) => {
            return Err(no_config("status", opts.json));
        }
        Ok(StoreLocation::Open(store)) => store,
        Err(source) => {
            return Err(Failure::handled(
                "status",
                crate::output::chain_message(&source),
            ));
        }
    };

    let governance = store.config.governance().ok().flatten();
    let shelf_life = store.config.shelf_life().ok().flatten();
    let now = Timestamp::now();
    let rule = StaleRule::from_config(shelf_life.as_ref());

    let mut domain_lines = Vec::new();
    let mut domains_json = Vec::new();
    for domain in store.domains() {
        let file = domain_file(&store.root, &domain);
        let (records, mtime) = read_domain(&file);
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
    type_counts:           BTreeMap<&'static str, usize>,
    classification_counts: BTreeMap<&'static str, usize>,
}

impl DomainStatus {
    fn compute(
        domain: &str,
        records: &[Record],
        mtime: Option<Timestamp>,
        now: Timestamp,
        rule: &StaleRule,
    ) -> Self {
        let mut type_counts = BTreeMap::new();
        let mut classification_counts = BTreeMap::new();
        let mut oldest_recorded = None;
        let mut newest_recorded = None;
        let mut stale_count = 0;

        for record in records {
            if let Some(kind) = record
                .record_type()
                .and_then(|t| TYPES.iter().find(|k| **k == t))
            {
                *type_counts.entry(*kind).or_insert(0) += 1;
            }
            if let Some(class) = record
                .classification()
                .and_then(|c| CLASSIFICATIONS.iter().find(|k| **k == c))
            {
                *classification_counts.entry(*class).or_insert(0) += 1;
            }
            if let Ok(Some(recorded)) = parse_timestamp(record.recorded_at()) {
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
                .map_or_else(|| "unknown".into(), |t| relative(t, now))
        );
        if let Some(newest) = self.newest_recorded {
            let _ = write!(line, " — recorded {}", relative(newest, now));
        }
        line
    }

    /// The `--json` domain object (health block included).
    fn into_json(self) -> Value {
        let mut health = Map::new();
        health.insert("governance_utilization".into(), json_num(self.count as u64));
        health.insert("stale_count".into(), json_num(self.stale_count as u64));
        health.insert(
            "type_distribution".into(),
            distribution(&self.type_counts, &TYPES),
        );
        health.insert(
            "classification_distribution".into(),
            distribution(&self.classification_counts, &CLASSIFICATIONS),
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

/// Reads a domain file leniently: every parseable line counts as a
/// record, malformed lines are skipped (status never fails on them).
fn read_domain(file: &Path) -> (Vec<Record>, Option<Timestamp>) {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let records = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| Record::parse(line).ok())
        .collect();
    let mtime = std::fs::metadata(file)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| Timestamp::try_from(time).ok());
    (records, mtime)
}

/// The record's classification, defaulting like the reference writer.
fn classification_of(record: &Record) -> &str {
    record.classification().unwrap_or("tactical")
}

/// Parses an ISO-8601 timestamp with `Z` offset.
fn parse_timestamp(raw: Option<&str>) -> Result<Option<Timestamp>, jiff::Error> {
    match raw {
        None => Ok(None),
        Some(text) => text.parse::<Timestamp>().map(Some),
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

/// The always-complete ordered distribution object.
fn distribution<'a>(counts: &BTreeMap<&'a str, usize>, order: &[&'a str]) -> Value {
    let mut map = Map::new();
    for key in order {
        map.insert(
            (*key).into(),
            json_num(counts.get(key).copied().unwrap_or(0) as u64),
        );
    }
    Value::Object(map)
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

/// The handled no-store failure (envelope on STDERR, exit 1).
fn no_store(command: &str, json: bool) -> Failure {
    let mut failure = Failure::handled(command, NO_STORE_MESSAGE);
    failure.envelope_to_stderr = json;
    failure
}

/// The no-config failure (clean rendering of the reference's crash
/// path — README DEVIATIONS).
fn no_config(command: &str, json: bool) -> Failure {
    let mut failure = Failure::handled(command, NO_CONFIG_MESSAGE);
    failure.envelope_to_stderr = json;
    failure
}
