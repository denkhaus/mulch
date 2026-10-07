//! `mulch.config.yaml` — the store's configuration document.
//!
//! The reference CLI (0.10.7) rewrites this file through its YAML
//! serializer on every mutating command, so the Rust core preserves the
//! document as an ordered mapping and exposes typed accessors for the
//! fields it interprets. Unknown top-level keys (including the optional
//! `prime`/`search` knobs and future additive fields) survive a
//! read-modify-write cycle in place.

use std::path::PathBuf;

use serde::ser::Error as _;
use serde_yaml::Mapping;

use crate::error::{Error, Result};

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

impl Governance {
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
    pub default_mode: Option<String>,
    /// Trust-tier ranking weights; unset fields keep their default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier_weights: Option<TierWeights>,
}

/// `search` optional knob block, typed.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct Search {
    /// Multiplier applied to BM25 scores; 0 disables the boost.
    pub boost_factor: f64,
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
    /// their set values, like the reference's `withDefaults`).
    #[must_use]
    pub fn effective_governance(&self) -> Governance {
        serde_yaml::from_value(self.governance_mapping())
            .unwrap_or_else(|_| Governance::reference_default())
    }

    /// The effective shelf life: the user block with the reference
    /// defaults backfilled per field.
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

    /// Any other top-level key (additive fields, `custom_types`, `hooks`,
    /// ...), as the raw YAML value.
    pub fn extra(&self, key: &str) -> Option<&serde_yaml::Value> {
        self.raw.get(yaml_str(key))
    }

    /// Register a domain (a no-op when it already exists).
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
}
