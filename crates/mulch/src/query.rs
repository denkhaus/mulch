//! The corpus-query module (mulch-27ec): the selection pipeline the
//! reference copy-pastes per command (`query.ts` inlines the chain
//! three times, `search.ts` re-implements it as `applyFilters` —
//! ~15 copies across the read commands). Domain-set resolution and
//! the five-filter chain live HERE, pure and lib-owned; the 16da
//! read commands (query/prime/search/rank/ready/learn) become thin
//! adapters, and scoring/sorting composes from `scoring` (mulch-228c)
//! instead of living here. Probed law: ml 0.10.7, 2026-10-18
//! (/tmp/sprint34/c2-probes).

use serde_json::Value;

/// The five corpus filters (`utils/expertise.ts`): applied
/// sequentially (AND) in reference order — type, tag,
/// classification, file, outcome-status. `None` = filter unset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordFilter {
    /// Exact type equality, case-sensitive, no aliasing
    /// (`filterByType`: `Convention` yields nothing). The CLI gates
    /// the value set per command (query takes any string, search
    /// choices-gates to the registry names).
    pub type_:          Option<String>,
    /// Case-insensitive EXACT tag equality (per-tag-element, never
    /// substring) — search-only at the reference CLI; the law lives
    /// here once.
    pub tag:            Option<String>,
    /// Exact classification equality (foundational | tactical |
    /// observational at the CLI).
    pub classification: Option<String>,
    /// Case-insensitive SUBSTRING over the `files` array (the field
    /// is read by name for every type — `filterByFile` never
    /// type-switches); records without a `files` field never match.
    pub file:           Option<String>,
    /// Any-match over outcome entries, exact status equality
    /// (success | failure | partial; the value gates diverge per
    /// command at the reference CLI).
    pub outcome_status: Option<String>,
}

impl RecordFilter {
    /// Whether every filter is unset (search's no-query guard: no
    /// term AND no filter is the usage error).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The chain over owned records (pipeline stage — consumes the
    /// read Vec, retains in reference order).
    #[must_use]
    pub fn apply(&self, mut records: Vec<Value>) -> Vec<Value> {
        if let Some(want) = &self.type_ {
            records
                .retain(|record| record.get("type").and_then(Value::as_str) == Some(want.as_str()));
        }
        if let Some(want) = &self.tag {
            let needle = want.to_lowercase();
            records.retain(|record| {
                record
                    .get("tags")
                    .and_then(Value::as_array)
                    .is_some_and(|tags| {
                        tags.iter()
                            .any(|tag| tag.as_str().is_some_and(|tag| tag.to_lowercase() == needle))
                    })
            });
        }
        if let Some(want) = &self.classification {
            records.retain(|record| {
                record.get("classification").and_then(Value::as_str) == Some(want.as_str())
            });
        }
        if let Some(want) = &self.file {
            let needle = want.to_lowercase();
            records.retain(|record| {
                record
                    .get("files")
                    .and_then(Value::as_array)
                    .is_some_and(|files| {
                        files.iter().any(|file| {
                            file.as_str()
                                .is_some_and(|file| file.to_lowercase().contains(&needle))
                        })
                    })
            });
        }
        if let Some(want) = &self.outcome_status {
            records.retain(|record| {
                record
                    .get("outcomes")
                    .and_then(Value::as_array)
                    .is_some_and(|outcomes| {
                        outcomes.iter().any(|outcome| {
                            outcome.get("status").and_then(Value::as_str) == Some(want.as_str())
                        })
                    })
            });
        }
        records
    }
}

/// Which domains a corpus command reads. The shapes differ per
/// command at the reference CLI (query: one positional or `--all`;
/// search: one `--domain` or all; prime: variadic union plus
/// exclusions) — the spec carries that shape, the resolution law is
/// shared.
#[derive(Clone, Debug, PartialEq)]
pub enum DomainSpec {
    /// Every configured domain, config order (`Object.keys`).
    All,
    /// Exactly one domain (query/rank positional, search `--domain`,
    /// ready `--domain`).
    One(String),
    /// Prime's shape: the first-occurrence-deduped union
    /// (positional-then-flag order, `new Set` semantics) minus the
    /// exclusions.
    Union {
        domains: Vec<String>,
        exclude: Vec<String>,
    },
}

/// The typed unknown-domain failure. The wordings stay CLI-side —
/// the reference drifts per command (query/rank/prime hint `ml add`,
/// search says `mulch add`, ready's plain mode rewords entirely).
#[derive(Clone, Debug, PartialEq)]
pub struct UnknownDomain {
    /// The missing domain, as given.
    pub domain:    String,
    /// The configured domains in config order (the `Available
    /// domains:` list).
    pub available: Vec<String>,
}

/// Resolve a spec against a store's config-order domain list.
///
/// `All` and empty unions return every domain in config order
/// (never alphabetical). Unknown union members fail with the first
/// unknown; exclusion members resolve after the union (a bad
/// exclusion reports its own name — the CLI knows which list it came
/// from).
///
/// # Errors
///
/// [`UnknownDomain`] for a name not in `domains`.
pub fn domain_set(domains: &[String], spec: &DomainSpec) -> Result<Vec<String>, UnknownDomain> {
    let known = |name: &str| domains.iter().any(|domain| domain == name);
    let unknown = |name: &str| UnknownDomain {
        domain:    name.to_string(),
        available: domains.to_vec(),
    };
    match spec {
        DomainSpec::All => Ok(domains.to_vec()),
        DomainSpec::One(name) => {
            if known(name) {
                Ok(vec![name.clone()])
            } else {
                Err(unknown(name))
            }
        }
        DomainSpec::Union {
            domains: wanted,
            exclude,
        } => {
            // The reference treats an empty union as every domain
            // (prime.ts: `unique.length > 0 ? unique : Object.keys`),
            // so bare `prime` primes everything — exclusions still
            // filter the all-set afterwards.
            let mut out: Vec<String> = if wanted.is_empty() {
                domains.to_vec()
            } else {
                Vec::with_capacity(wanted.len())
            };
            for name in wanted {
                if out.contains(name) {
                    continue;
                }
                if !known(name) {
                    return Err(unknown(name));
                }
                out.push(name.clone());
            }
            for name in exclude {
                if !known(name) {
                    return Err(unknown(name));
                }
            }
            out.retain(|name| !exclude.contains(name));
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn records() -> Vec<Value> {
        vec![
            json!({"type":"pattern","name":"A","tags":["Rust"],"classification":"foundational","files":["src/lib.rs.bak"]}),
            json!({"type":"pattern","name":"B","tags":["rust"],"classification":"tactical"}),
            json!({"type":"convention","content":"C","files":["src/other.rs"]}),
            json!({"type":"guide","name":"D","outcomes":[{"status":"failure"},{"status":"success"}]}),
            json!({"type":"guide","name":"E"}),
        ]
    }

    fn ids(out: &[Value]) -> Vec<&str> {
        out.iter()
            .map(|record| record.get("name").and_then(Value::as_str).unwrap_or("?"))
            .collect()
    }

    /// Type: exact, case-sensitive; `Convention` yields nothing.
    #[test]
    fn type_filter_is_exact() {
        let filter = RecordFilter {
            type_: Some("pattern".into()),
            ..RecordFilter::default()
        };
        assert_eq!(ids(&filter.apply(records())), ["A", "B"]);
        let wrong_case = RecordFilter {
            type_: Some("Pattern".into()),
            ..RecordFilter::default()
        };
        assert_eq!(wrong_case.apply(records()), [] as [serde_json::Value; 0]);
    }

    /// Tag: case-insensitive EXACT element equality, never substring.
    #[test]
    fn tag_filter_folds_case_but_not_substring() {
        for probe in ["Rust", "rust", "RUST"] {
            let filter = RecordFilter {
                tag: Some(probe.into()),
                ..RecordFilter::default()
            };
            assert_eq!(ids(&filter.apply(records())), ["A", "B"], "{probe}");
        }
        let substring = RecordFilter {
            tag: Some("us".into()),
            ..RecordFilter::default()
        };
        assert_eq!(substring.apply(records()), [] as [serde_json::Value; 0]);
    }

    /// Classification: exact equality.
    #[test]
    fn classification_filter_is_exact() {
        let filter = RecordFilter {
            classification: Some("tactical".into()),
            ..RecordFilter::default()
        };
        assert_eq!(ids(&filter.apply(records())), ["B"]);
    }

    /// File: case-insensitive substring over the `files` array;
    /// records without `files` never match.
    #[test]
    fn file_filter_substrings_case_folded() {
        for probe in ["src/lib.rs", "lib.rs", "SRC/LIB.RS", ".bak"] {
            let filter = RecordFilter {
                file: Some(probe.into()),
                ..RecordFilter::default()
            };
            assert_eq!(ids(&filter.apply(records())), ["A"], "{probe}");
        }
    }

    /// Outcome status: any-match over entries, exact status.
    #[test]
    fn outcome_filter_is_any_match() {
        for probe in ["success", "failure"] {
            let filter = RecordFilter {
                outcome_status: Some(probe.into()),
                ..RecordFilter::default()
            };
            assert_eq!(ids(&filter.apply(records())), ["D"], "{probe}");
        }
    }

    /// Filters AND (reference order is internal to an AND chain).
    #[test]
    fn filters_and_together() {
        let filter = RecordFilter {
            type_: Some("pattern".into()),
            classification: Some("tactical".into()),
            ..RecordFilter::default()
        };
        assert_eq!(ids(&filter.apply(records())), ["B"]);
        let contradictory = RecordFilter {
            type_: Some("pattern".into()),
            classification: Some("foundational".into()),
            tag: Some("nope".into()),
            ..RecordFilter::default()
        };
        assert_eq!(contradictory.apply(records()), [] as [serde_json::Value; 0]);
        assert!(!contradictory.is_empty());
        assert!(RecordFilter::default().is_empty());
    }

    /// Unset filters pass records through untouched.
    #[test]
    fn empty_filter_passes_all() {
        let all = records();
        let count = all.len();
        assert_eq!(RecordFilter::default().apply(all.clone()).len(), count);
    }

    fn domains() -> Vec<String> {
        // config order, NOT alphabetical (the zeta/alpha probe)
        ["zeta", "alpha", "mid"]
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// All: config order, never sorted.
    #[test]
    fn domain_set_all_is_config_order() {
        assert_eq!(domain_set(&domains(), &DomainSpec::All).unwrap(), domains());
    }

    /// One: known resolves, unknown carries the available list.
    #[test]
    fn domain_set_one_resolves_or_fails_typed() {
        assert_eq!(
            domain_set(&domains(), &DomainSpec::One("alpha".into())).unwrap(),
            ["alpha".to_string()]
        );
        let err = domain_set(&domains(), &DomainSpec::One("nosuch".into())).unwrap_err();
        assert_eq!(err.domain, "nosuch");
        assert_eq!(err.available, domains());
    }

    /// Union: first-occurrence dedup keeps order, exclusions filter,
    /// unknowns fail typed (union members first, exclusions after).
    #[test]
    fn domain_set_union_dedups_and_excludes() {
        let spec = DomainSpec::Union {
            domains: vec!["alpha".into(), "zeta".into(), "alpha".into(), "mid".into()],
            exclude: vec!["mid".into()],
        };
        assert_eq!(domain_set(&domains(), &spec).unwrap(), [
            "alpha".to_string(),
            "zeta".to_string()
        ]);
        let unknown_union = DomainSpec::Union {
            domains: vec!["nosuch".into()],
            exclude: vec![],
        };
        assert_eq!(
            domain_set(&domains(), &unknown_union).unwrap_err().domain,
            "nosuch"
        );
        let unknown_exclude = DomainSpec::Union {
            domains: vec!["alpha".into()],
            exclude: vec!["gone".into()],
        };
        assert_eq!(
            domain_set(&domains(), &unknown_exclude).unwrap_err().domain,
            "gone"
        );
        // empty union: EVERY domain in config order (prime.ts's
        // `unique.length > 0 ? unique : Object.keys` — bare prime
        // primes everything), exclusions filter the all-set
        let empty = DomainSpec::Union {
            domains: vec![],
            exclude: vec![],
        };
        assert_eq!(domain_set(&domains(), &empty).unwrap(), domains());
        let empty_but_excluded = DomainSpec::Union {
            domains: vec![],
            exclude: vec!["zeta".into()],
        };
        assert_eq!(domain_set(&domains(), &empty_but_excluded).unwrap(), [
            "alpha".to_string(),
            "mid".to_string()
        ]);
    }
}
