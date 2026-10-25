//! The failure and format surface of the parity commands
//! (mulch-3326): the unknown-domain shapes, the identifier and
//! flag-value failures, the strict-read error rendering, the raw
//! summary law, and the reference-format clock. The store seam
//! lives in [`super::store`].

use mulch::{Error, ResolveError};

use crate::output::{Failure, failure_envelope};

/// The current instant as reference-format `recorded_at`
/// (ISO-8601, millisecond precision, `Z`).
pub(crate) fn now_iso() -> String {
    jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Which unknown-domain surface a command renders — the reference
/// hand-writes a shape per command; this carries them as data.
#[derive(Clone, Copy)]
pub(crate) enum DomainFailure {
    /// Two-line plain (lowercase `domain`), one-line json (capital D).
    Standard,
    /// `move`'s single-line plain keeps the capital D.
    Move,
    /// `delete-domain`'s plain carries a Hint instead of the list.
    WithHint,
    /// query's read-family hint (query.ts:70-98): the message is the
    /// two-line hint in BOTH modes — the json surface rides the
    /// error-field envelope — and an empty store backfills `(none)`.
    QueryHint,
    /// search's hint (search.ts:153-170): the same envelope-carrying
    /// shape, but the list joins WITHOUT the `(none)` fallback (an
    /// empty store renders the trailing `Available domains: `).
    SearchHint,
    /// rank's hint (rank.ts): the json switch rides the shared handled
    /// law — json prints the bare sentence, plain the hint; the list
    /// backfills `(none)` like query's.
    RankHint,
    /// ready's rewording (ready.ts): json keeps the standard sentence,
    /// plain is the single lowercase line `domain "x" not found.
    /// Available: …` (no `in config`, no `domains`, no hint), the
    /// list without the `(none)` fallback.
    ReadyOneLine,
    /// prime's shape (prime.ts): the error-field envelope with the
    /// one-line sentence; `excluded` prefixes the subject (`Excluded
    /// domain`, the --exclude-domain walk).
    Prime {
        /// Whether the name came from the exclusion walk.
        excluded: bool,
    },
}

/// The one unknown-domain failure: the available-list join, every
/// message shape, and the two envelope laws (the shared handled
/// derivation vs the error-field envelope) live here, not in
/// per-command copies (mulch-9e70; the read family joined in
/// mulch-5366).
pub(crate) fn unknown_domain_failure(
    command: &str,
    domain: &str,
    available: &[String],
    json: bool,
    shape: DomainFailure,
) -> Failure {
    // search and ready join the list without the `(none)` backfill
    // every other shape takes (probe-pinned per-command drift).
    let bare_list = matches!(
        shape,
        DomainFailure::SearchHint | DomainFailure::ReadyOneLine
    );
    let list = if available.is_empty() && !bare_list {
        "(none)".to_string()
    } else {
        available.join(", ")
    };
    // prime's exclusion walk prefixes the subject; every other
    // shape reads `Domain`.
    let subject = match shape {
        DomainFailure::Prime { excluded: true } => "Excluded domain",
        _ => "Domain",
    };
    // The one-line capital-D sentence: json output for every
    // shape, and `move`'s plain text prefixed with `Error: `.
    let one_line = format!("{subject} \"{domain}\" not found in config. Available domains: {list}");
    // The read family's shared hint line (query, search, rank,
    // prime): it promotes THIS binary and names the config file.
    let hint = format!(
        "Hint: Run `mulch add {domain}` to create this domain, or check .mulch/mulch.config.yaml"
    );
    // The shape's plain wording (json output is the bare sentence).
    let plain = match shape {
        DomainFailure::Standard => {
            format!("Error: domain \"{domain}\" not found in config.\nAvailable domains: {list}")
        }
        DomainFailure::Move => format!("Error: {one_line}"),
        DomainFailure::WithHint => format!(
            "Error: domain \"{domain}\" not found in config.\nHint: Run `mulch add {domain}` to create it, or check `mulch status` for existing domains."
        ),
        // The read family's hint shapes share the config-file hint.
        DomainFailure::QueryHint
        | DomainFailure::SearchHint
        | DomainFailure::RankHint
        | DomainFailure::Prime { .. } => format!("Error: {one_line}\n{hint}"),
        DomainFailure::ReadyOneLine => {
            format!("Error: domain \"{domain}\" not found. Available: {list}")
        }
    };
    // The message: query and search keep the hint in BOTH modes
    // (their json surface rides the error-field envelope below, not
    // the message); prime keeps the `Error: `-prefixed sentence for
    // json (the envelope overrides the printed surface anyway); the
    // handled shapes print the bare sentence as their json output.
    let message = match shape {
        DomainFailure::QueryHint | DomainFailure::SearchHint => plain,
        DomainFailure::Prime { .. } if json => format!("Error: {one_line}"),
        _ if json => one_line.clone(),
        _ => plain,
    };
    // The error-field envelope family (query, search, prime): the
    // json surface is a failure envelope whose `error` field is the
    // sentence alone — the handled derivation would drag the plain
    // wording along.
    match shape {
        DomainFailure::QueryHint | DomainFailure::SearchHint | DomainFailure::Prime { .. } => {
            let mut failure = Failure::handled_on_stderr(command, message);
            let mut fields = serde_json::Map::new();
            fields.insert("error".into(), serde_json::Value::String(one_line));
            failure.envelope = failure_envelope(command, fields);
            failure
        }
        _ => Failure::handled_on_stderr(command, message),
    }
}

/// The shared identifier-resolution failure: the reference's
/// not-found and ambiguous texts (identical across the record
/// commands; `command` selects the envelope's command field).
pub(crate) fn resolve_failure(command: &str, error: ResolveError) -> Failure {
    match error {
        ResolveError::NotFound(identifier) => Failure::handled_on_stderr(
            command,
            format!(
                "Error: Record \"{identifier}\" not found. Run `mulch query` to see record IDs."
            ),
        ),
        ResolveError::Ambiguous {
            identifier,
            count,
            ids,
        } => Failure::handled_on_stderr(
            command,
            format!(
                "Error: Ambiguous identifier \"{identifier}\" matches {count} records: {}. Use more characters to disambiguate.",
                ids.join(", ")
            ),
        ),
    }
}

/// The strict flag-value failure (utils/numeric-flags.ts wording,
/// mulch-a878): `Error: --<flag> must be a <what> (got "<raw>").` on
/// stderr; the JSON envelope drops the `Error: ` prefix (the shared
/// handled law). One owner beside [`super::store::persist`] — rank,
/// ready, and prime carried private copies.
pub(crate) fn flag_value_failure(
    command: &'static str,
    flag: &str,
    what: &str,
    raw: &str,
) -> Failure {
    Failure::handled_on_stderr(
        command,
        format!("Error: --{flag} must be a {what} (got \"{raw}\")."),
    )
}

/// The raw summary law for user-visible surfaces (mulch-f06a): the
/// reference embeds `getRecordSummary`'s RAW value — JSON omits the
/// key when the summary field is absent and passes non-strings
/// through verbatim. Unregistered types keep the commands' `Unknown
/// record type` error; the truncating laws' TypeErrors render as a
/// clean error line (the reference crashes on the throw).
pub(crate) fn summary_raw(
    command: &str,
    registry: &mulch::Registry,
    record: &serde_json::Value,
) -> Result<Option<serde_json::Value>, Failure> {
    let type_name = record
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if registry.row(type_name).is_none() {
        return Err(Failure::handled_on_stderr(
            command,
            format!("Error: Unknown record type: {type_name}"),
        ));
    }
    mulch::record_summary_value(registry, record)
        .map_err(|error| Failure::error_line(command, error))
}

/// The plain rendering of a raw summary: `String(value)` — `None`
/// renders `undefined` like a JS template literal.
pub(crate) fn summary_text(raw: Option<&serde_json::Value>) -> String {
    raw.map_or_else(|| "undefined".into(), mulch::value_text)
}

/// Renders a format-core read error the way the reference does (the
/// malformed-line and unknown-type contracts are byte-pinned).
pub(crate) fn render_core_error(error: &Error) -> String {
    match error {
        Error::MalformedLine {
            path,
            line,
            preview,
            reason,
        } => format!(
            "Error: Malformed JSONL at {}:{line}: {reason}. Line: {preview}",
            path.display()
        ),
        Error::UnknownRecordType {
            path,
            line,
            id,
            record_type,
        } => {
            let id_part = id
                .as_ref()
                .map_or_else(String::new, |id| format!(" (id={id})"));
            format!(
                "Error: Unknown record type \"{record_type}\" at {}:{line}{id_part}. Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.",
                path.display()
            )
        }
        other => format!("Error: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_render_the_reference_surfaces() {
        let available = vec!["d".to_string()];
        let standard =
            unknown_domain_failure("delete", "x", &available, false, DomainFailure::Standard);
        assert_eq!(
            standard.message,
            "Error: domain \"x\" not found in config.\nAvailable domains: d"
        );
        let json = unknown_domain_failure("delete", "x", &available, true, DomainFailure::Standard);
        assert_eq!(
            json.message,
            "Domain \"x\" not found in config. Available domains: d"
        );
        let moved = unknown_domain_failure("move", "x", &available, false, DomainFailure::Move);
        assert_eq!(
            moved.message,
            "Error: Domain \"x\" not found in config. Available domains: d"
        );
        let hint = unknown_domain_failure(
            "delete-domain",
            "x",
            &available,
            false,
            DomainFailure::WithHint,
        );
        assert_eq!(
            hint.message,
            "Error: domain \"x\" not found in config.\nHint: Run `mulch add x` to create it, or check `mulch status` for existing domains."
        );
        let none = unknown_domain_failure("delete", "x", &[], false, DomainFailure::Standard);
        assert!(none.message.ends_with("Available domains: (none)"));
        let query =
            unknown_domain_failure("query", "x", &available, false, DomainFailure::QueryHint);
        assert_eq!(
            query.message,
            "Error: Domain \"x\" not found in config. Available domains: d\n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        assert_eq!(
            query.envelope["error"],
            "Domain \"x\" not found in config. Available domains: d"
        );
        assert!(query.envelope_to_stderr);
        // search joins the list without the `(none)` backfill.
        let search = unknown_domain_failure("search", "x", &[], false, DomainFailure::SearchHint);
        assert_eq!(
            search.message,
            "Error: Domain \"x\" not found in config. Available domains: \n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        let rank = unknown_domain_failure("rank", "x", &available, false, DomainFailure::RankHint);
        assert_eq!(
            rank.message,
            "Error: Domain \"x\" not found in config. Available domains: d\n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        let rank_json =
            unknown_domain_failure("rank", "x", &available, true, DomainFailure::RankHint);
        assert_eq!(
            rank_json.message,
            "Domain \"x\" not found in config. Available domains: d"
        );
        let ready =
            unknown_domain_failure("ready", "x", &available, false, DomainFailure::ReadyOneLine);
        assert_eq!(ready.message, "Error: domain \"x\" not found. Available: d");
        let ready_empty =
            unknown_domain_failure("ready", "x", &[], false, DomainFailure::ReadyOneLine);
        assert!(ready_empty.message.ends_with("Available: "));
        let prime = unknown_domain_failure("prime", "x", &available, false, DomainFailure::Prime {
            excluded: false,
        });
        assert_eq!(
            prime.message,
            "Error: Domain \"x\" not found in config. Available domains: d\n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        let excluded =
            unknown_domain_failure("prime", "x", &available, false, DomainFailure::Prime {
                excluded: true,
            });
        assert_eq!(
            excluded.message,
            "Error: Excluded domain \"x\" not found in config. Available domains: d\n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        assert_eq!(
            excluded.envelope["error"],
            "Excluded domain \"x\" not found in config. Available domains: d"
        );
    }
}
