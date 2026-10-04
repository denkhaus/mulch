//! Typed errors for the mulch format core.
//!
//! Layer boundary (style guide): branch-oriented variants, structured
//! fields (paths, ids), infrastructure causes kept as sources.

use std::path::PathBuf;

/// Crate-local error surface for the format core.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A store file could not be read.
    #[error("reading {path}")]
    Read {
        /// The file that failed to open.
        path:   PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },

    /// A domain name violates `^[a-zA-Z0-9][a-zA-Z0-9_-]*$`.
    #[error(
        "Invalid domain name: \"{domain}\". Only alphanumeric characters, hyphens, and underscores are allowed."
    )]
    InvalidDomain {
        /// The rejected name.
        domain: String,
    },

    /// A store file could not be written.
    #[error("writing {path}")]
    Write {
        /// The file that failed to write.
        path:   PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },

    /// A store file could not be removed.
    #[error("removing {path}")]
    Remove {
        /// The file that failed to be removed.
        path:   PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },

    /// `mulch.config.yaml` is not valid YAML or not a mapping.
    #[error("parsing config {path}")]
    ConfigParse {
        /// The config file that failed to parse.
        path:   PathBuf,
        /// The underlying YAML failure.
        #[source]
        source: serde_yaml::Error,
    },

    /// A config field has the wrong shape.
    #[error("parsing config field {field} in {path}")]
    ConfigField {
        /// The config file being parsed.
        path:   PathBuf,
        /// The field whose value did not fit the expected shape.
        field:  &'static str,
        /// The underlying YAML failure.
        #[source]
        source: serde_yaml::Error,
    },

    /// A JSONL line in a rewritten file is not valid JSON.
    #[error("Malformed JSONL at {path}:{line}")]
    MalformedLine {
        /// The file being read.
        path:    PathBuf,
        /// The 1-based physical line number.
        line:    usize,
        /// Truncated line preview (reference `slice(0, 77) + "..."`).
        preview: String,
        /// The parser's reason.
        reason:  String,
    },

    /// A record carries a type that is not registered.
    #[error("Unknown record type \"{record_type}\" at {path}:{line}")]
    UnknownRecordType {
        /// The file being read.
        path:        PathBuf,
        /// The 1-based physical line number.
        line:        usize,
        /// The record id, when present.
        id:          Option<String>,
        /// The offending type name.
        record_type: String,
    },

    /// A JSONL line parsed to a scalar or `null` (the reference reader
    /// crashes on such lines — README DEVIATIONS: clean error).
    #[error("non-object record at {path} line {line}")]
    NotAnObject {
        /// The JSONL file being parsed.
        path:    PathBuf,
        /// The 1-based line number.
        line:    usize,
        /// The offending line (truncated like [`Error::MalformedLine`]).
        preview: String,
    },

    /// A record is missing a field the format requires.
    #[error("record {path} line {line} is missing required field `{field}`")]
    MissingField {
        /// The JSONL file being parsed.
        path:  PathBuf,
        /// The 1-based line number.
        line:  usize,
        /// The absent field name.
        field: &'static str,
    },

    /// The store's config declares a version this crate cannot handle.
    #[error("unsupported mulch config version {version} (supported: {supported})")]
    UnsupportedVersion {
        /// The version string found on disk.
        version:   String,
        /// The versions this build understands.
        supported: &'static str,
    },

    /// No record with the given id exists where the operation looks.
    #[error("record {id} not found in {location}")]
    NotFound {
        /// The record id that was not found.
        id:       String,
        /// Where the lookup happened (`expertise/<domain>.jsonl` or the
        /// archive).
        location: String,
    },

    /// The store root does not hold a `.mulch` store.
    #[error("no mulch store at {path}: mulch.config.yaml not found")]
    NoStore {
        /// The directory that was expected to contain the store.
        path: PathBuf,
    },
}

/// Result alias for the format core.
pub type Result<T> = std::result::Result<T, Error>;
