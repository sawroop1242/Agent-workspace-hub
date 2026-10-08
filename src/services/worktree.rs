//! Canonical AWH-managed Git worktree isolation (GIT-001 / TW-004).
//!
//! Owns exactly one thing: the lifecycle of a Git worktree assigned to
//! an AWH agent session — identity, ownership, path containment, and
//! reconciliation against Git's own state. Everything else is reused:
//! Git invocation flows through the canonical [`GitService`] argv
//! boundary (never a shell), authorization stays with the existing
//! capability/policy system (possession of a path or id is never
//! authority, §10), audit goes through the canonical persistent store,
//! and downstream services consume the worktree root as their effective
//! workspace root rather than through any second FilesService.
//!
//! ## Storage layout (workspace-rooted)
//!
//! ```text
//! .agent/worktree-records/<worktree_id>.json   — lifecycle records
//! .agent/worktrees/<worktree_id>/               — the actual Git checkouts
//! .agent/worktree-records/.lock                — lifecycle mutation lock
//! ```
//!
//! Records persist only workspace-RELATIVE paths: the checkout location
//! is re-derived from the workspace root at use time, so persisted
//! metadata never carries host-absolute paths (§21) and a record cannot
//! be replayed against a moved host directory.
//!
//! ## Lifecycle (§8)
//!
//! ```text
//! validate identity -> validate repository -> validate branch
//!   -> reserve Creating record (before any Git mutation)
//!   -> git worktree add -> verify checkout -> publish Active -> audit
//! ```
//!
//! `Active` is never published before Git + filesystem verification
//! succeeds. Crash windows are reconciled deterministically at the next
//! load: a Creating record whose id-derived checkout exists is
//! provably this reservation's own worktree (unambiguous completion);
//! one whose checkout is absent is `Missing`. Unmanaged Git worktrees
//! are reported as anomalies and never silently adopted (§12).

use crate::mcp::store_lock::StoreLock;
use crate::services::git::GitService;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Schema version of the worktree lifecycle record.
pub const WORKTREE_SCHEMA_VERSION: u32 = 1;

/// Maximum number of managed records retained in the store (bounded
/// lifecycle metadata, §27).
pub const MAX_WORKTREE_RECORDS: usize = 256;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Stable identifier for one managed worktree (`wt-<nanos>-<pid>-<seq>`).
/// Identity is generated, never derived from a filesystem path (§5).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorktreeId(pub String);

impl WorktreeId {
    pub fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(1);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or_default();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self(format!("wt-{nanos:016x}-{}-{sequence}", std::process::id()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for WorktreeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorktreeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lifecycle state of a managed worktree (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeState {
    /// Reserved; Git worktree creation has not been verified yet.
    Creating,
    /// Created and verified against Git's own state.
    Active,
    /// Removal in progress.
    Removing,
    /// Removed; the checkout is gone. Terminal.
    Removed,
    /// The record exists but Git reports no worktree at the recorded
    /// path (deleted out of band, or creation never completed).
    Missing,
    /// The record cannot be reconciled unambiguously (path, branch,
    /// repository, or ownership mismatch) — an explicit anomaly that
    /// requires an operator decision; never silently repaired.
    RecoveryRequired,
}

impl WorktreeState {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorktreeState::Creating => "creating",
            WorktreeState::Active => "active",
            WorktreeState::Removing => "removing",
            WorktreeState::Removed => "removed",
            WorktreeState::Missing => "missing",
            WorktreeState::RecoveryRequired => "recovery_required",
        }
    }
}

/// One managed worktree: ownership binding + lifecycle (§5).
///
/// `path` is workspace-RELATIVE (e.g. `.agent/worktrees/wt-…`), so the
/// record carries no host-absolute path and the absolute checkout
/// location is always re-derived from the workspace root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRecord {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub worktree_id: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub session_id: String,
    /// Repository identity (the HEAD commit hash at creation — resolved
    /// via Git, never from directory names, §6).
    pub repo_id: String,
    /// Workspace-relative checkout path.
    pub path: String,
    /// The branch created for this worktree.
    pub branch: String,
    pub state: WorktreeState,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_at: Option<String>,
}

fn default_schema_version() -> u32 {
    WORKTREE_SCHEMA_VERSION
}

/// Structured worktree-domain errors. Client-facing messages carry ids
/// and workspace-relative paths only — never host paths, never Git
/// stderr that may embed host layout (§16 sanitization).
#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("worktree input rejected: {0}")]
    Invalid(String),
    #[error("not a git repository: AWH worktrees require a Git-backed workspace")]
    NotARepository,
    #[error("branch {0} is already checked out in another worktree")]
    BranchCheckedOut(String),
    #[error("worktree {0} already exists")]
    AlreadyExists(String),
    #[error("worktree {0} does not exist")]
    NotFound(String),
    #[error("worktree {0} is owned by another session")]
    Ownership(String),
    #[error("worktree {0} has uncommitted changes; removal refused to discard them")]
    Dirty(String),
    #[error("worktree {0} metadata is corrupt and was not interpreted")]
    Corrupt(String),
    #[error("worktree store error: {0}")]
    Store(String),
}

/// The canonical worktree lifecycle store + service over one workspace
/// root. All mutating operations serialize through one lifecycle lock;
/// Git mutation happens through [`GitService`]; records are persisted
/// atomically (tempfile + fsync + rename) and schema-checked.
pub struct WorktreeStore {
    root: PathBuf,
}

impl WorktreeStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn records_dir(&self) -> PathBuf {
        self.root.join(".agent").join("worktree-records")
    }

    fn checkouts_dir(&self) -> PathBuf {
        self.root.join(".agent").join("worktrees")
    }

    fn record_path(&self, id: &str) -> PathBuf {
        self.records_dir().join(format!("{id}.json"))
    }

    fn lock_path(&self) -> PathBuf {
        self.records_dir().join(".lock")
    }

    fn ensure_dirs(&self) -> Result<(), WorktreeError> {
        for dir in [self.records_dir(), self.checkouts_dir()] {
            std::fs::create_dir_all(&dir)
                .map_err(|e| WorktreeError::Store(format!("create worktree dirs: {e}")))?;
        }
        Ok(())
    }

    fn write_record_atomic(&self, record: &WorktreeRecord) -> Result<(), WorktreeError> {
        let path = self.record_path(&record.worktree_id);
        let bytes = serde_json::to_vec_pretty(record)
            .map_err(|e| WorktreeError::Store(format!("serialize record: {e}")))?;
        // Bounded record size (§21).
        if bytes.len() > 64 * 1024 {
            return Err(WorktreeError::Store("record exceeds size bound".into()));
        }
        let parent = path
            .parent()
            .ok_or_else(|| WorktreeError::Store("no parent".into()))?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| WorktreeError::Store(format!("stage record: {e}")))?;
        std::io::Write::write_all(&mut temp, &bytes)
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|e| WorktreeError::Store(format!("write record: {e}")))?;
        temp.persist(&path)
            .map_err(|e| WorktreeError::Store(format!("commit record: {}", e.error)))
            .map_err(|_| WorktreeError::Store("commit record failed".into()))?;
        Ok(())
    }

    fn load_record(&self, id: &str) -> Result<WorktreeRecord, WorktreeError> {
        let path = self.record_path(id);
        if !path.exists() {
            return Err(WorktreeError::NotFound(id.to_owned()));
        }
        let bytes = std::fs::read(&path).map_err(|_| WorktreeError::Corrupt(id.to_owned()))?;
        let record: WorktreeRecord =
            serde_json::from_slice(&bytes).map_err(|_| WorktreeError::Corrupt(id.to_owned()))?;
        if record.schema_version != WORKTREE_SCHEMA_VERSION {
            return Err(WorktreeError::Corrupt(id.to_owned()));
        }
        if record.worktree_id != id {
            return Err(WorktreeError::Corrupt(id.to_owned()));
        }
        Ok(record)
    }

    fn list_record_ids(&self) -> Result<Vec<String>, WorktreeError> {
        self.ensure_dirs()?;
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(self.records_dir())
            .map_err(|e| WorktreeError::Store(format!("list records: {e}")))?
        {
            let path = entry
                .map_err(|e| WorktreeError::Store(format!("list records: {e}")))?
                .path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem.starts_with("wt-") && is_safe_worktree_id(stem) {
                    ids.push(stem.to_owned());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// All managed records, deterministically ordered (newest id first).
    /// Corrupt records fail closed: the caller sees a structured
    /// corruption error listing the offending id rather than a silently
    /// truncated listing (§21).
    pub fn list(&self) -> Result<Vec<WorktreeRecord>, WorktreeError> {
        let mut records = Vec::new();
        for id in self.list_record_ids()? {
            records.push(self.load_record(&id)?);
        }
        // Newest first by reverse id order (ids embed a time prefix).
        records.sort_by(|a, b| b.worktree_id.cmp(&a.worktree_id));
        Ok(records)
    }

    /// The absolute checkout path for a record (re-derived from the
    /// workspace root — records only persist the relative path).
    fn absolute_checkout(&self, record: &WorktreeRecord) -> PathBuf {
        self.root.join(&record.path)
    }

    /// The effective filesystem root for a session: its Active managed
    /// worktree when one is bound, else the workspace root (§11). No
    /// second FilesService — callers construct existing services
    /// against this root.
    pub fn resolve_effective_root(&self, session_id: &str) -> Result<PathBuf, WorktreeError> {
        for record in self.list()? {
            if record.session_id == session_id && record.state == WorktreeState::Active {
                return Ok(self.absolute_checkout(&record));
            }
        }
        Ok(self.root.clone())
    }

    /// Validates that `path` (as recorded) is inside the approved
    /// worktree area and matches the id-derived allocation. Lexical
    /// component check + expected-prefix equality: no
    /// string-prefix-proves-containment assumption (§7).
    fn validate_containment(&self, record: &WorktreeRecord) -> Result<PathBuf, WorktreeError> {
        let expected_relative = format!(".agent/worktrees/{}", record.worktree_id);
        if record.path != expected_relative {
            return Err(WorktreeError::Corrupt(record.worktree_id.clone()));
        }
        let checkout = self.absolute_checkout(record);
        // The checkout lives strictly below the approved directory and
        // is never the workspace root itself.
        if checkout == self.root {
            return Err(WorktreeError::Invalid(
                "worktree path cannot be the workspace root".into(),
            ));
        }
        Ok(checkout)
    }

    /// Creates an isolated worktree bound to one agent session (§8).
    ///
    /// The branch defaults to `awh/<agent>/<session>` (an isolation-
    /// favoring unique name, §9) or uses the caller-supplied
    /// `branch` after Git's own ref-format validation. An existing
    /// branch checked out in another worktree is rejected — the
    /// implementation never switches or resets another worktree to
    /// resolve a collision.
    pub async fn create(
        &self,
        workspace_id: &str,
        agent_id: &str,
        session_id: &str,
        branch: Option<&str>,
        start_point: Option<&str>,
    ) -> Result<WorktreeRecord, WorktreeError> {
        for (field, value) in [("agent_id", agent_id), ("session_id", session_id)] {
            if value.is_empty()
                || !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            {
                return Err(WorktreeError::Invalid(format!(
                    "{field} must be a non-empty [A-Za-z0-9_.-] value"
                )));
            }
        }

        let git = GitService::open(&self.root).map_err(|e| WorktreeError::Store(format!("{e}")))?;
        // Validate the repository BEFORE any directory state exists: a
        // rejection leaves zero filesystem footprint (§6/§23-5) — a
        // non-Git workspace must not silently fall back to a shared
        // directory and must not pre-create managed paths.
        if !git.is_repo().await {
            return Err(WorktreeError::NotARepository);
        }
        self.ensure_dirs()?;

        let branch_name = match branch {
            Some(name) => {
                if !git.is_valid_ref(name).await {
                    return Err(WorktreeError::Invalid(format!(
                        "branch name {name:?} is not a valid Git ref"
                    )));
                }
                name.to_owned()
            }
            None => format!("awh/{agent_id}/{session_id}"),
        };
        let start = start_point.unwrap_or("HEAD");

        let _lock = StoreLock::acquire(&self.lock_path())
            .map_err(|e| WorktreeError::Store(format!("worktree lifecycle lock: {e}")))?;

        // Idempotence / collision checks under the lock: the session
        // must not get two worktrees and a branch must not be
        // double-checked-out (§8, §15).
        for record in self.list()? {
            if record.session_id == session_id
                && matches!(
                    record.state,
                    WorktreeState::Creating | WorktreeState::Active | WorktreeState::Removing
                )
            {
                return Err(WorktreeError::AlreadyExists(record.worktree_id.clone()));
            }
        }
        let listed_git = git
            .worktree_list()
            .await
            .map_err(|e| WorktreeError::Store(format!("{e:#}")))?;
        for (path, attached_branch) in listed_git {
            if let Some(attached) = attached_branch {
                if attached.trim_start_matches("refs/heads/") == branch_name {
                    return Err(WorktreeError::BranchCheckedOut(branch_name.clone()));
                }
            }
            // An unmanaged checkout occupying our future path would be
            // an anomaly; id-derived allocation under .agent/worktrees
            // makes accidental collisions practically impossible, but
            // the existence check below still fails closed.
            let _ = path;
        }

        let worktree_id = WorktreeId::new();
        let relative_path = format!(".agent/worktrees/{}", worktree_id.as_str());
        let checkout = self.root.join(&relative_path);
        if checkout.exists() {
            // Never overwrite an existing unrelated path (§23-5).
            return Err(WorktreeError::AlreadyExists(worktree_id.0));
        }

        // Repository identity via Git (§6) and reserve the lifecycle
        // identity BEFORE the Git mutation (§8).
        let repo_id = git
            .head_commit()
            .await
            .map_err(|e| WorktreeError::Store(format!("{e:#}")))?;
        let record = WorktreeRecord {
            schema_version: WORKTREE_SCHEMA_VERSION,
            worktree_id: worktree_id.0.clone(),
            workspace_id: workspace_id.to_owned(),
            agent_id: agent_id.to_owned(),
            session_id: session_id.to_owned(),
            repo_id,
            path: relative_path,
            branch: branch_name.clone(),
            state: WorktreeState::Creating,
            created_at: now_rfc3339(),
            removed_at: None,
        };
        self.write_record_atomic(&record)?;
        audit_lifecycle(
            "worktree_create_reserved",
            &record,
            "lifecycle record reserved before git mutation",
        );

        let created = git
            .worktree_add(&checkout, &branch_name, start)
            .await
            .map_err(|e| {
                // Git failed: keep the reservation for diagnosis but
                // classify honestly — never an apparently-active record
                // (§8). The reconciliation at next load will see the
                // Creating record with no checkout and classify Missing.
                audit_lifecycle(
                    "worktree_create_failed",
                    &record,
                    &format!("git worktree add failed: {e}"),
                );
                WorktreeError::Store(format!("git worktree add failed: {e}"))
            })?;
        let _ = created;

        // Verify before publishing Active (§8): the checkout exists and
        // Git itself lists the worktree at the expected path.
        if !checkout.is_dir() {
            audit_lifecycle(
                "worktree_create_unverified",
                &record,
                "checkout missing after git reported success",
            );
            return Err(WorktreeError::Store(
                "git reported success but the worktree path is missing".into(),
            ));
        }
        let listed = git
            .worktree_list()
            .await
            .map_err(|e| WorktreeError::Store(format!("{e:#}")))?;
        let absolute = checkout
            .to_str()
            .ok_or_else(|| WorktreeError::Store("path not UTF-8".into()))?
            .to_owned();
        let listed_ok = listed
            .iter()
            .any(|(path, _)| same_path(Path::new(path), Path::new(&absolute)));
        if !listed_ok {
            audit_lifecycle(
                "worktree_create_unverified",
                &record,
                "git does not list the created worktree",
            );
            return Err(WorktreeError::Store(
                "git does not list the created worktree".into(),
            ));
        }

        let active = WorktreeRecord {
            state: WorktreeState::Active,
            ..record
        };
        self.write_record_atomic(&active)?;
        audit_lifecycle("worktree_created", &active, "worktree verified and active");
        Ok(active)
    }

    /// Inspects one managed worktree, reconciling the record against
    /// Git's actual state (§12). The returned record's state is the
    /// RECONCILED state; discrepancies are audited.
    pub async fn inspect(&self, id: &str) -> Result<WorktreeRecord, WorktreeError> {
        if !is_safe_worktree_id(id) {
            return Err(WorktreeError::Invalid(id.to_owned()));
        }
        let mut record = self.load_record(id)?;
        let reconciled = self
            .reconcile_one(&record)
            .await
            .map_err(|e| WorktreeError::Store(format!("{e:#}")))?;
        if reconciled.state != record.state {
            let updated = reconciled.clone();
            self.write_record_atomic(&updated)?;
            record = updated;
        }
        Ok(record)
    }

    /// Reconciles one record against Git (§14 step 3-6). Returns the
    /// reconciled record without persisting (the caller decides).
    async fn reconcile_one(
        &self,
        record: &WorktreeRecord,
    ) -> Result<WorktreeRecord, anyhow::Error> {
        // Terminal states are never reinterpreted.
        if matches!(record.state, WorktreeState::Removed) {
            return Ok(record.clone());
        }
        let checkout = self.validate_containment(record)?;
        let git = GitService::open(&self.root).map_err(|e| WorktreeError::Store(format!("{e}")))?;
        let listed = git.worktree_list().await?;
        let absolute = checkout
            .to_str()
            .ok_or_else(|| WorktreeError::Store("path not UTF-8".into()))?
            .to_owned();
        let git_entry = listed
            .iter()
            .find(|(path, _)| same_path(Path::new(path), Path::new(&absolute)));

        let reconciled = match (git_entry, record.state) {
            (None, WorktreeState::Creating) => {
                // Crash before Git creation completed: the checkout
                // never existed. Missing, never fabricated Active.
                audit_lifecycle(
                    "worktree_reconcile_missing",
                    record,
                    "creating reservation with no checkout",
                );
                WorktreeRecord {
                    state: WorktreeState::Missing,
                    ..record.clone()
                }
            }
            (None, WorktreeState::Active | WorktreeState::Removing) => {
                audit_lifecycle(
                    "worktree_reconcile_missing",
                    record,
                    "recorded worktree absent from git state",
                );
                WorktreeRecord {
                    state: WorktreeState::Missing,
                    ..record.clone()
                }
            }
            (Some((_, branch)), WorktreeState::Creating) => {
                // Crash AFTER Git creation but before publication: the
                // checkout exists at exactly the id-derived reserved
                // path, so ownership is unambiguous — complete the
                // transition (§14 step 7: repair when unambiguous).
                let attached = branch
                    .as_deref()
                    .map(|b| b.trim_start_matches("refs/heads/"))
                    .unwrap_or("");
                if attached != record.branch {
                    audit_lifecycle(
                        "worktree_reconcile_mismatch",
                        record,
                        "checkout branch does not match the reservation",
                    );
                    WorktreeRecord {
                        state: WorktreeState::RecoveryRequired,
                        ..record.clone()
                    }
                } else {
                    audit_lifecycle(
                        "worktree_reconcile_completed",
                        record,
                        "crashed creation completed unambiguously",
                    );
                    WorktreeRecord {
                        state: WorktreeState::Active,
                        ..record.clone()
                    }
                }
            }
            (Some(_), WorktreeState::Active) => record.clone(),
            (Some(_), WorktreeState::Missing | WorktreeState::RecoveryRequired) => {
                // The checkout came back or still exists; require an
                // explicit reconcile decision before trusting it again —
                // this path is conservative: never silently re-activate.
                record.clone()
            }
            (_, _) => record.clone(),
        };
        Ok(reconciled)
    }

    /// Reconciles every record (startup/restart, §14). Corrupt
    /// records fail closed with a structured error naming the id.
    pub async fn reconcile(&self) -> Result<Vec<WorktreeRecord>, WorktreeError> {
        let ids = self.list_record_ids()?;
        let mut reconciled = Vec::with_capacity(ids.len());
        for id in &ids {
            reconciled.push(self.inspect(id).await?);
        }
        Ok(reconciled)
    }

    /// Ownership-checked removal (§13). The session must still own the
    /// worktree, the path must be the recorded managed path, and the
    /// checkout must still be the one this record describes. A dirty
    /// worktree is preserved and reported — never silently discarded.
    pub async fn remove(
        &self,
        id: &str,
        session_id: &str,
    ) -> Result<WorktreeRecord, WorktreeError> {
        if !is_safe_worktree_id(id) {
            return Err(WorktreeError::Invalid(id.to_owned()));
        }
        let _lock = StoreLock::acquire(&self.lock_path())
            .map_err(|e| WorktreeError::Store(format!("worktree lifecycle lock: {e}")))?;

        let record = self.load_record(id)?;
        // Ownership invariant (§10): the workspace/agent/session binding
        // must still hold. A changed-ownership worktree is refused.
        if record.session_id != session_id {
            audit_lifecycle(
                "worktree_remove_denied_ownership",
                &record,
                "remove requested by a non-owning session",
            );
            return Err(WorktreeError::Ownership(id.to_owned()));
        }
        match record.state {
            WorktreeState::Removed => {
                // Idempotent terminal state: report already removed.
                return Err(WorktreeError::NotFound(id.to_owned()));
            }
            WorktreeState::Missing => {
                // The checkout is already gone; complete the lifecycle.
                let removed = WorktreeRecord {
                    state: WorktreeState::Removed,
                    removed_at: Some(now_rfc3339()),
                    ..record
                };
                self.write_record_atomic(&removed)?;
                audit_lifecycle(
                    "worktree_removed_missing",
                    &removed,
                    "missing worktree finalized as removed",
                );
                return Ok(removed);
            }
            WorktreeState::RecoveryRequired => {
                return Err(WorktreeError::Corrupt(id.to_owned()));
            }
            WorktreeState::Creating | WorktreeState::Active | WorktreeState::Removing => {}
        }

        let checkout = self.validate_containment(&record)?;
        // Removing the main workspace is structurally impossible (the
        // path is strictly below .agent/worktrees) but the guard stays
        // explicit at the mutation boundary (§13).
        if checkout == self.root {
            return Err(WorktreeError::Invalid(
                "refusing to remove the workspace root".into(),
            ));
        }

        let git = GitService::open(&self.root).map_err(|e| WorktreeError::Store(format!("{e}")))?;
        // Structured, non-destructive removal: git refuses a dirty
        // worktree, preserving uncommitted agent/user changes (§13).
        if let Err(error) = git.worktree_remove(&checkout).await {
            let message = format!("{error:#}");
            if message.contains("dirty") || message.contains("contains modified") {
                audit_lifecycle(
                    "worktree_remove_dirty",
                    &record,
                    "removal refused: uncommitted changes preserved",
                );
                return Err(WorktreeError::Dirty(id.to_owned()));
            }
            // The checkout may already be gone out of band: verify
            // against Git before deciding (§14).
            let listed = git
                .worktree_list()
                .await
                .map_err(|e| WorktreeError::Store(format!("{e:#}")))?;
            let absolute = checkout
                .to_str()
                .ok_or_else(|| WorktreeError::Store("path not UTF-8".into()))?
                .to_owned();
            let still_listed = listed
                .iter()
                .any(|(path, _)| same_path(Path::new(path), Path::new(&absolute)));
            if still_listed {
                audit_lifecycle(
                    "worktree_remove_failed",
                    &record,
                    &format!("git worktree remove failed: {message}"),
                );
                return Err(WorktreeError::Store(format!(
                    "git worktree remove failed: {message}"
                )));
            }
        }

        let removed = WorktreeRecord {
            state: WorktreeState::Removed,
            removed_at: Some(now_rfc3339()),
            ..record
        };
        self.write_record_atomic(&removed)?;
        audit_lifecycle(
            "worktree_removed",
            &removed,
            "worktree removed and verified",
        );
        Ok(removed)
    }
}

/// Emits a worktree lifecycle event through the canonical persistent
/// audit boundary (§18) with bounded, non-secret correlation only.
fn audit_lifecycle(action: &'static str, record: &WorktreeRecord, detail: &str) {
    let correlation = crate::services::audit::AuditCorrelation {
        workspace_id: Some(record.workspace_id.clone()),
        agent_id: Some(record.agent_id.clone()),
        session_id: Some(record.session_id.clone()),
        reason: Some(action.to_owned()),
        ..crate::services::audit::AuditCorrelation::default()
    };
    // Subject = the stable worktree id (never a host path).
    crate::services::audit::record_outcome(
        "allow",
        action,
        &record.worktree_id,
        detail,
        &correlation,
    );
}

/// Whether two worktree paths name the same directory. `git worktree
/// list --porcelain` reports each checkout's REAL path (canonicalized:
/// `/private/var/...` on macOS when the root was under a `/var`
/// symlink; forward slashes on Windows), while AWH builds the expected
/// path lexically from the workspace root. Canonicalizing both sides
/// (§7: "where canonicalization is possible, verify the resolved
/// path") makes the comparison platform-honest; the lexical fallback
/// covers paths that cannot be resolved at comparison time.
fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(resolved_a), Ok(resolved_b)) => resolved_a == resolved_b,
        _ => a == b,
    }
}

/// Safe worktree-id shape: `wt-` prefix + [A-Za-z0-9-] only, bounded
/// length. Rejects traversal, separators, control characters, and
/// option-looking values before any filesystem use (§16).
pub fn is_safe_worktree_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.starts_with("wt-")
        && !id.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '-'))
}
