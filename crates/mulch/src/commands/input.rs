//! The CLI input adapters of the parity commands (mulch-3326):
//! the payload<->flag table `record` and `edit` read through, and
//! the stdin confirmation prompt. Pure input-side concerns — the
//! store seam lives in [`super::store`], the failure shapes in
//! [`super::failure`].

use mulch::{js_string, truthy_str};

use crate::cli::{EditArgs, RecordArgs};

/// The payload<->flag adapter (mulch-4ee2): the one owner of the six
/// registry payload fields' CLI flag sources. `record` and `edit`
/// read payload fields through [`PayloadFlags::get`], so the
/// field->flag mapping lives in one table instead of one hand-copy
/// per command.
///
/// Two views, two laws (both reference-pinned):
/// [`get`](PayloadFlags::get) is the raw flag view — edit stores
/// whatever was flagged, `Some("")` included (`value === undefined`
/// is edit's only gate). [`resolved`](PayloadFlags::resolved) is
/// record's `collectField` value: the flag when given (even empty),
/// else the positional `[content]` feeding the type's fallback field
/// (convention→content, pattern/reference/guide→description,
/// failure/decision→dropped — `positionalFallbackField`,
/// mulch-e2cf); record then treats empty strings as missing
/// (`value === undefined || value === ""`, mulch-1209). The retry
/// echo (`buildRetryCommand`) reads flag values alone — the
/// positional renders as a bare quoted argument, never as a pair.
pub(crate) struct PayloadFlags<'a> {
    /// (payload field, provided flag value) rows — raw flag values,
    /// `Some("")` included. `files` rides along: not a payload field,
    /// but a declared flag the row loops read when the type declares
    /// it (the reference's option key space, mulch-d524).
    table:      [(&'static str, Option<&'a str>); 7],
    /// Custom-type field flags (field name with underscores, the
    /// parsed `--<field>` value) — the registry-fed extension of the
    /// builtin table, one flat map across customs like the
    /// reference's option key space (mulch-d524).
    custom:     &'a [(String, String)],
    /// `record`'s positional `[content]` argument.
    positional: Option<&'a str>,
    /// The type's positional fallback field (record path only).
    fallback:   Option<&'static str>,
}

impl<'a> PayloadFlags<'a> {
    /// `record`'s rows: raw flag values; the positional fallback
    /// resolution happens in [`resolved`](PayloadFlags::resolved).
    pub(crate) fn from_record(args: &'a RecordArgs, record_type: &str) -> Self {
        Self::new(
            [
                ("content", args.content_flag.as_deref()),
                ("name", args.name.as_deref()),
                ("description", args.description.as_deref()),
                ("resolution", args.resolution.as_deref()),
                ("title", args.title.as_deref()),
                ("rationale", args.rationale.as_deref()),
                ("files", args.files.as_deref()),
            ],
            &args.custom,
            args.content.as_deref(),
            // The positional drops for the two flag-only builtins and
            // for EVERY custom (positionalFallbackField: builtin-only,
            // null for `kind !== "builtin"` — probed, mulch-d524). A
            // NEW REGISTRY BUILTIN lands in the named arms below; the
            // registry-shape test in ids.rs forces that revisit.
            match record_type {
                "convention" => Some("content"),
                "pattern" | "reference" | "guide" => Some("description"),
                _ => None,
            },
        )
    }

    /// `edit`'s rows: content is the `--content` flag alone.
    pub(crate) fn from_edit(args: &'a EditArgs) -> Self {
        Self::new(
            [
                ("content", args.content.as_deref()),
                ("name", args.name.as_deref()),
                ("description", args.description.as_deref()),
                ("resolution", args.resolution.as_deref()),
                ("title", args.title.as_deref()),
                ("rationale", args.rationale.as_deref()),
                ("files", args.files.as_deref()),
            ],
            &args.custom,
            None,
            None,
        )
    }

    /// The shared row constructor (the two commands differ in the
    /// positional pair alone).
    fn new(
        table: [(&'static str, Option<&'a str>); 7],
        custom: &'a [(String, String)],
        positional: Option<&'a str>,
        fallback: Option<&'static str>,
    ) -> Self {
        Self {
            table,
            custom,
            positional,
            fallback,
        }
    }

    /// The provided flag value for a payload field, if any — the raw
    /// flag view (edit's law: store what was flagged). Custom-type
    /// fields read the dynamic flag table (a field overlapping a
    /// builtin flag name reads the builtin's row — the reference's
    /// declaredOptionNames skip).
    pub(crate) fn get(&self, field: &str) -> Option<&'a str> {
        self.table
            .iter()
            .find(|(name, _)| *name == field)
            .and_then(|(_, value)| *value)
            .or_else(|| {
                self.custom
                    .iter()
                    .find(|(name, _)| name == field)
                    .map(|(_, value)| value.as_str())
            })
    }

    /// The record-path `collectField` value: the flag when given,
    /// else the positional feeding the type's fallback field. Empty
    /// strings survive here — record's missing/drop check filters
    /// them (mulch-1209).
    pub(crate) fn resolved(&self, field: &str) -> Option<&'a str> {
        match self.fallback {
            Some(fallback) if fallback == field => self.get(field).or(self.positional),
            _ => self.get(field),
        }
    }

    /// The retry-echo pair `--{field} "<value>"` of a provided,
    /// non-empty flag (buildRetryCommand echoes flag values alone,
    /// JSON.stringify-quoted).
    pub(crate) fn retry_pair(&self, field: &str) -> Option<String> {
        truthy_str(self.get(field)).map(|value| format!("--{field} {}", js_string(value)))
    }

    /// The table's field universe (the pin surface in
    /// [`payload_flags_tests`]).
    #[cfg(test)]
    pub(crate) fn fields(&self) -> impl Iterator<Item = &'static str> {
        self.table.iter().map(|(name, _)| *name)
    }
}

/// Reads a one-line answer from stdin (the delete-domain prompt).
pub(crate) fn read_confirmation() -> std::io::Result<String> {
    use std::io::BufRead as _;
    let mut buffer = String::new();
    std::io::stdin().lock().read_line(&mut buffer)?;
    Ok(buffer)
}

#[cfg(test)]
mod payload_flags_tests {
    use super::*;

    fn record_args(args: &[&str]) -> RecordArgs {
        use clap::Parser as _;
        let cli = crate::cli::Cli::parse_from(std::iter::once("mulch").chain(args.iter().copied()));
        let Some(crate::cli::Command::Record(args)) = cli.command else {
            panic!("test args must parse as a record invocation");
        };
        *args
    }

    fn edit_args(args: &[&str]) -> EditArgs {
        use clap::Parser as _;
        let cli = crate::cli::Cli::parse_from(std::iter::once("mulch").chain(args.iter().copied()));
        let Some(crate::cli::Command::Edit(args)) = cli.command else {
            panic!("test args must parse as an edit invocation");
        };
        *args
    }

    fn record_args_all_set() -> RecordArgs {
        record_args(&[
            "record",
            "d",
            "--content",
            "content",
            "--name",
            "name",
            "--description",
            "description",
            "--resolution",
            "resolution",
            "--title",
            "title",
            "--rationale",
            "rationale",
        ])
    }

    /// The adapter is the one flag-table owner: its universe is the
    /// registry's payload universe plus `files` (the declared flag
    /// the extracts-files row loop reads; the merged mulch-a3de pin —
    /// a registry row gaining a field without adapter plumbing fails
    /// here, and so does a table row the registry dropped).
    #[test]
    fn table_universe_is_the_registry_payload_universe() {
        let mut universe: Vec<&str> = mulch::REGISTRY
            .iter()
            .flat_map(|spec| spec.payload.iter().copied())
            .chain(["files"])
            .collect();
        universe.sort_unstable();
        universe.dedup();
        let args = record_args_all_set();
        let flags = PayloadFlags::from_record(&args, "pattern");
        let mut table: Vec<&str> = flags.fields().collect();
        table.sort_unstable();
        table.dedup();
        assert_eq!(
            table, universe,
            "a registry payload field has no flag plumbing"
        );
    }

    /// Every registry payload field resolves to its own flag value
    /// through both constructors (the record and edit rows stay the
    /// same table; unknown fields read as absent).
    #[test]
    fn registry_fields_reach_their_flag_values() {
        let record_args = record_args_all_set();
        let record_flags = PayloadFlags::from_record(&record_args, "pattern");
        let edit_args = edit_args(&[
            "edit",
            "d",
            "mx-x",
            "--content",
            "content",
            "--name",
            "name",
            "--description",
            "description",
            "--resolution",
            "resolution",
            "--title",
            "title",
            "--rationale",
            "rationale",
        ]);
        let edit_flags = PayloadFlags::from_edit(&edit_args);
        for spec in &mulch::REGISTRY {
            for field in spec.payload {
                assert_eq!(record_flags.get(field), Some(*field));
                assert_eq!(edit_flags.get(field), Some(*field));
            }
        }
        assert_eq!(record_flags.get("date"), None);
    }

    /// `record`'s content law: `--content` wins over the positional
    /// shorthand — in the RESOLVED view (collectField reads the flag
    /// first, even when empty; the raw view never sees the
    /// positional). An empty flag value survives resolution and
    /// becomes missing at record's non-empty gate (mulch-1209).
    #[test]
    fn record_content_flag_wins_over_the_positional() {
        let both = record_args(&["record", "d", "positional", "--content", "flag"]);
        let flags = PayloadFlags::from_record(&both, "convention");
        assert_eq!(flags.get("content"), Some("flag"));
        assert_eq!(flags.resolved("content"), Some("flag"));

        let positional = record_args(&["record", "d", "positional"]);
        let flags = PayloadFlags::from_record(&positional, "convention");
        assert_eq!(flags.get("content"), None);
        assert_eq!(flags.resolved("content"), Some("positional"));

        let empty_flag = record_args(&["record", "d", "positional", "--content", ""]);
        let flags = PayloadFlags::from_record(&empty_flag, "convention");
        assert_eq!(flags.get("content"), Some(""));
        assert_eq!(flags.resolved("content"), Some(""));
    }

    /// The positional fallback is type-specific (reference
    /// `positionalFallbackField`, mulch-e2cf): convention keys on
    /// content, pattern/reference/guide on description, and
    /// failure/decision drop the positional entirely.
    #[test]
    fn positional_fallback_follows_the_record_type() {
        let positional = record_args(&["record", "d", "content-x"]);
        for record_type in ["convention", "pattern", "reference", "guide"] {
            let flags = PayloadFlags::from_record(&positional, record_type);
            let fallback = if record_type == "convention" {
                "content"
            } else {
                "description"
            };
            assert_eq!(flags.resolved(fallback), Some("content-x"));
        }
        for record_type in ["failure", "decision"] {
            let flags = PayloadFlags::from_record(&positional, record_type);
            assert_eq!(flags.resolved("content"), None);
            assert_eq!(flags.resolved("description"), None);
        }
    }

    /// The retry-echo pair shape (the missing-flags failure's
    /// `Retry:` line): `--{field} "<value>"` JSON.stringify-quoted,
    /// absent without a value or with an empty one (buildRetryCommand
    /// reads flag values alone, JS truthiness).
    #[test]
    fn retry_pairs_render_the_reference_echo_shape() {
        let args = record_args(&["record", "d", "--name", "n v"]);
        let flags = PayloadFlags::from_record(&args, "pattern");
        assert_eq!(flags.retry_pair("name"), Some("--name \"n v\"".to_string()));
        assert_eq!(flags.retry_pair("description"), None);

        let quoted = record_args(&["record", "d", "--name", "a\"b"]);
        let flags = PayloadFlags::from_record(&quoted, "pattern");
        assert_eq!(
            flags.retry_pair("name"),
            Some("--name \"a\\\"b\"".to_string())
        );

        let empty = record_args(&["record", "d", "--name", ""]);
        let flags = PayloadFlags::from_record(&empty, "pattern");
        assert_eq!(flags.retry_pair("name"), None);
    }

    /// `edit`'s law is the raw flag view: an empty value is a
    /// provided value and stores (edit.ts `value === undefined` is
    /// its only gate) — and the positional fallback never applies.
    #[test]
    fn edit_reads_the_raw_flag_view_and_stores_empty_values() {
        let args = edit_args(&["edit", "d", "mx-x", "--name", ""]);
        let flags = PayloadFlags::from_edit(&args);
        assert_eq!(flags.get("name"), Some(""));
        assert_eq!(flags.resolved("name"), Some(""));
    }
}
