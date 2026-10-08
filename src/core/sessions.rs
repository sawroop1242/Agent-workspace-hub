//! Persistent AWH runtime-session storage (TW-002).
//!
//! One `AgentSessionRecord` per JSON file under `.agent/sessions/`, written
//! atomically (tempfile + fsync + rename) under `StoreLock` so concurrent
//! agents cannot tear or overwrite each other's session records.
//!
//! This store owns *persistence + lifecycle transitions only*. Whether a
//! session may be *used* is decided by the agent runtime service
//! ([`crate::services::agent_runtime`]), which re-validates the agent
//! profile and workspace binding on every resolution — a persisted record
//! is never trusted on its own.

use crate::mcp::store_lock::StoreLock;
use crate::models::{AgentSessionRecord, SessionStatus};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Persistent session storage under `.agent/sessions` as per-session JSON
/// files.
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    /// Creates a `SessionStore` rooted at `root` (the workspace root, not
    /// the `.agent` directory).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn sessions_dir(&self) -> PathBuf {
        self.root.join(".agent").join("sessions")
    }

    /// Persists a session record, keyed by its `session_id`. Session ids
    /// come from `SessionId::new()` (validated identity ids), so they are
    /// already safe filenames; unsafe ids fail closed here too.
    pub fn create(&self, session: &AgentSessionRecord) -> Result<()> {
        let target = self.record_path(&session.session_id)?;
        let _lock = StoreLock::acquire(&target)
            .with_context(|| format!("failed to lock session store for {target:?}"))?;
        self.write_record(&target, session)
    }

    /// Applies a lifecycle transition, validating it against the actual
    /// transition table first:
    ///
    /// ```text
    /// Active  → Paused, Stopped, Failed
    /// Paused  → Active (resume), Stopped, Failed
    /// Stopped → (terminal: no transitions)
    /// Failed  → (terminal: no transitions)
    /// ```
    ///
    /// Terminal states are never silently reactivated — reopening a
    /// stopped/failed caller requires opening a *new* session. Stopping an
    /// already-terminal session is a no-op returning the record (the
    /// end-state is the requested one), while any other illegal transition
    /// is an error.
    ///
    /// The read-modify-write happens under one `StoreLock` on the record,
    /// written via the lock-free internal path (acquiring the same lock
    /// twice from one thread would self-deadlock).
    pub fn transition(
        &self,
        session_id: &str,
        next: SessionStatus,
    ) -> Result<Option<AgentSessionRecord>> {
        // Unsafe ids can never name a record; reject before touching paths.
        if !is_safe_session_id(session_id) {
            return Err(anyhow::anyhow!(
                "invalid session id: {session_id:?} (must not be empty or contain path separators)"
            ));
        }
        // Ensure the lock file's parent exists (first transition on a
        // fresh store may precede any `create`).
        fs::create_dir_all(self.sessions_dir())?;
        let target = self.sessions_dir().join(format!("{session_id}.json"));
        let _lock = StoreLock::acquire(&target)
            .with_context(|| format!("failed to lock session store for {session_id}"))?;

        let mut session = match self.read_record(&target)? {
            Some(session) => session,
            None => return Ok(None),
        };
        let current = session.status.clone();
        if !is_valid_transition(&current, &next) {
            return Err(anyhow::anyhow!(
                "invalid session transition: session {} cannot go from {} to {} (stopped and failed are terminal — open a new session)",
                session_id,
                current.label(),
                next.label()
            ));
        }
        session.status = next;
        session.last_activity_at = chrono::Utc::now().to_rfc3339();
        self.write_record(&target, &session)
            .with_context(|| format!("failed to persist transition of session {session_id}"))?;
        Ok(Some(session))
    }

    /// Validated record path for a session id.
    fn record_path(&self, session_id: &str) -> Result<PathBuf> {
        if !is_safe_session_id(session_id) {
            return Err(anyhow::anyhow!(
                "invalid session id: {session_id:?} (must not be empty or contain path separators)"
            ));
        }
        fs::create_dir_all(self.sessions_dir())?;
        Ok(self.sessions_dir().join(format!("{session_id}.json")))
    }

    /// Lock-free read of a record at a known-good path (caller holds the
    /// lock or only reads).
    fn read_record(&self, path: &Path) -> Result<Option<AgentSessionRecord>> {
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    /// Lock-free atomic write of a record (caller holds the lock).
    fn write_record(&self, target: &Path, session: &AgentSessionRecord) -> Result<()> {
        atomic_write(target, &serde_json::to_string_pretty(session)?)
    }

    /// Returns the session with the given id, or `None`. Unsafe ids fail
    /// closed as `None` (TW-002 §15: no path manipulation can read
    /// outside `.agent/sessions/`). A present-but-unparseable record is a
    /// loud error naming the file, so a damaged record is distinguishable
    /// from a missing one.
    pub fn get(&self, session_id: &str) -> Result<Option<AgentSessionRecord>> {
        if !is_safe_session_id(session_id) {
            return Ok(None);
        }
        let path = self.sessions_dir().join(format!("{}.json", session_id));
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(
            serde_json::from_str(
                &fs::read_to_string(&path)
                    .with_context(|| format!("failed to read session record {}", path.display()))?,
            )
            .with_context(|| format!("corrupt session record {}", path.display()))?,
        ))
    }

    /// Lists all sessions sorted by `session_id`, optionally filtered by
    /// agent. Best-effort: a record that fails to parse is skipped rather
    /// than failing the whole listing — session records are high-churn
    /// runtime state, and one torn file must not brick `agent status`,
    /// `session list`, or `session show` workspace-wide. Every skip is
    /// surfaced as a `tracing::warn!` naming the file, so the durability
    /// failure stays operator-observable (the CLI writes tracing output
    /// to stderr). Targeted access ([`get`](Self::get)) still fails
    /// loudly with the file path so a damaged record can be inspected.
    pub fn list(&self, agent_id: Option<&str>) -> Result<Vec<AgentSessionRecord>> {
        let dir = self.sessions_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                tracing::warn!(path = %path.display(), "skipping unreadable session record");
                continue;
            };
            let Ok(session) = serde_json::from_str::<AgentSessionRecord>(&content) else {
                tracing::warn!(path = %path.display(), "skipping corrupt session record");
                continue;
            };
            if agent_id.map(|a| session.agent_id != a).unwrap_or(false) {
                continue;
            }
            sessions.push(session);
        }
        sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(sessions)
    }
}

/// The actual lifecycle table (TW-002 §8): `from → allowed next states`.
fn is_valid_transition(from: &SessionStatus, to: &SessionStatus) -> bool {
    match (from, to) {
        (SessionStatus::Active, SessionStatus::Paused)
        | (SessionStatus::Active, SessionStatus::Stopped)
        | (SessionStatus::Active, SessionStatus::Failed)
        | (SessionStatus::Paused, SessionStatus::Active)
        | (SessionStatus::Paused, SessionStatus::Stopped)
        | (SessionStatus::Paused, SessionStatus::Failed)
        | (SessionStatus::Stopped, SessionStatus::Stopped)
        | (SessionStatus::Failed, SessionStatus::Failed) => true,
        // Paused→Paused and Active→Active are no-op "transitions" callers
        // may idempotently request (e.g. a heartbeat re-pausing).
        (SessionStatus::Active, SessionStatus::Active)
        | (SessionStatus::Paused, SessionStatus::Paused) => true,
        _ => false,
    }
}

/// Returns whether `id` is safe to use as a session filename. Reuses the
/// identity validation rules: non-empty, bounded, no separators, no
/// traversal, no control characters.
pub fn is_safe_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= crate::core::identity::MAX_ID_LEN
        && id != "."
        && id != ".."
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains("..")
        && !id.chars().any(|c| c.is_control())
        && Path::new(id).file_name().and_then(|x| x.to_str()) == Some(id)
}

/// Atomic content write: tempfile in the same directory, fsync, rename.
/// The `sync_all` before the rename is the durability point — without it
/// an OS crash can persist the directory entry before the file contents,
/// leaving an empty or truncated record (this matches the manifest and
/// policy stores' write discipline).
fn atomic_write(target: &Path, data: &str) -> Result<()> {
    let parent = target
        .parent()
        .with_context(|| format!("no parent directory for {}", target.display()))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    tmp.write_all(data.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(target).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(session_id: &str, agent_id: &str, workspace_id: &str) -> AgentSessionRecord {
        AgentSessionRecord {
            session_id: session_id.into(),
            agent_id: agent_id.into(),
            workspace_id: workspace_id.into(),
            status: SessionStatus::Active,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_activity_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn create_then_get_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        let original = record("sess-1", "writer", "ws-1");
        store.create(&original).unwrap();
        let loaded = store.get("sess-1").unwrap().expect("session persisted");
        assert_eq!(loaded, original);
        assert!(temp
            .path()
            .join(".agent")
            .join("sessions")
            .join("sess-1.json")
            .exists());
    }

    #[test]
    fn get_fails_closed_on_unsafe_ids() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        for bad in ["../agents/writer", "a/b", "..", "", "a\\b", "."] {
            assert!(store.get(bad).unwrap().is_none(), "{bad:?} must be None");
        }
    }

    #[test]
    fn create_rejects_unsafe_ids() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        for bad in ["../escape", "a/b", "a\\b", ".."] {
            assert!(
                store.create(&record(bad, "writer", "ws-1")).is_err(),
                "{bad:?} must be rejected"
            );
        }
        // Nothing escaped the store.
        assert!(!temp.path().join("escape.json").exists());
    }

    #[test]
    fn list_filters_by_agent_and_sorts() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        store.create(&record("sess-2", "writer", "ws-1")).unwrap();
        store.create(&record("sess-3", "reviewer", "ws-1")).unwrap();
        store.create(&record("sess-1", "writer", "ws-1")).unwrap();

        let all = store.list(None).unwrap();
        assert_eq!(
            all.iter()
                .map(|s| s.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["sess-1", "sess-2", "sess-3"]
        );
        let writers = store.list(Some("writer")).unwrap();
        assert_eq!(
            writers
                .iter()
                .map(|s| s.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["sess-1", "sess-2"]
        );
    }

    #[test]
    fn transition_table_is_enforced() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        store.create(&record("sess-1", "writer", "ws-1")).unwrap();

        // Active → Paused → Active is legal.
        let s = store
            .transition("sess-1", SessionStatus::Paused)
            .unwrap()
            .unwrap();
        assert_eq!(s.status, SessionStatus::Paused);
        let s = store
            .transition("sess-1", SessionStatus::Active)
            .unwrap()
            .unwrap();
        assert_eq!(s.status, SessionStatus::Active);

        // Active → Stopped: terminal.
        store
            .transition("sess-1", SessionStatus::Stopped)
            .unwrap()
            .unwrap();
        // Terminal: no reactivation, no pause, no resume.
        for illegal in [SessionStatus::Active, SessionStatus::Paused] {
            let err = store.transition("sess-1", illegal.clone()).unwrap_err();
            assert!(
                err.to_string().contains("invalid session transition"),
                "{err}"
            );
        }
        // The record is untouched.
        assert_eq!(
            store.get("sess-1").unwrap().unwrap().status,
            SessionStatus::Stopped
        );
    }

    #[test]
    fn transition_missing_session_is_none() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        assert!(store
            .transition("sess-ghost", SessionStatus::Stopped)
            .unwrap()
            .is_none());
    }

    #[test]
    fn failed_is_terminal_too() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        store.create(&record("sess-1", "writer", "ws-1")).unwrap();
        store
            .transition("sess-1", SessionStatus::Failed)
            .unwrap()
            .unwrap();
        assert!(store.transition("sess-1", SessionStatus::Active).is_err());
        assert_eq!(
            store.get("sess-1").unwrap().unwrap().status,
            SessionStatus::Failed
        );
    }
}
