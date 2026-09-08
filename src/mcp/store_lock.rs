//! Cross-process advisory locking for JSON-backed project stores.
//!
//! [`MemoryMcp`](crate::mcp::MemoryMcp), [`TasksMcp`](crate::mcp::TasksMcp),
//! and [`ConnectorsMcp`](crate::mcp::ConnectorsMcp) each persist their state
//! as a single JSON file under a project's `.agent/` directory. When two AI
//! agents work against the *same* project at once — the core scenario this
//! runtime exists for — each agent is typically a separate OS process (one
//! per stdio connection, or concurrent handlers in one SSE process), so an
//! in-process `Mutex`/`RwLock` cannot protect a read-modify-write cycle
//! against a sibling process doing the same thing at the same time.
//!
//! [`StoreLock`] closes that gap without adding a dependency: it uses
//! exclusive file creation (`O_CREAT | O_EXCL` on Unix, `CREATE_NEW` on
//! Windows — `std::fs::OpenOptions::create_new` maps to both) as a portable
//! mutex. Acquiring the lock blocks (with a bounded timeout) until any other
//! holder releases it; the guard removes the lock file on drop, including on
//! an early return via `?`. A lock file left behind by a crashed holder is
//! treated as stale after [`STALE_LOCK_AGE`] and reclaimed rather than
//! waited on forever, so a killed agent cannot wedge a project permanently.

use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// How long a lock file may sit untouched before it is treated as abandoned
/// (its holder crashed or was killed without a chance to clean up) and
/// reclaimed by the next acquirer instead of being waited on forever.
const STALE_LOCK_AGE: Duration = Duration::from_secs(30);
/// How long [`StoreLock::acquire`] will wait for a live holder to release
/// the lock before giving up. Bounded so a wedged (but not stale) holder
/// cannot hang an agent's tool call indefinitely.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(10);
/// Delay between acquisition attempts while another process holds the lock.
const RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// A held advisory lock on a store file's `<file>.lock` sibling. The lock is
/// released (the lock file removed) when this guard is dropped, so callers
/// should hold it for the shortest span that covers the load-modify-save
/// cycle it protects and let it fall out of scope (or `drop(guard)`
/// explicitly) as soon as that cycle finishes.
pub struct StoreLock {
    lock_path: PathBuf,
}

impl StoreLock {
    /// Acquires an exclusive lock guarding `target` (e.g.
    /// `.agent/memory.json`), blocking the current thread until it is free
    /// or [`ACQUIRE_TIMEOUT`] elapses.
    ///
    /// This only serializes callers that go through `StoreLock` for the same
    /// `target`; it is not a general filesystem lock. It fails closed: a
    /// lock that cannot be acquired within the timeout is reported as an
    /// error rather than silently proceeding unlocked.
    pub fn acquire(target: &Path) -> Result<Self> {
        let lock_path = lock_path_for(target);
        let deadline = Instant::now() + ACQUIRE_TIMEOUT;
        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(_) => return Ok(Self { lock_path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if is_stale(&lock_path) {
                        // Best-effort reclaim of an abandoned lock. If the
                        // remove races with another reclaimer, the next loop
                        // iteration's create_new simply fails again and we
                        // fall back to waiting normally.
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    if Instant::now() >= deadline {
                        bail!(
                            "timed out waiting for another agent to release the lock on {}",
                            target.display()
                        );
                    }
                    thread::sleep(RETRY_INTERVAL);
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    // Windows can transiently report ACCESS_DENIED (os error
                    // 5) instead of AlreadyExists when create_new races a
                    // concurrent holder's delete of the same lock file: the
                    // directory entry is momentarily in a delete-pending
                    // state under NTFS. It resolves within microseconds, so
                    // treat it like AlreadyExists (retry within the same
                    // deadline) rather than failing the whole acquire — a
                    // hard failure here would spuriously break exactly the
                    // multi-agent contention this lock exists to serialize.
                    if Instant::now() >= deadline {
                        bail!(
                            "timed out waiting for another agent to release the lock on {}",
                            target.display()
                        );
                    }
                    thread::sleep(RETRY_INTERVAL);
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!("failed to create lock file {}", lock_path.display())
                    })
                }
            }
        }
    }
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

/// Derives the sibling lock-file path for a store file (`memory.json` ->
/// `memory.json.lock`), kept alongside it so it lives and dies with the
/// same `.agent/` directory as the store it guards.
fn lock_path_for(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".lock");
    target.with_file_name(name)
}

/// A lock file is stale once it is older than [`STALE_LOCK_AGE`]. If its
/// metadata can't be read at all, treat it as stale too rather than risk
/// waiting forever on a file whose age we can't determine.
fn is_stale(lock_path: &Path) -> bool {
    fs::metadata(lock_path)
        .and_then(|m| m.modified())
        .map(|modified| {
            SystemTime::now()
                .duration_since(modified)
                .map(|age| age > STALE_LOCK_AGE)
                .unwrap_or(false)
        })
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn second_acquire_blocks_until_first_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("store.json");
        let first = StoreLock::acquire(&target).unwrap();
        assert!(target.with_file_name("store.json.lock").exists());

        let target2 = target.clone();
        let handle = thread::spawn(move || {
            // Blocks until `first` is dropped on the main thread below.
            let _second = StoreLock::acquire(&target2).unwrap();
        });

        // Give the spawned thread a moment to start waiting, then release.
        thread::sleep(Duration::from_millis(50));
        drop(first);
        handle.join().unwrap();
        // The second guard is dropped at the end of the closure, so the
        // lock file should be gone again.
        assert!(!target.with_file_name("store.json.lock").exists());
    }

    #[test]
    fn stale_lock_is_reclaimed_instead_of_waited_on() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("store.json");
        let lock_path = target.with_file_name("store.json.lock");
        fs::write(&lock_path, b"").unwrap();
        // Backdate the lock file well past STALE_LOCK_AGE.
        let stale_time = SystemTime::now() - Duration::from_secs(3600);
        set_file_mtime(&lock_path, stale_time);

        // Should reclaim immediately rather than blocking for ACQUIRE_TIMEOUT.
        let started = Instant::now();
        let guard = StoreLock::acquire(&target).unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(guard);
    }

    #[test]
    fn concurrent_read_modify_write_cycles_serialize_without_lost_updates() {
        // Simulates two "agents" (threads racing on the same file) each
        // incrementing a shared counter through a naive load-mutate-save
        // cycle guarded by StoreLock. Without the lock this reliably loses
        // updates; with it, every increment must be preserved.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("counter.json");
        fs::write(&target, "0").unwrap();

        let counter_lost_without_lock = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let target = target.clone();
            let counter = Arc::clone(&counter_lost_without_lock);
            handles.push(thread::spawn(move || {
                for _ in 0..25 {
                    let _lock = StoreLock::acquire(&target).unwrap();
                    let current: u64 = fs::read_to_string(&target).unwrap().trim().parse().unwrap();
                    // Yield to make an unguarded race far more likely to
                    // manifest if the lock ever failed to exclude a sibling.
                    thread::yield_now();
                    fs::write(&target, (current + 1).to_string()).unwrap();
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let final_value: u64 = fs::read_to_string(&target).unwrap().trim().parse().unwrap();
        assert_eq!(final_value, 8 * 25);
        assert_eq!(counter_lost_without_lock.load(Ordering::SeqCst), 200);
    }

    /// Test-only helper: sets a file's mtime without adding a filetime
    /// dependency, using each platform's existing toolchain.
    fn set_file_mtime(path: &Path, time: SystemTime) {
        let secs = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ts = filetime_touch::to_timeval(secs);
        filetime_touch::set_mtime(path, ts);
    }

    /// Minimal libc-free mtime setter used only by the staleness test above.
    mod filetime_touch {
        use std::path::Path;

        pub fn to_timeval(secs: u64) -> u64 {
            secs
        }

        /// Sets both atime and mtime on `path` to `secs` since the Unix
        /// epoch by round-tripping through `std::fs::FileTimes`.
        pub fn set_mtime(path: &Path, secs: u64) {
            let time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs);
            let times = std::fs::FileTimes::new().set_modified(time);
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .expect("open for mtime update");
            file.set_times(times).expect("set mtime");
        }
    }
}
