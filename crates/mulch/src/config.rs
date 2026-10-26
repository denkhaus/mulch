//! `mulch.config.yaml` — the store's configuration document.
//!
//! The reference CLI (0.10.7) rewrites this file through its YAML
//! serializer on every mutating command, so the Rust core preserves the
//! document as an ordered mapping and exposes typed accessors for the
//! fields it interprets. Unknown top-level keys (including the optional
//! `prime`/`search` knobs and future additive fields) survive a
//! read-modify-write cycle in place.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::ser::Error as _;
use serde_yaml::Mapping;

use crate::error::{Error, Result};

/// Walks an alias chain to its canonical domain — the ONE alias walk
/// (ADR-0006, mulch-8deb): [`Config::validated_domain_aliases`] runs
/// it over a freshly built map to detect cycles, and
/// `StoreFiles::resolve_domain` runs it over the validated map at
/// every addressing entry. `None` means the chain exceeded the map's
/// own size, which a validated map cannot do — a cycle is the only
/// cause. Both inputs share the return lifetime because a non-alias
/// address comes back unchanged: the result borrows from the map OR
/// from `from` (the same twice-bound shape as `ensure_domain`).
pub(crate) fn walk_alias_chain<'a>(
    aliases: &'a HashMap<String, String>,
    from: &'a str,
) -> Option<&'a str> {
    let mut current = from;
    let mut hops = 0;
    while let Some(target) = aliases.get(current) {
        current = target.as_str();
        hops += 1;
        if hops > aliases.len() {
            return None;
        }
    }
    Some(current)
}

/// Config version written by `Config::default` and understood by the core.
pub const SUPPORTED_VERSION: &str = "1";

/// Governance thresholds (`governance` block), typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Governance {
    /// Soft target of live records per domain.
    pub max_entries:  u64,
    /// Warning threshold before the hard limit.
    pub warn_entries: u64,
    /// Hard ceiling; writes beyond this are refused by the reference CLI.
    pub hard_limit:   u64,
}

/// `classification_defaults.shelf_life` block, typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ShelfLife {
    /// Days a tactical record stays fresh.
    pub tactical:      u64,
    /// Days an observational record stays fresh.
    pub observational: u64,
}

/// One rung of the governance ladder. The ladder itself is a format
/// law — `>= hard_limit` then `>= warn_entries` then `>= max_entries`,
/// checked in exactly that order — so the verdict lives here beside
/// the values it reads (ADR-0005: registry/config-named laws get a
/// lib body; the four channel WORDINGS stay in the commands and are
/// byte-pinned by the parity batteries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// At or over `hard_limit`.
    Hard,
    /// At or over `warn_entries` (and under `hard_limit`).
    Warn,
    /// At or over `max_entries` (and under `warn_entries`).
    Approaching,
    /// Below every threshold.
    Ok,
}

impl Governance {
    /// The ladder verdict for a live-record count. The fired rung's
    /// threshold is the matching field — the callers that name it
    /// (`doctor`) read it off this struct in their match arms.
    #[must_use]
    pub fn tier(self, count: u64) -> Tier {
        if count >= self.hard_limit {
            Tier::Hard
        } else if count >= self.warn_entries {
            Tier::Warn
        } else if count >= self.max_entries {
            Tier::Approaching
        } else {
            Tier::Ok
        }
    }

    /// The reference governance defaults (100/150/200) — the format
    /// contract, owned here (mulch-53cb).
    #[must_use]
    pub fn reference_default() -> Self {
        Self {
            max_entries:  100,
            warn_entries: 150,
            hard_limit:   200,
        }
    }
}

impl ShelfLife {
    /// The reference shelf-life defaults (tactical 14, observational
    /// 30 days) — the format contract, owned here (mulch-53cb).
    #[must_use]
    pub fn reference_default() -> Self {
        Self {
            tactical:      14,
            observational: 30,
        }
    }
}

/// `prime.tier_weights` block, typed.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct TierWeights {
    /// Multiplier on the ★ confirmation count.
    pub star:          u64,
    /// Base score for foundational records.
    pub foundational:  u64,
    /// Base score for tactical records.
    pub tactical:      u64,
    /// Base score for observational records.
    pub observational: u64,
}

/// `prime` optional knob block, typed.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Prime {
    /// `full` or `manifest`; unset means the reference auto-flips.
    pub default_mode:  Option<String>,
    /// Trust-tier ranking weights; unset fields keep their default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier_weights:  Option<TierWeights>,
    /// Session-close footer preset (`prime.session_close`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_close: Option<SessionClose>,
}

/// `prime.session_close` block, typed. `custom` (when non-empty)
/// wins over `style`; an absent/unknown style falls back to the
/// reference's `conditional` preset.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct SessionClose {
    /// `conditional` (default), `minimal`, `directive`, or `none`.
    pub style:  Option<String>,
    /// Raw footer text shipped verbatim.
    pub custom: Option<String>,
}

/// `search` optional knob block, typed.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct Search {
    /// Multiplier applied to BM25 scores; 0 disables the boost.
    pub boost_factor: f64,
}

/// One `audit.thresholds` knob set — every field optional, a partial
/// block merges on top of [`DEFAULT_AUDIT_THRESHOLDS`] (the
/// reference's `resolveAuditThresholds` spread).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AuditThresholds {
    /// Evidence-coverage PASS band (`>=`), default 0.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_coverage:      Option<f64>,
    /// Evidence-coverage WARN band (`>=`; below is FAIL), default 0.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_coverage_warn: Option<f64>,
    /// Floater-rate PASS band (`<=`, fraction of records), default 0.2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floater_max:            Option<f64>,
    /// Rule-density PASS band (`>=`), default 0.25.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_density_min:       Option<f64>,
    /// Rule-density WARN band (`>=`; below is FAIL), default 0.15.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_density_warn:      Option<f64>,
    /// Per-domain record-count PASS band (`<=`), default 200.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_records_per_domain: Option<f64>,
    /// Per-domain stale-count PASS band (`<=`), default 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_stale:              Option<f64>,
}

/// The `audit` optional knob block, typed (thresholds, ignored
/// domains, per-domain overrides).
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AuditConfig {
    /// Global threshold overrides (partial blocks merge).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thresholds:     Option<AuditThresholds>,
    /// Domains excluded from audit metrics entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_domains: Option<Vec<String>>,
    /// Per-domain threshold overrides, merged on top of the global
    /// set for that domain only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_domain:     Option<HashMap<String, AuditThresholds>>,
}

/// The fully merged audit thresholds — every knob set. Serialized
/// numbers keep the reference's JS shape: integral doubles print
/// without a decimal point (`1`, not `1.0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedAuditThresholds {
    /// Evidence-coverage PASS band (`>=`).
    pub evidence_coverage:      f64,
    /// Evidence-coverage WARN band (`>=`; below is FAIL).
    pub evidence_coverage_warn: f64,
    /// Floater-rate PASS band (`<=`, fraction of records).
    pub floater_max:            f64,
    /// Rule-density PASS band (`>=`).
    pub rule_density_min:       f64,
    /// Rule-density WARN band (`>=`; below is FAIL).
    pub rule_density_warn:      f64,
    /// Per-domain record-count PASS band (`<=`).
    pub max_records_per_domain: f64,
    /// Per-domain stale-count PASS band (`<=`).
    pub max_stale:              f64,
}

/// The reference audit-threshold defaults (schemas/config.ts) —
/// relaxed from the Python prototype per V1_PLAN §4.2.
pub const DEFAULT_AUDIT_THRESHOLDS: ResolvedAuditThresholds = ResolvedAuditThresholds {
    evidence_coverage:      0.5,
    evidence_coverage_warn: 0.3,
    floater_max:            0.2,
    rule_density_min:       0.25,
    rule_density_warn:      0.15,
    max_records_per_domain: 200.0,
    max_stale:              0.0,
};

impl ResolvedAuditThresholds {
    /// Merges a partial override on top of `self` (set fields win,
    /// unset fields keep the base — the reference's object spread).
    #[must_use]
    fn merged(self, over: &AuditThresholds) -> Self {
        Self {
            evidence_coverage:      over.evidence_coverage.unwrap_or(self.evidence_coverage),
            evidence_coverage_warn: over
                .evidence_coverage_warn
                .unwrap_or(self.evidence_coverage_warn),
            floater_max:            over.floater_max.unwrap_or(self.floater_max),
            rule_density_min:       over.rule_density_min.unwrap_or(self.rule_density_min),
            rule_density_warn:      over.rule_density_warn.unwrap_or(self.rule_density_warn),
            max_records_per_domain: over
                .max_records_per_domain
                .unwrap_or(self.max_records_per_domain),
            max_stale:              over.max_stale.unwrap_or(self.max_stale),
        }
    }
}

/// The reference `resolveAuditThresholds`: defaults ⊕ the global
/// `audit.thresholds` block, then (for a named domain) that domain's
/// `audit.per_domain.<name>` override merged on top.
#[must_use]
pub fn resolve_audit_thresholds(
    cfg: Option<&AuditConfig>,
    domain: Option<&str>,
) -> ResolvedAuditThresholds {
    let global = DEFAULT_AUDIT_THRESHOLDS;
    let global = cfg
        .and_then(|cfg| cfg.thresholds.as_ref())
        .map_or(global, |over| global.merged(over));
    let Some(domain) = domain else {
        return global;
    };
    cfg.and_then(|cfg| cfg.per_domain.as_ref())
        .and_then(|per| per.get(domain))
        .map_or(global, |over| global.merged(over))
}

/// Serialize one threshold knob as a JS number: integral doubles
/// print bare (`1`/`200`), fractional keep serde's shortest form
/// (mirrors [`crate::jsval::js_number`], the report JSON's law).
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's serialize_with passes field references"
)]
pub(crate) fn serialize_js_f64<S: serde::Serializer>(
    value: &f64,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the bound check keeps the integer exact"
        )]
        serializer.serialize_i64(*value as i64)
    } else {
        serializer.serialize_f64(*value)
    }
}

impl serde::Serialize for ResolvedAuditThresholds {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct as _;
        let mut out = serializer.serialize_struct("ResolvedAuditThresholds", 7)?;
        out.serialize_field("evidence_coverage", &JsF64(self.evidence_coverage))?;
        out.serialize_field(
            "evidence_coverage_warn",
            &JsF64(self.evidence_coverage_warn),
        )?;
        out.serialize_field("floater_max", &JsF64(self.floater_max))?;
        out.serialize_field("rule_density_min", &JsF64(self.rule_density_min))?;
        out.serialize_field("rule_density_warn", &JsF64(self.rule_density_warn))?;
        out.serialize_field(
            "max_records_per_domain",
            &JsF64(self.max_records_per_domain),
        )?;
        out.serialize_field("max_stale", &JsF64(self.max_stale))?;
        out.end()
    }
}

/// Newtype carrying [`serialize_js_f64`] as a `Serialize` impl (the
/// struct serializer above needs a value with the impl, not a bare
/// fn).
struct JsF64(f64);

impl serde::Serialize for JsF64 {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serialize_js_f64(&self.0, serializer)
    }
}

/// The `.mulch/mulch.config.yaml` document.
///
/// Field order on write matches the reference serializer: `version`,
/// `domains`, `governance`, `classification_defaults`, then everything
/// else in original read order.
#[derive(Clone, Debug)]
pub struct Config {
    raw: Mapping,
}

impl Default for Config {
    /// The config `ml init` writes, in the reference key order.
    fn default() -> Self {
        let mut config = Self::empty();
        config.set_governance(Governance::reference_default());
        config.set_shelf_life(ShelfLife::reference_default());
        config
    }
}

impl Config {
    /// A minimal `version: '1'` config with empty domains and no defaults.
    pub fn empty() -> Self {
        let mut raw = Mapping::new();
        raw.insert(yaml_str("version"), yaml_str(SUPPORTED_VERSION));
        raw.insert(
            yaml_str("domains"),
            serde_yaml::Value::Mapping(Mapping::new()),
        );
        Self { raw }
    }

    /// Parse a config document, keeping all keys in file order.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: Mapping = serde_yaml::from_str(text).map_err(|source| Error::ConfigParse {
            path: PathBuf::from("mulch.config.yaml"),
            source,
        })?;
        Ok(Self { raw })
    }

    /// Serialize in the reference key order (comments are not preserved;
    /// the reference CLI strips them too).
    pub fn to_yaml(&self) -> String {
        // The reference applies config defaults before every write
        // (`applyConfigDefaults`): governance and shelf-life blocks are
        // always present, with user values merged over the defaults.
        let mut ordered = Mapping::new();
        ordered.insert(yaml_str("version"), self.version_value());
        ordered.insert(yaml_str("domains"), self.domains_mapping());
        ordered.insert(yaml_str("governance"), self.governance_mapping());
        ordered.insert(
            yaml_str("classification_defaults"),
            self.classification_defaults_mapping(),
        );
        for (key, value) in &self.raw {
            if !matches!(key.as_str(), Some(k) if matches!(k, "version" | "domains" | "governance" | "classification_defaults"))
            {
                ordered.insert(key.clone(), value.clone());
            }
        }
        serde_yaml::to_string(&ordered).expect("YAML mapping serialization is infallible")
    }

    /// `version`, falling back to the supported version.
    fn version_value(&self) -> serde_yaml::Value {
        self.raw
            .get(yaml_str("version"))
            .cloned()
            .unwrap_or_else(|| yaml_str(SUPPORTED_VERSION))
    }

    /// The raw `custom_types` value (config order preserved; the
    /// registry view parses and validates it).
    #[must_use]
    pub fn custom_types_value(&self) -> Option<&serde_yaml::Value> {
        self.raw.get(yaml_str("custom_types"))
    }

    /// The alias -> canonical pairs of the `domain_aliases` block
    /// (config order), VALIDATED (ADR-0006, mulch-b0f7): every side
    /// is a string, every canonical side is a configured domain, no
    /// alias collides with a configured domain or itself, and the
    /// graph is acyclic. An absent block is empty; a malformed one is
    /// a typed error — never silently dropped.
    ///
    /// # Errors
    ///
    /// [`Error::DomainAliases`] naming the first finding.
    pub fn validated_domain_aliases(&self) -> Result<Vec<(String, String)>> {
        let Some(value) = self.raw.get(yaml_str("domain_aliases")) else {
            return Ok(Vec::new());
        };
        let Some(mapping) = value.as_mapping() else {
            return Err(Error::DomainAliases {
                message: "the block must be a mapping of alias -> canonical domain".into(),
            });
        };
        let domains = self.domains();
        let alias_targets: Vec<&str> = mapping.keys().filter_map(|k| k.as_str()).collect();
        let mut pairs = Vec::new();
        for (alias, canonical) in mapping {
            let (Some(alias), Some(canonical)) = (alias.as_str(), canonical.as_str()) else {
                return Err(Error::DomainAliases {
                    message: format!(
                        "alias pair {alias:?} -> {canonical:?}: both sides must be domain-name strings"
                    ),
                });
            };
            if alias == canonical {
                return Err(Error::DomainAliases {
                    message: format!("{alias:?} maps onto itself"),
                });
            }
            if domains.contains(&alias) {
                return Err(Error::DomainAliases {
                    message: format!(
                        "{alias:?} is both a configured domain and an alias — rename one"
                    ),
                });
            }
            if !domains.contains(&canonical) && !alias_targets.contains(&canonical) {
                return Err(Error::DomainAliases {
                    message: format!(
                        "{alias:?} maps onto {canonical:?}, which is neither a configured domain nor another alias"
                    ),
                });
            }
            pairs.push((alias.to_string(), canonical.to_string()));
        }
        // Cycle detection through the shared walk: build the map, then
        // resolve every alias on it. Non-termination IS the cycle;
        // `StoreFiles::resolve_domain` runs the same function over the
        // validated map at every addressing entry (mulch-8deb: one
        // walk, one budget).
        let map: HashMap<String, String> = pairs.iter().cloned().collect();
        for (alias, _) in &pairs {
            if walk_alias_chain(&map, alias).is_none() {
                return Err(Error::DomainAliases {
                    message: format!("{alias:?} is part of an alias cycle"),
                });
            }
        }
        Ok(pairs)
    }

    /// The `domains` mapping (empty when absent), order preserved.
    fn domains_mapping(&self) -> serde_yaml::Value {
        self.raw
            .get(yaml_str("domains"))
            .cloned()
            .unwrap_or_else(|| serde_yaml::Value::Mapping(Mapping::new()))
    }

    /// Governance thresholds with defaults backfilled.
    fn governance_mapping(&self) -> serde_yaml::Value {
        let user = self
            .raw
            .get(yaml_str("governance"))
            .and_then(serde_yaml::Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        let reference = Governance::reference_default();
        let defaults: [(&str, u64); 3] = [
            ("max_entries", reference.max_entries),
            ("warn_entries", reference.warn_entries),
            ("hard_limit", reference.hard_limit),
        ];
        let mut merged = Mapping::new();
        for (key, default) in defaults {
            let value = user
                .get(yaml_str(key))
                .cloned()
                .unwrap_or_else(|| serde_yaml::Value::from(default));
            merged.insert(yaml_str(key), value);
        }
        serde_yaml::Value::Mapping(merged)
    }

    /// The effective governance thresholds: the user block with the
    /// reference defaults backfilled per field (partial blocks keep
    /// their set values, like the reference's `applyConfigDefaults`
    /// shallow spread; a block with a wrong-typed field falls back to
    /// the reference defaults wholesale).
    #[must_use]
    pub fn effective_governance(&self) -> Governance {
        serde_yaml::from_value(self.governance_mapping())
            .unwrap_or_else(|_| Governance::reference_default())
    }

    /// The effective shelf life: the user block with the reference
    /// defaults backfilled per field; a wrong-typed field falls back
    /// to the reference defaults wholesale.
    #[must_use]
    pub fn effective_shelf_life(&self) -> ShelfLife {
        self.classification_defaults_mapping()
            .get(yaml_str("shelf_life"))
            .cloned()
            .and_then(|value| serde_yaml::from_value(value).ok())
            .unwrap_or_else(ShelfLife::reference_default)
    }

    /// `classification_defaults.shelf_life` with defaults backfilled.
    fn classification_defaults_mapping(&self) -> serde_yaml::Value {
        let user = self
            .raw
            .get(yaml_str("classification_defaults"))
            .and_then(serde_yaml::Value::as_mapping)
            .and_then(|mapping| mapping.get(yaml_str("shelf_life")))
            .and_then(serde_yaml::Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        let reference = ShelfLife::reference_default();
        let mut shelf = Mapping::new();
        for (key, default) in [
            ("tactical", reference.tactical),
            ("observational", reference.observational),
        ] {
            let value = user
                .get(yaml_str(key))
                .cloned()
                .unwrap_or_else(|| serde_yaml::Value::from(default));
            shelf.insert(yaml_str(key), value);
        }
        let mut outer = Mapping::new();
        outer.insert(yaml_str("shelf_life"), serde_yaml::Value::Mapping(shelf));
        serde_yaml::Value::Mapping(outer)
    }

    /// The config format version (`version: '1'`).
    pub fn version(&self) -> &str {
        self.raw
            .get(yaml_str("version"))
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or_default()
    }

    /// Fail unless the document declares a version this core understands.
    pub fn ensure_supported(&self) -> Result<()> {
        let version = self.version();
        if version == SUPPORTED_VERSION {
            Ok(())
        } else {
            Err(Error::UnsupportedVersion {
                version:   version.to_owned(),
                supported: SUPPORTED_VERSION,
            })
        }
    }

    /// Domain names in config order.
    pub fn domains(&self) -> Vec<&str> {
        self.raw
            .get(yaml_str("domains"))
            .and_then(serde_yaml::Value::as_mapping)
            .map_or_default(|domains| {
                domains
                    .keys()
                    .filter_map(serde_yaml::Value::as_str)
                    .collect()
            })
    }

    /// The typed `governance` block, if present.
    pub fn governance(&self) -> Result<Option<Governance>> {
        self.typed_field("governance")
    }

    /// Replace the `governance` block (insertion keeps its canonical slot).
    pub fn set_governance(&mut self, governance: Governance) {
        self.set_typed("governance", governance);
    }

    /// The typed `classification_defaults.shelf_life` block, if present.
    pub fn shelf_life(&self) -> Result<Option<ShelfLife>> {
        match self.typed_field::<serde_yaml::Mapping>("classification_defaults")? {
            None => Ok(None),
            Some(defaults) => defaults
                .get(yaml_str("shelf_life"))
                .cloned()
                .map(|value| {
                    serde_yaml::from_value(value).map_err(|source| Error::ConfigField {
                        path: PathBuf::from("mulch.config.yaml"),
                        field: "classification_defaults.shelf_life",
                        source,
                    })
                })
                .transpose(),
        }
    }

    /// Replace the `classification_defaults.shelf_life` block, creating
    /// the parent block if needed.
    pub fn set_shelf_life(&mut self, shelf_life: ShelfLife) {
        let mut defaults = self
            .raw
            .get(yaml_str("classification_defaults"))
            .and_then(serde_yaml::Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        defaults.insert(
            yaml_str("shelf_life"),
            serde_yaml::to_value(shelf_life).expect("serializable"),
        );
        self.set_typed(
            "classification_defaults",
            serde_yaml::Value::Mapping(defaults),
        );
    }

    /// The typed optional `prime` knob block, if set.
    pub fn prime(&self) -> Result<Option<Prime>> {
        self.typed_field("prime")
    }

    /// The typed optional `search` knob block, if set.
    pub fn search(&self) -> Result<Option<Search>> {
        self.typed_field("search")
    }

    /// The typed optional `audit` knob block, if set.
    pub fn audit(&self) -> Result<Option<AuditConfig>> {
        self.typed_field("audit")
    }

    /// Any other top-level key (additive fields, `custom_types`, `hooks`,
    /// ...), as the raw YAML value.
    pub fn extra(&self, key: &str) -> Option<&serde_yaml::Value> {
        self.raw.get(yaml_str(key))
    }

    /// Register a domain (a no-op when it already exists).
    ///
    /// A RAW document primitive: it does not check the alias
    /// collision invariant — a name that is a live alias written
    /// here fails the next open's validation. Callers go through
    /// [`crate::StoreFiles::register_domain`] /
    /// [`crate::StoreFiles::adopt_domain`], which own the guard
    /// (mulch-8deb).
    pub fn add_domain(&mut self, domain: &str) {
        let Some(domains) = self
            .raw
            .get_mut(yaml_str("domains"))
            .and_then(serde_yaml::Value::as_mapping_mut)
        else {
            self.raw.insert(
                yaml_str("domains"),
                serde_yaml::Value::Mapping(Mapping::new()),
            );
            return self.add_domain(domain);
        };
        domains
            .entry(yaml_str(domain))
            .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
    }

    fn typed_field<T: serde::de::DeserializeOwned>(
        &self,
        field: &'static str,
    ) -> Result<Option<T>> {
        self.raw
            .get(yaml_str(field))
            .cloned()
            .map(|value| {
                serde_yaml::from_value(value).map_err(|source| Error::ConfigField {
                    path: PathBuf::from("mulch.config.yaml"),
                    field,
                    source,
                })
            })
            .transpose()
    }

    fn set_typed<T: serde::Serialize>(&mut self, field: &str, value: T) {
        self.raw.insert(
            yaml_str(field),
            serde_yaml::to_value(&value).expect("serializable"),
        );
    }

    /// Remove a domain entry (a no-op when absent). The remaining
    /// domains keep their original order — `Mapping::remove` alone
    /// would swap the last entry into the hole.
    ///
    /// A RAW document primitive like [`Config::add_domain`]: it does
    /// not check for live aliases, which would dangle. Callers go
    /// through [`crate::StoreFiles::remove_domain`], which owns the
    /// guard (mulch-8deb).
    pub fn remove_domain(&mut self, domain: &str) {
        if let Some(domains) = self
            .raw
            .get_mut(yaml_str("domains"))
            .and_then(serde_yaml::Value::as_mapping_mut)
        {
            let kept: Vec<(serde_yaml::Value, serde_yaml::Value)> = domains
                .iter()
                .filter(|(key, _)| key.as_str() != Some(domain))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            *domains = Mapping::from_iter(kept);
        }
    }

    /// A domain's `required_fields` list, when configured.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigField`] when the value is present but not
    /// a list of strings.
    pub fn required_fields(&self, domain: &str) -> Result<Option<Vec<String>>> {
        self.domain_list(domain, "required_fields")
    }

    /// A domain's `allowed_types` list, when configured.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigField`] when the value is present but not
    /// a list of strings (a wrongly shaped rule must not silently pass
    /// an enforcement gate).
    pub fn allowed_types(&self, domain: &str) -> Result<Option<Vec<String>>> {
        self.domain_list(domain, "allowed_types")
    }

    /// A per-domain string-list rule.
    fn domain_list(&self, domain: &str, field: &'static str) -> Result<Option<Vec<String>>> {
        let Some(domains) = self.raw.get(yaml_str("domains")) else {
            return Ok(None);
        };
        let Some(entry) = domains
            .as_mapping()
            .and_then(|mapping| mapping.get(yaml_str(domain)))
        else {
            return Ok(None);
        };
        let Some(value) = entry
            .as_mapping()
            .and_then(|mapping| mapping.get(yaml_str(field)))
        else {
            return Ok(None);
        };
        let list = value.as_sequence().ok_or_else(|| Error::ConfigField {
            path: PathBuf::from("mulch.config.yaml"),
            field,
            source: serde_yaml::Error::custom("expected a list of strings"),
        })?;
        let mut types = Vec::with_capacity(list.len());
        for item in list {
            let text = item.as_str().ok_or_else(|| Error::ConfigField {
                path: PathBuf::from("mulch.config.yaml"),
                field,
                source: serde_yaml::Error::custom("expected string entries"),
            })?;
            types.push(text.to_string());
        }
        Ok(Some(types))
    }
}

fn yaml_str(s: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(s.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aliases(text: &str) -> crate::error::Result<Vec<(String, String)>> {
        Config::parse(text).and_then(|config| config.validated_domain_aliases())
    }

    /// The domain_aliases validation table (ADR-0006). Catches: the
    /// four rejections (unknown canonical, domain/alias collision,
    /// self-map, cycle) plus the two acceptances (plain pairs,
    /// transitive chains) and the absent/malformed block shapes.
    #[test]
    fn domain_aliases_validation_table() {
        let base = "version: '1'\ndomains:\n  dev: {}\n  rust: {}\n";
        // absent -> empty
        assert_eq!(
            aliases(base).expect("absent"),
            Vec::<(String, String)>::new()
        );
        // plain pairs pass, config order preserved
        let pairs = aliases(&format!(
            "{base}domain_aliases:\n  develop: dev\n  rs: rust\n"
        ))
        .expect("pairs");
        assert_eq!(pairs, vec![
            ("develop".to_string(), "dev".to_string()),
            ("rs".to_string(), "rust".to_string())
        ]);
        // transitive chains pass (devloop -> develop -> dev)
        aliases(&format!(
            "{base}domain_aliases:\n  develop: dev\n  devloop: develop\n"
        ))
        .expect("chain");
        // unknown canonical
        let err =
            aliases(&format!("{base}domain_aliases:\n  x: nope\n")).expect_err("unknown canonical");
        assert!(
            err.to_string()
                .contains("neither a configured domain nor another alias"),
            "{err}"
        );
        // collision: alias is also a configured domain
        let err = aliases(&format!("{base}domain_aliases:\n  rust: dev\n")).expect_err("collision");
        assert!(
            err.to_string()
                .contains("both a configured domain and an alias"),
            "{err}"
        );
        // self-map
        let err = aliases(&format!("{base}domain_aliases:\n  x: x\n")).expect_err("self");
        assert!(err.to_string().contains("maps onto itself"), "{err}");
        // cycle
        let err = aliases(&format!("{base}domain_aliases:\n  a: b\n  b: a\n")).expect_err("cycle");
        assert!(err.to_string().contains("cycle"), "{err}");
        // malformed block
        let err = aliases(&format!("{base}domain_aliases:\n  - a\n")).expect_err("malformed");
        assert!(err.to_string().contains("mapping"), "{err}");
    }

    fn ladder() -> Governance {
        Governance {
            max_entries:  10,
            warn_entries: 15,
            hard_limit:   20,
        }
    }

    /// The ladder's decision table. Catches: (a) `>` where the law is
    /// `>=` (every boundary count below must fire its rung), (b) a
    /// reordered ladder (a count above TWO thresholds must take the
    /// HIGHER rung — checking warn before hard would answer Warn for
    /// 25), (c) a lost arm (an under-everything count must stay Ok).
    #[test]
    fn tier_boundaries_and_order() {
        let cases = [
            (9_u64, Tier::Ok),
            (10, Tier::Approaching),
            (14, Tier::Approaching),
            (15, Tier::Warn),
            (19, Tier::Warn),
            (20, Tier::Hard),
            (25, Tier::Hard),
            (u64::MAX, Tier::Hard),
        ];
        for (count, want) in cases {
            assert_eq!(ladder().tier(count), want, "count {count}");
        }
    }

    /// The defaults the parity batteries see (100/150/200): the same
    /// ladder at reference scale. Catches a defaults triple with
    /// warn/hard swapped — 150 would answer Hard here.
    #[test]
    fn tier_at_reference_defaults() {
        let governance = Governance::reference_default();
        assert_eq!(governance.tier(99), Tier::Ok);
        assert_eq!(governance.tier(100), Tier::Approaching);
        assert_eq!(governance.tier(150), Tier::Warn);
        assert_eq!(governance.tier(200), Tier::Hard);
    }

    const REFERENCE_SERIALIZED: &str = "\
version: '1'
domains:
  rust: {}
governance:
  max_entries: 100
  warn_entries: 150
  hard_limit: 200
classification_defaults:
  shelf_life:
    tactical: 14
    observational: 30
";

    #[test]
    fn default_config_matches_reference_serialization() {
        let mut config = Config::default();
        config.add_domain("rust");
        assert_eq!(config.to_yaml(), REFERENCE_SERIALIZED);
    }

    #[test]
    fn unknown_keys_and_knobs_survive_round_trip() {
        let text = "\
version: '1'
domains:
  rust: {}
governance:
  max_entries: 100
  warn_entries: 150
  hard_limit: 200
classification_defaults:
  shelf_life:
    tactical: 14
    observational: 30
prime:
  default_mode: full
  tier_weights:
    star: 100
    foundational: 50
    tactical: 20
    observational: 10
search:
  boost_factor: 0.1
custom_types:
  hypothesis:
    required: [statement, prediction]
";
        let config = Config::parse(text).expect("parses");
        config.ensure_supported().expect("version 1");
        assert_eq!(config.domains(), vec!["rust"]);

        let prime = config.prime().expect("prime parses").expect("present");
        assert_eq!(prime.default_mode.as_deref(), Some("full"));
        assert_eq!(prime.tier_weights.expect("weights").star, 100);

        let search = config.search().expect("search parses").expect("present");
        assert!((search.boost_factor - 0.1).abs() < f64::EPSILON);
        assert!(config.extra("custom_types").is_some());

        let rewritten = config.to_yaml();
        assert!(rewritten.contains("boost_factor: 0.1"));
        assert!(rewritten.contains("hypothesis:"));
        let reparsed = Config::parse(&rewritten).expect("reparses");
        assert_eq!(reparsed.domains(), vec!["rust"]);
        assert_eq!(
            reparsed
                .governance()
                .expect("governance")
                .expect("present")
                .hard_limit,
            200
        );
        assert_eq!(
            reparsed
                .shelf_life()
                .expect("shelf life")
                .expect("present")
                .tactical,
            14
        );
    }

    #[test]
    fn remove_domain_and_allowed_types() {
        let mut config =
            Config::parse("version: '1'\ndomains:\n  alpha: {}\n  beta:\n    allowed_types:\n      - convention\n")
                .expect("parses");
        assert_eq!(config.domains(), vec!["alpha", "beta"]);
        assert_eq!(
            config.allowed_types("beta").expect("shape ok"),
            Some(vec!["convention".to_string()])
        );
        assert_eq!(config.allowed_types("alpha").expect("no rules"), None);
        config.remove_domain("alpha");
        assert_eq!(config.domains(), vec!["beta"]);
        assert!(config.to_yaml().contains("beta"));
        assert!(!config.to_yaml().contains("alpha"));

        let broken =
            Config::parse("version: '1'\ndomains:\n  beta:\n    allowed_types: convention\n")
                .expect("parses");
        assert!(
            broken.allowed_types("beta").is_err(),
            "a scalar allowed_types must not pass the gate"
        );
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let config = Config::parse("version: '2'\ndomains: {}\n").expect("parses");
        let err = config.ensure_supported().expect_err("version 2 refused");
        assert!(
            err.to_string()
                .contains("unsupported mulch config version 2")
        );
    }

    #[test]
    fn effective_accessors_backfill_reference_defaults_per_field() {
        // empty config: pure reference defaults
        let empty = Config::empty();
        assert_eq!(
            empty.effective_governance(),
            Governance::reference_default()
        );
        assert_eq!(empty.effective_shelf_life(), ShelfLife::reference_default());
        // partial blocks: set fields survive, missing fields backfill
        // (the reference's withDefaults semantics)
        let partial = Config::parse(
            "version: '1'\ndomains: {}\ngovernance:\n  max_entries: 5\nclassification_defaults:\n  shelf_life:\n    tactical: 7\n",
        )
        .expect("partial config");
        let governance = partial.effective_governance();
        assert_eq!(governance.max_entries, 5);
        assert_eq!(governance.warn_entries, 150);
        assert_eq!(governance.hard_limit, 200);
        let shelf = partial.effective_shelf_life();
        assert_eq!(shelf.tactical, 7);
        assert_eq!(shelf.observational, 30);
        // full blocks: user values win
        let full = Config::parse(
            "version: '1'\ndomains: {}\ngovernance:\n  max_entries: 1\n  warn_entries: 2\n  hard_limit: 3\nclassification_defaults:\n  shelf_life:\n    tactical: 4\n    observational: 5\n",
        )
        .expect("full config");
        assert_eq!(full.effective_governance(), Governance {
            max_entries:  1,
            warn_entries: 2,
            hard_limit:   3,
        });
        assert_eq!(full.effective_shelf_life(), ShelfLife {
            tactical:      4,
            observational: 5,
        });
    }

    #[test]
    fn reference_defaults_are_the_format_contract() {
        assert_eq!(Governance::reference_default(), Governance {
            max_entries:  100,
            warn_entries: 150,
            hard_limit:   200,
        });
        assert_eq!(ShelfLife::reference_default(), ShelfLife {
            tactical:      14,
            observational: 30,
        });
        // StaleRule's default IS the reference shelf life
        assert_eq!(
            crate::stale::StaleRule::default(),
            crate::stale::StaleRule::from_shelf_life(&ShelfLife::reference_default())
        );
    }

    #[test]
    fn wrong_typed_fields_fall_back_to_reference_defaults() {
        let malformed = Config::parse(
            "version: '1'\ndomains: {}\ngovernance:\n  max_entries: 5\n  warn_entries: lots\nclassification_defaults:\n  shelf_life:\n    tactical: nope\n",
        )
        .expect("config parses (yaml is valid)");
        // a wrong-typed field drops the whole block to the reference
        // defaults (documented caveat of the effective accessors)
        assert_eq!(
            malformed.effective_governance(),
            Governance::reference_default()
        );
        assert_eq!(
            malformed.effective_shelf_life(),
            ShelfLife::reference_default()
        );
    }

    /// The audit threshold merge law: defaults ⊕ global ⊕ per-domain
    /// (reference `resolveAuditThresholds`). Rows: absent block,
    /// global-only partial, per-domain partial on top, an unlisted
    /// domain falling back to global.
    #[test]
    fn audit_thresholds_merge_law() {
        // no audit block at all: the pinned defaults
        assert_eq!(
            resolve_audit_thresholds(None, None),
            DEFAULT_AUDIT_THRESHOLDS
        );
        assert_eq!(DEFAULT_AUDIT_THRESHOLDS.floater_max, 0.2);
        assert_eq!(DEFAULT_AUDIT_THRESHOLDS.max_records_per_domain, 200.0);
        // a global PARTIAL block: set fields win, unset backfill
        let cfg = AuditConfig {
            thresholds:     Some(AuditThresholds {
                evidence_coverage: Some(0.1),
                floater_max: Some(0.9),
                ..AuditThresholds::default()
            }),
            ignore_domains: None,
            per_domain:     Some(HashMap::from([("dev".into(), AuditThresholds {
                max_stale: Some(5.0),
                ..AuditThresholds::default()
            })])),
        };
        let global = resolve_audit_thresholds(Some(&cfg), None);
        assert_eq!(global.evidence_coverage, 0.1);
        assert_eq!(global.evidence_coverage_warn, 0.3);
        assert_eq!(global.floater_max, 0.9);
        assert_eq!(global.max_records_per_domain, 200.0);
        // per-domain override merges ON TOP of the merged global
        let dev = resolve_audit_thresholds(Some(&cfg), Some("dev"));
        assert_eq!(dev.max_stale, 5.0);
        assert_eq!(dev.evidence_coverage, 0.1, "global values carry");
        // an unlisted domain falls back to the merged global
        let other = resolve_audit_thresholds(Some(&cfg), Some("ops"));
        assert_eq!(other, global);
        // a block with only per_domain keeps defaults globally
        let per_only = AuditConfig {
            per_domain: Some(HashMap::from([("x".into(), AuditThresholds {
                rule_density_min: Some(1.0),
                ..AuditThresholds::default()
            })])),
            ..AuditConfig::default()
        };
        assert_eq!(
            resolve_audit_thresholds(Some(&per_only), Some("x")).rule_density_min,
            1.0
        );
        assert_eq!(
            resolve_audit_thresholds(Some(&per_only), None).rule_density_min,
            0.25
        );
    }

    /// The audit block parses through the config accessor: thresholds
    /// and per-domain overrides survive, and the serialized resolved
    /// knobs keep the reference's JS number shape (integral doubles
    /// print bare).
    #[test]
    fn audit_block_parses_and_serializes_js_numbers() {
        let config = Config::parse(
            "version: '1'\ndomains:\n  dev: {}\naudit:\n  thresholds:\n    evidence_coverage: 0.1\n  ignore_domains:\n    - ghost\n  per_domain:\n    dev:\n      max_stale: 5\n",
        )
        .expect("parses");
        let audit = config.audit().expect("shape ok").expect("present");
        assert_eq!(
            audit.thresholds.expect("thresholds").evidence_coverage,
            Some(0.1)
        );
        assert_eq!(audit.ignore_domains, Some(vec!["ghost".to_string()]));
        assert!(
            audit
                .per_domain
                .as_ref()
                .expect("per_domain")
                .contains_key("dev")
        );
        // absent block reads None
        assert!(
            Config::parse("version: '1'\ndomains: {}\n")
                .expect("parses")
                .audit()
                .expect("shape ok")
                .is_none()
        );
        // the resolved knobs serialize with JS numbers: 200/0/5 bare,
        // 0.5/0.25 fractional
        let resolved = resolve_audit_thresholds(Some(&audit), Some("dev"));
        let json = serde_json::to_value(resolved).expect("serializes");
        assert_eq!(json["max_records_per_domain"], serde_json::json!(200));
        assert_eq!(json["max_stale"], serde_json::json!(5));
        assert_eq!(json["evidence_coverage"], serde_json::json!(0.1));
        assert_eq!(json["floater_max"], serde_json::json!(0.2));
    }
}
