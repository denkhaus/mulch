//! The `.mulch/` writer lock: advisory, exclusive, crash-safe.
//!
//! Every mutating command wraps its whole load→mutate→save cycle in
//! [`WriterLock`] (mulch-228d, ported from seeds-0d7b): an advisory
//! exclusive lock on `.mulch/.lock`, taken via
//! [`std::fs::File::try_lock`] (flock semantics on Unix). Two mulch
//! processes can therefore never silently drop each other's records —
//! the second writer waits, and fails fast once the timeout passes.
//!
//! The lock is deliberately kernel-owned: it lives on the open file
//! description, so the kernel releases it when the holder exits,
//! crashed or not — a dead writer cannot wedge the store, and there
//! is no stale-lockfile recovery machinery. This deviates from the
//! reference's per-file `O_CREAT | O_EXCL` lockfiles with 30-second
//! staleness recovery (README DEVIATIONS); the wait budget matches
//! (5 seconds).
//!
//! The lock file is a permanent zero-byte `.mulch/` sibling that
//! `init`'s `.mulch/.gitignore` (`*.lock`) keeps out of version
//! control.

use std::fs::OpenOptions;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::error::Error;

/// How long a writer waits for the lock before failing fast.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Poll interval while waiting for the lock.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The advisory writer lock on `.mulch/.lock`.
///
/// Dropping the guard closes the file description and releases the
/// lock; process exit does the same through the kernel.
#[derive(Debug)]
pub struct WriterLock {
    /// Held for its open file description — the kernel owns the lock
    /// and releases it when the description closes (drop or process
    /// exit). Never read or written.
    #[expect(
        dead_code,
        reason = "the file is the lock; its contents are irrelevant"
    )]
    file: std::fs::File,
}

impl WriterLock {
    /// Acquires the writer lock at `root/.lock`, waiting up to the
    /// 5-second acquire budget.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LockTimeout`] when another process still holds
    /// the lock past the timeout, and [`Error::Lock`] on lock-file or
    /// locking failures.
    pub fn acquire(root: &Path) -> Result<Self, Error> {
        Self::acquire_with_timeout(root, ACQUIRE_TIMEOUT)
    }

    /// Acquires the writer lock with an explicit wait budget (the
    /// unit pins' seam — the public surface is [`WriterLock::acquire`]).
    ///
    /// # Errors
    ///
    /// Returns [`Error::LockTimeout`] when another process still holds
    /// the lock past `timeout`, and [`Error::Lock`] on lock-file or
    /// locking failures.
    fn acquire_with_timeout(root: &Path, timeout: Duration) -> Result<Self, Error> {
        let path = root.join(".lock");
        // create + !truncate: the lock file is a permanent zero-byte
        // sibling; opening must never clobber an existing one.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| Error::Lock {
                path: path.clone(),
                source,
            })?;
        let deadline = Instant::now() + timeout;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { file }),
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(source)) => {
                    return Err(Error::Lock { path, source });
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::LockTimeout { path });
            }
            std::thread::sleep(POLL_INTERVAL.min(remaining));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc;

    use super::*;

    fn temp_store_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mulch-lock-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp store dir");
        dir
    }

    #[test]
    fn second_writer_times_out_while_first_holds() {
        let dir = temp_store_dir("timeout");
        let first = WriterLock::acquire(&dir).expect("first writer acquires");
        let error = WriterLock::acquire_with_timeout(&dir, Duration::from_millis(100))
            .expect_err("second writer must not acquire");
        assert!(
            matches!(error, Error::LockTimeout { .. }),
            "expected LockTimeout, got {error:?}"
        );
        drop(first);
    }

    #[test]
    fn lock_releases_on_drop() {
        let dir = temp_store_dir("release");
        drop(WriterLock::acquire(&dir).expect("first writer acquires"));
        WriterLock::acquire_with_timeout(&dir, Duration::from_millis(100))
            .expect("reacquires after drop");
    }

    #[test]
    fn waiting_writer_acquires_after_release() {
        let dir = temp_store_dir("handoff");
        let first = WriterLock::acquire(&dir).expect("first writer acquires");
        let (held_tx, held_rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let _second =
                WriterLock::acquire_with_timeout(&dir, Duration::from_secs(5)).expect("waits");
            held_tx.send(()).expect("sender alive");
        });
        // While `first` holds the lock, the waiter must not get it.
        assert!(held_rx.recv_timeout(Duration::from_millis(150)).is_err());
        drop(first);
        held_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("waiter acquired after release");
        waiter.join().expect("waiter thread");
    }
}
