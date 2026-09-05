use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Maximum number of entries a single project memory store may hold.
const MAX_MEMORY_ENTRIES: usize = 10_000;
/// Maximum size of a single memory entry's content, in bytes.
const MAX_MEMORY_CONTENT_BYTES: usize = 1024 * 1024;
/// Maximum length of a memory entry id.
const MAX_MEMORY_ID_LEN: usize = 256;
/// Maximum number of tags per entry.
const MAX_MEMORY_TAGS: usize = 64;
/// Maximum length of a single tag.
const MAX_MEMORY_TAG_LEN: usize = 128;

/// A single persisted memory record within a project.
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
struct MemoryStore {
    entries: Vec<MemoryEntry>,
}

/// Project-scoped MCP memory store persisted to `.agent/memory.json`.
pub struct MemoryMcp {
    path: PathBuf,
}

impl MemoryMcp {
    /// Creates a memory store backed by `.agent/memory.json` under the project root.
    pub fn new(project_root: impl Into<PathBuf>) -> Result<Self> {
        let root = project_root.into();
        fs::create_dir_all(root.join(".agent"))?;
        Ok(Self {
            path: root.join(".agent").join("memory.json"),
        })
    }

    fn load(&self) -> Result<MemoryStore> {
        if !self.path.exists() {
            return Ok(MemoryStore::default());
        }
        Ok(serde_json::from_str(
            &fs::read_to_string(&self.path).context("read memory store")?,
        )?)
    }

    fn save(&self, store: &MemoryStore) -> Result<()> {
        fs::write(&self.path, serde_json::to_string_pretty(store)?)?;
        Ok(())
    }

    /// Inserts or overwrites a memory entry keyed by `id`, updating timestamps.
    ///
    /// Fails closed when the entry would exceed the store's enforced size
    /// limits (entry count, content bytes, id length, tag count/length), so a
    /// misbehaving client cannot grow the on-disk store without bound.
    pub fn store(
        &self,
        id: String,
        content: String,
        scope: MemoryScope,
        tags: Vec<String>,
    ) -> Result<MemoryEntry> {
        validate_memory_input(&id, &content, &tags)?;
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

    /// Lists every entry across all scopes, oldest first.
    pub fn list_all(&self) -> Result<Vec<MemoryEntry>> {
        Ok(self.load()?.entries)
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
        let mut db = self.load()?;
        let before = db.entries.len();
        db.entries.retain(|e| e.id != id);
        self.save(&db)?;
        Ok(before != db.entries.len())
    }
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

    fn temp_store() -> (MemoryMcp, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryMcp::new(dir.path()).unwrap();
        (store, dir)
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
        // Fill the store to capacity using pre-serialized state to avoid
        // creating 10k entries through the public API one at a time.
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
            serde_json::to_string(&MemoryStore { entries }).unwrap(),
        )
        .unwrap();

        // Inserting one more must fail.
        assert!(store
            .store(
                "overflow".into(),
                "content".into(),
                MemoryScope::Project,
                vec![]
            )
            .is_err());

        // Overwriting an existing entry must still succeed.
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
    fn projects_are_isolated_by_root() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let a = MemoryMcp::new(dir_a.path()).unwrap();
        let b = MemoryMcp::new(dir_b.path()).unwrap();
        a.store(
            "shared-id".into(),
            "from a".into(),
            MemoryScope::Project,
            vec![],
        )
        .unwrap();
        assert!(b.get("shared-id").unwrap().is_none());
    }
}
