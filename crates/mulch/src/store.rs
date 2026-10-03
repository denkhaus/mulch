//! The `.mulch/` store: `mulch.config.yaml`, `expertise/<domain>.jsonl`
//! (live records) and `archive/<domain>.jsonl` (soft archive).
//!
//! The store is read fully into memory, mutated through the typed
//! accessors, and written back with [`Store::write_all`]. Writes are
//! canonical: config in the reference key order, records as compact
//! JSONL in their file (or canonical) field order, the archive file
//! with the fixed header comment the reference writer emits.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::record::{Record, RecordId};

/// Header line of every `archive/<domain>.jsonl` (pinned from the
/// reference `ml archive` writer, 0.10.7).
const ARCHIVE_HEADER: &str = "# ARCHIVED — not for active use. Run `ml restore <id>` to revive.";

/// An open `.mulch` store.
#[derive(Debug)]
pub struct Store {
    root:      PathBuf,
    config:    Config,
    /// `(domain, records)` in file order; keys sorted for stable writes.
    expertise: Vec<(String, Vec<Record>)>,
    archive:   Vec<(String, Vec<Record>)>,
}

impl Store {
    /// Create a fresh store at `root/.mulch` (the parent directory must
    /// exist) and write its config.
    pub fn create(root: &Path, config: Config) -> Result<Self> {
        config.ensure_supported()?;
        let store_root = root.join(".mulch");
        std::fs::create_dir_all(store_root.join("expertise")).map_err(|source| Error::Write {
            path: store_root.join("expertise"),
            source,
        })?;
        let store = Self {
            root: store_root,
            config,
            expertise: Vec::new(),
            archive: Vec::new(),
        };
        store.write_config()?;
        Ok(store)
    }

    /// Open the store rooted at `root/.mulch`, reading config, live and
    /// archived records.
    pub fn open(root: &Path) -> Result<Self> {
        let store_root = root.join(".mulch");
        let config_path = store_root.join("mulch.config.yaml");
        let text = std::fs::read_to_string(&config_path).map_err(|source| Error::Read {
            path: config_path.clone(),
            source,
        })?;
        let config = Config::parse(&text)?;
        config.ensure_supported()?;

        let mut store = Self {
            root: store_root.clone(),
            config,
            expertise: Vec::new(),
            archive: Vec::new(),
        };
        load_domain_dir(&store_root, "expertise", &mut store.expertise)?;
        load_domain_dir(&store_root, "archive", &mut store.archive)?;
        Ok(store)
    }

    /// The store root (`.mulch`).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The parsed config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Mutable config access; call [`Store::write_config`] to persist.
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Register a domain and create its (possibly empty) live file.
    pub fn add_domain(&mut self, domain: &str) -> Result<()> {
        self.config.add_domain(domain);
        ensure_domain(&mut self.expertise, domain);
        self.write_config()?;
        self.write_domain_file("expertise", domain)
    }

    /// Live domains (config order), archive-only domains excluded.
    pub fn domains(&self) -> Vec<&str> {
        self.config.domains()
    }

    /// Live records of a domain, file order. Unknown domains error with
    /// [`Error::NotFound`] rather than panicking.
    pub fn records(&self, domain: &str) -> Result<&[Record]> {
        domain_slice(&self.expertise, domain, "expertise")
    }

    /// Look up one live record by id.
    pub fn find(&self, domain: &str, id: &RecordId) -> Option<&Record> {
        self.records(domain)
            .ok()?
            .iter()
            .find(|record| record_has_id(record, *id))
    }

    /// Append a live record and persist the domain file.
    pub fn append_record(&mut self, domain: &str, record: Record) -> Result<()> {
        record.ensure_required()?;
        self.config.add_domain(domain);
        ensure_domain(&mut self.expertise, domain);
        if let Some((_, records)) = self.expertise.iter_mut().find(|(name, _)| name == domain) {
            records.push(record);
        }
        self.write_config()?;
        self.write_domain_file("expertise", domain)
    }

    /// Soft-archive a live record (the `ml archive` semantics): it moves
    /// to `archive/<domain>.jsonl` with `status`, `archived_at` and
    /// `archive_reason` appended; both files are persisted.
    pub fn archive_record(
        &mut self,
        domain: &str,
        id: &RecordId,
        archived_at: &str,
        reason: &str,
    ) -> Result<()> {
        let index = {
            let records = domain_records_mut(&mut self.expertise, domain, "expertise")?;
            records.iter().position(|record| record_has_id(record, *id))
        };
        let Some(index) = index else {
            return Err(Error::NotFound {
                id:       id.as_str(),
                location: format!("expertise/{domain}.jsonl"),
            });
        };
        let mut record =
            domain_records_mut(&mut self.expertise, domain, "expertise")?.remove(index);
        record.append("status", serde_json::Value::String("archived".into()));
        record.append("archived_at", serde_json::Value::String(archived_at.into()));
        record.append("archive_reason", serde_json::Value::String(reason.into()));
        ensure_domain(&mut self.archive, domain);
        if let Some((_, archived)) = self.archive.iter_mut().find(|(name, _)| name == domain) {
            archived.push(record);
        }
        self.write_domain_file("expertise", domain)?;
        self.write_domain_file("archive", domain)
    }

    /// Restore a soft-archived record (the `ml restore` semantics): the
    /// three archive fields are stripped, the record returns to the end
    /// of the live file, and the archive file keeps its header.
    pub fn restore(&mut self, id: &RecordId) -> Result<String> {
        let domain = self
            .archive
            .iter()
            .find(|(_, archived)| archived.iter().any(|record| record_has_id(record, *id)))
            .map(|(domain, _)| domain.clone());
        let Some(domain) = domain else {
            return Err(Error::NotFound {
                id:       id.as_str(),
                location: "archive".to_owned(),
            });
        };
        let mut restored = None;
        for (name, archived) in &mut self.archive {
            if *name != domain {
                continue;
            }
            let position = archived
                .iter()
                .position(|record| record_has_id(record, *id));
            if let Some(position) = position {
                let mut record = archived.remove(position);
                for field in ["archive_reason", "archived_at", "status"] {
                    record.remove(field);
                }
                restored = Some(record);
                break;
            }
        }
        let restored = restored.expect("position re-checked after domain match");
        self.config.add_domain(&domain);
        ensure_domain(&mut self.expertise, &domain);
        if let Some((_, records)) = self.expertise.iter_mut().find(|(name, _)| *name == domain) {
            records.push(restored);
        }
        self.write_config()?;
        self.write_domain_file("expertise", &domain)?;
        self.write_domain_file("archive", &domain)?;
        Ok(domain)
    }

    /// Persist config and every non-empty record file.
    pub fn write_all(&self) -> Result<()> {
        self.write_config()?;
        for (domain, _) in &self.expertise {
            self.write_domain_file("expertise", domain)?;
        }
        for (domain, _) in &self.archive {
            self.write_domain_file("archive", domain)?;
        }
        Ok(())
    }

    /// Persist the config document.
    pub fn write_config(&self) -> Result<()> {
        let path = self.root.join("mulch.config.yaml");
        std::fs::write(&path, self.config.to_yaml()).map_err(|source| Error::Write { path, source })
    }

    fn write_domain_file(&self, dir: &str, domain: &str) -> Result<()> {
        let dir_path = self.root.join(dir);
        std::fs::create_dir_all(&dir_path).map_err(|source| Error::Write {
            path: dir_path.clone(),
            source,
        })?;
        let path = dir_path.join(format!("{domain}.jsonl"));
        let source = if dir == "archive" {
            &self.archive
        } else {
            &self.expertise
        };
        let records = domain_slice(source, domain, dir)?;
        let mut text = String::new();
        if dir == "archive" {
            text.push_str(ARCHIVE_HEADER);
            text.push('\n');
        }
        for record in records {
            text.push_str(&record.to_json_line());
            text.push('\n');
        }
        std::fs::write(&path, text).map_err(|source| Error::Write { path, source })
    }
}

fn load_domain_dir(root: &Path, dir: &str, target: &mut Vec<(String, Vec<Record>)>) -> Result<()> {
    let dir_path = root.join(dir);
    if !dir_path.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir_path)
        .map_err(|source| Error::Read {
            path: dir_path.clone(),
            source,
        })?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    entries.sort();
    for path in entries {
        let domain = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned();
        let text = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        let records = text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'))
            .map(|(index, line)| {
                Record::parse(line).map_err(|error| match error {
                    Error::RecordParse { source, .. } => Error::RecordParse {
                        path: path.clone(),
                        line: index + 1,
                        source,
                    },
                    other => other,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        target.push((domain, records));
    }
    Ok(())
}

fn domain_slice<'a>(
    source: &'a [(String, Vec<Record>)],
    domain: &str,
    dir: &str,
) -> Result<&'a [Record]> {
    source
        .iter()
        .find(|(name, _)| name == domain)
        .map(|(_, records)| records.as_slice())
        .ok_or_else(|| Error::NotFound {
            id:       domain.to_owned(),
            location: format!("{dir}/{domain}.jsonl"),
        })
}

fn ensure_domain(source: &mut Vec<(String, Vec<Record>)>, domain: &str) {
    if !source.iter().any(|(name, _)| name == domain) {
        source.push((domain.to_owned(), Vec::new()));
    }
}

fn domain_records_mut<'a>(
    source: &'a mut [(String, Vec<Record>)],
    domain: &str,
    dir: &str,
) -> Result<&'a mut Vec<Record>> {
    source
        .iter_mut()
        .find(|(name, _)| name == domain)
        .map(|(_, records)| records)
        .ok_or_else(|| Error::NotFound {
            id:       domain.to_owned(),
            location: format!("{dir}/{domain}.jsonl"),
        })
}

fn record_has_id(record: &Record, id: RecordId) -> bool {
    record
        .id()
        .is_ok_and(|found| found.is_some_and(|found| found == id))
}
