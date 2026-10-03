//! `mulch.config.yaml` — the store's configuration document.
//!
//! The reference CLI (0.10.7) rewrites this file through its YAML
//! serializer on every mutating command, so the Rust core preserves the
//! document as an ordered mapping and exposes typed accessors for the
//! fields it interprets. Unknown top-level keys (including the optional
//! `prime`/`search` knobs and future additive fields) survive a
//! read-modify-write cycle in place.

use std::path::PathBuf;

use serde_yaml::Mapping;

use crate::error::{Error, Result};

/// Config version written by `Config::default` and understood by the core.
pub const SUPPORTED_VERSION: &str = "1";

/// Governance thresholds (`governance` block), typed.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct Governance {
    /// Soft target of live records per domain.
    pub max_entries:  u64,
    /// Warning threshold before the hard limit.
    pub warn_entries: u64,
    /// Hard ceiling; writes beyond this are refused by the reference CLI.
    pub hard_limit:   u64,
}

/// `classification_defaults.shelf_life` block, typed.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct ShelfLife {
    /// Days a tactical record stays fresh.
    pub tactical:      u64,
    /// Days an observational record stays fresh.
    pub observational: u64,
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
        config.set_governance(Governance {
            max_entries:  100,
            warn_entries: 150,
            hard_limit:   200,
        });
        config.set_shelf_life(ShelfLife {
            tactical:      14,
            observational: 30,
        });
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
        let mut ordered = Mapping::new();
        for key in [
            "version",
            "domains",
            "governance",
            "classification_defaults",
        ] {
            if let Some(value) = self.raw.get(yaml_str(key)) {
                ordered.insert(yaml_str(key), value.clone());
            }
        }
        for (key, value) in &self.raw {
            if !matches!(key.as_str(), Some(k) if matches!(k, "version" | "domains" | "governance" | "classification_defaults"))
            {
                ordered.insert(key.clone(), value.clone());
            }
        }
        serde_yaml::to_string(&ordered).expect("YAML mapping serialization is infallible")
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

    /// Remove a domain entry (a no-op when absent).
    pub fn remove_domain(&mut self, domain: &str) {
        if let Some(domains) = self
            .raw
            .get_mut(yaml_str("domains"))
            .and_then(serde_yaml::Value::as_mapping_mut)
        {
            domains.remove(yaml_str(domain));
        }
    }

    /// A domain's `allowed_types` list, when configured.
    pub fn allowed_types(&self, domain: &str) -> Option<Vec<String>> {
        let domains = self.raw.get(yaml_str("domains"))?.as_mapping()?;
        let entry = domains.get(yaml_str(domain))?.as_mapping()?;
        let list = entry.get(yaml_str("allowed_types"))?.as_sequence()?;
        Some(
            list.iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect(),
        )
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
    fn unsupported_version_is_rejected() {
        let config = Config::parse("version: '2'\ndomains: {}\n").expect("parses");
        let err = config.ensure_supported().expect_err("version 2 refused");
        assert!(
            err.to_string()
                .contains("unsupported mulch config version 2")
        );
    }
}
