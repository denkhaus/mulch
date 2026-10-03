//! Output rendering: the CLI boundary between command outcomes and text.
//!
//! Channel contract pinned from the reference 0.10.7 (mulch-da8b probe):
//! success envelopes and validate/doctor failure envelopes print to
//! STDOUT, the status handled-error envelope prints to STDERR, plain
//! summaries print to STDOUT and plain error details to STDERR. JSON is
//! pretty-printed with 2-space indent, like the reference.

use serde_json::{Map, Value};

/// Process exit code for command failures (reference contract).
pub(crate) const EXIT_ERROR: u8 = 1;

/// A command failure carrying the process exit contract.
#[derive(Debug)]
pub(crate) struct Failure {
    /// Rendered error line for plain mode (stderr).
    pub message:            String,
    /// Exit code the process terminates with.
    pub code:               u8,
    /// JSON error envelope body, printed instead of the plain line in
    /// `--json` mode.
    pub envelope:           Value,
    /// Whether the JSON error envelope prints to stderr (the status
    /// handled-error channel; validate/doctor print envelopes to stdout).
    pub envelope_to_stderr: bool,
    /// Set when the command already rendered its full output (report,
    /// envelope, details) — the dispatcher must not print anything more.
    pub rendered:           bool,
}

impl Failure {
    /// Builds a handled error with the reference envelope shape.
    pub(crate) fn handled(command: &str, error: impl Into<String>) -> Self {
        let error = error.into();
        let mut body = Map::new();
        body.insert("success".into(), Value::Bool(false));
        body.insert("command".into(), Value::String(command.into()));
        // The json error field carries no `Error: ` prefix (reference
        // json contract; the prefix is plain-stderr only).
        let json_error = error.strip_prefix("Error: ").unwrap_or(&error).to_string();
        body.insert("error".into(), Value::String(json_error));
        Self {
            message:            error,
            code:               EXIT_ERROR,
            envelope:           Value::Object(body),
            envelope_to_stderr: false,
            rendered:           false,
        }
    }
}

/// Renders an error with its source chain (`local: cause: cause…`),
/// preserving typed causes until this rendering boundary.
pub(crate) fn chain_message(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// Renders a failure: JSON envelope or the plain stderr line.
pub(crate) fn render_failure(failure: &Failure, json: bool, envelope_to_stderr: bool) {
    if failure.rendered {
        return;
    }
    if json {
        print_json(&failure.envelope, envelope_to_stderr);
    } else {
        #[allow(clippy::print_stderr, reason = "error rendering is the CLI boundary")]
        {
            if !failure.message.is_empty() {
                eprintln!("{}", failure.message);
            }
        }
    }
}

/// Prints a JSON document, 2-space indented, on the chosen stream.
#[allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "rendering is this module's job"
)]
pub(crate) fn print_json(value: &Value, stderr: bool) {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into());
    if stderr {
        eprintln!("{text}");
    } else {
        println!("{text}");
    }
}

/// Prints a plain stdout line unless suppressed.
#[allow(clippy::print_stdout, reason = "rendering is this module's job")]
pub(crate) fn print_line(quiet: bool, text: &str) {
    if !quiet {
        println!("{text}");
    }
}

/// Builds a success envelope: `{"success":true,"command":…}` plus fields.
pub(crate) fn success_envelope(command: &str, fields: Map<String, Value>) -> Value {
    let mut body = Map::new();
    body.insert("success".into(), Value::Bool(true));
    body.insert("command".into(), Value::String(command.into()));
    body.extend(fields);
    Value::Object(body)
}
