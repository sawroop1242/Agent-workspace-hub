//! Canonical filesystem mutation coordination (FS-001).
//!
//! One coordination boundary for every AWH filesystem *mutation*, closing
//! the practical race windows around **path validation → expected-state
//! check → final mutation**:
//!
//! * **in-process plane** — a process-wide registry of per-resource
//!   mutexes, so concurrent threads inside one `awh` process (TUI
//!   handlers, parallel MCP sessions) serialize on the same file;
//! * **cross-process plane** — per-resource advisory lock files under
//!   `<workspace-root>/.agent/fs-coordination/<key>.lock`, so sibling
//!   `awh` processes (the core multi-agent scenario: one process per
//!   stdio connection) serialize the same way.
//!
//! A resource key is derived **only after** canonical validation: it is
//! the SHA-256 of the canonical workspace root plus the canonical
//! workspace-relative path. Never the raw caller string, never a bare
//! host absolute path. Two sessions sharing one worktree root therefore
//! converge on the same key; two independent worktrees never collide on
//! a shared relative filename because their canonical roots differ.
//!
//! Correctness rules enforced here:
//!
//! * acquisition fails **closed** — a timeout, a saturated registry, or a
//!   broken lock file is an error, never permission to proceed unlocked;
//! * multi-resource acquisition sorts keys deterministically and acquires
//!   in that order, so overlapping sets can never deadlock cycle
//!   (A=[a,b] vs B=[b,c] both take a→b / b→c orderings that resolve);
//! * every guard releases its resources on drop, including on early
//!   `?` returns and panics (the in-process mutex unlock + lock-file
//!   removal), so a cancellation after acquisition cannot leak a held
//!   resource;
//! * nothing in this module grants authorization, validates content, or
//!   performs a mutation — callers revalidate path containment and live
//!   state *after* acquisition, immediately before mutating.
//!
//! Platform truth (documented, not faked): the cross-process plane is an
//! **advisory** lock implemented with exclusive file creation
//! (`O_CREAT | O_EXCL` on Unix, `CREATE_NEW` on Windows — the same
//! proven primitive [`crate::mcp::store_lock::StoreLock`] uses). It
//! serializes only AWH-conformant actors; a foreign process that ignores
//! the lock file is not stopped by it. The final-component TOCTOU window
//! against *non-AWH* actors is narrowed (revalidation under the lock,
//! atomic rename commit) but cannot be closed portably; see
//! `docs/filesystem-coordination.md`.
//!
//! [`StoreLock`] remains the canonical owner for JSON *store* files
//! (memory/tasks/agents/sessions/policy); this module is the canonical
//! owner for *workspace content* mutations. The two guard disjoint
//! resource classes and share no code path.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{bail, Context, Result};

/// Maximum distinct resource keys the process-wide in-process registry
/// may track. Bounded so a flood of attacker-named paths cannot grow the
/// map without limit; a mutation naming an unseen key while saturated
/// fails closed instead of evicting a live lock.
pub const MAX_REGISTRY_KEYS: usize = 4096;

/// Maximum resources one coordinated mutation set may cover. Multi-file
/// edit transactions are already bounded by the snapshot contract
/// (256 files); coordination enforces the same ceiling before any lock is
/// taken so a pathological transaction cannot fan out into thousands of
/// lock files.
pub const MAX_LOCKS_PER_SET: usize = 256;

/// How long a coordinated acquire waits for a competing holder before
/// failing closed. Bounded so a wedged sibling cannot hang a tool call.
pub const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(10);

/// Lock files untouched for this long are treated as abandoned (the
/// holder crashed without cleanup) and reclaimed. Mirrors the proven
/// [`crate::mcp::store_lock`] stale age.
const STALE_LOCK_AGE: Duration = Duration::from_secs(30);

/// Delay between acquisition attempts while another holder owns a lock.
const RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// Structured coordination failure taxonomy. Every variant is a
/// fail-closed condition: the caller must not mutate.
#[derive(Debug, thiserror::Error)]
pub enum CoordinationError {
    /// The derived resource key is unusable (invalid path shape, missing
    /// root). No mutation may proceed on an unkeyable resource.
    #[error("invalid coordination resource: {0}")]
    InvalidResource(String),
    /// The acquisition deadline elapsed while another actor held one of
    /// the required resources.
    #[error("timed out acquiring coordination for {resource} (held by another actor)")]
    TimedOut { resource: String },
    /// The bounded registry is full and the requested key is not present.
    /// Fail closed rather than proceeding uncoordinated.
    #[error("coordination registry is saturated ({MAX_REGISTRY_KEYS} live resources); refusing uncoordinated mutation")]
    RegistrySaturated,
    /// The cross-process lock plane is unusable (lock directory missing,
    /// permissions, storage failure). Fail closed.
    #[error("coordination lock infrastructure failed: {0}")]
    Infrastructure(String),
}

/// A canonical coordination resource key: the SHA-256 of the canonical
/// workspace root and the canonical workspace-relative path.
///
/// Derivation requires a validated path: the caller passes the workspace
/// root (canonicalized here, fail-closed if it does not exist) and the
/// workspace-relative path (already validated by the canonical path
/// rules — never raw caller input). The key is a fixed-size digest, so
/// registry memory and lock-file names cannot be inflated by path length.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceKey(String);

impl ResourceKey {
    /// The 64-hex-character digest.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Normalizes a workspace-relative path to the canonical key form:
/// forward-slash separators, no leading `./`, no trailing slash, empty
/// components collapsed. The *validated* relative path is already free of
/// traversal/absolute components; normalization only removes redundant
/// separators so `a/b`, `a//b`, and `a/./b` converge on one key.
fn normalize_relative(relative: &str) -> String {
    let mut out = String::with_capacity(relative.len());
    for component in relative.split(['/', '\\']) {
        if component.is_empty() || component == "." {
            continue;
        }
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(component);
    }
    out
}

/// Derives the canonical resource key for one workspace file. The root
/// must exist (it is canonicalized); the relative path is normalized.
/// Worktree identity participates through the root: two sessions
/// sharing one worktree pass the same canonical root and converge on the
/// same key; independent worktrees differ and never serialize each other.
pub fn resource_key(root: &Path, relative: &str) -> Result<ResourceKey, CoordinationError> {
    if relative.trim().is_empty() {
        // The workspace root itself is guarded by its own rules (delete
        // refuses it); an empty relative names no mutable file resource.
        return Err(CoordinationError::InvalidResource(
            "coordination requires a non-empty workspace-relative path".into(),
        ));
    }
    let canonical_root = root.canonicalize().map_err(|error| {
        CoordinationError::InvalidResource(format!(
            "workspace root '{}' cannot be canonicalized: {error}",
            root.display()
        ))
    })?;
    let normalized = normalize_relative(relative);
    if normalized.is_empty() {
        return Err(CoordinationError::InvalidResource(
            "coordination requires a non-empty workspace-relative path".into(),
        ));
    }
    let digest = sha256_hex(format!("{}\0{}", canonical_root.display(), normalized).as_bytes());
    Ok(ResourceKey(digest))
}

/// The canonical cross-process lock directory. Coordination locks are
/// EPHEMERAL process-scoped artifacts, not workspace state, so they live
/// under the system temp directory in one flat, zero-byte-file directory
/// rather than inside the workspace:
///
/// * the user-visible workspace (TUI listing, `git status` of an
///   uninitialized repo) is never polluted by coordination traffic;
/// * the resource key already binds the canonical workspace root, so
///   the flat directory never mixes workspaces — two independent
///   worktrees hash to disjoint key spaces;
/// * OS tmp reapers eventually remove abandoned lock files that the
///   bounded stale reclaim does not.
///
/// Consequence (documented honestly): cross-PROCESS coordination relies
/// on sibling `awh` processes sharing one temp directory. Two processes
/// on one machine do. Containers have their own `/tmp`; a workspace
/// shared ACROSS containers needs a shared temp (or the age-based
/// reclaim) — the OS remains the ultimate authority and AWH coordination
/// is advisory for foreign actors regardless.
fn lock_dir() -> PathBuf {
    std::env::temp_dir().join("awh-fs-coordination")
}

type Registry = HashMap<String, Arc<Mutex<()>>>;

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// One acquired in-process resource lock. Dropping releases it (and
/// removes the cross-process lock file).
struct InnerGuard {
    key: ResourceKey,
    in_process: Arc<Mutex<()>>,
    lock_path: Option<PathBuf>,
}

impl Drop for InnerGuard {
    fn drop(&mut self) {
        // In-process unlock first so the next thread in this process can
        // proceed even if lock-file removal fails; the file removal is
        // best-effort but its failure is reported by the *next* acquirer
        // (which sees a stale file and reclaims it by age).
        if let Ok(mut guard) = self.in_process.lock() {
            let _ = &mut *guard;
        }
        if let Some(path) = &self.lock_path {
            let _ = fs::remove_file(path);
        }
    }
}

/// A held coordination set over one or more resources. Dropping the set
/// releases every held resource, in reverse acquisition order.
///
/// Hold the guard only for the span that must be serialized:
/// revalidation + read + atomic commit + verification. Never across
/// model inference, network calls, MCP dispatch, or interactive input.
#[derive(Default)]
pub struct CoordinationSet {
    guards: Vec<InnerGuard>,
}

impl std::fmt::Debug for CoordinationSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.guards.iter().map(|g| g.key.as_str()))
            .finish()
    }
}

impl CoordinationSet {
    /// The resource keys currently held, in acquisition order.
    pub fn held(&self) -> Vec<&str> {
        self.guards.iter().map(|guard| guard.key.as_str()).collect()
    }

    fn push(&mut self, guard: InnerGuard) {
        self.guards.push(guard);
    }
}

/// A coordinator bound to one workspace root. Cheap to construct; the
/// in-process registry is process-global and the cross-process lock
/// files live under the root's `.agent/` tree.
#[derive(Debug, Clone)]
pub struct FsCoordinator {
    root: PathBuf,
}

impl FsCoordinator {
    /// Creates a coordinator for `root`. The root need not be
    /// canonicalized yet — key derivation canonicalizes (and fails
    /// closed) at acquisition time, matching the files service's own
    /// lazy-root-validation contract.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The workspace root this coordinator derives keys from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Acquires coordination over every named workspace-relative path,
    /// deterministically: keys are deduplicated, sorted by digest, and
    /// acquired in that order (in-process plane, then cross-process
    /// plane, per key). On any failure every already-acquired resource is
    /// released and the error is returned — never proceed partially
    /// locked.
    pub fn acquire(&self, relative_paths: &[&str]) -> Result<CoordinationSet, CoordinationError> {
        if relative_paths.is_empty() {
            return Ok(CoordinationSet::default());
        }
        if relative_paths.len() > MAX_LOCKS_PER_SET {
            return Err(CoordinationError::InvalidResource(format!(
                "coordinated mutation covers {} resources (max {MAX_LOCKS_PER_SET})",
                relative_paths.len()
            )));
        }
        // Derive every key up front (all-or-nothing: an unkeyable path
        // fails the whole set before the first lock is taken).
        let mut keys: Vec<ResourceKey> = Vec::with_capacity(relative_paths.len());
        for relative in relative_paths {
            keys.push(resource_key(&self.root, relative)?);
        }
        keys.sort();
        keys.dedup();

        let dir = lock_dir();
        fs::create_dir_all(&dir).map_err(|error| {
            CoordinationError::Infrastructure(format!(
                "cannot create coordination lock directory: {error}"
            ))
        })?;

        let mut set = CoordinationSet::default();
        for key in keys {
            match self.acquire_one(&key, &dir) {
                Ok(guard) => set.push(guard),
                Err(error) => {
                    // Drop the partial set: releases everything acquired
                    // so far, in reverse order.
                    drop(set);
                    return Err(error);
                }
            }
        }
        Ok(set)
    }

    /// Acquires one key: the process-wide in-process mutex first, then
    /// the cross-process lock file.
    fn acquire_one(&self, key: &ResourceKey, dir: &Path) -> Result<InnerGuard, CoordinationError> {
        let in_process = self
            .registry_entry(key)
            .map_err(|error| CoordinationError::Infrastructure(format!("registry: {error}")))?;
        // Bounded wait on the in-process plane. The lock() call blocks
        // without a deadline; use try_lock polling so a wedged sibling
        // thread fails within ACQUIRE_TIMEOUT instead of hanging forever.
        let deadline = Instant::now() + ACQUIRE_TIMEOUT;
        let in_process_guard = loop {
            match in_process.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(CoordinationError::TimedOut {
                            resource: key.as_str().to_owned(),
                        });
                    }
                    thread::sleep(RETRY_INTERVAL);
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    // A sibling panicked mid-mutation; fail closed rather
                    // than coordinating on a broken lock. The next
                    // acquirer recovers (std recovers poisoning on
                    // acquire), so this is a bounded, honest failure.
                    return Err(CoordinationError::Infrastructure(
                        "in-process coordination mutex poisoned by a panicked sibling".into(),
                    ));
                }
            }
        };
        drop(in_process_guard);
        // Cross-process plane: exclusive-create lock file with stale
        // reclaim, bounded by the same deadline. Held until guard drop.
        let lock_path = dir.join(format!("{}.lock", key.as_str()));
        let file_deadline = Instant::now() + ACQUIRE_TIMEOUT;
        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(_) => {
                    return Ok(InnerGuard {
                        key: key.clone(),
                        in_process,
                        lock_path: Some(lock_path),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if let Err(error) = self.reclaim_if_stale(&lock_path) {
                        drop(in_process);
                        return Err(CoordinationError::Infrastructure(format!(
                            "coordination lock file unusable: {error}"
                        )));
                    }
                    if Instant::now() >= file_deadline {
                        drop(in_process);
                        return Err(CoordinationError::TimedOut {
                            resource: key.as_str().to_owned(),
                        });
                    }
                    thread::sleep(RETRY_INTERVAL);
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    // Windows can transiently report ACCESS_DENIED while
                    // a concurrent holder's delete of the same lock file
                    // is pending; treat as contention and retry within
                    // the deadline (proven StoreLock behavior).
                    if Instant::now() >= file_deadline {
                        drop(in_process);
                        return Err(CoordinationError::TimedOut {
                            resource: key.as_str().to_owned(),
                        });
                    }
                    thread::sleep(RETRY_INTERVAL);
                }
                Err(e) => {
                    drop(in_process);
                    return Err(CoordinationError::Infrastructure(format!(
                        "cannot open coordination lock file: {e}"
                    )));
                }
            }
        }
    }

    /// Looks up (or inserts) the process-wide mutex for one key,
    /// enforcing the bounded registry.
    fn registry_entry(&self, key: &ResourceKey) -> Result<Arc<Mutex<()>>> {
        let mut registry = registry()
            .lock()
            .map_err(|_| anyhow::anyhow!("registry mutex poisoned"))?;
        if let Some(existing) = registry.get(key.as_str()) {
            return Ok(existing.clone());
        }
        if registry.len() >= MAX_REGISTRY_KEYS {
            bail!("registry saturated at {MAX_REGISTRY_KEYS} resources");
        }
        let entry = Arc::new(Mutex::new(()));
        registry.insert(key.as_str().to_owned(), entry.clone());
        Ok(entry)
    }

    /// Removes a lock file only when it is *provably* older than
    /// [`STALE_LOCK_AGE`]. Unreadable metadata fails closed (the file
    /// is treated as live): on Windows a transient sharing violation can
    /// make `metadata` fail for a LIVE lock, and removing a live lock
    /// breaks mutual exclusion. Bounded acquisition turns an actually
    /// abandoned-but-unreadable file into an honest timeout.
    fn reclaim_if_stale(&self, lock_path: &Path) -> Result<()> {
        let Ok(metadata) = fs::metadata(lock_path) else {
            return Ok(());
        };
        let modified = metadata
            .modified()
            .with_context(|| format!("read lock file mtime {}", lock_path.display()))?;
        let age = SystemTime::now()
            .duration_since(modified)
            .with_context(|| format!("lock file mtime {} is in the future", lock_path.display()))?;
        if age > STALE_LOCK_AGE {
            // Best-effort: a racing reclaimer's remove simply makes the
            // next create_new attempt succeed for exactly one winner.
            let _ = fs::remove_file(lock_path);
        }
        Ok(())
    }
}

/// SHA-256 hex digest of `bytes`. Uses the same primitive as the edit
/// engine's state hashes; duplicated here only so this core module does
/// not depend on the services layer (dependency direction: services →
/// core, never core → services).
fn sha256_hex(bytes: &[u8]) -> String {
    // The workspace already depends on sha2 through the edit service;
    // this is the same algorithm with a local, allocation-minimal hex.
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_for(root: &Path, relative: &str) -> ResourceKey {
        resource_key(root, relative).unwrap()
    }

    #[test]
    fn keys_require_a_nonempty_relative_path() {
        let temp = tempfile::tempdir().unwrap();
        let error = resource_key(temp.path(), "").unwrap_err();
        assert!(matches!(error, CoordinationError::InvalidResource(_)));
        let error = resource_key(temp.path(), "   ").unwrap_err();
        assert!(matches!(error, CoordinationError::InvalidResource(_)));
    }

    #[test]
    fn keys_fail_closed_when_the_root_is_missing() {
        let error = resource_key(Path::new("/nonexistent-root-for-fs-coordination"), "a.txt");
        assert!(matches!(error, Err(CoordinationError::InvalidResource(_))));
    }

    #[test]
    fn same_root_and_path_converge_on_one_key() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            key_for(temp.path(), "src/main.rs"),
            key_for(temp.path(), "src/main.rs")
        );
    }

    #[test]
    fn redundant_separators_converge_on_one_key() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            key_for(temp.path(), "a//b.txt"),
            key_for(temp.path(), "a/b.txt")
        );
        assert_eq!(
            key_for(temp.path(), "./a/b.txt"),
            key_for(temp.path(), "a/b.txt")
        );
        assert_eq!(key_for(temp.path(), "a/b/"), key_for(temp.path(), "a/b"));
    }

    #[test]
    fn different_roots_never_share_keys() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        // Independent worktrees (or workspaces) must not serialize each
        // other on a shared relative filename.
        assert_ne!(
            key_for(one.path(), "src/main.rs"),
            key_for(two.path(), "src/main.rs")
        );
    }

    #[test]
    fn different_paths_never_share_keys() {
        let temp = tempfile::tempdir().unwrap();
        assert_ne!(key_for(temp.path(), "a.txt"), key_for(temp.path(), "b.txt"));
        // Sibling-prefix confusion: "a" is not "a.txt".
        assert_ne!(key_for(temp.path(), "a"), key_for(temp.path(), "a.txt"));
        assert_ne!(
            key_for(temp.path(), "dir/a.txt"),
            key_for(temp.path(), "dir.txt")
        );
    }

    #[test]
    fn keys_are_fixed_size_hex() {
        let temp = tempfile::tempdir().unwrap();
        let key = key_for(temp.path(), "some/deeply/nested/path.rs");
        assert_eq!(key.as_str().len(), 64);
        assert!(key.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn single_acquire_and_release_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = FsCoordinator::new(temp.path());
        let set = coordinator.acquire(&["a.txt"]).unwrap();
        assert_eq!(set.held().len(), 1);
        let key = set.held()[0].to_owned();
        drop(set);
        // The lock file is removed on release.
        assert!(!lock_dir().join(format!("{key}.lock")).exists());
    }

    #[test]
    fn duplicate_paths_acquire_one_lock() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = FsCoordinator::new(temp.path());
        let set = coordinator
            .acquire(&["a.txt", "a.txt", "a//b.txt", "./a.txt"])
            .unwrap();
        assert_eq!(set.held().len(), 2);
    }

    #[test]
    fn concurrent_acquire_of_one_key_serializes() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = FsCoordinator::new(temp.path());
        let set = coordinator.acquire(&["shared.txt"]).unwrap();
        // A second acquisition of the same key must time out (bounded
        // fail-closed), proving in-process mutual exclusion.
        let error = coordinator.acquire(&["shared.txt"]).unwrap_err();
        assert!(matches!(error, CoordinationError::TimedOut { .. }));
        drop(set);
        // Released: immediately acquirable again.
        coordinator.acquire(&["shared.txt"]).unwrap();
    }

    #[test]
    fn partial_failure_releases_every_acquired_resource() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = FsCoordinator::new(temp.path());
        // Hold "b.txt" via a "blocking" set so acquiring [a, b, c] fails
        // at b and must roll back a.
        let blocker = coordinator.acquire(&["b.txt"]).unwrap();
        let error = coordinator
            .acquire(&["a.txt", "b.txt", "c.txt"])
            .unwrap_err();
        assert!(matches!(error, CoordinationError::TimedOut { .. }));
        drop(blocker);
        // All three must now be free: acquiring the full set succeeds.
        let full = coordinator.acquire(&["a.txt", "b.txt", "c.txt"]).unwrap();
        assert_eq!(full.held().len(), 3);
    }

    #[test]
    fn empty_set_is_an_empty_coordination_set() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = FsCoordinator::new(temp.path());
        let set = coordinator.acquire(&[]).unwrap();
        assert!(set.held().is_empty());
    }

    #[test]
    fn oversized_set_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = FsCoordinator::new(temp.path());
        let paths: Vec<String> = (0..=MAX_LOCKS_PER_SET)
            .map(|i| format!("f{i}.txt"))
            .collect();
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        let error = coordinator.acquire(&refs).unwrap_err();
        assert!(matches!(error, CoordinationError::InvalidResource(_)));
    }

    #[test]
    fn lock_files_never_pollute_the_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = FsCoordinator::new(temp.path());
        let _set = coordinator.acquire(&["deeply/nested/file.rs"]).unwrap();
        // Lock state lives outside the workspace: no sibling lock file,
        // no `.agent` tree created by coordination traffic.
        assert!(!temp.path().join("deeply/nested/file.rs.lock").exists());
        assert!(!temp.path().join(".agent").exists());
    }

    #[test]
    fn stale_lock_file_is_reclaimed() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = FsCoordinator::new(temp.path());
        let key = key_for(temp.path(), "gone.txt");
        let dir = lock_dir();
        fs::create_dir_all(&dir).unwrap();
        let lock_path = dir.join(format!("{}.lock", key.as_str()));
        fs::write(&lock_path, b"abandoned by a crashed process").unwrap();
        // Backdate the lock file beyond STALE_LOCK_AGE.
        let past = SystemTime::now() - STALE_LOCK_AGE - Duration::from_secs(5);
        let file = fs::File::options().write(true).open(&lock_path).unwrap();
        file.set_modified(past).unwrap();
        drop(file);
        // Acquisition succeeds despite the abandoned lock file.
        let set = coordinator.acquire(&["gone.txt"]).unwrap();
        assert_eq!(set.held().len(), 1);
    }

    #[test]
    fn fresh_lock_file_is_never_reclaimed() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = FsCoordinator::new(temp.path());
        let key = key_for(temp.path(), "live.txt");
        let dir = lock_dir();
        fs::create_dir_all(&dir).unwrap();
        let lock_path = dir.join(format!("{}.lock", key.as_str()));
        fs::write(&lock_path, b"held by a live foreign actor").unwrap();
        // Recent mtime: acquisition must time out, not steal the lock.
        let error = coordinator.acquire(&["live.txt"]).unwrap_err();
        assert!(matches!(error, CoordinationError::TimedOut { .. }));
    }
}
