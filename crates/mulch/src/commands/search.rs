//! `mulch search` — BM25 search with confirmation boost (mulch-16da).
//!
//! Probed law (ml 0.10.7, 2026-10-23, /tmp/opencode/childA):
//! - The usage guard fires BEFORE the store opens (search.ts:122-143) — a
//!   no-store bare `search` still reports it, not the no-store error. An
//!   empty-string query is falsy: `search ""` trips the guard, `search ""
//!   --type pattern` runs the filter path with the "matching filters" label.
//! - The confirmation boost is a stable RE-SORT of the pure-BM25 result
//!   (expertise.ts:275-298): `--no-boost` (or a configured
//!   `search.boost_factor` of 0) leaves the BM25 order; the factor otherwise
//!   defaults to 0.1 and multiplies each hit's score by `1 + factor *
//!   confirmation_score`.
//! - `--archived` reads `.mulch/archive/<domain>.jsonl` (unknown-type tolerant)
//!   as a SEPARATE corpus per domain; the archived section renders after the
//!   live one and its summary is registry-gated: an unknown type THROWS
//!   (`getRecordSummary`) and fails the whole plain render — json mode never
//!   renders a summary and survives.
//! - The unknown-domain hint says `mulch add` here (the reference drifts per
//!   command — query says `ml add`), and the available list has NO `(none)`
//!   fallback (search.ts:158).
//! - `--format` follows the global four-choice option in both flag positions
//!   (the command's own ids declaration is unreachable, same law as query).

use jiff::Timestamp;
use mulch::bm25::{Bm25Result, DEFAULT_BM25_PARAMS, search_bm25};
use mulch::{DomainSpec, RecordFilter, apply_confirmation_boost};
use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, SearchArgs};
use crate::commands::open_store;
use crate::commands::render::{
    format_domain_expertise, format_domain_expertise_compact, format_domain_expertise_plain,
    format_domain_expertise_xml,
};
use crate::output::{
    EXIT_ERROR, Failure, failure_envelope, print_json, print_line, success_envelope,
};

/// `DEFAULT_SEARCH_BOOST_FACTOR` (schemas/config.ts:19).
const DEFAULT_SEARCH_BOOST_FACTOR: f64 = 0.1;

/// Runs `search` against the store at the current directory.
pub(super) fn run(opts: &GlobalOpts, args: &SearchArgs) -> Result<(), Failure> {
    let query = mulch::truthy_str(args.query.as_deref());
    let domain_flag = mulch::truthy_str(args.domain.as_deref());
    let filter = RecordFilter {
        type_:          truthy_owned(args.record_type.as_deref()),
        tag:            truthy_owned(args.tag.as_deref()),
        classification: truthy_owned(args.classification.as_deref()),
        file:           truthy_owned(args.file.as_deref()),
        outcome_status: truthy_owned(args.outcome_status.as_deref()),
    };
    if query.is_none() && domain_flag.is_none() && filter.is_empty() {
        return Err(Failure::handled_on_stderr(
            "search",
            "Error: Provide a search query or use --type, --domain, --tag, --classification, --file, or --outcome-status to filter.",
        ));
    }

    let store = open_store("search", true)?;
    let boost_factor = effective_boost(&store, args.no_boost)?;
    let configured = store.domains();
    let domains = match domain_flag {
        Some(domain) => mulch::domain_set(&configured, &DomainSpec::One(domain.to_string()))
            .map_err(|unknown| unknown_domain_failure(&unknown.domain, &unknown.available))?,
        None => configured,
    };

    let mut total: u64 = 0;
    let mut rows: Vec<Value> = Vec::new();
    let mut sections: Vec<String> = Vec::new();
    let now = Timestamp::now();
    for domain in &domains {
        let records = filter.apply(read_domain(&store, domain, opts)?);
        let matches = search_matches(records, query, boost_factor, args.sort_by_score);
        let archived = if args.archived {
            let records = filter.apply(read_archived(&store, domain)?);
            Some(search_matches(
                records,
                query,
                boost_factor,
                args.sort_by_score,
            ))
        } else {
            None
        };
        let archived_len = archived.as_ref().map_or(0, Vec::len);

        if opts.json {
            if !matches.is_empty() || archived_len > 0 {
                total += (matches.len() + archived_len) as u64;
                let mut row = Map::new();
                row.insert("domain".into(), Value::String(domain.clone()));
                row.insert("matches".into(), Value::Array(matches));
                if let Some(archived) = archived {
                    row.insert("archived".into(), Value::Array(archived));
                }
                rows.push(Value::Object(row));
            }
        } else {
            if !matches.is_empty() {
                total += matches.len() as u64;
                let last_updated = modified(&store, domain)?;
                sections.push(render_section(
                    opts.format.as_deref().unwrap_or("markdown"),
                    store.registry(),
                    domain,
                    &matches,
                    last_updated,
                    now,
                ));
            }
            if let Some(archived) = archived
                && !archived.is_empty()
            {
                total += archived.len() as u64;
                sections.push(archived_section(domain, &archived, store.registry())?);
            }
        }
    }

    if opts.json {
        let mut fields = Map::new();
        fields.insert(
            "query".into(),
            query.map_or(Value::Null, |query| Value::String(query.to_string())),
        );
        fields.insert("total".into(), Value::from(total));
        fields.insert("domains".into(), Value::Array(rows));
        print_json(&success_envelope("search", fields), false);
    } else {
        let label = query.map_or_else(
            || "matching filters".to_string(),
            |query| format!("matching \"{query}\""),
        );
        if sections.is_empty() {
            print_line(false, &format!("No records {label} found."));
        } else {
            print_line(false, &sections.join("\n\n"));
            let plural = if total == 1 { "" } else { "es" };
            print_line(false, &format!("\n{total} match{plural} found."));
        }
    }
    Ok(())
}

/// The unknown-domain failure (search.ts:153-170): the hint already
/// says `mulch add` in the reference, and the available list joins
/// WITHOUT the `(none)` fallback query uses.
fn unknown_domain_failure(domain: &str, available: &[String]) -> Failure {
    let one_line = format!(
        "Domain \"{domain}\" not found in config. Available domains: {}",
        available.join(", ")
    );
    let mut fields = Map::new();
    fields.insert("error".into(), Value::String(one_line.clone()));
    Failure {
        message:            format!(
            "Error: {one_line}\nHint: Run `mulch add {domain}` to create this domain, or check .mulch/mulch.config.yaml"
        ),
        code:               EXIT_ERROR,
        envelope:           failure_envelope("search", fields),
        envelope_to_stderr: true,
        rendered:           false,
    }
}

/// `searchRecords` (expertise.ts:275-298): BM25 ranking (with the
/// boost's own stable re-sort) when a query is given, the filter
/// result otherwise; `--sort-by-score` re-sorts the final set.
fn search_matches(
    records: Vec<Value>,
    query: Option<&str>,
    boost_factor: f64,
    sort_by_score: bool,
) -> Vec<Value> {
    let mut matches = match query {
        Some(query) => ranked(&records, query, boost_factor),
        None => records,
    };
    if sort_by_score {
        mulch::scoring::sort_by_confirmation(&mut matches);
    }
    matches
}

/// The BM25 half: pure scores, then the boosted stable re-sort when
/// the factor is positive (a factor of 0 keeps the BM25 order).
fn ranked(records: &[Value], query: &str, boost_factor: f64) -> Vec<Value> {
    let mut results = search_bm25(records, query, DEFAULT_BM25_PARAMS);
    if boost_factor > 0.0 {
        results.sort_by(|a, b| {
            let boosted = |result: &Bm25Result| {
                apply_confirmation_boost(result.score, &result.record, boost_factor)
            };
            boosted(b)
                .partial_cmp(&boosted(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    results.into_iter().map(|result| result.record).collect()
}

/// `search.boost_factor ?? 0.1`, zeroed by `--no-boost`
/// (search.ts:146-149).
fn effective_boost(store: &mulch::StoreFiles, no_boost: bool) -> Result<f64, Failure> {
    if no_boost {
        return Ok(0.0);
    }
    Ok(store
        .config()
        .search()
        .map_err(|source| Failure::handled_on_stderr("search", format!("Error: {source}")))?
        .map_or(DEFAULT_SEARCH_BOOST_FACTOR, |search| search.boost_factor))
}

/// `formatArchivedSection` (search.ts:27-40).
fn archived_section(
    domain: &str,
    records: &[Value],
    registry: &mulch::Registry,
) -> Result<String, Failure> {
    let plural = if records.len() == 1 { "" } else { "s" };
    let mut lines = vec![format!(
        "## {domain} (archived, {} record{plural})",
        records.len()
    )];
    for record in records {
        let date = archived_date(record);
        let reason = mulch::truthy_str(record.get("archive_reason").and_then(Value::as_str))
            .map_or_else(String::new, |reason| format!(" {reason}"));
        let id = mulch::truthy_str(record.get("id").and_then(Value::as_str))
            .map_or_else(String::new, |id| format!("{id} "));
        let summary = archived_summary(record, registry)?;
        lines.push(format!(
            "- [ARCHIVED {date}{reason}] {id}[{}] {summary}",
            record_type(record)
        ));
    }
    Ok(lines.join("\n"))
}

/// `archived_at` sliced to its first 10 chars, `unknown` when empty
/// (search.ts:32).
fn archived_date(record: &Value) -> String {
    let raw = record
        .get("archived_at")
        .and_then(Value::as_str)
        .unwrap_or("");
    let sliced: String = raw.chars().take(10).collect();
    if sliced.is_empty() {
        "unknown".into()
    } else {
        sliced
    }
}

/// The record's type for the `[…]` bracket (JS interpolates a
/// missing type as `undefined`).
fn record_type(record: &Value) -> &str {
    record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("undefined")
}

/// The archived bullet's summary — registry-gated like the
/// reference's `getRecordSummary`, which THROWS for unregistered
/// types and fails the whole plain render (probe: an archived
/// unknown-type record matching via its tags).
fn archived_summary(record: &Value, registry: &mulch::Registry) -> Result<String, Failure> {
    let type_name = record_type(record);
    if registry.row(type_name).is_none() {
        return Err(Failure::handled_on_stderr(
            "search",
            format!("Error: Unknown record type: {type_name}"),
        ));
    }
    Ok(mulch::record_summary(registry, record))
}

/// Reads one domain's live records strictly (the global
/// unknown-type flag gates, search.ts:186).
fn read_domain(
    store: &mulch::StoreFiles,
    domain: &str,
    opts: &GlobalOpts,
) -> Result<Vec<Value>, Failure> {
    store
        .read_records(domain, opts.allow_unknown_types)
        .map(|records| records.into_iter().map(|row| row.record).collect())
        .map_err(|source| {
            Failure::handled_on_stderr("search", crate::commands::render_core_error(&source))
        })
}

/// Reads one domain's archive (hard-wired unknown-type tolerance,
/// `readArchiveFile`).
fn read_archived(store: &mulch::StoreFiles, domain: &str) -> Result<Vec<Value>, Failure> {
    store
        .read_archive(domain)
        .map(|records| records.into_iter().map(|row| row.record).collect())
        .map_err(|source| {
            Failure::handled_on_stderr("search", crate::commands::render_core_error(&source))
        })
}

/// An optional flag value under JS truthiness (`None` and `Some("")`
/// both read as unset — search.ts gates with `if (options.…)`).
fn truthy_owned(value: Option<&str>) -> Option<String> {
    mulch::truthy_str(value).map(ToString::to_string)
}

/// A domain file's mtime for the `updated …` header suffix.
fn modified(store: &mulch::StoreFiles, domain: &str) -> Result<Option<Timestamp>, Failure> {
    match store.domain_modified(domain) {
        Ok(Some(time)) => Timestamp::try_from(time)
            .map(Some)
            .map_err(|source| Failure::handled_on_stderr("search", format!("Error: {source}"))),
        Ok(None) => Ok(None),
        Err(source) => Err(Failure::handled_on_stderr(
            "search",
            format!("Error: {source}"),
        )),
    }
}

/// The four domain-section families (search.ts:271-284).
fn render_section(
    format: &str,
    registry: &mulch::Registry,
    domain: &str,
    records: &[Value],
    last_updated: Option<Timestamp>,
    now: Timestamp,
) -> String {
    match format {
        "compact" => {
            format_domain_expertise_compact(registry, domain, records, last_updated, None, now)
        }
        "xml" => format_domain_expertise_xml(registry, domain, records, last_updated, None, now),
        "plain" => {
            format_domain_expertise_plain(registry, domain, records, last_updated, None, now)
        }
        _ => format_domain_expertise(registry, domain, records, last_updated, false, None, now),
    }
}
