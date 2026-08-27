use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// A single persisted memory record within a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub scope: MemoryScope,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The visibility scope of a memory entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryScope {
    Session,
    Project,
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
    pub fn store(
        &self,
        id: String,
        content: String,
        scope: MemoryScope,
        tags: Vec<String>,
    ) -> Result<MemoryEntry> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut db = self.load()?;
        if let Some(entry) = db.entries.iter_mut().find(|e| e.id == id) {
            entry.content = content;
            entry.tags = tags;
            entry.scope = scope;
            entry.updated_at = now.clone();
            let result = entry.clone();
            self.save(&db)?;
            return Ok(result);
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
