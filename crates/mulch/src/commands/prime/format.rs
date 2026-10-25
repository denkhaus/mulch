//! Prime's text envelopes: the four full-mode wrappers, the manifest
//! families (plus the JSON payload), the project-contract block, and
//! the session-close footer (mulch-16da).
//!
//! Every static line is promoted through [`promote_mulch`] (user
//! directive 2026-10-13: the shipped binary carries no `ml`
//! references). Record-bearing sections are inserted UNPROMOTED —
//! record content is user data and must never be rewritten.
//!
//! Probed byte-exactly against `ml` 0.10.7 (2026-10-23,
//! /tmp/sprint35b/probes).

use jiff::Timestamp;
use serde_json::{Map, Value};

use crate::commands::render::{promote_mulch, time_ago, xml_attr_escape, xml_escape};

/// One manifest row: a configured domain's unfiltered counts.
#[derive(Clone, Debug)]
pub(crate) struct ManifestDomain {
    pub(crate) domain:       String,
    pub(crate) count:        usize,
    pub(crate) last_updated: Option<Timestamp>,
    /// First-seen type → count (`computeTypeCounts`).
    pub(crate) type_counts:  Vec<(String, u64)>,
}

/// The builtin order of [`format_type_counts`] (reference
/// `TYPE_COUNT_ORDER`).
const TYPE_COUNT_ORDER: [&str; 6] = [
    "pattern",
    "convention",
    "failure",
    "decision",
    "reference",
    "guide",
];

/// The manifest quick-reference rows (reference `MANIFEST_QUICK_REF`).
const MANIFEST_QUICK_REF: [(&str, &str); 6] = [
    ("ml prime <domain>", "load full records for one domain"),
    (
        "ml prime --files <path>",
        "load records relevant to specific files",
    ),
    ("ml search \"<query>\"", "search records across domains"),
    (
        "ml record <domain> --type <type> --description \"...\"",
        "store an insight",
    ),
    ("ml learn", "discover what to record from changed files"),
    ("ml sync", "validate, stage, and commit .mulch/ changes"),
];

/// Promotes a static line list and joins it with newlines.
fn promoted(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|line| promote_mulch(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Assembles `before + middle + after` where only `middle` (record
/// sections) stays unpromoted.
fn assemble(before: &[&str], middle: &str, after: &[&str]) -> String {
    let mut out = String::new();
    if !before.is_empty() {
        out.push_str(&promoted(before));
        out.push('\n');
    }
    out.push_str(middle);
    if !after.is_empty() {
        out.push('\n');
        out.push_str(&promoted(after));
    }
    out
}

// ---- the four full-mode envelopes (format.ts) ----

/// The full-mode envelope TABLE (format.ts): keyed by the same
/// [`Format`] the section table reads, parsed once at the command
/// edge — the last string dispatch of the family dies (mulch-1c32).
pub(crate) fn prime_output(format: crate::commands::render::Format, sections: &[String]) -> String {
    use crate::commands::render::Format;
    match format {
        Format::Xml => prime_output_xml(sections),
        Format::Plain => prime_output_plain(sections),
        Format::Compact => prime_output_compact(sections),
        Format::Markdown => prime_output_markdown(sections),
    }
}

/// `formatPrimeOutputCompact`.
pub(crate) fn prime_output_compact(sections: &[String]) -> String {
    let empty = "No expertise recorded yet. Use `ml add <domain>` to create a domain, then `ml record` to add records.";
    let middle = if sections.is_empty() {
        promote_mulch(empty)
    } else {
        sections.join("\n\n")
    };
    let after = [
        "",
        "## Quick Reference",
        "",
        "- `ml search \"query\"` — find relevant records before implementing",
        "- `ml prime --files src/foo.ts` — prime **before** editing a file, not just at session start",
        "- `ml prime --context` — load records for git-changed files",
        "- `ml record <domain> --type <type> --description \"...\"`",
        "  - Evidence: commit + files auto-populate from git. Trackers: `--evidence-seeds` / `--evidence-gh` / `--evidence-linear` / `--evidence-bead`. Override commit: `--evidence-commit <sha>`. Link records: `--relates-to <mx-id>`",
        "- `ml doctor` — check record health",
        "",
        "**Record types and required flags:**",
        "",
        "| Type | Required flags |",
        "|------|----------------|",
        "| `convention` | `\"<content>\"` (positional) |",
        "| `pattern` | `--name \"...\" --description \"...\"` |",
        "| `failure` | `--description \"...\" --resolution \"...\"` |",
        "| `decision` | `--title \"...\" --rationale \"...\"` |",
        "| `reference` | `--name \"...\" --description \"...\"` |",
        "| `guide` | `--name \"...\" --description \"...\"` |",
    ];
    assemble(&["# Project Expertise (via Mulch)", ""], &middle, &after)
}

/// `formatPrimeOutput` (markdown, full details).
pub(crate) fn prime_output_markdown(sections: &[String]) -> String {
    let empty = "No expertise recorded yet. Use `ml add <domain>` to create a domain, then `ml record` to add records.";
    let middle = if sections.is_empty() {
        promote_mulch(empty)
    } else {
        sections.join("\n\n")
    };
    let before = [
        "# Project Expertise (via Mulch)",
        "",
        "> **Context Recovery**: Run `ml prime` after compaction, clear, or new session",
        "",
        "## Rules",
        "",
        "- **Record learnings**: When you discover a pattern, fix a bug, or make a design decision — record it with `ml record`",
        "- **Check expertise first**: Before implementing, check if relevant expertise exists with `ml search` or `ml prime --context`",
        "- **Targeted priming**: Use `ml prime --files src/foo.ts` to load only records relevant to specific files",
        "- **Do NOT** store expertise in code comments, markdown files, or memory tools — use `ml record`",
        "- Run `ml doctor` if you are unsure whether records are healthy",
        "",
    ];
    let after = [
        "",
        "",
        "## Recording New Learnings",
        "",
        "When you discover a pattern, convention, failure, or make an architectural decision:",
        "",
        "```bash",
        "ml record <domain> --type convention \"description\"",
        "ml record <domain> --type failure --description \"...\" --resolution \"...\"",
        "ml record <domain> --type decision --title \"...\" --rationale \"...\"",
        "ml record <domain> --type pattern --name \"...\" --description \"...\" --files \"...\"",
        "ml record <domain> --type reference --name \"...\" --description \"...\" --files \"...\"",
        "ml record <domain> --type guide --name \"...\" --description \"...\"",
        "```",
        "",
        "**Required fields by type:**",
        "",
        "| Type | Required flags |",
        "|------|----------------|",
        "| `convention` | `\"<content>\"` (positional) |",
        "| `pattern` | `--name \"...\" --description \"...\"` |",
        "| `failure` | `--description \"...\" --resolution \"...\"` |",
        "| `decision` | `--title \"...\" --rationale \"...\"` |",
        "| `reference` | `--name \"...\" --description \"...\"` |",
        "| `guide` | `--name \"...\" --description \"...\"` |",
        "",
        "**Link evidence** to records. The current commit and changed files auto-populate from git; link trackers or related records explicitly:",
        "",
        "```bash",
        "ml record <domain> --type pattern --name \"...\" --description \"...\" --evidence-seeds SEED-123",
        "ml record <domain> --type decision --title \"...\" --rationale \"...\" --evidence-gh 42 --evidence-linear ENG-9",
        "ml record <domain> --type convention \"...\" --relates-to mx-abc  # link to related records",
        "```",
        "",
        "**Batch record** multiple records at once:",
        "",
        "```bash",
        "ml record <domain> --batch records.json  # from file",
        "echo '[{\"type\":\"convention\",\"content\":\"...\"}]' | ml record <domain> --stdin  # from stdin",
        "```",
        "",
        "## Searching Expertise",
        "",
        "Use `ml search` to find relevant records across all domains. Results are ranked by relevance (BM25):",
        "",
        "```bash",
        "ml search \"file locking\"              # multi-word queries ranked by relevance",
        "ml search \"atomic\" --domain cli        # limit to a specific domain",
        "ml search \"ESM\" --type convention      # filter by record type",
        "ml search \"concurrency\" --tag safety   # filter by tag",
        "```",
        "",
        "Search before implementing — existing expertise may already cover your use case.",
        "",
        "## Domain Maintenance",
        "",
        "When a domain grows large, compact it to keep expertise focused:",
        "",
        "```bash",
        "ml compact --auto --dry-run     # preview what would be merged",
        "ml compact --auto               # merge same-type record groups",
        "```",
        "",
        "Use `ml diff` to review what expertise changed:",
        "",
        "```bash",
        "ml diff HEAD~3                  # see record changes over last 3 commits",
        "```",
        "",
        "## Session End",
        "",
        "**IMPORTANT**: Before ending your session, record what you learned and sync:",
        "",
        "```",
        "[ ] ml learn          # see what files changed — decide what to record",
        "[ ] ml record ...     # record learnings (see above)",
        "[ ] ml sync           # validate, stage, and commit .mulch/ changes",
        "```",
        "",
        "Do NOT skip this. Unrecorded learnings are lost for the next session.",
    ];
    assemble(&before, &middle, &after)
}

/// `formatPrimeOutputXml`.
pub(crate) fn prime_output_xml(sections: &[String]) -> String {
    let empty =
        "  <empty>No expertise recorded yet. Use ml add and ml record to get started.</empty>";
    let middle = if sections.is_empty() {
        promote_mulch(empty)
    } else {
        sections.join("\n")
    };
    assemble(&["<expertise>"], &middle, &["</expertise>"])
}

/// `formatPrimeOutputPlain`: the spawn-injection contract — domain
/// sections only, no envelope.
pub(crate) fn prime_output_plain(sections: &[String]) -> String {
    if sections.is_empty() {
        return promote_mulch(
            "No expertise recorded yet. Use `ml add <domain>` and `ml record` to get started.",
        );
    }
    sections.join("\n\n")
}

// ---- manifest families ----

/// `pluralize`: naive `+s` (a custom type name pluralizes as
/// `<name>s`).
fn pluralize(n: u64, singular: &str) -> String {
    if n == 1 {
        singular.to_string()
    } else {
        format!("{singular}s")
    }
}

/// `formatTypeCounts`: builtins in canonical order, then any extra
/// types in first-seen order.
fn format_type_counts(counts: &[(String, u64)]) -> String {
    let mut parts = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for name in TYPE_COUNT_ORDER {
        if let Some((_, n @ 1..)) = counts.iter().find(|(key, _)| key == name) {
            parts.push(format!("{n} {}", pluralize(*n, name)));
        }
        seen.push(name);
    }
    for (name, n) in counts {
        if seen.contains(&name.as_str()) || *n == 0 {
            continue;
        }
        parts.push(format!("{n} {}", pluralize(*n, name)));
    }
    parts.join(", ")
}

/// `manifestStatusSuffix` — the governance thresholds.
fn manifest_status_suffix(count: usize, governance: &mulch::Governance) -> String {
    let count = u64::try_from(count).unwrap_or(u64::MAX);
    match governance.tier(count) {
        mulch::Tier::Hard => " ⚠ OVER HARD LIMIT — must decompose".into(),
        mulch::Tier::Warn => " ⚠ consider splitting domain".into(),
        mulch::Tier::Approaching => " — approaching limit".into(),
        mulch::Tier::Ok => String::new(),
    }
}

/// `formatPrimeManifest` (markdown is the default family).
pub(crate) fn manifest_output(
    domains: &[ManifestDomain],
    governance: &mulch::Governance,
    format: crate::commands::render::Format,
    now: Timestamp,
) -> String {
    use crate::commands::render::Format;
    let mut lines: Vec<String> = match format {
        Format::Xml => vec!["<manifest>".into()],
        Format::Plain => vec![
            "Project Expertise Manifest (via Mulch)".into(),
            "======================================".into(),
        ],
        Format::Markdown | Format::Compact => {
            vec!["# Project Expertise Manifest (via Mulch)".into()]
        }
    };
    match format {
        Format::Xml => {
            lines.push("  <description>Manifest mode lists available domains. Load records on demand with `ml prime &lt;domain&gt;` or `ml prime --files &lt;path&gt;`.</description>".into());
            lines.push("  <quick_reference>".into());
            for (command, description) in MANIFEST_QUICK_REF {
                lines.push(format!(
                    "    <command name=\"{}\">{}</command>",
                    xml_attr_escape(&promote_mulch(command)),
                    xml_escape(&promote_mulch(description))
                ));
            }
            lines.push("  </quick_reference>".into());
            lines.push("  <domains>".into());
            for domain in domains {
                let updated = domain.last_updated.map_or_else(String::new, |then| {
                    format!(" updated=\"{}\"", time_ago(then, now))
                });
                let status = manifest_status_suffix(domain.count, governance);
                let status_attr = if status.trim().is_empty() {
                    String::new()
                } else {
                    format!(" status=\"{}\"", xml_attr_escape(status.trim()))
                };
                lines.push(format!(
                    "    <domain name=\"{}\" entries=\"{}\"{updated}{status_attr}>",
                    xml_attr_escape(&domain.domain),
                    domain.count
                ));
                for name in TYPE_COUNT_ORDER {
                    if let Some((_, n @ 1..)) =
                        domain.type_counts.iter().find(|(key, _)| key == name)
                    {
                        lines.push(format!(
                            "      <type_count type=\"{name}\" count=\"{n}\" />"
                        ));
                    }
                }
                lines.push("    </domain>".into());
            }
            lines.push("  </domains>".into());
            lines.push("</manifest>".into());
        }
        Format::Plain => {
            lines.push(String::new());
            lines.push(
                "Manifest mode lists available domains. Load records on demand with `ml prime <domain>` or `ml prime --files <path>`."
                    .into(),
            );
            lines.push(String::new());
            lines.push("Quick Reference:".into());
            for (command, description) in MANIFEST_QUICK_REF {
                lines.push(format!("  - {command} — {description}"));
            }
            lines.push(String::new());
            lines.push("Available Domains:".into());
            if domains.is_empty() {
                lines.push(
                    "  No expertise recorded yet. Use `ml add <domain>` and `ml record` to get started."
                        .into(),
                );
            } else {
                for domain in domains {
                    lines.push(format!(
                        "  - {}: {}",
                        domain.domain,
                        manifest_tail(domain, governance, now)
                    ));
                }
            }
        }
        Format::Markdown | Format::Compact => {
            lines.push(String::new());
            lines.push(
                "> Manifest mode lists available domains. Load records on demand with `ml prime <domain>` or `ml prime --files <path>`."
                    .into(),
            );
            lines.push(String::new());
            lines.push("## Quick Reference".into());
            lines.push(String::new());
            for (command, description) in MANIFEST_QUICK_REF {
                lines.push(format!("- `{command}` — {description}"));
            }
            lines.push(String::new());
            lines.push("## Available Domains".into());
            lines.push(String::new());
            if domains.is_empty() {
                lines.push(
                    "No expertise recorded yet. Use `ml add <domain>` to create a domain, then `ml record` to add records."
                        .into(),
                );
            } else {
                for domain in domains {
                    // The markdown family bolds the domain name only.
                    lines.push(format!(
                        "- **{}**: {}",
                        domain.domain,
                        manifest_tail(domain, governance, now)
                    ));
                }
            }
        }
    }
    let text = lines.join("\n");
    // Manifest text carries no record content, so promoting the whole
    // body is safe.
    promote_mulch(&text)
}

/// The shared `: N records (…) — updated … status` tail of the
/// markdown/plain manifest lines (the markdown family bolds the
/// domain name around it).
fn manifest_tail(
    domain: &ManifestDomain,
    governance: &mulch::Governance,
    now: Timestamp,
) -> String {
    let types = format_type_counts(&domain.type_counts);
    let type_suffix = if types.is_empty() {
        String::new()
    } else {
        format!(" ({types})")
    };
    let updated = domain.last_updated.map_or_else(String::new, |then| {
        format!(" — updated {}", time_ago(then, now))
    });
    let status = manifest_status_suffix(domain.count, governance);
    let records = pluralize(u64::try_from(domain.count).unwrap_or(u64::MAX), "record");
    format!("{} {records}{type_suffix}{updated}{status}", domain.count)
}

/// `buildManifestPayload`: the `--manifest --json` document.
pub(crate) fn manifest_payload(
    domains: &[ManifestDomain],
    governance: &mulch::Governance,
) -> Value {
    let quick_reference: Vec<Value> = MANIFEST_QUICK_REF
        .iter()
        .map(|(command, description)| {
            let mut row = Map::new();
            row.insert("command".into(), Value::String(promote_mulch(command)));
            row.insert(
                "description".into(),
                Value::String(promote_mulch(description)),
            );
            Value::Object(row)
        })
        .collect();
    let domains: Vec<Value> = domains
        .iter()
        .map(|domain| {
            let mut row = Map::new();
            row.insert("domain".into(), Value::String(domain.domain.clone()));
            row.insert("count".into(), Value::from(domain.count));
            row.insert(
                "lastUpdated".into(),
                domain.last_updated.map_or(Value::Null, |then| {
                    Value::String(then.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
                }),
            );
            let mut counts = Map::new();
            for (name, n) in &domain.type_counts {
                counts.insert(name.clone(), Value::from(*n));
            }
            row.insert("type_counts".into(), Value::Object(counts));
            let count = u64::try_from(domain.count).unwrap_or(u64::MAX);
            let status = match governance.tier(count) {
                mulch::Tier::Hard => "over_hard_limit",
                mulch::Tier::Warn => "over_warn_threshold",
                mulch::Tier::Approaching => "approaching_limit",
                mulch::Tier::Ok => "ok",
            };
            let mut health = Map::new();
            health.insert("status".into(), Value::String(status.into()));
            health.insert("max_entries".into(), Value::from(governance.max_entries));
            health.insert("warn_entries".into(), Value::from(governance.warn_entries));
            health.insert("hard_limit".into(), Value::from(governance.hard_limit));
            row.insert("health".into(), Value::Object(health));
            Value::Object(row)
        })
        .collect();
    let mut payload = Map::new();
    payload.insert("type".into(), Value::String("manifest".into()));
    payload.insert("quick_reference".into(), Value::Array(quick_reference));
    payload.insert("domains".into(), Value::Array(domains));
    Value::Object(payload)
}

// ---- project contract ----

/// One custom type row of the contract block.
pub(crate) struct ContractType {
    pub(crate) name:     String,
    pub(crate) extends:  Option<String>,
    pub(crate) required: Vec<String>,
    pub(crate) optional: Vec<String>,
}

/// One per-domain rule row.
pub(crate) struct ContractDomain {
    pub(crate) domain:          String,
    pub(crate) allowed_types:   Vec<String>,
    pub(crate) required_fields: Vec<String>,
}

/// The write-side gates surfaced ahead of prime's content.
pub(crate) struct ProjectContract {
    pub(crate) custom_types:   Vec<ContractType>,
    pub(crate) disabled_types: Vec<String>,
    pub(crate) domains:        Vec<ContractDomain>,
    pub(crate) hooks:          Vec<String>,
}

/// The reference's `HOOK_EVENTS` order.
const HOOK_EVENTS: [&str; 5] = [
    "pre-record",
    "post-record",
    "pre-prime",
    "pre-prune",
    "pre-compact",
];

/// `buildProjectContract` over our config + registry.
pub(crate) fn build_project_contract(
    config: &mulch::Config,
    registry: &mulch::Registry,
) -> ProjectContract {
    let raw_custom = config.custom_types_value();
    let extends_of = |name: &str| -> Option<String> {
        raw_custom
            .and_then(|value| value.get(name))
            .and_then(|entry| entry.get("extends"))
            .and_then(serde_yaml::Value::as_str)
            .map(ToString::to_string)
    };
    let custom_types = registry
        .rows()
        .iter()
        .filter(|row| !row.builtin)
        .map(|row| ContractType {
            name:     row.name.clone(),
            extends:  extends_of(&row.name),
            required: row.payload.clone(),
            optional: row.optional.clone(),
        })
        .collect();
    let disabled_types = config
        .extra("disabled_types")
        .and_then(|value| value.as_sequence())
        .map_or_else(Vec::new, |items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        });
    let domains = config
        .domains()
        .into_iter()
        .filter_map(|domain| {
            let allowed = config
                .allowed_types(domain)
                .ok()
                .flatten()
                .unwrap_or_default();
            let required = config
                .required_fields(domain)
                .ok()
                .flatten()
                .unwrap_or_default();
            (!allowed.is_empty() || !required.is_empty()).then(|| ContractDomain {
                domain:          domain.to_string(),
                allowed_types:   allowed,
                required_fields: required,
            })
        })
        .collect();
    let hooks = HOOK_EVENTS
        .iter()
        .filter(|event| {
            config
                .extra("hooks")
                .and_then(|hooks| hooks.get(**event))
                .and_then(|scripts| scripts.as_sequence())
                .is_some_and(|scripts| !scripts.is_empty())
        })
        .map(|event| (*event).to_string())
        .collect();
    ProjectContract {
        custom_types,
        disabled_types,
        domains,
        hooks,
    }
}

impl ProjectContract {
    /// `hasContractContent`.
    fn has_content(&self) -> bool {
        !self.custom_types.is_empty()
            || !self.disabled_types.is_empty()
            || !self.domains.is_empty()
            || !self.hooks.is_empty()
    }

    /// The custom type field suffix (`; required: …; optional: …`).
    fn fields_suffix(custom: &ContractType) -> String {
        let mut parts = Vec::new();
        if !custom.required.is_empty() {
            parts.push(format!("required: {}", custom.required.join(", ")));
        }
        if !custom.optional.is_empty() {
            parts.push(format!("optional: {}", custom.optional.join(", ")));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("; {}", parts.join("; "))
        }
    }
}

/// `formatProjectContract`: `None` when the project has no gates to
/// surface.
pub(crate) fn contract_block(
    config: &mulch::Config,
    registry: &mulch::Registry,
    format: crate::commands::render::Format,
) -> Option<String> {
    use crate::commands::render::Format;
    let contract = build_project_contract(config, registry);
    if !contract.has_content() {
        return None;
    }
    let mut lines: Vec<String> = Vec::new();
    match format {
        Format::Xml => {
            lines.push("<contract>".into());
            if !contract.custom_types.is_empty() {
                lines.push("  <custom_types>".into());
                for custom in &contract.custom_types {
                    let ext = custom.extends.as_ref().map_or_else(String::new, |extends| {
                        format!(" extends=\"{}\"", xml_attr_escape(extends))
                    });
                    lines.push(format!(
                        "    <type name=\"{}\"{ext}>",
                        xml_attr_escape(&custom.name)
                    ));
                    if !custom.required.is_empty() {
                        lines.push(format!(
                            "      <required>{}</required>",
                            xml_escape(&custom.required.join(", "))
                        ));
                    }
                    if !custom.optional.is_empty() {
                        lines.push(format!(
                            "      <optional>{}</optional>",
                            xml_escape(&custom.optional.join(", "))
                        ));
                    }
                    lines.push("    </type>".into());
                }
                lines.push("  </custom_types>".into());
            }
            if !contract.disabled_types.is_empty() {
                lines.push(format!(
                    "  <disabled_types>{}</disabled_types>",
                    xml_escape(&contract.disabled_types.join(", "))
                ));
            }
            if !contract.domains.is_empty() {
                lines.push("  <domains>".into());
                for domain in &contract.domains {
                    let allowed = if domain.allowed_types.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " allowed=\"{}\"",
                            xml_attr_escape(&domain.allowed_types.join(", "))
                        )
                    };
                    let required = if domain.required_fields.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " required=\"{}\"",
                            xml_attr_escape(&domain.required_fields.join(", "))
                        )
                    };
                    lines.push(format!(
                        "    <domain name=\"{}\"{allowed}{required} />",
                        xml_attr_escape(&domain.domain)
                    ));
                }
                lines.push("  </domains>".into());
            }
            if !contract.hooks.is_empty() {
                lines.push(format!(
                    "  <hooks>{}</hooks>",
                    xml_escape(&contract.hooks.join(", "))
                ));
            }
            lines.push("</contract>".into());
            Some(promote_mulch(&lines.join("\n")))
        }
        Format::Plain => {
            lines.push("Project Contract (write-side gates)".into());
            lines.push("===================================".into());
            if !contract.custom_types.is_empty() {
                lines.push(String::new());
                lines.push("Custom types:".into());
                for custom in &contract.custom_types {
                    let ext = custom
                        .extends
                        .as_ref()
                        .map_or_else(String::new, |extends| format!(" (extends {extends})"));
                    lines.push(format!(
                        "  - {}{ext}{}",
                        custom.name,
                        ProjectContract::fields_suffix(custom)
                    ));
                }
            }
            if !contract.disabled_types.is_empty() {
                lines.push(String::new());
                lines.push(format!(
                    "Disabled types: {} (writes emit a deprecation warning)",
                    contract.disabled_types.join(", ")
                ));
            }
            if !contract.domains.is_empty() {
                lines.push(String::new());
                lines.push("Per-domain rules:".into());
                for domain in &contract.domains {
                    let mut parts = Vec::new();
                    if !domain.allowed_types.is_empty() {
                        parts.push(format!("allowed: {}", domain.allowed_types.join(", ")));
                    }
                    if !domain.required_fields.is_empty() {
                        parts.push(format!("required: {}", domain.required_fields.join(", ")));
                    }
                    lines.push(format!("  - {}: {}", domain.domain, parts.join("; ")));
                }
            }
            if !contract.hooks.is_empty() {
                lines.push(String::new());
                lines.push(format!("Active hooks: {}", contract.hooks.join(", ")));
            }
            Some(promote_mulch(&lines.join("\n")))
        }
        Format::Markdown | Format::Compact => {
            lines.push("## Project Contract".into());
            lines.push(String::new());
            lines.push("Write-side gates `ml record` enforces in this project.".into());
            lines.push(String::new());
            if !contract.custom_types.is_empty() {
                lines.push("**Custom types**:".into());
                for custom in &contract.custom_types {
                    let ext = custom
                        .extends
                        .as_ref()
                        .map_or_else(String::new, |extends| format!(" (extends `{extends}`)"));
                    lines.push(format!(
                        "- `{}`{ext}{}",
                        custom.name,
                        ProjectContract::fields_suffix(custom)
                    ));
                }
                lines.push(String::new());
            }
            if !contract.disabled_types.is_empty() {
                let names = contract
                    .disabled_types
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!(
                    "**Disabled types**: {names} (writes emit a deprecation warning)"
                ));
                lines.push(String::new());
            }
            if !contract.domains.is_empty() {
                lines.push("**Per-domain rules**:".into());
                for domain in &contract.domains {
                    let mut parts = Vec::new();
                    if !domain.allowed_types.is_empty() {
                        parts.push(format!(
                            "allowed types — {}",
                            domain.allowed_types.join(", ")
                        ));
                    }
                    if !domain.required_fields.is_empty() {
                        parts.push(format!(
                            "required fields — {}",
                            domain.required_fields.join(", ")
                        ));
                    }
                    lines.push(format!("- `{}`: {}", domain.domain, parts.join("; ")));
                }
                lines.push(String::new());
            }
            if !contract.hooks.is_empty() {
                lines.push(format!("**Active hooks**: {}", contract.hooks.join(", ")));
            }
            let text = lines.join("\n");
            let trimmed = text.trim_end();
            Some(promote_mulch(trimmed))
        }
    }
}

// ---- session-close footer ----

/// `getSessionEndReminder`: `custom` wins, then the style preset
/// (`conditional` for absent/unknown styles).
/// The `plain` arms inside the presets are unreachable through
/// `prime` (the caller gates `format != "plain"`) — PARITY, not
/// drift: the reference carries the same arm in its shared
/// sessionClose for embedders (format.ts:533), and its prime gate
/// skips the reminder for the same reason ours does (prime.ts:431:
/// plain is the spawn-injection contract, embedders frame sessions
/// themselves). Kept, not deleted.
/// The reference's sessionClose also carries an `"embedded"` arm
/// per preset — the spawn-injection EMBEDDER dispatch (format.ts),
/// never a CLI `--format` value. This port has no embedder path into
/// the channel layer, so the arm is DROPPED here; it re-lands with a
/// lib embedder seam if one ever exists (D1, mulch-3e3b).
///
/// The `plain` arms inside the presets are unreachable through
/// `prime` (the caller gates `format != "plain"`) — PARITY, not
/// drift: the reference carries the same arm in its shared
/// sessionClose for embedders (format.ts:533) and its prime gate
/// skips the reminder for the same reason ours does (prime.ts:431:
/// plain is the spawn-injection contract, embedders frame sessions
/// themselves). Kept, not deleted.
pub(crate) fn session_close(
    format: crate::commands::render::Format,
    config: Option<&mulch::SessionClose>,
) -> String {
    if let Some(custom) = config.and_then(|config| config.custom.as_deref())
        && !custom.is_empty()
    {
        return promote_mulch(custom);
    }
    let style = config
        .and_then(|config| config.style.as_deref())
        .unwrap_or("conditional");
    match style {
        "none" => String::new(),
        "minimal" => promote_mulch(&minimal_session_close(format)),
        "directive" => promote_mulch(&directive_session_close(format)),
        _ => promote_mulch(&conditional_session_close(format)),
    }
}

/// The `conditional` preset (the v0.10 default).
fn conditional_session_close(format: crate::commands::render::Format) -> String {
    use crate::commands::render::Format;
    match format {
        Format::Xml => [
            "<session_close>",
            "  <instruction>If you discovered insights worth preserving — a new convention, a pattern that worked, a decision made, a failure encountered — record them before closing this session.</instruction>",
            "  <commands>",
            "    <command>ml learn — see what files changed</command>",
            "    <command>ml record &lt;domain&gt; --type &lt;type&gt; --description &quot;...&quot;</command>",
            "    <command>ml sync — validate, stage, commit</command>",
            "  </commands>",
            "  <note>Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records are also noise.</note>",
            "</session_close>",
        ]
        .join("\n"),
        Format::Plain => [
            "=== 🚨 SESSION CLOSE 🚨 ===",
            "",
            "If you discovered insights worth preserving — a new convention, a pattern that worked,",
            "a decision made, a failure encountered — record them before closing this session:",
            "",
            "  ml learn                              (see what files changed)",
            "  ml record <domain> --type <type> ...  (record the insight)",
            "  ml sync                               (validate, stage, commit)",
            "",
            "Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records are also noise.",
        ]
        .join("\n"),
        Format::Markdown | Format::Compact => [
            "# 🚨 SESSION CLOSE 🚨",
            "",
            "**If you discovered insights worth preserving** — a new convention, a pattern that worked, a decision made, a failure encountered — record them before closing this session:",
            "",
            "```bash",
            "ml learn                              # see what files changed",
            "ml record <domain> --type <type> ...  # record the insight",
            "ml sync                               # validate, stage, commit",
            "```",
            "",
            "Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records are also noise.",
        ]
        .join("\n"),
    }
}

/// The `minimal` preset.
fn minimal_session_close(format: crate::commands::render::Format) -> String {
    use crate::commands::render::Format;
    match format {
        Format::Xml => [
            "<session_close>",
            "  <instruction>Before closing, record any insight worth preserving: `ml learn` → `ml record &lt;domain&gt; --type &lt;type&gt; …` → `ml sync`. Skip if no insight surfaced.</instruction>",
            "</session_close>",
        ]
        .join("\n"),
        Format::Plain => "Before closing, record any insight worth preserving: ml learn → ml record <domain> --type <type> … → ml sync. Skip if no insight surfaced.".into(),
        Format::Markdown | Format::Compact => "**Before closing**: record any insight worth preserving — `ml learn` → `ml record <domain> --type <type> …` → `ml sync`. Skip if no insight surfaced.".into(),
    }
}

/// The `directive` preset.
fn directive_session_close(format: crate::commands::render::Format) -> String {
    use crate::commands::render::Format;
    match format {
        Format::Xml => [
            "<session_close>",
            "  <heading>SESSION CLOSE — record insights before exiting</heading>",
            "  <steps>",
            "    <step n=\"1\">Run `ml learn` to see changed files and suggested domains.</step>",
            "    <step n=\"2\">Run `ml record &lt;domain&gt; --type &lt;type&gt; …` for each insight worth preserving.</step>",
            "    <step n=\"3\">Run `ml sync` to validate, stage, and commit the .mulch/ changes.</step>",
            "  </steps>",
            "  <types>",
            "    <type name=\"convention\">a norm the project follows (e.g. naming, formatting)</type>",
            "    <type name=\"pattern\">how-to: a repeatable approach to a class of problem</type>",
            "    <type name=\"failure\">anti-pattern + fix: what broke and how you resolved it</type>",
            "    <type name=\"decision\">we picked X over Y, and why</type>",
            "    <type name=\"reference\">pointer to an external spec, doc, or codebase landmark</type>",
            "    <type name=\"guide\">a walk-through of a multi-step workflow</type>",
            "  </types>",
            "  <dedup>`--name` upserts merge; re-recording is safe — outcomes accumulate, fields update in place.</dedup>",
            "  <do_not_record>",
            "    <item>Restatements of CLAUDE.md or prior records</item>",
            "    <item>Generic truisms (\"use TypeScript\", \"write tests\")</item>",
            "    <item>Single-PR-scoped facts (\"this PR adds X\")</item>",
            "    <item>Ritual confirmations with no new information</item>",
            "  </do_not_record>",
            "  <note>Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records are also noise.</note>",
            "</session_close>",
        ]
        .join("\n"),
        Format::Plain => [
            "=== 🚨 SESSION CLOSE 🚨 ===",
            "",
            "Record insights before exiting:",
            "",
            "  1. ml learn                              (see changed files + suggested domains)",
            "  2. ml record <domain> --type <type> ...  (record each insight)",
            "  3. ml sync                               (validate, stage, commit)",
            "",
            "Types:",
            "  - convention : a norm the project follows (naming, formatting, ...)",
            "  - pattern    : how-to — a repeatable approach to a class of problem",
            "  - failure    : anti-pattern + fix — what broke and how you resolved it",
            "  - decision   : we picked X over Y, and why",
            "  - reference  : pointer to an external spec / doc / codebase landmark",
            "  - guide      : a walk-through of a multi-step workflow",
            "",
            "Dedup: --name upserts merge; re-recording is safe — outcomes accumulate, fields update in place.",
            "",
            "Do NOT record:",
            "  - Restatements of CLAUDE.md or prior records",
            "  - Generic truisms (\"use TypeScript\", \"write tests\")",
            "  - Single-PR-scoped facts (\"this PR adds X\")",
            "  - Ritual confirmations with no new information",
            "",
            "Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records are also noise.",
        ]
        .join("\n"),
        Format::Markdown | Format::Compact => [
            "# 🚨 SESSION CLOSE 🚨",
            "",
            "**Record insights before exiting:**",
            "",
            "1. `ml learn` — see changed files and suggested domains",
            "2. `ml record <domain> --type <type> …` — record each insight",
            "3. `ml sync` — validate, stage, commit",
            "",
            "**Types**:",
            "- `convention` — a norm the project follows (naming, formatting, …)",
            "- `pattern` — how-to: a repeatable approach to a class of problem",
            "- `failure` — anti-pattern + fix: what broke and how you resolved it",
            "- `decision` — we picked X over Y, and why",
            "- `reference` — pointer to an external spec / doc / codebase landmark",
            "- `guide` — a walk-through of a multi-step workflow",
            "",
            "**Dedup**: `--name` upserts merge; re-recording is safe — outcomes accumulate, fields update in place.",
            "",
            "**Do not record**: restatements of CLAUDE.md or prior records, generic truisms (\"use TypeScript\"), single-PR-scoped facts (\"this PR adds X\"), ritual confirmations with no new information.",
            "",
            "Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records are also noise.",
        ]
        .join("\n"),
    }
}

#[cfg(test)]
mod golden_tables {
    use jiff::Timestamp;

    use super::{ManifestDomain, contract_block, manifest_output, prime_output, session_close};
    use crate::commands::render::Format;

    /// The golden tables (D2, mulch-3e3b): the pure text builders get
    /// unit coverage that does NOT need the reference binary on PATH
    /// — the class ADR-0005 named for the merge law, extended to the
    /// channel's own bytes. Every test names the mix-up it catches.
    fn domain(count: usize) -> ManifestDomain {
        ManifestDomain {
            domain: "dev".into(),
            count,
            last_updated: None,
            type_counts: vec![("pattern".into(), count as u64)],
        }
    }

    fn now() -> Timestamp {
        Timestamp::now()
    }

    #[test]
    fn manifest_head_is_the_family_and_compact_shares_markdown() {
        // Catches: (a) a family mix-up (Xml rendering the markdown
        // head or vice versa), (b) compact growing its OWN manifest
        // family — the old `_`-arm law is Compact == Markdown here.
        let empty: Vec<ManifestDomain> = Vec::new();
        let governance = mulch::Governance::reference_default();

        let markdown = manifest_output(&empty, &governance, Format::Markdown, now());
        assert!(markdown.starts_with("# Project Expertise Manifest (via Mulch)\n"));
        let compact = manifest_output(&empty, &governance, Format::Compact, now());
        assert_eq!(
            compact.lines().next(),
            markdown.lines().next(),
            "compact shares the markdown manifest family"
        );

        let xml = manifest_output(&empty, &governance, Format::Xml, now());
        assert!(xml.starts_with("<manifest>\n"));
        assert!(xml.trim_end().ends_with("</manifest>"));

        let plain = manifest_output(&empty, &governance, Format::Plain, now());
        assert!(plain.starts_with("Project Expertise Manifest (via Mulch)\n"));
        assert!(
            plain
                .lines()
                .nth(1)
                .is_some_and(|l| l.starts_with("======="))
        );
    }

    #[test]
    fn manifest_governance_suffix_follows_the_tier() {
        // Catches: the manifest status suffix drifting from the Tier
        // ladder (mulch-0337's four wordings — this is one of them).
        let governance = mulch::Governance {
            max_entries:  1,
            warn_entries: 5,
            hard_limit:   10,
        };
        let over = manifest_output(&[domain(10)], &governance, Format::Markdown, now());
        assert!(
            over.contains(" ⚠ OVER HARD LIMIT — must decompose"),
            "hard-tier suffix missing: {over}"
        );
        let ok = manifest_output(&[domain(0)], &governance, Format::Markdown, now());
        assert!(!ok.contains("OVER HARD LIMIT"));
    }

    #[test]
    fn contract_block_is_none_without_gates_and_families_differ() {
        // Catches: (a) a config with nothing to surface rendering an
        // empty block instead of None, (b) a family mix-up.
        let plain_config =
            mulch::Config::parse("version: '1'\ndomains:\n  dev: {}\n").expect("config");
        let registry = mulch::Registry::from_config(&plain_config).expect("registry");
        for format in [
            Format::Markdown,
            Format::Compact,
            Format::Xml,
            Format::Plain,
        ] {
            assert!(
                contract_block(&plain_config, &registry, format).is_none(),
                "no gates -> None, got {format:?}"
            );
        }

        let gated = mulch::Config::parse(
            "version: '1'\ndomains:\n  dev: {}\ncustom_types:\n  hypo:\n    required: [a]\n    dedup_key: a\n    summary: \"{a}\"\n",
        )
        .expect("config");
        let registry = mulch::Registry::from_config(&gated).expect("registry");
        let xml = contract_block(&gated, &registry, Format::Xml).expect("gated -> Some");
        assert!(xml.starts_with("<contract>\n"));
        assert!(xml.trim_end().ends_with("</contract>"));
        let markdown = contract_block(&gated, &registry, Format::Markdown).expect("gated -> Some");
        assert!(
            !markdown.starts_with("<contract>"),
            "markdown family must not render xml"
        );
    }

    #[test]
    fn session_close_styles_and_formats() {
        // Catches: (a) the style table drifting (none/minimal/custom),
        // (b) a format mix-up inside the presets, (c) the promote law
        // (self-referential commands name THIS binary).
        let none = mulch::SessionClose {
            style:  Some("none".into()),
            custom: None,
        };
        for format in [Format::Markdown, Format::Xml, Format::Plain] {
            assert_eq!(session_close(format, Some(&none)), "");
        }

        // Default style (config None): the conditional preset.
        let markdown = session_close(Format::Markdown, None);
        assert!(markdown.contains("SESSION CLOSE"), "{markdown}");
        assert!(
            markdown.contains("mulch learn"),
            "the promote law names this binary: {markdown}"
        );

        let plain = session_close(Format::Plain, None);
        assert!(
            plain.starts_with("=== 🚨 SESSION CLOSE 🚨 ==="),
            "plain preset reachable when called directly: {plain}"
        );

        let minimal = mulch::SessionClose {
            style:  Some("minimal".into()),
            custom: None,
        };
        let xml = session_close(Format::Xml, Some(&minimal));
        assert!(xml.starts_with("<session_close>"), "{xml}");

        let custom = mulch::SessionClose {
            style:  None,
            custom: Some("goodbye from the config".into()),
        };
        assert_eq!(
            session_close(Format::Markdown, Some(&custom)),
            "goodbye from the config"
        );
    }

    #[test]
    fn prime_output_empty_states_per_family() {
        // Catches: the empty-store message drifting (it is the FIRST
        // byte every fresh user sees) and the plain family's
        // no-envelope law.
        let empty: Vec<String> = Vec::new();
        let plain = prime_output(Format::Plain, &empty);
        assert_eq!(
            plain,
            "No expertise recorded yet. Use `mulch add <domain>` and `mulch record` to get started."
        );
        let xml = prime_output(Format::Xml, &empty);
        assert!(xml.starts_with("<expertise>"));
        assert!(xml.contains("<empty>"));
        let markdown = prime_output(Format::Markdown, &empty);
        assert!(markdown.contains("# Project Expertise (via Mulch)"));

        // Sections join per family: plain joins sections with the
        // same separator and adds nothing around them.
        let sections = vec!["one".to_string(), "two".to_string()];
        assert_eq!(prime_output(Format::Plain, &sections), "one\n\ntwo");
    }
}
