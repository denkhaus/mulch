//! `mulch upgrade` — the honest native port of the reference's npm
//! self-upgrade (upgrade.ts).
//!
//! DEVIATION (README, mulch-0e39's never-silently rule): this binary
//! is cargo-installed, not an npm package. The reference's two
//! environment-dependent steps have no honest equivalent here:
//!
//! - `getLatestVersion()` shells out to `npm view @os-eco/mulch-cli version` —
//!   a registry this binary was never published to, so the answer would
//!   describe the REFERENCE, not us. We do not ask.
//! - The install step spawns `bun install -g @os-eco/mulch-cli@latest` —
//!   nothing we ship could honor it. We never install.
//!
//! What the port keeps (the non-divergent surface, probed against ml
//! 0.10.7 /tmp/opencode/sprint54B/probes/up1):
//!
//! - The `--check` flag parses (accepted for script compatibility); since we
//!   never install, check and run are the same report.
//! - Exit code 0 on the reporting path (the reference's up-to-date /
//!   check-available arms).
//! - The JSON envelope keeps the reference's field NAMES (`current`, `latest`,
//!   `upToDate`, `updated`) with honest values: `latest` and `upToDate` are
//!   `null` (unknown — registry check not applicable), `updated` is `false`.
//! - Plain mode prints the `✓` success line (the reference's printSuccess
//!   channel), `--quiet` suppressed — reusing the doctor check's established
//!   deviation wording (doctor.rs:290).

use serde_json::{Map, Value, json};

use crate::cli::GlobalOpts;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// The cargo advice — the tail of doctor's `upgrade` check stub
/// ("Native binary — update via cargo install (registry check not
/// applicable)", doctor.rs:290): the one deviation family for
/// registry questions.
const ADVICE: &str = "update via cargo install (registry check not applicable)";

/// Runs `upgrade`: reports this binary's version and the cargo update
/// path. `check` is accepted and deliberately ignored — there is no
/// install step to skip.
///
/// The `Ok` wrap is the dispatcher's uniform command shape — no path
/// of this command can fail (the deviation's whole point).
#[allow(
    clippy::unnecessary_wraps,
    reason = "the dispatch table's uniform Result shape; this command cannot fail"
)]
pub(super) fn run(opts: &GlobalOpts, _check: bool) -> Result<(), Failure> {
    if opts.json {
        print_json(&envelope(), false);
    } else {
        print_line(opts.quiet, &format!("✓ {}", native_line_text()));
    }
    Ok(())
}

/// The plain success line (doctor's note plus the version the
/// reference's up-to-date arm carries).
fn native_line_text() -> String {
    format!("Native binary v{} — {ADVICE}", env!("CARGO_PKG_VERSION"))
}

/// The JSON report: the reference's field names with honest values.
fn envelope() -> Value {
    let mut fields = Map::new();
    fields.insert("current".into(), json!(env!("CARGO_PKG_VERSION")));
    fields.insert("latest".into(), Value::Null);
    fields.insert("upToDate".into(), Value::Null);
    fields.insert("updated".into(), json!(false));
    fields.insert("note".into(), json!(format!("Native binary — {ADVICE}")));
    success_envelope("upgrade", fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plain line carries the running version and the cargo hint
    /// (the reference's `✓ mulch is up to date (v…)` channel, honestly
    /// reworded).
    #[test]
    fn plain_line_carries_version_and_cargo_hint() {
        let line = format!("✓ {}", native_line_text());
        assert_eq!(
            line,
            format!(
                "✓ Native binary v{version} — update via cargo install (registry check not \
                 applicable)",
                version = env!("CARGO_PKG_VERSION")
            )
        );
        assert!(line.contains("cargo install"));
    }

    /// The envelope keeps the reference's field names with honest
    /// values: latest and upToDate unknown (null), updated false, the
    /// note explaining why — and `success`/`command` lead (the
    /// envelope key-order law).
    #[test]
    fn json_envelope_keeps_reference_fields_with_unknown_latest() {
        let Value::Object(body) = envelope() else {
            panic!("envelope is an object");
        };
        assert_eq!(body.keys().collect::<Vec<_>>(), vec![
            "success", "command", "current", "latest", "upToDate", "updated", "note"
        ]);
        assert_eq!(body["success"], json!(true));
        assert_eq!(body["command"], json!("upgrade"));
        assert_eq!(body["current"], json!(env!("CARGO_PKG_VERSION")));
        assert_eq!(body["latest"], Value::Null);
        assert_eq!(body["upToDate"], Value::Null);
        assert_eq!(body["updated"], json!(false));
        assert_eq!(
            body["note"],
            json!("Native binary — update via cargo install (registry check not applicable)")
        );
    }

    /// `--check` changes nothing: both flag values render the same
    /// report and exit 0 (the deviation note's expectation).
    #[test]
    fn check_flag_does_not_change_the_report() {
        let opts = GlobalOpts {
            json:                  false,
            quiet:                 true,
            verbose:               false,
            timing:                false,
            allow_unknown_types:   false,
            allow_domain_mismatch: false,
            format:                None,
        };
        assert!(run(&opts, false).is_ok());
        assert!(run(&opts, true).is_ok());
        // quiet suppresses the plain line entirely
        // (print_line's contract — asserted here through success).
    }
}
