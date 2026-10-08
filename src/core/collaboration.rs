//! Canonical collaboration ownership store (COL-001).
//!
//! One authoritative collaboration representation per workspace, persisted
//! as a single JSON object at `.agent/collaboration.json`. The store owns
//! ONLY the coordination state between already-authoritative runtime
//! entities: which agent/session currently owns a task or worktree
//! resource, ownership transitions (assign/activate/handoff/accept/
//! release), and the revision used for compare-and-swap. Agent, session,
//! task, worktree, and audit identities stay owned by their canonical
//! stores — this module never re-validates them itself (the
//! [`crate::services::collaboration::CollaborationService`] layer does)
//! and never persists a second copy of them beyond opaque id references.
//!
//! Concurrency: every mutation runs under the store's [`StoreLock`] and
//! takes `expected_revision` (the revision the caller observed). A stale
//! caller loses; a matching revision wins and bumps the revision, so a
//! replayed handoff either lands idempotently (identical request) or is
//! rejected as stale — it can never silently overwrite a newer owner.
//! Because each resource has exactly one record with exactly one
//! `owner_agent` field, two simultaneous owners of the same exclusive
//! resource are structurally impossible.

use crate::mcp::store_lock::StoreLock;
use anyhow::{bail, Context as _, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Schema version of `.agent/collaboration.json`.
const COLLAB_SCHEMA_VERSION: u32 = 1;
/// Maximum number of distinct resources the store may track.
const MAX_COLLAB_RECORDS: usize = 10_000;
/// Maximum length of a task/worktree resource id.
const MAX_RESOURCE_ID_LEN: usize = 256;
/// Maximum length of an ownership note.
const MAX_NOTE_LEN: usize = 512;
/// Default and maximum bounds for list/event output.
pub const DEFAULT_LIST_LIMIT: usize = 50;
pub const MAX_LIST_LIMIT: usize = 500;

/// The kind of exclusive resource a collaboration record coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    /// A canonical task (`task-…` id owned by the task store).
    Task,
    /// An isolated worktree (id owned by the worktree store).
    Worktree,
}

impl ResourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceKind::Task => "task",
            ResourceKind::Worktree => "worktree",
        }
    }
}

/// Ownership state of one resource (COL-001 lifecycle). `Released` and
/// `HandedOff` end an ownership cycle: the resource is free again and a
/// new `assign` may reactivate the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum OwnershipState {
    /// Assigned to an agent, not yet activated.
    Assigned,
    /// Actively owned.
    Active,
    /// A handoff was requested to `pending_to_agent` and is awaiting
    /// acceptance.
    HandoffRequested,
    /// Handoff completed: `owner_agent` is now the accepting agent.
    HandedOff,
    /// Ownership released; the resource is free.
    Released,
}

impl OwnershipState {
    pub fn as_str(&self) -> &'static str {
        match self {
            OwnershipState::Assigned => "Assigned",
            OwnershipState::Active => "Active",
            OwnershipState::HandoffRequested => "HandoffRequested",
            OwnershipState::HandedOff => "HandedOff",
            OwnershipState::Released => "Released",
        }
    }

    /// True while an agent holds (or is entitled to) the resource.
    pub fn is_held(&self) -> bool {
        !matches!(self, OwnershipState::Released)
    }
}

/// Legal transition check for the recorded lifecycle. `HandedOff` and
/// `Released` end a cycle (only a fresh `assign` reopens the resource);
/// everything else follows the COL-001 state machine.
pub fn is_valid_ownership_transition(from: &OwnershipState, to: &OwnershipState) -> bool {
    use OwnershipState::*;
    matches!(
        (from, to),
        (Assigned, Active)
            | (Assigned, HandoffRequested)
            | (Assigned, Released)
            | (Active, HandoffRequested)
            | (Active, Released)
            | (HandoffRequested, HandedOff)
            | (HandoffRequested, Released)
            | (HandoffRequested, Assigned) // handoff withdrawn by the owner
            | (HandedOff, Released)
            | (HandedOff, Active) // new owner activates after accepting
    )
}

/// One ownership record for one exclusive resource. A record exists only
/// for resources that have been assigned at least once; the resource key
/// `(kind, id)` is unique in the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabRecord {
    /// Resource kind this record coordinates.
    pub resource_kind: ResourceKind,
    /// Canonical resource id (opaque reference; never validated here).
    pub resource_id: String,
    /// The agent currently holding the resource (`agent-…`).
    pub owner_agent: String,
    /// The runtime session the assignment is bound to, when one was
    /// supplied. Collaboration never invents one.
    pub owner_session: Option<String>,
    /// Current ownership state.
    pub state: OwnershipState,
    /// Target agent of a pending handoff (`HandoffRequested` only).
    pub pending_to_agent: Option<String>,
    /// Optimistic-concurrency token. Callers must echo the revision they
    /// observed; every successful mutation increments it by exactly one.
    pub revision: u64,
    /// Bounded operator/agent note (ids and reasons only, never secrets).
    pub note: Option<String>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 last-update timestamp.
    pub updated_at: String,
}

/// On-disk representation: one JSON object, one versioned array.
#[derive(Debug, Serialize, Deserialize)]
struct CollabStoreFile {
    version: u32,
    records: Vec<CollabRecord>,
}

/// Stable error categories surfaced verbatim across interface planes.
#[derive(Debug, PartialEq, Eq)]
pub enum CollabError {
    /// A live record already owns the resource.
    AlreadyOwned,
    /// The resource has no record (never assigned, or released without
    /// reassignment).
    NotOwned,
    /// Only the current owner may perform this transition.
    NotOwner,
    /// No handoff is pending for this resource/agent.
    NoPendingHandoff,
    /// The caller's revision is stale — reload and retry.
    StaleRevision,
    /// The record was in an unexpected state for this transition.
    InvalidState,
}

impl CollabError {
    /// Stable, greppable error text (part of the CLI/MCP contract).
    pub fn message(&self) -> String {
        match self {
            CollabError::AlreadyOwned => {
                "resource already has an active owner (release or hand off first)".to_owned()
            }
            CollabError::NotOwner => "only the current owner may change this ownership".to_owned(),
            CollabError::NotOwned => "no active ownership record for this resource".to_owned(),
            CollabError::NoPendingHandoff => "no handoff is pending for this resource".to_owned(),
            CollabError::StaleRevision => {
                "stale revision: the record changed, reload and retry".to_owned()
            }
            CollabError::InvalidState => {
                "transition is not legal from the current state".to_owned()
            }
        }
    }
}

/// Result alias for store operations: domain failures are typed
/// (`CollabError`) so every plane reports identical categories, while
/// I/O and corruption surface as `anyhow` errors.
pub type CollabResult<T> = std::result::Result<T, CollabError>;

/// The workspace collaboration store.
pub struct CollaborationStore {
    path: PathBuf,
}

impl CollaborationStore {
    /// Creates the store bound to `root` (the workspace root holding
    /// `.agent/`); persistence is a single
    /// `<root>/.agent/collaboration.json`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            path: root.into().join(".agent").join("collaboration.json"),
        }
    }

    fn load(&self) -> Result<CollabStoreFile> {
        match fs::read(&self.path) {
            Ok(bytes) => {
                let file: CollabStoreFile = serde_json::from_slice(&bytes).with_context(|| {
                    format!("collaboration store is corrupt: {}", self.path.display())
                })?;
                if file.version != COLLAB_SCHEMA_VERSION {
                    // Fail closed: an incompatible schema is never silently
                    // reset (that would fabricate ownership state).
                    bail!(
                        "unsupported collaboration store schema version {} (expected {COLLAB_SCHEMA_VERSION}): {}",
                        file.version,
                        self.path.display()
                    );
                }
                if file.records.len() > MAX_COLLAB_RECORDS {
                    bail!("collaboration store exceeds its record bound");
                }
                Ok(file)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(CollabStoreFile {
                version: COLLAB_SCHEMA_VERSION,
                records: Vec::new(),
            }),
            Err(e) => Err(e).context("failed to read the collaboration store"),
        }
    }

    fn save(&self, file: &CollabStoreFile) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("collaboration store has no parent directory")?;
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)
            .context("failed to create collaboration temp file")?;
        std::io::Write::write_all(&mut temp, serde_json::to_vec_pretty(file)?.as_slice())?;
        temp.as_file().sync_all()?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("failed to atomically write the collaboration store")?;
        Ok(())
    }

    fn validate_resource(kind: ResourceKind, id: &str) -> Result<()> {
        if id.is_empty() || id.len() > MAX_RESOURCE_ID_LEN {
            bail!("invalid resource id length (max {MAX_RESOURCE_ID_LEN})");
        }
        if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
            bail!("invalid resource id: {id:?}");
        }
        match kind {
            ResourceKind::Task | ResourceKind::Worktree => Ok(()),
        }
    }

    /// Creates `.agent/` if needed so the lock file can be created
    /// (lock-file creation ENOENTs on a missing parent).
    fn ensure_parent(&self) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("collaboration store has no parent directory")?;
        fs::create_dir_all(parent)?;
        Ok(())
    }

    /// Assigns `resource` to `owner_agent`. Fails with
    /// [`CollabError::AlreadyOwned`] when a held record exists for the
    /// resource. A released/handed-off record may be reactivated, but only
    /// when the caller echoes the revision of the terminal record it
    /// observed (`expected_revision`) — a blind reactivation is rejected
    /// as stale so a stale client can never clobber a concurrent cycle.
    /// A fresh assignment always starts at revision 1.
    pub fn assign(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        owner_agent: &str,
        owner_session: Option<&str>,
        note: Option<&str>,
        expected_revision: Option<u64>,
    ) -> Result<CollabResult<CollabRecord>> {
        Self::validate_resource(kind, resource_id)?;
        validate_note(note)?;
        self.ensure_parent()?;
        let _lock = StoreLock::acquire(&self.path)?;
        let mut file = self.load()?;
        let next_revision = match file
            .records
            .iter()
            .find(|r| r.resource_kind == kind && r.resource_id == resource_id)
        {
            Some(existing) => {
                if existing.state.is_held() {
                    return Ok(Err(CollabError::AlreadyOwned));
                }
                match expected_revision {
                    Some(expected) if expected == existing.revision => existing.revision + 1,
                    _ => return Ok(Err(CollabError::StaleRevision)),
                }
            }
            None => {
                if file.records.len() >= MAX_COLLAB_RECORDS {
                    bail!("collaboration store is full (max {MAX_COLLAB_RECORDS} records)");
                }
                1
            }
        };
        let now = Utc::now().to_rfc3339();
        let record = CollabRecord {
            resource_kind: kind,
            resource_id: resource_id.to_owned(),
            owner_agent: owner_agent.to_owned(),
            owner_session: owner_session.map(str::to_owned),
            state: OwnershipState::Assigned,
            pending_to_agent: None,
            revision: next_revision,
            note: note.map(str::to_owned),
            created_at: now.clone(),
            updated_at: now,
        };
        file.records
            .retain(|r| !(r.resource_kind == kind && r.resource_id == resource_id));
        file.records.push(record.clone());
        self.save(&file)?;
        Ok(Ok(record))
    }

    fn mutate<T>(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        acting_agent: Option<&str>,
        expected_revision: u64,
        apply: impl FnOnce(&mut CollabRecord) -> CollabResult<T>,
    ) -> Result<CollabResult<T>> {
        Self::validate_resource(kind, resource_id)?;
        self.ensure_parent()?;
        let _lock = StoreLock::acquire(&self.path)?;
        let mut file = self.load()?;
        let index = match file
            .records
            .iter()
            .position(|r| r.resource_kind == kind && r.resource_id == resource_id)
        {
            Some(index) => index,
            None => return Ok(Err(CollabError::NotOwned)),
        };
        let record = &mut file.records[index];
        if let Some(acting) = acting_agent {
            if record.owner_agent != acting {
                return Ok(Err(CollabError::NotOwner));
            }
        }
        if record.revision != expected_revision {
            return Ok(Err(CollabError::StaleRevision));
        }
        let outcome = match apply(record) {
            Ok(outcome) => outcome,
            Err(err) => return Ok(Err(err)),
        };
        if record.revision == expected_revision {
            // `apply` performed no mutation (idempotent replay).
            return Ok(Ok(outcome));
        }
        record.updated_at = Utc::now().to_rfc3339();
        self.save(&file)?;
        Ok(Ok(outcome))
    }

    /// Marks the held resource as actively worked on (`Assigned`/`HandedOff`
    /// → `Active`). Owner-only.
    pub fn activate(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        acting_agent: &str,
        expected_revision: u64,
    ) -> Result<CollabResult<CollabRecord>> {
        self.mutate(
            kind,
            resource_id,
            Some(acting_agent),
            expected_revision,
            |record| {
                let next = OwnershipState::Active;
                if !is_valid_ownership_transition(&record.state, &next) {
                    return Err(CollabError::InvalidState);
                }
                record.state = next;
                record.revision += 1;
                Ok(record.clone())
            },
        )
    }

    /// Requests a handoff from the current owner to `to_agent`
    /// (`Assigned`/`Active` → `HandoffRequested`). Repeating the exact
    /// same request is idempotent (no revision bump), so a replayed
    /// request can never clobber a concurrent state change. Any other
    /// handoff request on the same resource (different target, or
    /// acting through a non-owner) fails.
    pub fn request_handoff(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        from_agent: &str,
        to_agent: &str,
        note: Option<&str>,
        expected_revision: u64,
    ) -> Result<CollabResult<CollabRecord>> {
        validate_note(note)?;
        self.mutate(
            kind,
            resource_id,
            Some(from_agent),
            expected_revision,
            |record| {
                if record.state == OwnershipState::HandoffRequested
                    && record.pending_to_agent.as_deref() == Some(to_agent)
                {
                    // Idempotent replay of the identical pending request.
                    return Ok(record.clone());
                }
                let next = OwnershipState::HandoffRequested;
                if !is_valid_ownership_transition(&record.state, &next) {
                    return Err(CollabError::InvalidState);
                }
                record.state = next;
                record.pending_to_agent = Some(to_agent.to_owned());
                if let Some(note) = note {
                    record.note = Some(note.to_owned());
                }
                record.revision += 1;
                Ok(record.clone())
            },
        )
    }

    /// Accepts the pending handoff as `accepting_agent`
    /// (`HandoffRequested` → `HandedOff`, owner becomes the accepting
    /// agent). Only the pending target may accept.
    pub fn accept_handoff(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        accepting_agent: &str,
        expected_revision: u64,
    ) -> Result<CollabResult<CollabRecord>> {
        self.mutate(kind, resource_id, None, expected_revision, |record| {
            if record.state != OwnershipState::HandoffRequested {
                return Err(CollabError::InvalidState);
            }
            if record.pending_to_agent.as_deref() != Some(accepting_agent) {
                return Err(CollabError::NoPendingHandoff);
            }
            record.state = OwnershipState::HandedOff;
            record.owner_agent = accepting_agent.to_owned();
            record.owner_session = None;
            record.pending_to_agent = None;
            record.revision += 1;
            Ok(record.clone())
        })
    }

    /// Releases ownership of a held resource (any held state →
    /// `Released`). Owner-only. After release the resource may be
    /// assigned again.
    pub fn release(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        acting_agent: &str,
        expected_revision: u64,
    ) -> Result<CollabResult<CollabRecord>> {
        self.mutate(
            kind,
            resource_id,
            Some(acting_agent),
            expected_revision,
            |record| {
                let next = OwnershipState::Released;
                if !is_valid_ownership_transition(&record.state, &next) {
                    return Err(CollabError::InvalidState);
                }
                record.state = next;
                record.pending_to_agent = None;
                record.revision += 1;
                Ok(record.clone())
            },
        )
    }

    /// Returns the current record for one resource (the unique record,
    /// whatever its state), if one exists.
    pub fn get(&self, kind: ResourceKind, resource_id: &str) -> Result<Option<CollabRecord>> {
        Self::validate_resource(kind, resource_id)?;
        let file = self.load()?;
        Ok(file
            .records
            .into_iter()
            .find(|r| r.resource_kind == kind && r.resource_id == resource_id))
    }

    /// Lists records, optionally filtered by resource kind. Records are
    /// returned newest-updated first.
    pub fn list(&self, kind: Option<ResourceKind>) -> Result<Vec<CollabRecord>> {
        let file = self.load()?;
        let mut records: Vec<CollabRecord> = match kind {
            Some(kind) => file
                .records
                .into_iter()
                .filter(|r| r.resource_kind == kind)
                .collect(),
            None => file.records,
        };
        records.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(records)
    }
}

/// Bounds a free-text note and rejects control characters (notes are
/// persisted and echoed into audit subjects/details).
fn validate_note(note: Option<&str>) -> Result<()> {
    if let Some(note) = note {
        if note.len() > MAX_NOTE_LEN {
            bail!("note exceeds {MAX_NOTE_LEN} bytes");
        }
        if note.contains(['\0', '\r', '\n']) {
            bail!("note contains control characters");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, CollaborationStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = CollaborationStore::new(dir.path());
        (dir, s)
    }

    const TASK: (ResourceKind, &str) = (ResourceKind::Task, "task-1");

    fn assign(s: &CollaborationStore) -> CollabRecord {
        s.assign(TASK.0, TASK.1, "agent-a", None, None, None)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn assign_then_reassign_fails_while_held() {
        let (_dir, s) = store();
        let record = assign(&s);
        let err = s
            .assign(TASK.0, TASK.1, "agent-b", None, None, None)
            .unwrap()
            .unwrap_err();
        assert_eq!(err, CollabError::AlreadyOwned);
        assert_eq!(record.state, OwnershipState::Assigned);
        assert_eq!(record.revision, 1);
    }

    #[test]
    fn activate_then_handoff_request_accept() {
        let (_dir, s) = store();
        let r0 = assign(&s);
        let r1 = s
            .activate(TASK.0, TASK.1, "agent-a", r0.revision)
            .unwrap()
            .unwrap();
        assert_eq!(r1.state, OwnershipState::Active);
        let r2 = s
            .request_handoff(TASK.0, TASK.1, "agent-a", "agent-b", None, r1.revision)
            .unwrap()
            .unwrap();
        assert_eq!(r2.state, OwnershipState::HandoffRequested);
        assert_eq!(r2.pending_to_agent.as_deref(), Some("agent-b"));
        let r3 = s
            .accept_handoff(TASK.0, TASK.1, "agent-b", r2.revision)
            .unwrap()
            .unwrap();
        assert_eq!(r3.state, OwnershipState::HandedOff);
        assert_eq!(r3.owner_agent, "agent-b");
        assert_eq!(r3.revision, 4);
        // Two owners are impossible: the record has exactly one owner.
        let current = s.get(TASK.0, TASK.1).unwrap().unwrap();
        assert_eq!(current.owner_agent, "agent-b");
    }

    #[test]
    fn handoff_replay_is_idempotent() {
        let (_dir, s) = store();
        let r0 = assign(&s);
        let r1 = s
            .request_handoff(TASK.0, TASK.1, "agent-a", "agent-b", None, r0.revision)
            .unwrap()
            .unwrap();
        // Exact replay: same result, no revision bump.
        let r2 = s
            .request_handoff(TASK.0, TASK.1, "agent-a", "agent-b", None, r1.revision)
            .unwrap()
            .unwrap();
        assert_eq!(r1.revision, r2.revision);
        // Retargeting an existing pending request fails (stale semantics
        // keep the first request authoritative).
        let err = s
            .request_handoff(TASK.0, TASK.1, "agent-a", "agent-c", None, r1.revision)
            .unwrap()
            .unwrap_err();
        assert_eq!(err, CollabError::InvalidState);
    }

    #[test]
    fn stale_revision_is_rejected() {
        let (_dir, s) = store();
        let r0 = assign(&s);
        let _r1 = s
            .activate(TASK.0, TASK.1, "agent-a", r0.revision)
            .unwrap()
            .unwrap();
        let err = s
            .release(TASK.0, TASK.1, "agent-a", r0.revision)
            .unwrap()
            .unwrap_err();
        assert_eq!(err, CollabError::StaleRevision);
    }

    #[test]
    fn non_owner_cannot_mutate() {
        let (_dir, s) = store();
        let r0 = assign(&s);
        let err = s
            .activate(TASK.0, TASK.1, "agent-b", r0.revision)
            .unwrap()
            .unwrap_err();
        assert_eq!(err, CollabError::NotOwner);
        let err = s
            .accept_handoff(TASK.0, TASK.1, "agent-b", r0.revision)
            .unwrap()
            .unwrap_err();
        assert_eq!(err, CollabError::InvalidState);
    }

    #[test]
    fn release_then_reactivate_reopens_the_resource() {
        let (_dir, s) = store();
        let r0 = assign(&s);
        let r1 = s
            .release(TASK.0, TASK.1, "agent-a", r0.revision)
            .unwrap()
            .unwrap();
        assert_eq!(r1.state, OwnershipState::Released);
        // Reassigning without the expected revision is stale; with it,
        // the record reactivates at the next revision for a new owner.
        let err = s
            .assign(TASK.0, TASK.1, "agent-b", None, None, None)
            .unwrap()
            .unwrap_err();
        assert_eq!(err, CollabError::StaleRevision);
        let r2 = s
            .assign(TASK.0, TASK.1, "agent-b", None, None, Some(r1.revision))
            .unwrap()
            .unwrap();
        assert_eq!(r2.state, OwnershipState::Assigned);
        assert_eq!(r2.owner_agent, "agent-b");
        assert_eq!(r2.revision, r1.revision + 1);
    }

    #[test]
    fn missing_resource_is_not_owned() {
        let (_dir, s) = store();
        let err = s
            .activate(TASK.0, TASK.1, "agent-a", 1)
            .unwrap()
            .unwrap_err();
        assert_eq!(err, CollabError::NotOwned);
        assert!(s.get(TASK.0, TASK.1).unwrap().is_none());
    }

    #[test]
    fn corrupt_store_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let s = CollaborationStore::new(dir.path());
        let agent_dir = dir.path().join(".agent");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(agent_dir.join("collaboration.json"), "{ truncated").unwrap();
        let err = s.list(None).unwrap_err().to_string();
        assert!(err.contains("corrupt"), "{err}");
        // And a bad schema version is refused, never silently reset.
        fs::write(
            agent_dir.join("collaboration.json"),
            r#"{"version":99,"records":[]}"#,
        )
        .unwrap();
        let err = s.list(None).unwrap_err().to_string();
        assert!(err.contains("schema version"), "{err}");
    }

    #[test]
    fn resource_ids_and_notes_are_bounded() {
        let (_dir, s) = store();
        assert!(s
            .assign(ResourceKind::Task, "../escape", "agent-a", None, None, None)
            .is_err());
        assert!(s
            .assign(
                ResourceKind::Task,
                &"x".repeat(MAX_RESOURCE_ID_LEN + 1),
                "agent-a",
                None,
                None,
                None
            )
            .is_err());
        assert!(s
            .assign(
                TASK.0,
                TASK.1,
                "agent-a",
                None,
                Some(&"n".repeat(MAX_NOTE_LEN + 1)),
                None
            )
            .is_err());
        assert!(s
            .assign(TASK.0, TASK.1, "agent-a", None, Some("bad\nnote"), None)
            .is_err());
    }

    #[test]
    fn parallel_assign_yields_exactly_one_owner() {
        let dir = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(CollaborationStore::new(dir.path()));
        let agents: Vec<String> = (0..8).map(|i| format!("agent-{i}")).collect();
        let mut handles = Vec::new();
        for agent in &agents {
            let store = store.clone();
            let agent = agent.clone();
            handles.push(std::thread::spawn(move || {
                store
                    .assign(TASK.0, TASK.1, &agent, None, None, None)
                    .unwrap()
            }));
        }
        let mut owners = 0;
        for handle in handles {
            if handle.join().unwrap().is_ok() {
                owners += 1;
            }
        }
        assert_eq!(owners, 1, "exactly one assign may win");
        let record = store.get(TASK.0, TASK.1).unwrap().unwrap();
        assert!(agents.contains(&record.owner_agent));
    }

    #[test]
    fn reload_after_restart_preserves_ownership() {
        let dir = tempfile::tempdir().unwrap();
        {
            let s = CollaborationStore::new(dir.path());
            let r0 = assign(&s);
            s.activate(TASK.0, TASK.1, "agent-a", r0.revision)
                .unwrap()
                .unwrap();
        }
        // Simulated restart: a fresh store instance over the same root.
        let s = CollaborationStore::new(dir.path());
        let record = s.get(TASK.0, TASK.1).unwrap().unwrap();
        assert_eq!(record.state, OwnershipState::Active);
        assert_eq!(record.owner_agent, "agent-a");
        assert_eq!(record.revision, 2);
    }
}
