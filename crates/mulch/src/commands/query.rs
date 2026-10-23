//! `mulch query` — filtered record listing across domains (mulch-16da).
//!
//! Probed law (ml 0.10.7, 2026-10-23, /tmp/opencode/childA):
//! - `--all` wins over the `[domain]` positional (query.ts:59-86); an empty
//!   `--all` store prints the no-domains line (the same bytes `status` prints)
//!   with exit 0, before any file is read.
//! - The command's own five-choice `--format` declaration (query.ts: 44-51) is
//!   DEAD: commander matches the token against the GLOBAL four-choice option in
//!   both flag positions, so `--format ids` rejects everywhere and the ids
//!   render path (query.ts:131-159) is unreachable — the four families arrive
//!   through the global flag alone (cli.rs).
//! - Empty-string flags are JS-falsy: `--type ""` filters nothing, and `query
//!   ""` is the please-specify error.
//! - The no-store wording is the readConfig throw ("to set up this project"),
//!   not the catch's ENOENT branch — the thrown error carries no `code`, so the
//!   generic arm renders it.
//! - The unknown-domain hint keeps the reference's `ml add` (the reference
//!   drifts per command — search says `mulch add`; the promote law covers the
//!   Retry echo only, README DEVIATIONS).

use jiff::Timestamp;
use mulch::{DomainSpec, RecordFilter};
use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, QueryArgs};
use crate::commands::open_store;
use crate::commands::render::{
    format_domain_expertise, format_domain_expertise_compact, format_domain_expertise_plain,
    format_domain_expertise_xml,
};
use crate::output::{
    EXIT_ERROR, Failure, failure_envelope, print_json, print_line, success_envelope,
};

/// Runs `query` against the store at the current directory.
pub(super) fn run(opts: &GlobalOpts, args: &QueryArgs) -> Result<(), Failure> {
    let store = open_store("query", true)?;
    let configured = store.domains();

    // query.ts order: --all first (a positional beside it is
    // ignored), then the positional, then the usage error.
    let domains = if args.all {
        if configured.is_empty() {
            if opts.json {
                let mut fields = Map::new();
                fields.insert("domains".into(), Value::Array(Vec::new()));
                print_json(&success_envelope("query", fields), false);
            } else {
                print_line(
                    false,
                    "No domains configured. Run `ml add <domain>` to get started.",
                );
            }
            return Ok(());
        }
        configured
    } else if let Some(domain) = mulch::truthy_str(args.domain.as_deref()) {
        mulch::domain_set(&configured, &DomainSpec::One(domain.to_string()))
            .map_err(|unknown| unknown_domain_failure(&unknown.domain, &unknown.available))?
    } else {
        return Err(Failure::handled_on_stderr(
            "query",
            "Error: Please specify a domain or use --all to query all domains.",
        ));
    };

    let filter = RecordFilter {
        type_:          truthy_owned(args.record_type.as_deref()),
        tag:            None,
        classification: truthy_owned(args.classification.as_deref()),
        file:           truthy_owned(args.file.as_deref()),
        outcome_status: truthy_owned(args.outcome_status.as_deref()),
    };

    if opts.json {
        let mut rows = Vec::new();
        for domain in &domains {
            let records = filtered_records(&store, domain, &filter, opts, args.sort_by_score)?;
            let mut row = Map::new();
            row.insert("domain".into(), Value::String(domain.clone()));
            row.insert("records".into(), Value::Array(records));
            rows.push(Value::Object(row));
        }
        let mut fields = Map::new();
        fields.insert("domains".into(), Value::Array(rows));
        print_json(&success_envelope("query", fields), false);
    } else {
        let now = Timestamp::now();
        let mut sections = Vec::new();
        for domain in &domains {
            let records = filtered_records(&store, domain, &filter, opts, args.sort_by_score)?;
            let last_updated = modified(&store, domain)?;
            sections.push(render_section(
                opts.format.as_deref().unwrap_or("markdown"),
                store.registry(),
                domain,
                &records,
                last_updated,
                now,
            ));
        }
        // `--quiet` does not suppress query output (reference quirk).
        print_line(false, &sections.join("\n\n"));
    }
    Ok(())
}

/// The unknown-domain failure (query.ts:70-98): the one-line
/// capital-D sentence for json, plus the two-line hint for plain.
/// Unlike search, the available-list join falls back to `(none)`.
fn unknown_domain_failure(domain: &str, available: &[String]) -> Failure {
    let list = if available.is_empty() {
        "(none)".to_string()
    } else {
        available.join(", ")
    };
    let one_line = format!("Domain \"{domain}\" not found in config. Available domains: {list}");
    let mut fields = Map::new();
    fields.insert("error".into(), Value::String(one_line.clone()));
    Failure {
        message:            format!(
            "Error: {one_line}\nHint: Run `ml add {domain}` to create this domain, or check .mulch/mulch.config.yaml"
        ),
        code:               EXIT_ERROR,
        envelope:           failure_envelope("query", fields),
        envelope_to_stderr: true,
        rendered:           false,
    }
}

/// An optional flag value under JS truthiness: `None` and `Some("")`
/// both read as unset (query.ts gates every filter with
/// `if (options.…)`).
fn truthy_owned(value: Option<&str>) -> Option<String> {
    mulch::truthy_str(value).map(ToString::to_string)
}

/// Reads one domain strictly, applies the filter chain, then the
/// optional confirmation-score sort (query.ts:104-121).
fn filtered_records(
    store: &mulch::StoreFiles,
    domain: &str,
    filter: &RecordFilter,
    opts: &GlobalOpts,
    sort_by_score: bool,
) -> Result<Vec<Value>, Failure> {
    let records = store
        .read_records(domain, opts.allow_unknown_types)
        .map_err(|source| {
            Failure::handled_on_stderr("query", crate::commands::render_core_error(&source))
        })?;
    let mut records = filter.apply(records.into_iter().map(|row| row.record).collect());
    if sort_by_score {
        mulch::scoring::sort_by_confirmation(&mut records);
    }
    Ok(records)
}

/// A domain file's mtime for the `updated …` header suffix
/// (`getFileModTime`; `None` when the file does not exist).
fn modified(store: &mulch::StoreFiles, domain: &str) -> Result<Option<Timestamp>, Failure> {
    match store.domain_modified(domain) {
        Ok(Some(time)) => Timestamp::try_from(time)
            .map(Some)
            .map_err(|source| Failure::handled_on_stderr("query", format!("Error: {source}"))),
        Ok(None) => Ok(None),
        Err(source) => Err(Failure::handled_on_stderr(
            "query",
            format!("Error: {source}"),
        )),
    }
}

/// The four domain-section families (query.ts:183-196); the default
/// arm is the markdown family.
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
