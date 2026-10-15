//! `mulch init` — byte-parity with the reference 0.10.7 templates.

use std::path::PathBuf;

use crate::cli::GlobalOpts;
use crate::output::{Failure, print_line};

/// Reference README template, pinned byte-for-byte (0.10.7).
const README_TEMPLATE: &str = include_str!("../../fixtures/init/README.md");

/// Reference config template, pinned byte-for-byte (0.10.7): version,
/// empty domains, governance and shelf-life defaults, plus the
/// commented documentation knobs.
const CONFIG_TEMPLATE: &str = include_str!("../../fixtures/init/mulch.config.yaml");

/// The store-local ignore for the writer lock's permanent zero-byte
/// `.lock` sibling (mulch-228d deviation — the reference has no
/// persistent lock file to ignore).
const LOCK_GITIGNORE: &str = "*.lock\n";

/// Runs `init`: fresh message on first creation, restore message when
/// `.mulch/` already exists; missing artifacts are re-created
/// byte-identically, the command never fails short of I/O errors.
pub(super) fn run(opts: &GlobalOpts) -> Result<(), Failure> {
    let cwd: PathBuf = std::env::current_dir().map_err(|source| io_failure(&source))?;
    let store_root = cwd.join(".mulch");
    let fresh = !store_root.is_dir();

    std::fs::create_dir_all(store_root.join("expertise")).map_err(|source| io_failure(&source))?;

    // The store writer lock (mulch-228d): init writes under it too,
    // so a parallel init cannot interleave artifact creation. The
    // `.gitignore` keeps the permanent zero-byte `.lock` sibling out
    // of version control (README DEVIATIONS — the reference writes
    // no such file).
    let _writer_lock = mulch::lock::WriterLock::acquire(&store_root)
        .map_err(|source| Failure::handled("init", crate::output::chain_message(&source)))?;
    // The reference builds its registry at CLI start for EVERY
    // command — `init` included: an existing config with invalid
    // custom_types fails the same global way (probe-pinned).
    if let Ok(text) = std::fs::read_to_string(store_root.join("mulch.config.yaml"))
        && let Ok(config) = mulch::Config::parse(&text)
        && let Err(message) = mulch::Registry::from_config(&config)
    {
        return Err(Failure::config_error(&message));
    }
    write_missing(&store_root.join(".gitignore"), LOCK_GITIGNORE)?;

    write_missing(&store_root.join("README.md"), README_TEMPLATE)?;
    write_missing(&store_root.join("mulch.config.yaml"), CONFIG_TEMPLATE)?;

    if fresh {
        print_line(
            opts.quiet,
            &format!("Initialized .mulch/ in {}", cwd.display()),
        );
    } else {
        print_line(
            opts.quiet,
            "Updated .mulch/ — filled in any missing artifacts.",
        );
    }
    Ok(())
}

/// Writes `contents` to `path` only when the file is absent (re-init
/// restores artifacts byte-identically and never rewrites existing ones).
fn write_missing(path: &std::path::Path, contents: &str) -> Result<(), Failure> {
    if path.is_file() {
        return Ok(());
    }
    std::fs::write(path, contents)
        .map_err(|source| Failure::handled("init", format!("writing {}: {source}", path.display())))
}

/// Maps an I/O failure to the handled-error shape.
fn io_failure(source: &std::io::Error) -> Failure {
    Failure::handled("init", format!("initializing .mulch/: {source}"))
}
