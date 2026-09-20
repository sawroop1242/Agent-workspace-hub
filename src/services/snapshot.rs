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
/// contract as [`SnapshotId`].
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
    /// Underlying filesystem error (containment, locking, I/O).
    #[error("snapshot storage error: {0}")]
    Storage(String),
}

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

    fn content_path(&self, entry: &SnapshotEntryId) -> PathBuf {
        self.contents_dir().join(format!("{}.bin", entry.0))
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
            fs::create_dir_all(&dir)
                .map_err(|e| SnapshotError::Storage(format!("{}, {}", dir.display(), e)))?;
        }
        Ok(())
    }

    /// Captures a snapshot of `files: Vec<(path, exact original bytes)>` for
    /// the transaction identified by `edit_id`, returning the committed
    /// manifest. This is written *before* any mutation proceeds: if any
    /// content can't be written atomically and verified, the operation fails
    /// and nothing is left visible for recovery to misread.
    pub fn create(
        &self,
        edit_id: &str,
        files: &[(String, Vec<u8>)],
        reason: Option<String>,
    ) -> Result<FileSnapshot, SnapshotError> {
        validate_id_component(edit_id)?;
        if files.len() > MAX_SNAPSHOT_FILES {
            return Err(SnapshotError::LimitExceeded(format!(
                "snapshot covers {} files (max {MAX_SNAPSHOT_FILES})",
                files.len()
            )));
        }
        // Integrity + size limit checks first. No single file may exceed
        // the canonical file limit; the total is additionally capped.
        let total: u64 = files.iter().map(|(_, b)| b.len() as u64).sum();
        if let Some((path, _)) = files
            .iter()
            .find(|(_, b)| b.len() as u64 > MAX_SNAPSHOT_CONTENT_BYTES)
        {
            return Err(SnapshotError::LimitExceeded(format!(
                "snapshot content for '{path}' exceeds the {MAX_SNAPSHOT_CONTENT_BYTES}-byte limit"
            )));
        }
        if total > MAX_SNAPSHOT_CONTENT_BYTES * MAX_SNAPSHOT_FILES as u64 {
            return Err(SnapshotError::LimitExceeded(format!(
                "snapshot total size {total} exceeds cap"
            )));
        }

        self.ensure_dirs()?;
        let sorted: Vec<(String, Vec<u8>)> = {
            let mut v: Vec<(String, Vec<u8>)> = files.to_vec();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };

        let id = SnapshotId::new();
        id.validate()?;

        // Write content blobs first (uncommitted state must not look
        // recovery-valid). Then the manifest commits under a lock so a reader
        // sees either the complete artifact or nothing.
        let manifest_lock = self.manifest_path(&id);
        let _lock = StoreLock::acquire(&manifest_lock)
            .map_err(|e| SnapshotError::Storage(format!("snapshot lock: {e}")))?;

        let mut entries = Vec::with_capacity(sorted.len());
        for (idx, (path, bytes)) in sorted.iter().enumerate() {
            let entry_id = SnapshotEntryId::new(idx);
            entry_id.validate()?;
            let raw_len = bytes.len() as u64;
            let hash = sha256_hex(bytes);
            write_blob(&self.content_path(&entry_id), bytes)
                .map_err(|e| SnapshotError::Storage(format!("write content blob: {e}")))?;
            entries.push(FileSnapshotEntry {
                entry_id,
                path: path.clone(),
                existed_before: true,
                hash_before: hash,
                size_before: raw_len,
                line_count_before: Some(
                    crate::services::edit::FileState::from_content(
                        path,
                        &String::from_utf8_lossy(bytes),
                    )
                    .line_count,
                ),
                content_len: raw_len,
                captured_at: now_rfc3339(),
            });
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
            let blob = self.content_path(&entry.entry_id);
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
    /// against the manifest: no silent truncation, no partial bytes.
    pub fn entry_bytes(
        &self,
        manifest: &FileSnapshot,
        entry_id: &SnapshotEntryId,
    ) -> Result<Vec<u8>, SnapshotError> {
        let path = self.content_path(entry_id);
        let data = if path.exists() {
            fs::read(&path).map_err(|_| SnapshotError::Corruption("entry unreadable".into()))?
        } else {
            return Err(SnapshotError::Corruption("entry missing".into()));
        };
        // Verify against the manifest entry when present.
        if let Some(entry) = manifest.entries.iter().find(|e| &e.entry_id == entry_id) {
            if data.len() as u64 != entry.content_len {
                return Err(SnapshotError::Corruption("entry size mismatch".into()));
            }
            if sha256_hex(&data) != entry.hash_before {
                return Err(SnapshotError::Corruption("entry hash mismatch".into()));
            }
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

    /// Produces a view of the files addressed by `edit_id` ready for a task
    /// that needs exact pre-edit bytes for recovery. Fails closed on any
    /// sign of corruption or missing data.
    pub fn recovery_view(&self, edit_id: &str) -> Result<Vec<(String, Vec<u8>)>, SnapshotError> {
        validate_id_component(edit_id)?;
        let prov = self.provenance(edit_id)?;
        let manifest = self.load(&prov.snapshot_id)?;
        let mut pairs = Vec::with_capacity(manifest.entries.len());
        for entry in &manifest.entries {
            pairs.push((
                entry.path.clone(),
                self.entry_bytes(&manifest, &entry.entry_id)?,
            ));
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
        // Tamper with the stored blob after commit.
        let blob = temp
            .path()
            .join(".agent/snapshot-contents/entry-000000.bin");
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
        let manifest = store
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
