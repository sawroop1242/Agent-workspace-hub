//! Canonical AWH memory store (ARCH-001).
//!
//! One authoritative representation of agent memory per project scope,
//! persisted at `.agent/memory.json` as a JSON object holding one JSON
//! array of [`MemoryEntry`] records. Every interface plane - MCP tools,
//! Control API, TUI, and the context engine - reads and writes this
//! single store; no plane may keep a second memory file.
//!
//! ## Legacy data (`.agent/memory.jsonl`)
//!
//! Earlier versions of the Control API and TUI appended records to an
//! append-only JSONL file (`.agent/memory.jsonl`) whose records carried
//! only a timestamp and content, while MCP tools persisted rich records
//! to `.agent/memory.json`. Both files coexisting meant the planes
//! observed *different* memory state, so convergence migrates the legacy
//! JSONL file into the canonical store on first open:
//!
//! * deterministic - each legacy line becomes one record whose id is
//!   `legacy-jsonl-<line-index>` (blank lines do not consume an index),
//!   so re-running the migration after a crash regenerates identical ids;
//! * idempotent - records whose id already exists in the canonical store
//!   are skipped, so an interrupted run cannot duplicate data;
//! * atomic - the canonical store is published via temp-file + rename
//!   *before* the legacy file is renamed away, and the legacy file is
//!   kept (as `.agent/memory.jsonl.migrated`) so no data is ever
//!   silently discarded;
//! * fail-closed - a malformed JSONL line or an unreadable canonical
//!   store is an error, never an implicit empty store.

use crate::mcp::store_lock::StoreLock;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum number of entries a single project memory store may hold.
pub const MAX_MEMORY_ENTRIES: usize = 10_000;
/// Maximum size of a single memory entry's content, in bytes.
pub const MAX_MEMORY_CONTENT_BYTES: usize = 1024 * 1024;
/// Maximum length of a memory entry id.
pub const MAX_MEMORY_ID_LEN: usize = 256;
/// Maximum number of tags per entry.
pub const MAX_MEMORY_TAGS: usize = 64;
/// Maximum length of a single tag.
pub const MAX_MEMORY_TAG_LEN: usize = 128;

/// A single persisted memory record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique entry id.
    pub id: String,
    /// Visibility scope of the entry.
    pub scope: MemoryScope,
    /// The memory content.
    pub content: String,
    /// Categorization tags.
    pub tags: Vec<String>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 last-update timestamp.
    pub updated_at: String,
}

/// The visibility scope of a memory entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryScope {
    /// Visible only within the current session.
    Session,
    /// Visible to the whole project.
    Project,
    /// Visible globally across projects.
    Global,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MemoryFile {
    entries: Vec<MemoryEntry>,
}

/// Canonical project-scoped memory store persisted to `.agent/memory.json`.
///
/// Opening a store migrates a legacy `.agent/memory.jsonl` file into the
/// canonical representation exactly once (see the module docs); after the
/// migration the legacy file remains on disk renamed to
/// `.agent/memory.jsonl.migrated`.
pub struct MemoryStore {
    path: PathBuf,
    legacy_path: PathBuf,
}

impl MemoryStore {
    /// Creates a memory store backed by `.agent/memory.json` under the
    /// project root, migrating any legacy `.agent/memory.jsonl` data.
    pub fn new(project_root: impl Into<PathBuf>) -> Result<Self> {
        Self::for_project(project_root.into().as_path())
    }

    /// Creates a memory store backed by `.agent/memory.json` under the
    /// project root, migrating any legacy `.agent/memory.jsonl` data.
    pub fn for_project(project_root: &Path) -> Result<Self> {
        let store = Self {
            path: project_root.join(".agent").join("memory.json"),
            legacy_path: project_root.join(".agent").join("memory.jsonl"),
        };
        // The `.agent` parent must exist before any StoreLock::acquire or
        // temp-file write inside append/save; creating it here keeps every
        // caller (MCP, API, TUI) from having to remember this ordering.
        if let Some(parent) = store.path.parent() {
            fs::create_dir_all(parent)
                .context("failed to create the .agent directory for the memory store")?;
        }
        store.migrate_legacy_jsonl()?;
        Ok(store)
    }

    /// Returns the backing memory file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> Result<MemoryFile> {
        if !self.path.exists() {
            return Ok(MemoryFile::default());
        }
        Ok(serde_json::from_str(
            &fs::read_to_string(&self.path).context("read memory store")?,
        )?)
    }

    /// Writes `file` atomically: the new content lands in a temp file in
    /// the same directory, then replaces the target via rename, so a reader
    /// (including a sibling agent process) never observes a truncated or
    /// partially written file.
    fn save(&self, file: &MemoryFile) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("memory store path has no parent directory")?;
        fs::create_dir_all(parent)?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).context("failed to create temp file")?;
        std::io::Write::write_all(&mut temp, serde_json::to_string_pretty(file)?.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("failed to atomically write memory store")?;
        Ok(())
    }

    /// Inserts or overwrites a memory entry keyed by `id`, updating timestamps.
    ///
    /// Fails closed when the entry would exceed the store's enforced size
    /// limits (entry count, content bytes, id length, tag count/length), so a
    /// misbehaving client cannot grow the on-disk store without bound.
    ///
    /// Holds a [`StoreLock`] across the whole load-modify-save cycle so a
    /// second process calling `store`/`delete` on this same project at the
    /// same time serializes behind it instead of racing.
    pub fn store(
        &self,
        id: String,
        content: String,
        scope: MemoryScope,
        tags: Vec<String>,
    ) -> Result<MemoryEntry> {
        validate_memory_input(&id, &content, &tags)?;
        let _lock = StoreLock::acquire(&self.path)?;
        let now = chrono::Utc::now().to_rfc3339();
        let mut db = self.load()?;
        if db.entries.iter().any(|e| e.id == id) {
            if let Some(entry) = db.entries.iter_mut().find(|e| e.id == id) {
                entry.content = content;
                entry.tags = tags;
                entry.scope = scope;
                entry.updated_at = now.clone();
                let result = entry.clone();
                self.save(&db)?;
                return Ok(result);
            }
        }
        if db.entries.len() >= MAX_MEMORY_ENTRIES {
            bail!("memory store is full (max {MAX_MEMORY_ENTRIES} entries)");
        }
        let entry = MemoryEntry {
            id,
            scope,
            content,
            tags,
            created_at: now.clone(),
            updated_at: now,
        };
        db.entries.push(entry.clone());
        self.save(&db)?;
        Ok(entry)
    }

    /// Updates an existing entry's content, scope, and tags, failing if no
    /// entry with `id` exists yet.
    ///
    /// This performs the existence check and the write under the same
    /// [`StoreLock`] hold, unlike a caller doing `get` then `store`: that
    /// two-step version has a check-then-act gap where a second process
    /// could delete the entry in between, turning what looked like an
    /// "update" into a silent re-creation. Locking across both steps here
    /// closes it.
    pub fn update_existing(
        &self,
        id: String,
        content: String,
        scope: MemoryScope,
        tags: Vec<String>,
    ) -> Result<MemoryEntry> {
        validate_memory_input(&id, &content, &tags)?;
        let _lock = StoreLock::acquire(&self.path)?;
        let now = chrono::Utc::now().to_rfc3339();
        let mut db = self.load()?;
        let entry = db
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("memory entry not found: {id}"))?;
        entry.content = content;
        entry.tags = tags;
        entry.scope = scope;
        entry.updated_at = now;
        let result = entry.clone();
        self.save(&db)?;
        Ok(result)
    }

    /// Appends a new entry with a generated id, the way the TUI and Control
    /// API planes record memory: content in, record out.
    pub fn append(&self, content: &str) -> Result<MemoryEntry> {
        if content.trim().is_empty() {
            bail!("memory content must not be empty");
        }
        let id = self.generate_id();
        self.store(id, content.to_owned(), MemoryScope::Project, vec![])
    }

    /// Appends a new entry with a generated id at an explicit scope with
    /// tags — the CLI's `awh memory add` shape. Id minting stays inside
    /// the store so no adapter can invent ids or bypass validation.
    pub fn append_scoped(
        &self,
        content: &str,
        scope: MemoryScope,
        tags: Vec<String>,
    ) -> Result<MemoryEntry> {
        if content.trim().is_empty() {
            bail!("memory content must not be empty");
        }
        let id = self.generate_id();
        self.store(id, content.to_owned(), scope, tags)
    }

    /// Partially updates an existing entry: only the provided fields
    /// change. Fails closed when the id is unknown and when no field was
    /// provided at all. The existence check and the write share one
    /// [`StoreLock`] hold, so a concurrent delete from another process
    /// cannot race a separate read-then-write into resurrecting the
    /// entry (the same guarantee `update_existing` gives).
    pub fn update_partial(
        &self,
        id: &str,
        content: Option<String>,
        scope: Option<MemoryScope>,
        tags: Option<Vec<String>>,
    ) -> Result<MemoryEntry> {
        if content.is_none() && scope.is_none() && tags.is_none() {
            bail!("memory update requires at least one of content, scope, or tags");
        }
        // Id bounds plus the bounds of whichever fields are present.
        validate_memory_input(id, content.as_deref().unwrap_or(""), &[])?;
        if let Some(tags) = &tags {
            validate_memory_input(id, "", tags)?;
        }
        let _lock = StoreLock::acquire(&self.path)?;
        let mut db = self.load()?;
        let now = chrono::Utc::now().to_rfc3339();
        let entry = db
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("memory entry not found: {id}"))?;
        if let Some(content) = content {
            entry.content = content;
        }
        if let Some(scope) = scope {
            entry.scope = scope;
        }
        if let Some(tags) = tags {
            entry.tags = tags;
        }
        entry.updated_at = now;
        let result = entry.clone();
        self.save(&db)?;
        Ok(result)
    }

    /// Lists every entry across all scopes, oldest first.
    pub fn list_all(&self) -> Result<Vec<MemoryEntry>> {
        Ok(self.load()?.entries)
    }

    /// Reads all memory entries; a missing store is empty, a corrupt store
    /// is an error (fail closed, never a silent empty list).
    pub fn read_all(&self) -> Result<Vec<MemoryEntry>> {
        self.list_all()
    }

    /// Searches entries by content or tag, optionally constrained to a scope.
    pub fn search(&self, query: &str, scope: Option<MemoryScope>) -> Result<Vec<MemoryEntry>> {
        let q = query.to_ascii_lowercase();
        Ok(self
            .load()?
            .entries
            .into_iter()
            .filter(|e| {
                scope.as_ref().is_none_or(|s| &e.scope == s)
                    && (e.content.to_ascii_lowercase().contains(&q)
                        || e.tags.iter().any(|t| t.to_ascii_lowercase().contains(&q)))
            })
            .collect())
    }

    /// Returns the memory entry with the given `id`, if present.
    pub fn get(&self, id: &str) -> Result<Option<MemoryEntry>> {
        Ok(self.load()?.entries.into_iter().find(|e| e.id == id))
    }

    /// Deletes the memory entry with the given `id`, returning whether it existed.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let _lock = StoreLock::acquire(&self.path)?;
        let mut db = self.load()?;
        let before = db.entries.len();
        db.entries.retain(|e| e.id != id);
        if before == db.entries.len() {
            return Ok(false);
        }
        self.save(&db)?;
        Ok(true)
    }

    /// Id for [`Self::append`]: time-ordered and collision-resistant across
    /// both threads AND processes. The nanosecond timestamp plus an
    /// in-process atomic counter are not sufficient on their own — two
    /// sibling processes can read the same coarse clock tick (macOS/Windows
    /// clock granularity) and each mint `<nanos>-0000`, which the store's
    /// upsert would then silently collapse into one record. Mixing in the
    /// process id makes concurrent sibling processes mint distinct ids
    /// (live processes always have distinct pids), while the atomic counter
    /// separates threads within one process.
    fn generate_id(&self) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        format!("mem-{nanos:016x}-{pid:08x}-{seq:04x}")
    }

    /// Migrates a legacy `.agent/memory.jsonl` file into the canonical
    /// `.agent/memory.json` store. A no-op when no legacy file exists.
    ///
    /// See the module docs for the determinism/idempotency/atomicity
    /// contract. If the canonical store already contains every legacy
    /// record (a crash-recovery re-run), nothing is republished but the
    /// legacy file is still archived, so the migration is idempotent.
    fn migrate_legacy_jsonl(&self) -> Result<()> {
        if !self.legacy_path.exists() {
            return Ok(());
        }
        let _lock = StoreLock::acquire(&self.path)?;

        let mut db = self.load()?;
        let raw = fs::read_to_string(&self.legacy_path)
            .context("read legacy memory.jsonl for migration")?;
        let mut added = 0usize;
        for (index, line) in raw.lines().filter(|l| !l.trim().is_empty()).enumerate() {
            let id = format!("legacy-jsonl-{index}");
            if db.entries.iter().any(|e| e.id == id) {
                continue; // idempotent re-run after a crash between publish and rename
            }
            let legacy: LegacyMemoryEntry = serde_json::from_str(line)
                .with_context(|| format!("legacy memory.jsonl line {} is malformed", index + 1))?;
            db.entries.push(MemoryEntry {
                id,
                scope: MemoryScope::Project,
                content: legacy.content,
                tags: Vec::new(),
                created_at: legacy.timestamp.clone(),
                updated_at: legacy.timestamp,
            });
            added += 1;
        }
        if added > 0 {
            if db.entries.len() > MAX_MEMORY_ENTRIES {
                bail!("legacy memory migration would exceed the {MAX_MEMORY_ENTRIES} entry limit");
            }
            self.save(&db)?;
        }
        let migrated = self.legacy_path.with_extension("jsonl.migrated");
        fs::rename(&self.legacy_path, &migrated).with_context(|| {
            format!("archive legacy memory file {}", self.legacy_path.display())
        })?;
        Ok(())
    }
}

/// Legacy `.agent/memory.jsonl` record (timestamp + content only), used by
/// the migration reader. Not the authoritative memory representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyMemoryEntry {
    /// RFC 3339 timestamp of the record.
    pub timestamp: String,
    /// The memory content.
    pub content: String,
}

/// Rejects memory inputs that would violate the store's size limits.
fn validate_memory_input(id: &str, content: &str, tags: &[String]) -> Result<()> {
    if id.trim().is_empty() {
        bail!("memory id must not be empty");
    }
    if id.len() > MAX_MEMORY_ID_LEN {
        bail!("memory id exceeds {MAX_MEMORY_ID_LEN} bytes");
    }
    if content.len() > MAX_MEMORY_CONTENT_BYTES {
        bail!("memory content exceeds {MAX_MEMORY_CONTENT_BYTES} bytes");
    }
    if tags.len() > MAX_MEMORY_TAGS {
        bail!("memory entry exceeds {MAX_MEMORY_TAGS} tags");
    }
    if let Some(tag) = tags.iter().find(|t| t.len() > MAX_MEMORY_TAG_LEN) {
        bail!("memory tag exceeds {MAX_MEMORY_TAG_LEN} bytes: {tag:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (MemoryStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::for_project(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn for_project_points_at_agent_memory_json() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::for_project(temp.path()).unwrap();
        assert_eq!(store.path(), temp.path().join(".agent/memory.json"));
    }

    #[test]
    fn read_all_on_missing_file_is_empty_not_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::for_project(temp.path()).unwrap();
        assert!(store.read_all().unwrap().is_empty());
    }

    #[test]
    fn append_creates_parent_dirs_and_round_trips() {
        let (store, _dir) = temp_store();
        let first = store.append("first").unwrap();
        let second = store.append("second").unwrap();
        assert!(store.path().is_file());
        let entries = store.read_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "first");
        assert_eq!(entries[1].content, "second");
        assert_eq!(entries[0].id, first.id);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn append_generates_unique_ids_under_concurrency() {
        // Leaked dir: the store must outlive the test-scoped TempDir since
        // the worker threads hold references past the main body.
        let dir: &'static tempfile::TempDir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let store: &'static MemoryStore =
            Box::leak(Box::new(MemoryStore::for_project(dir.path()).unwrap()));
        let handles: Vec<_> = (0..8)
            .map(|i| {
                std::thread::spawn(move || {
                    for n in 0..10 {
                        store.append(&format!("thread-{i}-note-{n}")).unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let entries = store.read_all().unwrap();
        assert_eq!(entries.len(), 80);
        let ids: std::collections::HashSet<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), entries.len(), "append ids must be unique");
    }

    #[test]
    fn generated_ids_embed_the_process_id_for_cross_process_uniqueness() {
        // Regression: two sibling processes on a coarse clock (macOS/Windows)
        // can read the same nanosecond tick; without a process-unique
        // component they mint the same id and the store's upsert silently
        // drops one record. The pid must be part of every generated id.
        let (store, _dir) = temp_store();
        let pid = format!("{:08x}", std::process::id());
        let mut ids = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = store.generate_id();
            assert!(id.starts_with("mem-"), "{id}");
            assert!(
                id.contains(&format!("-{pid}-")),
                "generated id must embed the pid: {id}"
            );
            assert!(ids.insert(id), "generated ids must be unique");
        }
    }

    #[test]
    fn store_rejects_empty_id() {
        let (store, _dir) = temp_store();
        let result = store.store(
            String::new(),
            "content".into(),
            MemoryScope::Project,
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn store_rejects_oversized_content() {
        let (store, _dir) = temp_store();
        let oversized = "a".repeat(MAX_MEMORY_CONTENT_BYTES + 1);
        let result = store.store("id".into(), oversized, MemoryScope::Project, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn store_rejects_oversized_id() {
        let (store, _dir) = temp_store();
        let oversized = "i".repeat(MAX_MEMORY_ID_LEN + 1);
        let result = store.store(oversized, "content".into(), MemoryScope::Project, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn store_rejects_too_many_tags_and_oversized_tags() {
        let (store, _dir) = temp_store();
        let many: Vec<String> = (0..MAX_MEMORY_TAGS + 1).map(|i| i.to_string()).collect();
        assert!(store
            .store("id".into(), "content".into(), MemoryScope::Project, many)
            .is_err());

        let long_tag = vec!["t".repeat(MAX_MEMORY_TAG_LEN + 1)];
        assert!(store
            .store(
                "id2".into(),
                "content".into(),
                MemoryScope::Project,
                long_tag
            )
            .is_err());
    }

    #[test]
    fn store_enforces_entry_count_limit() {
        let (store, _dir) = temp_store();
        let entries: Vec<MemoryEntry> = (0..MAX_MEMORY_ENTRIES as u64)
            .map(|i| MemoryEntry {
                id: format!("entry-{i}"),
                scope: MemoryScope::Project,
                content: "c".into(),
                tags: vec![],
                created_at: String::new(),
                updated_at: String::new(),
            })
            .collect();
        fs::write(
            store.path.clone(),
            serde_json::to_string(&MemoryFile { entries }).unwrap(),
        )
        .unwrap();

        assert!(store
            .store(
                "overflow".into(),
                "content".into(),
                MemoryScope::Project,
                vec![]
            )
            .is_err());

        assert!(store
            .store(
                "entry-0".into(),
                "updated".into(),
                MemoryScope::Project,
                vec![]
            )
            .is_ok());
    }

    #[test]
    fn corrupted_store_fails_closed() {
        let (store, _dir) = temp_store();
        fs::write(&store.path, "not json {").unwrap();
        assert!(store
            .store("id".into(), "content".into(), MemoryScope::Project, vec![])
            .is_err());
        assert!(store.search("query", None).is_err());
    }

    #[test]
    fn update_existing_rejects_missing_entry_and_updates_present_one() {
        let (store, _dir) = temp_store();
        assert!(store
            .update_existing(
                "missing".into(),
                "content".into(),
                MemoryScope::Project,
                vec![]
            )
            .is_err());

        store
            .store("id".into(), "original".into(), MemoryScope::Project, vec![])
            .unwrap();
        let updated = store
            .update_existing(
                "id".into(),
                "revised".into(),
                MemoryScope::Global,
                vec!["tag".into()],
            )
            .unwrap();
        assert_eq!(updated.content, "revised");
        assert_eq!(updated.scope, MemoryScope::Global);
        assert_eq!(updated.tags, vec!["tag".to_string()]);
    }

    #[test]
    fn projects_are_isolated_by_root() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let a = MemoryStore::for_project(dir_a.path()).unwrap();
        let b = MemoryStore::for_project(dir_b.path()).unwrap();
        a.store(
            "shared-id".into(),
            "from a".into(),
            MemoryScope::Project,
            vec![],
        )
        .unwrap();
        assert!(b.get("shared-id").unwrap().is_none());
    }

    // ------------------------------------------------------------------
    // Legacy JSONL migration
    // ------------------------------------------------------------------

    fn seed_legacy(dir: &Path, lines: &[serde_json::Value]) {
        let agent = dir.join(".agent");
        fs::create_dir_all(&agent).unwrap();
        let mut raw = String::new();
        for value in lines {
            raw.push_str(&serde_json::to_string(value).unwrap());
            raw.push('\n');
        }
        fs::write(agent.join("memory.jsonl"), raw).unwrap();
    }

    #[test]
    fn legacy_jsonl_is_migrated_on_open() {
        let dir = tempfile::tempdir().unwrap();
        seed_legacy(
            dir.path(),
            &[
                serde_json::json!({"timestamp": "2026-01-01T00:00:00Z", "content": "first"}),
                serde_json::json!({"timestamp": "2026-01-02T00:00:00Z", "content": "second"}),
            ],
        );

        let store = MemoryStore::for_project(dir.path()).unwrap();
        let entries = store.list_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "legacy-jsonl-0");
        assert_eq!(entries[0].content, "first");
        assert_eq!(entries[0].created_at, "2026-01-01T00:00:00Z");
        assert_eq!(entries[0].scope, MemoryScope::Project);
        assert_eq!(entries[1].id, "legacy-jsonl-1");

        // The legacy file is archived, not deleted, and the canonical
        // store is a single JSON document.
        assert!(dir.path().join(".agent/memory.jsonl.migrated").is_file());
        assert!(!dir.path().join(".agent/memory.jsonl").exists());
        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(raw.starts_with('{') && raw.ends_with('}'));
    }

    #[test]
    fn legacy_migration_merges_with_existing_canonical_data() {
        let dir = tempfile::tempdir().unwrap();
        seed_legacy(
            dir.path(),
            &[serde_json::json!({"timestamp": "2026-01-01T00:00:00Z", "content": "old note"})],
        );
        // An MCP-era record predates the migration attempt.
        {
            let store = MemoryStore::for_project(dir.path()).unwrap();
            store
                .store(
                    "agent-note".into(),
                    "rich note".into(),
                    MemoryScope::Global,
                    vec!["x".into()],
                )
                .unwrap();
        }

        let store = MemoryStore::for_project(dir.path()).unwrap();
        let entries = store.list_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.id == "agent-note"));
        assert!(entries.iter().any(|e| e.id == "legacy-jsonl-0"));
    }

    #[test]
    fn legacy_migration_is_idempotent_after_interrupted_run() {
        let dir = tempfile::tempdir().unwrap();
        seed_legacy(
            dir.path(),
            &[serde_json::json!({"timestamp": "2026-01-01T00:00:00Z", "content": "note"})],
        );
        // First open migrates and archives.
        let first = MemoryStore::for_project(dir.path()).unwrap();
        assert_eq!(first.list_all().unwrap().len(), 1);

        // Simulate the crash window: restore the legacy file exactly as it
        // was, with the canonical store already published.
        fs::rename(
            dir.path().join(".agent/memory.jsonl.migrated"),
            dir.path().join(".agent/memory.jsonl"),
        )
        .unwrap();

        let second = MemoryStore::for_project(dir.path()).unwrap();
        let entries = second.list_all().unwrap();
        assert_eq!(
            entries.len(),
            1,
            "re-running the migration must not duplicate records"
        );
        assert_eq!(entries[0].id, "legacy-jsonl-0");
    }

    #[test]
    fn legacy_migration_skips_blank_lines_without_consuming_ids() {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join(".agent");
        fs::create_dir_all(&agent).unwrap();
        let one = serde_json::to_string(&serde_json::json!({
            "timestamp": "2026-01-01T00:00:00Z", "content": "first"
        }))
        .unwrap();
        let two = serde_json::to_string(&serde_json::json!({
            "timestamp": "2026-01-02T00:00:00Z", "content": "second"
        }))
        .unwrap();
        fs::write(agent.join("memory.jsonl"), format!("{one}\n\n   \n{two}\n")).unwrap();

        let store = MemoryStore::for_project(dir.path()).unwrap();
        let entries = store.list_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "legacy-jsonl-0");
        assert_eq!(entries[1].id, "legacy-jsonl-1");
    }

    #[test]
    fn legacy_migration_fails_closed_on_corrupt_line() {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join(".agent");
        fs::create_dir_all(&agent).unwrap();
        fs::write(agent.join("memory.jsonl"), "not json at all\n").unwrap();

        let result = MemoryStore::for_project(dir.path());
        assert!(result.is_err(), "corrupt legacy data must fail closed");
        // and it must not have archived or destroyed the data
        assert!(agent.join("memory.jsonl").exists());
        assert!(!agent.join("memory.json").exists());
    }

    #[test]
    fn missing_legacy_file_is_not_an_error() {
        let (store, _dir) = temp_store();
        assert!(store.list_all().unwrap().is_empty());
    }

    #[test]
    fn append_scoped_honors_scope_tags_and_mints_ids() {
        let (store, _dir) = temp_store();
        let a = store
            .append_scoped(
                "session note",
                MemoryScope::Session,
                vec!["note".to_string()],
            )
            .unwrap();
        let b = store
            .append_scoped("global fact", MemoryScope::Global, vec![])
            .unwrap();
        assert_ne!(a.id, b.id, "ids must be minted uniquely");
        assert_eq!(a.scope, MemoryScope::Session);
        assert_eq!(a.tags, vec!["note".to_string()]);
        assert_eq!(b.scope, MemoryScope::Global);
        // Validation flows through the store path.
        assert!(store
            .append_scoped(
                &"x".repeat(MAX_MEMORY_CONTENT_BYTES + 1),
                MemoryScope::Project,
                vec![]
            )
            .is_err());
    }

    #[test]
    fn update_partial_changes_only_provided_fields() {
        let (store, _dir) = temp_store();
        let entry = store
            .store(
                "m1".to_string(),
                "original".to_string(),
                MemoryScope::Project,
                vec!["t1".to_string()],
            )
            .unwrap();

        // content-only update keeps scope and tags
        let updated = store
            .update_partial("m1", Some("new content".to_string()), None, None)
            .unwrap();
        assert_eq!(updated.content, "new content");
        assert_eq!(updated.scope, MemoryScope::Project);
        assert_eq!(updated.tags, vec!["t1".to_string()]);
        assert_eq!(updated.created_at, entry.created_at);

        // scope + tags replace wholesale; content untouched
        let updated = store
            .update_partial(
                "m1",
                None,
                Some(MemoryScope::Global),
                Some(vec!["t2".to_string(), "t3".to_string()]),
            )
            .unwrap();
        assert_eq!(updated.content, "new content");
        assert_eq!(updated.scope, MemoryScope::Global);
        assert_eq!(updated.tags, vec!["t2".to_string(), "t3".to_string()]);

        // unknown id fails closed, no field provided fails closed
        assert!(store
            .update_partial("ghost", Some("c".to_string()), None, None)
            .is_err());
        assert!(store.update_partial("m1", None, None, None).is_err());
    }

    #[test]
    fn updates_remain_possible_at_the_entry_count_limit() {
        // §21: the entry-count cap gates only NEW ids; updates to existing
        // entries must keep working at the boundary (a full store is not a
        // read-only store).
        let (store, _dir) = temp_store();
        let entries: Vec<MemoryEntry> = (0..MAX_MEMORY_ENTRIES as u64)
            .map(|i| MemoryEntry {
                id: format!("entry-{i}"),
                scope: MemoryScope::Project,
                content: "c".into(),
                tags: vec![],
                created_at: String::new(),
                updated_at: String::new(),
            })
            .collect();
        fs::write(
            store.path.clone(),
            serde_json::to_string(&MemoryFile { entries }).unwrap(),
        )
        .unwrap();

        let updated = store
            .update_partial("entry-0", Some("new".into()), None, None)
            .unwrap();
        assert_eq!(updated.content, "new");
        // and a genuinely new id is still refused
        assert!(store
            .store("overflow".into(), "x".into(), MemoryScope::Project, vec![])
            .is_err());
    }
}
