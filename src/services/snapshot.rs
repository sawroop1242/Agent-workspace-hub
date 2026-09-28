//! Durable file snapshots and provenance for the canonical edit lifecycle
//! (AWE-012 / #33).
//!
//! Ownership contract:
//!
//! - this store owns *exact* pre-edit bytes for rollback plus the queryable
//!   provenance record for every committed edit;
//! - [`crate::services::edit::EditService`] owns transaction orchestration;
//! - [`crate::core::policy::PolicyStore`] owns policy rules; the capability
//!   stores own caller identity; [`crate::services::audit`] owns security
//!   event records. Nothing here duplicates those systems.
//!
//! Storage layout, all under the workspace's `.agent` state:
//!
//! ```text
//! .agent/snapshots/<snapshot_id>.json          — manifest (metadata + links)
//! .agent/snapshot-contents/<entry_id>.bin      — exact pre-edit bytes
//! .agent/provenance/<edit_id>.json             — provenance record
//! ```
//!
//! Durability contract:
//!
//! - a snapshot is only visible to readers once its content blobs and its
//!   manifest have both committed atomically and serialisation finished. All
//!   writes go through tempfile + fsync + rename, guarded by `StoreLock`, so
//!   a reader never observes a torn snapshot;
//! - a corrupt, truncated, or integrity-mismatched record fails closed: it
//!   is never mistaken for a valid snapshot;
//! - `SnapshotId` / entry ids are validated against the same rules as grant
//!   ids — no separators, traversal, absolute paths, or oversized names.
//!
//! This module contains no workspace-sensitive secrets in error messages or
//! log output: only ids, hashes, counts, and status values may be surfaced.

use crate::mcp::store_lock::StoreLock;
use crate::services::edit::sha256_hex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Wire/serialization version of the snapshot storage schema. Bump when the
/// on-disk layout changes; unknown future versions fail closed on read.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Size limits — never let one snapshot exhaust disk or abstain-bomb the
/// workspace state directory.
pub const MAX_SNAPSHOT_CONTENT_BYTES: u64 = crate::services::files::MAX_FILE_BYTES;
pub const MAX_SNAPSHOT_FILES: usize = 256;
pub const MAX_SNAPSHOT_ID_LEN: usize = 128;
pub const MAX_PROVENANCE_BYTES: u64 = 512 * 1024;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// A validated, filesystem-safe snapshot identifier.
///
/// See `is_safe_grant_id` for the equivalent rule in capability storage. A
/// snapshot id follows the same contract: it is namespaced to a single
/// directory, debuggable, and cannot escape it through path manipulation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub String);

impl Default for SnapshotId {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotId {
    /// Creates a fresh, unique, collision-resistant snapshot identifier.
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let pid = std::process::id() as u64;
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("snap-{:016x}-{}-{}", nanos as u64, pid, seq))
    }

    /// Validates an externally supplied identifier; rejects separators,
    /// traversal, absolute paths, control characters, and oversized ids.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        let id = &self.0;
        if id.is_empty() {
            return Err(SnapshotError::InvalidId("snapshot id is empty".into()));
        }
        if id.len() > MAX_SNAPSHOT_ID_LEN {
            return Err(SnapshotError::InvalidId("snapshot id is too long".into()));
        }
        if id.contains('/') || id.contains('\\') || id.contains("..") {
            return Err(SnapshotError::InvalidId(
                "snapshot id must not contain path separators or traversal".into(),
            ));
        }
        if id.bytes().any(|b| b == 0 || b.is_ascii_control()) {
            return Err(SnapshotError::InvalidId(
                "snapshot id must not contain control characters".into(),
            ));
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifies one file inside a snapshot. Declared with the same validation
/// contract as [`SnapshotId`]. Entry ids are unique *within* their
/// snapshot; the content directory is namespaced per snapshot id, so two
/// snapshots never share or clobber each other's blobs (§22: no manifest
/// may point at another snapshot's content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotEntryId(pub String);

impl SnapshotEntryId {
    pub fn new(trace: usize) -> Self {
        Self(format!("entry-{trace:06}"))
    }

    pub fn validate(&self) -> Result<(), SnapshotError> {
        let id = &self.0;
        if id.is_empty() || id.len() > MAX_SNAPSHOT_ID_LEN {
            return Err(SnapshotError::InvalidId("invalid entry id length".into()));
        }
        if id.contains('/') || id.contains('\\') || id.contains("..") {
            return Err(SnapshotError::InvalidId(
                "entry id must not contain path separators or traversal".into(),
            ));
        }
        if id.bytes().any(|b| b == 0 || b.is_ascii_control()) {
            return Err(SnapshotError::InvalidId(
                "entry id must not contain control characters".into(),
            ));
        }
        Ok(())
    }
}

/// One file's pre-edit state supplied to [`SnapshotStore::create_snapshot`].
///
/// An existing file contributes its exact bytes; a file the edit is about
/// to CREATE contributes [`SnapshotFile::Missing`] — recovery material
/// must distinguish "did not exist" from "existed empty" (§6.2: missing
/// is never encoded as an empty file, and an existing zero-byte file is
/// a valid state with its own hash).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotFile {
    /// The file existed; `bytes` are its exact pre-edit content.
    Existing { path: String, bytes: Vec<u8> },
    /// The file did not exist; the edit creates it and rollback may
    /// delete it (subject to the rollback owner's authorization and
    /// produced-state guard).
    Missing { path: String },
}

impl SnapshotFile {
    fn path(&self) -> &str {
        match self {
            SnapshotFile::Existing { path, .. } => path,
            SnapshotFile::Missing { path } => path,
        }
    }
}

/// Per-file snapshot entry: metadata plus the rollback-safe bytes link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshotEntry {
    /// Identifier of this entry's content blob.
    pub entry_id: SnapshotEntryId,
    /// Workspace-relative path.
    pub path: String,
    /// True when the file existed before the edit, false when this edit
    /// created it (rollback must delete the created file).
    pub existed_before: bool,
    /// SHA-256 of the exact pre-edit bytes.
    pub hash_before: String,
    /// Size in bytes of the pre-edit file.
    pub size_before: u64,
    /// Logical line count of the pre-edit file under the canonical
    /// `line_count` semantics; `None` for a missing file.
    pub line_count_before: Option<usize>,
    /// Encoded size of the stored content blob; protects truncation and
    /// lets readers detect a torn blob before attempting decode.
    pub content_len: u64,
    /// RFC 3339 timestamp when the entry was captured.
    pub captured_at: String,
}

/// Top-level snapshot manifest: transaction identity plus every entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// Durable snapshot identifier.
    pub id: SnapshotId,
    /// The transaction these files were prepared for.
    pub edit_id: String,
    /// Schema version for future-safe reads; must equal
    /// [`SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// All per-file entries in deterministic (path-ascending) order.
    pub entries: Vec<FileSnapshotEntry>,
    /// Free-form operator supplied or system note; never raw file content.
    pub reason: Option<String>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}

/// Structured provenance of one edit transaction. This record explains
/// *what happened*, while the snapshot stores *what exact bytes recovery
/// needs*. Provenance must never carry recovery payload content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    /// Transaction identifier (same as the snapshot's `edit_id`).
    pub edit_id: String,
    /// Snapshot reference created for this transaction.
    pub snapshot_id: SnapshotId,
    /// The operation(s) applied (stable string forms).
    pub operations: Vec<String>,
    /// Affected paths, in deterministic order.
    pub paths: Vec<String>,
    /// Caller/agent identity when present.
    pub agent_id: Option<String>,
    /// Caller session identity when present.
    pub session_id: Option<String>,
    /// Workspace identity when present.
    pub workspace_id: Option<String>,
    /// Entry path → (before hash, after hash).
    pub hash_edges: Vec<(String, String)>,
    /// Final outcome of the transaction.
    pub outcome: ProvenanceOutcome,
    /// When the record landed, RFC 3339.
    pub created_at: String,
}

/// Result of an edit transaction as recorded by provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceOutcome {
    Committed,
    RolledBack { reason: Option<String> },
    Failed { reason: String },
}

/// Error surface of the snapshot layer; deliberately separated from
/// [`crate::services::edit::EditError`] so unauthorised restore attempts and
/// durability failures stay distinguishable.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// Input identifier was structurally invalid.
    #[error("invalid snapshot input: {0}")]
    InvalidId(String),
    /// The snapshot record does not exist.
    #[error("snapshot not found: {0}")]
    NotFound(String),
    /// The record exists but is corrupted or tampered with.
    #[error("snapshot integrity failure in {0}")]
    Corruption(String),
    /// Resource limits would be exceeded.
    #[error("snapshot exceeds configured limits: {0}")]
    LimitExceeded(String),
    /// The on-disk schema version is not supported.
    #[error("unsupported snapshot schema version {got}; expected {expected}")]
    UnsupportedSchema { expected: u32, got: u32 },
    /// The requested entry records a file that did not exist before the
    /// edit; there are no original bytes to return (the rollback owner
    /// deletes the edit-created file instead of restoring bytes).
    #[error("entry {0} records a pre-edit missing file; no original bytes are stored")]
    MissingContent(String),
    /// Underlying filesystem error (containment, locking, I/O).
    #[error("snapshot storage error: {0}")]
    Storage(String),
}

/// The recovery material for one edit: each affected path with its exact
/// pre-edit bytes, or `None` when the edit created the file (the rollback
/// owner deletes it instead of restoring bytes).
pub type RecoveryView = Vec<(String, Option<Vec<u8>>)>;

/// Canonical store for file snapshots and provenance.
///
/// All operations are fail-closed and never mutate or reveal workspace
/// content. Reads verify schema version and content hashes; writes are
/// atomic (tempfile + fsync + rename) under `StoreLock` and only become
/// visible once committed. The store never searches arbitrary paths: every
/// lookup is keyed by pre-validated ids under fixed directories.
#[derive(Clone)]
pub struct SnapshotStore {
    root: PathBuf,
}

impl SnapshotStore {
    /// Creates a store rooted at the workspace root. The store itself holds
    /// no mutable state; every operation sets up its own lock.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            root: workspace_root.into(),
        }
    }

    fn snapshots_dir(&self) -> PathBuf {
        self.root.join(".agent").join("snapshots")
    }

    fn contents_dir(&self) -> PathBuf {
        self.root.join(".agent").join("snapshot-contents")
    }

    fn provenance_dir(&self) -> PathBuf {
        self.root.join(".agent").join("provenance")
    }

    fn manifest_path(&self, id: &SnapshotId) -> PathBuf {
        self.snapshots_dir().join(format!("{}.json", id.as_str()))
    }

    /// Content blobs live in a per-snapshot subdirectory of the fixed
    /// snapshot-contents directory: `entry-000000.bin` inside snapshot A
    /// is a different file from the one inside snapshot B, so concurrent
    /// or sequential snapshots never overwrite each other's recovery
    /// material (§22), and a manifest's blobs are always its own.
    fn content_path(&self, snapshot: &SnapshotId, entry: &SnapshotEntryId) -> PathBuf {
        self.contents_dir()
            .join(snapshot.as_str())
            .join(format!("{}.bin", entry.0))
    }

    fn provenance_path(&self, edit_id: &str) -> Result<PathBuf, SnapshotError> {
        validate_id_component(edit_id)?;
        Ok(self.provenance_dir().join(format!("{}.json", edit_id)))
    }

    fn ensure_dirs(&self) -> Result<(), SnapshotError> {
        for dir in [
            self.snapshots_dir(),
            self.contents_dir(),
            self.provenance_dir(),
        ] {
            // The error message names the operation, never the host
            // path: caller-visible errors must not leak absolute paths.
            fs::create_dir_all(&dir)
                .map_err(|e| SnapshotError::Storage(format!("create snapshot state dir: {e}")))?;
        }
        Ok(())
    }

    /// Captures a snapshot of `files: Vec<(path, exact original bytes)>` for
    /// the transaction identified by `edit_id`. Compatibility wrapper over
    /// [`Self::create_snapshot`]: every file is treated as existing.
    pub fn create(
        &self,
        edit_id: &str,
        files: &[(String, Vec<u8>)],
        reason: Option<String>,
    ) -> Result<FileSnapshot, SnapshotError> {
        let inputs: Vec<SnapshotFile> = files
            .iter()
            .map(|(path, bytes)| SnapshotFile::Existing {
                path: path.clone(),
                bytes: bytes.clone(),
            })
            .collect();
        self.create_snapshot(edit_id, &inputs, reason)
    }

    /// The canonical creation API (§11 sequence): validates every
    /// workspace-relative path, enforces limits, writes every content
    /// blob, VERIFIES each written blob against its recorded length and
    /// SHA-256, and only then atomically publishes one manifest. A
    /// failure at any stage leaves no published manifest, so no reader
    /// can mistake incomplete material for a recovery source.
    ///
    /// Existing files contribute exact bytes; [`SnapshotFile::Missing`]
    /// records that the edit creates the file (rollback deletes rather
    /// than restores, subject to the rollback owner's guards). A missing
    /// file is never encoded as an empty file.
    ///
    /// This API authorizes nothing and reads nothing beyond the supplied
    /// states — capture happens at the edit boundary's direction.
    pub fn create_snapshot(
        &self,
        edit_id: &str,
        files: &[SnapshotFile],
        reason: Option<String>,
    ) -> Result<FileSnapshot, SnapshotError> {
        validate_id_component(edit_id)?;
        if files.len() > MAX_SNAPSHOT_FILES {
            return Err(SnapshotError::LimitExceeded(format!(
                "snapshot covers {} files (max {MAX_SNAPSHOT_FILES})",
                files.len()
            )));
        }
        // Canonical workspace-relative path validation (§6.3): reject
        // absolute/traversal/ambiguous/unsafe paths BEFORE persistence,
        // reusing the edit model's validator rather than a second
        // normalization.
        for file in files {
            crate::services::edit::validate_path(file.path()).map_err(|e| {
                SnapshotError::InvalidId(format!("snapshot path {:?}: {e}", file.path()))
            })?;
        }
        // Integrity + size limit checks first. No single existing file may
        // exceed the canonical file limit; the total is additionally
        // capped. Missing files carry no bytes.
        let total: u64 = files
            .iter()
            .map(|f| match f {
                SnapshotFile::Existing { bytes, .. } => bytes.len() as u64,
                SnapshotFile::Missing { .. } => 0,
            })
            .sum();
        for file in files {
            if let SnapshotFile::Existing { path, bytes } = file {
                if bytes.len() as u64 > MAX_SNAPSHOT_CONTENT_BYTES {
                    return Err(SnapshotError::LimitExceeded(format!(
                        "snapshot content for '{path}' exceeds the {MAX_SNAPSHOT_CONTENT_BYTES}-byte limit"
                    )));
                }
            }
        }
        if total > MAX_SNAPSHOT_CONTENT_BYTES * MAX_SNAPSHOT_FILES as u64 {
            return Err(SnapshotError::LimitExceeded(format!(
                "snapshot total size {total} exceeds cap"
            )));
        }

        self.ensure_dirs()?;
        let mut sorted: Vec<&SnapshotFile> = files.iter().collect();
        sorted.sort_by(|a, b| a.path().cmp(b.path()));

        let id = SnapshotId::new();
        id.validate()?;

        // Write content blobs first (uncommitted state must not look
        // recovery-valid). Then the manifest commits under a lock so a
        // reader sees either the complete artifact or nothing.
        let manifest_lock = self.manifest_path(&id);
        let _lock = StoreLock::acquire(&manifest_lock)
            .map_err(|e| SnapshotError::Storage(format!("snapshot lock: {e}")))?;

        let mut entries = Vec::with_capacity(sorted.len());
        for (idx, file) in sorted.iter().enumerate() {
            let entry_id = SnapshotEntryId::new(idx);
            entry_id.validate()?;
            let path = file.path().to_owned();
            match file {
                SnapshotFile::Existing { bytes, .. } => {
                    let raw_len = bytes.len() as u64;
                    let hash = sha256_hex(bytes);
                    let blob_path = self.content_path(&id, &entry_id);
                    write_blob(&blob_path, bytes)
                        .map_err(|e| SnapshotError::Storage(format!("write content blob: {e}")))?;
                    // §11 step 6 — verify the written blob BEFORE the
                    // manifest can be published: read-back must match the
                    // recorded length and hash exactly.
                    let read_back = fs::read(&blob_path)
                        .map_err(|e| SnapshotError::Storage(format!("verify content blob: {e}")))?;
                    if read_back.len() as u64 != raw_len || sha256_hex(&read_back) != hash {
                        return Err(SnapshotError::Storage(
                            "content blob failed read-back verification".into(),
                        ));
                    }
                    // Line count is diagnostic only (§13): computed over the
                    // raw bytes, never through a lossy text decode.
                    let line_count = if bytes.is_empty() {
                        Some(0)
                    } else if let Ok(text) = std::str::from_utf8(bytes) {
                        Some(crate::services::edit::FileState::from_content(&path, text).line_count)
                    } else {
                        None
                    };
                    entries.push(FileSnapshotEntry {
                        entry_id,
                        path,
                        existed_before: true,
                        hash_before: hash,
                        size_before: raw_len,
                        line_count_before: line_count,
                        content_len: raw_len,
                        captured_at: now_rfc3339(),
                    });
                }
                SnapshotFile::Missing { .. } => {
                    // No blob, no hash, no bytes: the manifest records the
                    // pre-edit absence explicitly so recovery can never
                    // confuse "missing" with "existed empty" (§6.2).
                    entries.push(FileSnapshotEntry {
                        entry_id,
                        path,
                        existed_before: false,
                        hash_before: String::new(),
                        size_before: 0,
                        line_count_before: None,
                        content_len: 0,
                        captured_at: now_rfc3339(),
                    });
                }
            }
        }

        let manifest = FileSnapshot {
            id: id.clone(),
            edit_id: edit_id.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            entries,
            reason,
            created_at: now_rfc3339(),
        };
        write_blob(
            &manifest_lock,
            &serde_json::to_vec_pretty(&manifest)
                .map_err(|e| SnapshotError::Storage(format!("serialise manifest: {e}")))?,
        )
        .map_err(|e| SnapshotError::Storage(format!("commit manifest: {e}")))?;
        Ok(manifest)
    }

    /// Loads a snapshot manifest only if every referenced content blob is
    /// present and hash-verified. Returns a corruption error otherwise.
    /// Entries recording pre-edit-missing files carry no blob and are
    /// validated structurally instead.
    pub fn load(&self, id: &SnapshotId) -> Result<FileSnapshot, SnapshotError> {
        id.validate()?;
        let path = self.manifest_path(id);
        if !path.exists() {
            return Err(SnapshotError::NotFound(id.to_string()));
        }
        let bytes = fs::read(&path)
            .map_err(|_| SnapshotError::Corruption(format!("unreadable manifest {id}")))?;
        let manifest: FileSnapshot = serde_json::from_slice(&bytes)
            .map_err(|e| SnapshotError::Corruption(format!("manifest parse: {e}")))?;
        if manifest.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(SnapshotError::UnsupportedSchema {
                expected: SNAPSHOT_SCHEMA_VERSION,
                got: manifest.schema_version,
            });
        }
        if manifest.id != *id {
            return Err(SnapshotError::Corruption(format!(
                "manifest id mismatch: declared {} vs requested {}",
                manifest.id.as_str(),
                id.as_str()
            )));
        }
        for entry in &manifest.entries {
            entry.entry_id.validate()?;
            if !entry.existed_before {
                // Pre-edit-missing file: no blob may exist and the
                // recorded shape must be the documented "no bytes" one.
                if !entry.hash_before.is_empty() || entry.content_len != 0 {
                    return Err(SnapshotError::Corruption(format!(
                        "missing-file entry {} carries content metadata",
                        entry.entry_id.0
                    )));
                }
                continue;
            }
            let blob = self.content_path(id, &entry.entry_id);
            if !blob.exists() {
                return Err(SnapshotError::Corruption(format!(
                    "missing content blob for entry {}",
                    entry.entry_id.0
                )));
            }
            let data = fs::read(&blob).map_err(|_| {
                SnapshotError::Corruption(format!("unreadable blob {}", entry.entry_id.0))
            })?;
            if data.len() as u64 != entry.content_len {
                return Err(SnapshotError::Corruption(format!(
                    "blob length mismatch for {} ({} != {})",
                    entry.entry_id.0,
                    data.len(),
                    entry.content_len
                )));
            }
            if sha256_hex(&data) != entry.hash_before {
                return Err(SnapshotError::Corruption(format!(
                    "integrity mismatch for {}",
                    entry.entry_id.0
                )));
            }
        }
        Ok(manifest)
    }

    /// Returns the exact pre-edit bytes for `entry_id` after hashing them
    /// against the manifest: no silent truncation, no partial bytes, and
    /// never bytes from outside this snapshot. Fails closed when the
    /// entry id is not part of the supplied manifest (§18: no
    /// cross-snapshot or unreferenced bytes) or when the entry records a
    /// pre-edit-missing file (there are no original bytes to return).
    pub fn entry_bytes(
        &self,
        manifest: &FileSnapshot,
        entry_id: &SnapshotEntryId,
    ) -> Result<Vec<u8>, SnapshotError> {
        entry_id.validate()?;
        let Some(entry) = manifest.entries.iter().find(|e| &e.entry_id == entry_id) else {
            return Err(SnapshotError::NotFound(format!(
                "entry {} is not part of snapshot {}",
                entry_id.0,
                manifest.id.as_str()
            )));
        };
        if !entry.existed_before {
            return Err(SnapshotError::MissingContent(entry_id.0.clone()));
        }
        let path = self.content_path(&manifest.id, entry_id);
        let data = fs::read(&path)
            .map_err(|_| SnapshotError::Corruption(format!("entry {} unreadable", entry_id.0)))?;
        if data.len() as u64 != entry.content_len {
            return Err(SnapshotError::Corruption("entry size mismatch".into()));
        }
        if sha256_hex(&data) != entry.hash_before {
            return Err(SnapshotError::Corruption("entry hash mismatch".into()));
        }
        Ok(data)
    }

    /// Records provenance for `edit_id` linked to `snapshot_id`. Never
    /// written until the edit committed or failed alongside recovery; the
    /// record is an observation, not a precondition for entering the file.
    pub fn record_provenance(&self, record: &ProvenanceRecord) -> Result<(), SnapshotError> {
        validate_id_component(&record.edit_id)?;
        self.ensure_dirs()?;
        let path = self.provenance_path(&record.edit_id)?;
        record
            .snapshot_id
            .validate()
            .map_err(|e| SnapshotError::Storage(format!("snapshot id in provenance: {e}")))?;
        let bytes = serde_json::to_vec_pretty(record)
            .map_err(|e| SnapshotError::Storage(format!("serialise provenance: {e}")))?;
        if bytes.len() as u64 > MAX_PROVENANCE_BYTES {
            return Err(SnapshotError::LimitExceeded(format!(
                "provenance record for {} exceeds {} bytes",
                record.edit_id, MAX_PROVENANCE_BYTES
            )));
        }
        write_blob(&path, &bytes)
            .map_err(|e| SnapshotError::Storage(format!("commit provenance: {e}")))?;
        Ok(())
    }

    /// Reads the provenance committed for `edit_id`.
    pub fn provenance(&self, edit_id: &str) -> Result<ProvenanceRecord, SnapshotError> {
        let path = self.provenance_path(edit_id)?;
        if !path.exists() {
            return Err(SnapshotError::NotFound(format!(
                "provenance missing for {edit_id}"
            )));
        }
        let bytes = fs::read(&path)
            .map_err(|_| SnapshotError::Corruption(format!("provenance read {edit_id}")))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| SnapshotError::Corruption(format!("provenance parse: {e}")))
    }

    /// AWE-014: the canonical bounded history read over the provenance
    /// records — newest first, at most `limit` entries (callers bound
    /// the page themselves; this store never returns unbounded history).
    /// Every listed record must parse and validate: a corrupt record
    /// fails closed with a structured error rather than being silently
    /// skipped or fabricated (§19/§43).
    pub fn list_provenance(&self, limit: usize) -> Result<Vec<ProvenanceRecord>, SnapshotError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        self.ensure_dirs()?;
        let dir = self.provenance_dir();
        let mut ids: Vec<String> = Vec::new();
        for entry in fs::read_dir(&dir)
            .map_err(|e| SnapshotError::Storage(format!("listing provenance: {e}")))?
        {
            let path = entry
                .map_err(|e| SnapshotError::Storage(format!("listing provenance: {e}")))?
                .path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                validate_id_component(stem)?;
                ids.push(stem.to_owned());
            }
        }
        // Newest first: reverse lexicographic order over the edit-id
        // stems, which start with a monotonically increasing sequence
        // prefix from EditId::new.
        ids.sort();
        ids.reverse();
        let mut records = Vec::with_capacity(ids.len().min(limit));
        for id in ids.iter().take(limit) {
            records.push(self.provenance(id)?);
        }
        Ok(records)
    }

    /// Lists snapshot ids, deterministically ordered for the caller.
    pub fn list(&self) -> Result<Vec<SnapshotId>, SnapshotError> {
        self.ensure_dirs()?;
        let mut out = Vec::new();
        let dir = self.snapshots_dir();
        for raw in fs::read_dir(&dir)
            .map_err(|e| SnapshotError::Storage(format!("listing snapshots: {e}")))?
        {
            let path = raw
                .map_err(|e| SnapshotError::Storage(format!("listing entry: {e}")))?
                .path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let id = SnapshotId(stem.to_string());
                id.validate()?;
                out.push(id);
            }
        }
        out.sort();
        Ok(out)
    }

    /// Produces the recovery view for the edit identified by `edit_id`:
    /// for every affected resource, the pre-edit state the rollback owner
    /// needs — exact original bytes for files that existed, and an
    /// explicit `None` for files the edit created (rollback deletes
    /// those, subject to the rollback owner's authorization and
    /// produced-state guard; §19 requires the metadata to distinguish
    /// the two). Fails closed on any sign of corruption or missing
    /// data, and never returns bytes from outside the referenced
    /// snapshot.
    pub fn recovery_view(&self, edit_id: &str) -> Result<RecoveryView, SnapshotError> {
        validate_id_component(edit_id)?;
        let prov = self.provenance(edit_id)?;
        let manifest = self.load(&prov.snapshot_id)?;
        // Snapshot-to-edit binding (§16): the snapshot must belong to
        // exactly the edit being recovered.
        if manifest.edit_id != edit_id {
            return Err(SnapshotError::Corruption(format!(
                "snapshot {} belongs to edit {}, not {edit_id}",
                manifest.id.as_str(),
                manifest.edit_id
            )));
        }
        let mut pairs = Vec::with_capacity(manifest.entries.len());
        for entry in &manifest.entries {
            let bytes = if entry.existed_before {
                Some(self.entry_bytes(&manifest, &entry.entry_id)?)
            } else {
                None
            };
            pairs.push((entry.path.clone(), bytes));
        }
        Ok(pairs)
    }
}

fn validate_id_component(id: &str) -> Result<(), SnapshotError> {
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || id.len() > MAX_SNAPSHOT_ID_LEN
    {
        return Err(SnapshotError::InvalidId(format!("invalid id: {id:?}")));
    }
    Ok(())
}

/// Atomic content write: tempfile in the same directory, fsync, rename.
fn write_blob(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::other("target has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut tmp, bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(target).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_at(dir: &Path) -> SnapshotStore {
        SnapshotStore::new(dir.to_path_buf())
    }

    #[test]
    fn snapshot_id_validation_rejects_unsafe_names() {
        assert!(SnapshotId::new().validate().is_ok());
        for bad in [
            "",
            "../escape",
            "a/b",
            "a\\b",
            "has\ncntrl",
            "x".repeat(200).as_str(),
        ] {
            let id = SnapshotId(bad.to_string());
            assert!(id.validate().is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn snapshot_ids_are_collision_resistant() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..256 {
            let id = SnapshotId::new();
            assert!(seen.insert(id.clone()), "duplicate snapshot id");
        }
    }

    #[test]
    fn manifest_round_trip_preserves_every_byte() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let files = vec![
            ("a.txt".to_string(), b"alpha\nbeta\n".to_vec()),
            ("b.txt".to_string(), Vec::new()), // empty-but-existing
            ("unicode.txt".to_string(), "बीटा 🦀\n".as_bytes().to_vec()),
        ];
        let manifest = store.create("edit-001", &files, None).unwrap();
        assert_eq!(manifest.edit_id, "edit-001");
        assert_eq!(manifest.entries.len(), 3);
        assert_eq!(manifest.schema_version, SNAPSHOT_SCHEMA_VERSION);

        let loaded = store.load(&manifest.id).unwrap();
        assert_eq!(loaded, manifest);
        let bytes_a = store
            .entry_bytes(&loaded, &loaded.entries[0].entry_id)
            .unwrap();
        assert_eq!(bytes_a, b"alpha\nbeta\n");
        let bytes_empty = store
            .entry_bytes(&loaded, &loaded.entries[1].entry_id)
            .unwrap();
        assert!(bytes_empty.is_empty());
        let bytes_unicode = store
            .entry_bytes(&loaded, &loaded.entries[2].entry_id)
            .unwrap();
        assert_eq!(bytes_unicode, "बीटा 🦀\n".as_bytes());
    }

    #[test]
    fn snapshot_survives_store_recreation_simulating_process_restart() {
        let temp = tempfile::tempdir().unwrap();
        let one = store_at(temp.path());
        let manifest = one
            .create(
                "edit-777",
                &[("f.txt".to_string(), b"hello\n".to_vec())],
                None,
            )
            .unwrap();
        drop(one);
        let rebuilt = store_at(temp.path());
        let loaded = rebuilt.load(&manifest.id).unwrap();
        assert_eq!(loaded.id, manifest.id);
        assert_eq!(loaded.edit_id, "edit-777");
        assert_eq!(loaded.entries.len(), 1);
    }

    #[test]
    fn corrupted_blob_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let manifest = store
            .create(
                "edit-corrupt",
                &[("f.txt".to_string(), b"valid content\n".to_vec())],
                None,
            )
            .unwrap();
        // Tamper with the stored blob after commit (per-snapshot dir).
        let blob = temp
            .path()
            .join(".agent/snapshot-contents")
            .join(manifest.id.as_str())
            .join("entry-000000.bin");
        std::fs::write(&blob, b"tampered\n").unwrap();
        let err = store.load(&manifest.id).unwrap_err();
        assert!(
            matches!(err, SnapshotError::Corruption(_)),
            "tampered blob must be detected, got {err:?}"
        );
    }

    #[test]
    fn unknown_snapshot_id_is_not_found() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let err = store.load(&SnapshotId("missing-1".into())).unwrap_err();
        assert!(matches!(err, SnapshotError::NotFound(_)));
    }

    #[test]
    fn oversized_snapshot_is_rejected_before_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let huge = vec![b'x'; (MAX_SNAPSHOT_CONTENT_BYTES + 1) as usize];
        let err = store
            .create("edit-huge", &[("big.bin".to_string(), huge)], None)
            .unwrap_err();
        assert!(matches!(err, SnapshotError::LimitExceeded(_)));
    }

    #[test]
    fn provenance_round_trips_and_never_embeds_file_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let manifest = store
            .create(
                "edit-prov",
                &[("f.txt".to_string(), b"abc\n".to_vec())],
                None,
            )
            .unwrap();
        let record = ProvenanceRecord {
            edit_id: "edit-prov".into(),
            snapshot_id: manifest.id.clone(),
            operations: vec!["Replace".into()],
            paths: vec!["f.txt".into()],
            agent_id: Some("writer".into()),
            session_id: Some("sess-1".into()),
            workspace_id: None,
            hash_edges: vec![("f.txt".into(), manifest.entries[0].hash_before.clone())],
            outcome: ProvenanceOutcome::Committed,
            created_at: now_rfc3339(),
        };
        store.record_provenance(&record).unwrap();
        let loaded = store.provenance("edit-prov").unwrap();
        assert_eq!(loaded, record);
        let serialised = serde_json::to_string(&loaded).unwrap();
        assert!(
            !serialised.contains("abc"),
            "provenance must never embed file content bytes"
        );
    }

    #[test]
    fn recovery_view_rejects_corrupt_snapshot_linkage() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let _manifest = store
            .create("edit-rv", &[("f.txt".to_string(), b"x\n".to_vec())], None)
            .unwrap();
        // Forge a provenance pointing at a non-existent snapshot id.
        let forged = ProvenanceRecord {
            edit_id: "edit-rv".into(),
            snapshot_id: SnapshotId("not-real".into()),
            operations: vec!["Replace".into()],
            paths: vec![],
            agent_id: None,
            session_id: None,
            workspace_id: None,
            hash_edges: vec![],
            outcome: ProvenanceOutcome::Committed,
            created_at: now_rfc3339(),
        };
        store.record_provenance(&forged).unwrap();
        let err = store.recovery_view("edit-rv").unwrap_err();
        assert!(matches!(err, SnapshotError::NotFound(_)));
    }
}

/// Prompt 08 (AWE-012/TW-005) tests: cross-snapshot isolation, missing-file
/// semantics, the full §21 corruption matrix, concurrency, fault
/// boundaries, and the recovery-read contract.
#[cfg(test)]
mod snapshot_recovery_tests {
    use super::*;

    fn store_at(dir: &Path) -> SnapshotStore {
        SnapshotStore::new(dir.to_path_buf())
    }

    fn provenance_for(
        _store: &SnapshotStore,
        edit_id: &str,
        manifest: &FileSnapshot,
        paths: Vec<String>,
    ) -> ProvenanceRecord {
        ProvenanceRecord {
            edit_id: edit_id.into(),
            snapshot_id: manifest.id.clone(),
            operations: vec!["Replace".into()],
            paths,
            agent_id: None,
            session_id: None,
            workspace_id: None,
            hash_edges: vec![],
            outcome: ProvenanceOutcome::Committed,
            created_at: now_rfc3339(),
        }
    }

    /// §22 regression: two snapshots in the same store used to share
    /// `entry-000000.bin` — the second create silently destroyed the
    /// first snapshot's recovery material. Both must now load with their
    /// exact original bytes.
    #[test]
    fn two_snapshots_never_share_or_clobber_content() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let first = store
            .create(
                "edit-1",
                &[("a.txt".into(), b"first bytes\n".to_vec())],
                None,
            )
            .unwrap();
        let second = store
            .create(
                "edit-2",
                &[(
                    "a.txt".into(),
                    b"completely different second bytes\n".to_vec(),
                )],
                None,
            )
            .unwrap();
        assert_ne!(first.id, second.id);

        // Both load and both return THEIR OWN exact bytes.
        let loaded_first = store.load(&first.id).unwrap();
        let loaded_second = store.load(&second.id).unwrap();
        let first_bytes = store
            .entry_bytes(&loaded_first, &loaded_first.entries[0].entry_id)
            .unwrap();
        let second_bytes = store
            .entry_bytes(&loaded_second, &loaded_second.entries[0].entry_id)
            .unwrap();
        assert_eq!(first_bytes, b"first bytes\n");
        assert_eq!(second_bytes, b"completely different second bytes\n");
    }

    /// §22 concurrency: simultaneous snapshot creation must isolate
    /// content — no cross-snapshot mix, no torn manifest, no hash
    /// mismatch after the fact.
    #[test]
    fn concurrent_snapshot_creation_isolates_every_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let store = std::sync::Arc::new(store);
        let mut handles = Vec::new();
        for worker in 0..4 {
            let store = std::sync::Arc::clone(&store);
            handles.push(std::thread::spawn(move || {
                for round in 0..4 {
                    let marker = format!("worker-{worker}-round-{round}\n");
                    let manifest = store
                        .create(
                            &format!("edit-{worker}-{round}"),
                            &[("f.txt".into(), marker.as_bytes().to_vec())],
                            None,
                        )
                        .unwrap();
                    let loaded = store.load(&manifest.id).unwrap();
                    let bytes = store
                        .entry_bytes(&loaded, &loaded.entries[0].entry_id)
                        .unwrap();
                    assert_eq!(bytes, marker.as_bytes());
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        // Every published snapshot still verifies after the storm.
        for id in store.list().unwrap() {
            store.load(&id).expect("every snapshot must verify");
        }
        assert_eq!(store.list().unwrap().len(), 16);
    }

    /// §6.2: a pre-edit-missing file is recorded as missing — never as an
    /// empty file — and stays distinct from an existing empty file.
    #[test]
    fn missing_file_is_distinct_from_existing_empty_file() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let manifest = store
            .create_snapshot(
                "edit-mixed",
                &[
                    SnapshotFile::Existing {
                        path: "empty.txt".into(),
                        bytes: Vec::new(),
                    },
                    SnapshotFile::Missing {
                        path: "created.txt".into(),
                    },
                ],
                None,
            )
            .unwrap();
        assert_eq!(manifest.entries.len(), 2);
        // Entries are path-sorted; look them up by path, not index.
        let empty = manifest
            .entries
            .iter()
            .find(|e| e.path == "empty.txt")
            .unwrap();
        let missing = manifest
            .entries
            .iter()
            .find(|e| e.path == "created.txt")
            .unwrap();
        assert!(empty.existed_before);
        assert_eq!(empty.size_before, 0);
        assert!(!missing.existed_before);
        assert_eq!(missing.size_before, 0);
        // Hash distinguishes: an existing empty file hashes SHA-256("")
        // while missing records no hash at all.
        assert_eq!(empty.hash_before, sha256_hex(b""));
        assert_eq!(missing.hash_before, "");

        let loaded = store.load(&manifest.id).unwrap();
        // Existing empty → Some(empty bytes); missing → error on bytes.
        let empty_bytes = store.entry_bytes(&loaded, &empty.entry_id).unwrap();
        assert!(empty_bytes.is_empty());
        assert!(matches!(
            store.entry_bytes(&loaded, &missing.entry_id),
            Err(SnapshotError::MissingContent(_))
        ));

        // Recovery view exposes the distinction (§19).
        store
            .record_provenance(&provenance_for(
                &store,
                "edit-mixed",
                &manifest,
                vec!["empty.txt".into(), "created.txt".into()],
            ))
            .unwrap();
        let view = store.recovery_view("edit-mixed").unwrap();
        // Path-sorted: created.txt precedes empty.txt.
        assert_eq!(view[0], ("created.txt".to_string(), None));
        assert_eq!(view[1], ("empty.txt".to_string(), Some(Vec::new())));
    }

    /// §21 corruption matrix — every invalid state fails closed with the
    /// structured error it deserves.
    #[test]
    fn corruption_matrix_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let manifest = store
            .create(
                "edit-matrix",
                &[("f.txt".into(), b"matrix content\n".to_vec())],
                None,
            )
            .unwrap();
        let manifest_path = temp
            .path()
            .join(".agent/snapshots")
            .join(format!("{}.json", manifest.id.as_str()));
        let blob_path = temp
            .path()
            .join(".agent/snapshot-contents")
            .join(manifest.id.as_str())
            .join("entry-000000.bin");

        // Truncated blob.
        std::fs::write(&blob_path, b"short").unwrap();
        assert!(matches!(
            store.load(&manifest.id),
            Err(SnapshotError::Corruption(_))
        ));
        // Restore; malformed manifest.
        std::fs::write(&blob_path, b"matrix content\n").unwrap();
        std::fs::write(&manifest_path, b"{not json").unwrap();
        assert!(matches!(
            store.load(&manifest.id),
            Err(SnapshotError::Corruption(_))
        ));
        // Manifest with an unsupported schema version.
        let mut tampered = manifest.clone();
        tampered.schema_version = 99;
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&tampered).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            store.load(&manifest.id),
            Err(SnapshotError::UnsupportedSchema { got: 99, .. })
        ));
        // Manifest with a foreign id / edit binding: the manifest itself
        // stays internally consistent, but the recovery view — which
        // binds snapshot to edit — must fail closed on substitution.
        let mut tampered = manifest.clone();
        tampered.edit_id = "edit-someone-else".into();
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&tampered).unwrap(),
        )
        .unwrap();
        store
            .record_provenance(&provenance_for(
                &store,
                "edit-matrix",
                &manifest,
                vec!["f.txt".into()],
            ))
            .unwrap();
        let err = store.recovery_view("edit-matrix").unwrap_err();
        assert!(
            matches!(err, SnapshotError::Corruption(_)),
            "cross-edit substitution must fail closed, got {err:?}"
        );
        // Restore a valid manifest; missing blob.
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::remove_file(&blob_path).unwrap();
        assert!(matches!(
            store.load(&manifest.id),
            Err(SnapshotError::Corruption(_))
        ));
    }

    /// §16: recovery must use the snapshot bound to exactly this edit — a
    /// snapshot from another edit cannot be substituted.
    #[test]
    fn recovery_view_requires_snapshot_bound_to_the_requested_edit() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let manifest = store
            .create("edit-other", &[("f.txt".into(), b"x\n".to_vec())], None)
            .unwrap();
        // Provenance for THIS edit pointing at another edit's snapshot.
        let forged = ProvenanceRecord {
            edit_id: "edit-this".into(),
            snapshot_id: manifest.id.clone(),
            operations: vec![],
            paths: vec![],
            agent_id: None,
            session_id: None,
            workspace_id: None,
            hash_edges: vec![],
            outcome: ProvenanceOutcome::Committed,
            created_at: now_rfc3339(),
        };
        store.record_provenance(&forged).unwrap();
        let err = store.recovery_view("edit-this").unwrap_err();
        assert!(
            matches!(err, SnapshotError::Corruption(_)),
            "cross-edit substitution must fail closed, got {err:?}"
        );
    }

    /// §10/§18: entry ids are validated and must belong to the manifest —
    /// no arbitrary-path lookup, no unreferenced bytes.
    #[test]
    fn entry_bytes_rejects_unknown_or_unsafe_entry_ids() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let manifest = store
            .create(
                "edit-strict",
                &[("f.txt".into(), b"strict\n".to_vec())],
                None,
            )
            .unwrap();
        let loaded = store.load(&manifest.id).unwrap();
        // Unknown entry id (belongs to no snapshot).
        assert!(matches!(
            store.entry_bytes(&loaded, &SnapshotEntryId("entry-999999".into())),
            Err(SnapshotError::NotFound(_))
        ));
        // Unsafe entry id never reaches the filesystem.
        assert!(matches!(
            store.entry_bytes(&loaded, &SnapshotEntryId("../escape".into())),
            Err(SnapshotError::InvalidId(_))
        ));
    }

    /// §6.3: unsafe resource paths are rejected before persistence.
    #[test]
    fn unsafe_paths_are_rejected_before_persistence() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        for bad in ["../escape.txt", "/abs.txt", "a\u{0000}.txt", ""] {
            let err = store
                .create_snapshot(
                    "edit-paths",
                    &[SnapshotFile::Existing {
                        path: bad.into(),
                        bytes: b"x".to_vec(),
                    }],
                    None,
                )
                .unwrap_err();
            assert!(
                matches!(err, SnapshotError::InvalidId(_)),
                "path {bad:?} must be rejected"
            );
        }
        // Nothing was persisted.
        assert!(store.list().unwrap().is_empty());
    }

    /// §21 restart: mixed existing/missing entries survive a store
    /// recreation with identical recovery material.
    #[test]
    fn restart_preserves_mixed_recovery_material() {
        let temp = tempfile::tempdir().unwrap();
        let one = store_at(temp.path());
        let manifest = one
            .create_snapshot(
                "edit-restart",
                &[
                    SnapshotFile::Existing {
                        path: "exists.txt".into(),
                        bytes: "exact bytes 🦀\r\n".as_bytes().to_vec(),
                    },
                    SnapshotFile::Missing {
                        path: "made-up.txt".into(),
                    },
                ],
                None,
            )
            .unwrap();
        one.record_provenance(&provenance_for(
            &one,
            "edit-restart",
            &manifest,
            vec!["exists.txt".into(), "made-up.txt".into()],
        ))
        .unwrap();
        drop(one);

        let rebuilt = store_at(temp.path());
        let view = rebuilt.recovery_view("edit-restart").unwrap();
        assert_eq!(
            view,
            vec![
                (
                    "exists.txt".to_string(),
                    Some("exact bytes 🦀\r\n".as_bytes().to_vec())
                ),
                ("made-up.txt".to_string(), None),
            ]
        );
    }

    /// §13: binary (non-UTF-8) content is snapshotted byte-exactly with a
    /// diagnostic line count of `None` — no lossy conversion participates.
    #[test]
    fn binary_content_snapshots_exact_bytes_without_lossy_decoding() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let bytes: Vec<u8> = vec![0xff, 0xfe, 0x00, 0x01, 0x80];
        let manifest = store
            .create_snapshot(
                "edit-binary",
                &[SnapshotFile::Existing {
                    path: "blob.bin".into(),
                    bytes: bytes.clone(),
                }],
                None,
            )
            .unwrap();
        assert_eq!(manifest.entries[0].line_count_before, None);
        let loaded = store.load(&manifest.id).unwrap();
        assert_eq!(
            store
                .entry_bytes(&loaded, &loaded.entries[0].entry_id)
                .unwrap(),
            bytes
        );
    }

    /// §30 fault boundary: when content persistence fails, no manifest is
    /// published and the store exposes nothing recoverable-looking.
    #[cfg(unix)]
    #[test]
    fn content_write_failure_publishes_no_snapshot() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        // Prepare the directories writable first so ensure_dirs passes,
        // then lock the contents directory for the write phase.
        store.ensure_dirs().unwrap();
        let contents = temp.path().join(".agent/snapshot-contents");
        let mut perms = std::fs::metadata(&contents).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&contents, perms).unwrap();

        let err = store
            .create("edit-fault", &[("f.txt".into(), b"data\n".to_vec())], None)
            .unwrap_err();
        assert!(matches!(err, SnapshotError::Storage(_)));

        let mut perms = std::fs::metadata(&contents).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&contents, perms).unwrap();
        // No manifest was published: the failure left no visible snapshot.
        assert!(store.list().unwrap().is_empty());
    }

    /// §25: provenance persistence failure is separate from snapshot
    /// validity — the committed recovery material must remain usable.
    #[cfg(unix)]
    #[test]
    fn provenance_failure_does_not_invalidate_the_snapshot() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let manifest = store
            .create(
                "edit-prov-fail",
                &[("f.txt".into(), b"kept\n".to_vec())],
                None,
            )
            .unwrap();
        let provenance_dir = temp.path().join(".agent/provenance");
        let mut perms = std::fs::metadata(&provenance_dir).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&provenance_dir, perms).unwrap();

        let record = provenance_for(&store, "edit-prov-fail", &manifest, vec!["f.txt".into()]);
        assert!(store.record_provenance(&record).is_err());

        let mut perms = std::fs::metadata(&provenance_dir).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&provenance_dir, perms).unwrap();
        // The snapshot remains valid and loadable — separate concerns.
        let loaded = store.load(&manifest.id).unwrap();
        assert_eq!(loaded.entries.len(), 1);
    }

    /// §27: only supplied resources are captured — nothing else under the
    /// workspace enters the snapshot store.
    #[test]
    fn only_supplied_resources_are_captured() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        std::fs::write(temp.path().join("unrelated.txt"), b"secret").unwrap();
        let manifest = store
            .create(
                "edit-minimal",
                &[("only-this.txt".into(), b"content\n".to_vec())],
                None,
            )
            .unwrap();
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].path, "only-this.txt");
        let serialized = serde_json::to_string(&manifest).unwrap();
        assert!(
            !serialized.contains("secret"),
            "unrelated workspace content must not leak into the manifest"
        );
    }

    /// §12: file-count limit rejects before persistence.
    #[test]
    fn file_count_limit_is_enforced() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_at(temp.path());
        let files: Vec<(String, Vec<u8>)> = (0..MAX_SNAPSHOT_FILES + 1)
            .map(|i| (format!("f{i}.txt"), b"x".to_vec()))
            .collect();
        let err = store.create("edit-many", &files, None).unwrap_err();
        assert!(matches!(err, SnapshotError::LimitExceeded(_)));
        assert!(store.list().unwrap().is_empty());
    }
}
