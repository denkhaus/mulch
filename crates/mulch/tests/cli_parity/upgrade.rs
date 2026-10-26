//! Battery for `mulch upgrade` (mulch-176f) — the documented
//! DEVIATION surface (README): this binary is cargo-installed, so
//! there is no registry check and no install step. The battery pins
//! OUR side byte-for-byte (the deviation's own expectation, the
//! mulch-0e39 rule) and runs the reference `ml upgrade --check` only
//! as a recorded observation: its output depends on the npm registry
//! and the reference's published version (probed 2026-10-26: registry
//! reachable, `! Update available: v0.10.7 → v0.11.0`), none of which
//! this port may mirror.

use std::path::Path;

use super::support::{TempDir, mulch_bin, reference_ml, run_in};

/// The deviation's plain law: exit 0, the `✓` native-binary line
/// with the running version and the cargo hint — on both the plain
/// run and `--check` (check and run are the same report here).
#[test]
fn upgrade_reports_the_native_binary_path() {
    let dir = TempDir::new("upgrade-plain");
    for args in [vec!["upgrade"], vec!["upgrade", "--check"]] {
        let our = run_in(&dir.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?}: exit 0");
        assert!(our.stderr.is_empty(), "{args:?}: stderr empty");
        assert_eq!(
            our.stdout,
            format!(
                "✓ Native binary v{version} — update via cargo install (registry check not \
                 applicable)\n",
                version = env!("CARGO_PKG_VERSION")
            ),
            "{args:?}: the plain line"
        );
    }
    // --quiet suppresses the line entirely (print_line contract)
    let quiet = run_in(&dir.0, Path::new(mulch_bin()), &["--quiet", "upgrade"]);
    assert_eq!(quiet.code, 0);
    assert_eq!(quiet.stdout, "", "--quiet suppresses the plain line");
}

/// The deviation's json law: the reference's field names with honest
/// values — latest and upToDate unknown (null), updated false, plus
/// the note explaining the divergence.
#[test]
fn upgrade_json_keeps_reference_fields_with_honest_nulls() {
    let dir = TempDir::new("upgrade-json");
    let our = run_in(&dir.0, Path::new(mulch_bin()), &["--json", "upgrade"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stderr, "", "json mode prints only the envelope");
    let expected = format!(
        "{{\n  \"success\": true,\n  \"command\": \"upgrade\",\n  \"current\": \"{version}\",\n  \
         \"latest\": null,\n  \"upToDate\": null,\n  \"updated\": false,\n  \"note\": \"Native \
         binary — update via cargo install (registry check not applicable)\"\n}}\n",
        version = env!("CARGO_PKG_VERSION")
    );
    assert_eq!(our.stdout, expected, "the json envelope");
}

/// Observation-only: the reference's check output is
/// environment-dependent (registry reachability, published version).
/// It must not crash our battery when offline — we only record what
/// it printed.
#[test]
fn upgrade_reference_check_is_recorded_not_asserted() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let dir = TempDir::new("upgrade-ref");
    let their = run_in(&dir.0, &ml, &["upgrade", "--check"]);
    eprintln!(
        "reference `ml upgrade --check` (observation only): exit {} stdout {:?} stderr {:?}",
        their.code, their.stdout, their.stderr
    );
    // The one shape law that holds regardless of registry state: the
    // reference never writes to stdout on the failure arm and never
    // exits anything but 0 or 1.
    assert!(
        their.code == 0 || their.code == 1,
        "reference exit within its contract"
    );
}
