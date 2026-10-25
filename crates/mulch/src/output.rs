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
    /// The global config-error surface (mulch-b4bd): every store
    /// command fails before doing work when a `custom_types` block
    /// violates the registry validation law. Plain mode prints the
    /// two reference lines on stderr; json mode prints the envelope
    /// on stderr — with `command: "init"`, the reference's global
    /// registry-init handler hardcodes it regardless of the actual
    /// command (probe-pinned vs ml 0.10.7).
    pub(crate) fn config_error(message: &str) -> Self {
        let mut body = Map::new();
        body.insert("success".into(), Value::Bool(false));
        body.insert("command".into(), Value::String("init".into()));
        body.insert(
            "error".into(),
            Value::String(format!("Config error: {message}")),
        );
        Self {
            message:            format!(
                "Config error: {message}\nEdit .mulch/mulch.config.yaml to resolve, or run `mulch doctor` for details."
            ),
            code:               EXIT_ERROR,
            envelope:           Value::Object(body),
            envelope_to_stderr: true,
            rendered:           false,
        }
    }

    /// Builds a handled error whose JSON envelope renders on stderr
    /// (the per-command channel contract; probe-pinned).
    pub(crate) fn handled_on_stderr(command: &str, error: impl Into<String>) -> Self {
        let mut failure = Self::handled(command, error);
        failure.envelope_to_stderr = true;
        failure
    }

    /// The `Error:` prefix idiom, named once (mulch-39eb): plain
    /// stderr renders `Error: <message>`, the json envelope carries
    /// the bare message ([`Failure::handled`] strips the prefix this
    /// adds). Sites with MULTI-LINE or hint-carrying wordings
    /// (`commands::failure`'s domain shapes) deliberately stay on
    /// `handled*` — the deviators are visible by not calling this.
    pub(crate) fn error_line(command: &str, message: impl std::fmt::Display) -> Self {
        let mut failure = Self::handled(command, format!("Error: {message}"));
        failure.envelope_to_stderr = true;
        failure
    }

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
pub(crate) fn render_failure(failure: &Failure, json: bool) {
    if failure.rendered {
        return;
    }
    if json {
        print_json(&failure.envelope, failure.envelope_to_stderr);
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

/// Builds a failure envelope: `{"success":false,"command":…}` plus
/// fields — the failure mirror of [`success_envelope`] (mulch-3368:
/// the shape was hand-assembled at four sites).
pub(crate) fn failure_envelope(command: &str, fields: Map<String, Value>) -> Value {
    let mut body = Map::new();
    body.insert("success".into(), Value::Bool(false));
    body.insert("command".into(), Value::String(command.into()));
    body.extend(fields);
    Value::Object(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The envelope's key order is pinned: `success`, `command`, then
    /// the caller's fields in insertion order (serde preserve_order).
    #[test]
    fn failure_envelope_puts_success_command_before_fields() {
        let mut fields = Map::new();
        fields.insert("error".into(), Value::String("boom".into()));
        fields.insert("fixed".into(), Value::Array(Vec::new()));
        let envelope = failure_envelope("doctor", fields);
        let Value::Object(body) = &envelope else {
            panic!("envelope is an object");
        };
        assert_eq!(body.keys().collect::<Vec<_>>(), [
            "success", "command", "error", "fixed"
        ]);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(body["command"], Value::String("doctor".into()));
    }
}
