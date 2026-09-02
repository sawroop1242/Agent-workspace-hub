//! Context snapshots.
//!
//! A snapshot captures the engine's full visible state at a point in time:
//! the active items, references to offloaded items, budget information, and
//! task/session metadata. Snapshots hold items by value (they must be
//! independently reproducible for future partial-rollout branching — see the
//! roadmap notes in [`crate::context::policy`]) but never duplicate *source
//! files*: file-backed items reference paths, the engine re-reads them.

use crate::context::budget::ContextBudget;
use crate::context::item::ContextItem;
use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum snapshots per project.
const MAX_SNAPSHOTS: usize = 1_000;
/// Maximum length of a snapshot id.
const MAX_SNAPSHOT_ID_LEN: usize = 128;

/// A durable snapshot of the engine state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// Unique snapshot id (caller-supplied or engine-generated).
    pub id: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// The active items at snapshot time.
    pub active_items: Vec<ContextItem>,
    /// References (ids only) to items that were offloaded at snapshot time.
    pub offloaded_item_ids: Vec<String>,
    /// Task metadata recorded at snapshot time.
    pub task: Option<String>,
    /// Session metadata recorded at snapshot time.
    pub session: Option<String>,
    /// The budget configuration at snapshot time.
    pub budget: ContextBudget,
}

/// Durable snapshot storage under `.agent/context-engine/snapshots/`.
pub struct SnapshotStore {
    dir: PathBuf,
}

impl SnapshotStore {
    /// Creates a snapshot store under `project_root/.agent/context-engine/snapshots`.
    pub fn new(project_root: &Path) -> Result<Self> {
        let dir = project_root
            .join(".agent")
            .join("context-engine")
            .join("snapshots");
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create snapshot dir {}", dir.display()))?;
        Ok(Self { dir })
    }

    fn path_for(&self, id: &str) -> Result<PathBuf> {
        if !is_valid_snapshot_id(id) {
            bail!("invalid snapshot id: {id:?}");
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    /// Persists a snapshot durably.
    pub fn put(&self, snapshot: &ContextSnapshot) -> Result<()> {
        let path = self.path_for(&snapshot.id)?;
        if !path.exists() && self.list_ids()?.len() >= MAX_SNAPSHOTS {
            bail!("snapshot store is full (max {MAX_SNAPSHOTS} snapshots)");
        }
        let data = serde_json::to_string_pretty(snapshot)
            .map_err(|e| anyhow::anyhow!("failed to serialize snapshot: {e}"))?;
        fs::write(&path, data)
            .map_err(|e| anyhow::anyhow!("failed to write snapshot {}: {e}", path.display()))?;
        Ok(())
    }

    /// Reads a snapshot by id, `None` when absent.
    pub fn get(&self, id: &str) -> Result<Option<ContextSnapshot>> {
        let Ok(path) = self.path_for(id) else {
            return Ok(None);
        };
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("failed to read snapshot {}: {e}", path.display()))?;
        Ok(Some(serde_json::from_str(&raw).map_err(|e| {
            anyhow::anyhow!("corrupt snapshot {}: {e}", path.display())
        })?))
    }

    /// Lists snapshot ids sorted by name (chronological when ids are
    /// timestamps or otherwise sortable).
    pub fn list_ids(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for entry in
            fs::read_dir(&self.dir).map_err(|e| anyhow::anyhow!("snapshot dir unreadable: {e}"))?
        {
            let path = entry
                .map_err(|e| anyhow::anyhow!("snapshot dir entry failed: {e}"))?
                .path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Lists full snapshot metadata (ids and timestamps), newest first.
    pub fn list_summaries(&self) -> Result<Vec<SnapshotSummary>> {
        let mut summaries = Vec::new();
        for id in self.list_ids()? {
            if let Some(s) = self.get(&id)? {
                summaries.push(SnapshotSummary {
                    id: s.id,
                    created_at: s.created_at,
                    active_items: s.active_items.len(),
                    active_tokens: s.active_items.iter().map(|i| i.token_count).sum(),
                    offloaded_items: s.offloaded_item_ids.len(),
                    task: s.task,
                });
            }
        }
        // Newest first: id-ordered ascending reversed, which matches
        // lexicographic (chronological) timestamp ids.
        summaries.reverse();
        Ok(summaries)
    }

    /// Deletes a snapshot explicitly. Snapshots are only removed on explicit
    /// request; the engine never garbage-collects them silently.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let path = self.path_for(id)?;
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path)
            .map_err(|e| anyhow::anyhow!("failed to delete snapshot {}: {e}", path.display()))?;
        Ok(true)
    }
}

/// Summary metadata for `context.snapshot list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub id: String,
    pub created_at: String,
    pub active_items: usize,
    pub active_tokens: usize,
    pub offloaded_items: usize,
    pub task: Option<String>,
}

/// Whether an id is safe to use as a snapshot filename.
pub fn is_valid_snapshot_id(id: &str) -> bool {
    crate::context::item::is_valid_item_id(id) && id.len() <= MAX_SNAPSHOT_ID_LEN
}

/// Builds a fresh snapshot id: timestamp + short random-ish suffix.
pub fn generate_snapshot_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("snap-{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::item::ContextSource;

    fn snapshot(id: &str) -> ContextSnapshot {
        ContextSnapshot {
            id: id.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            active_items: vec![ContextItem::new("item-1", ContextSource::User, "hello", 1)],
            offloaded_item_ids: vec!["off-1".to_string()],
            task: Some("do the thing".to_string()),
            session: Some("session-42".to_string()),
            budget: ContextBudget::default(),
        }
    }

    fn store(temp: &tempfile::TempDir) -> SnapshotStore {
        SnapshotStore::new(temp.path()).unwrap()
    }

    #[test]
    fn create_list_inspect_restore_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);

        let snap = snapshot("snap-1");
        store.put(&snap).unwrap();

        assert_eq!(store.list_ids().unwrap(), vec!["snap-1"]);
        let loaded = store.get("snap-1").unwrap().unwrap();
        assert_eq!(loaded.active_items.len(), 1);
        assert_eq!(loaded.active_items[0].content, "hello");
        assert_eq!(loaded.offloaded_item_ids, vec!["off-1".to_string()]);
        assert_eq!(loaded.task.as_deref(), Some("do the thing"));
        assert_eq!(loaded.budget, ContextBudget::default());
    }

    #[test]
    fn invalid_snapshot_ids_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        for bad in ["..", "a/b", "x y"] {
            // Writes fail closed on unsafe ids.
            assert!(store.put(&snapshot(bad)).is_err(), "{bad:?}");
            // Reads of unsafe ids return None rather than touching the path.
            assert!(store.get(bad).unwrap().is_none(), "{bad:?}");
        }
        assert!(store.list_ids().unwrap().is_empty());
    }

    #[test]
    fn snapshots_are_isolated_per_project() {
        let temp_a = tempfile::tempdir().unwrap();
        let temp_b = tempfile::tempdir().unwrap();
        let a = store(&temp_a);
        let b = store(&temp_b);
        a.put(&snapshot("s")).unwrap();
        assert!(b.get("s").unwrap().is_none());
        assert_eq!(a.list_ids().unwrap(), vec!["s"]);
    }

    #[test]
    fn summaries_report_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        store.put(&snapshot("snap-a")).unwrap();
        store.put(&snapshot("snap-b")).unwrap();
        let summaries = store.list_summaries().unwrap();
        assert_eq!(summaries.len(), 2);
        // Newest first: snap-b listed before snap-a.
        assert_eq!(summaries[0].id, "snap-b");
        assert_eq!(summaries[0].active_items, 1);
        assert_eq!(summaries[0].active_tokens, 1);
        assert_eq!(summaries[0].offloaded_items, 1);
    }

    #[test]
    fn delete_is_explicit_and_returns_existence() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        assert!(!store.delete("absent").unwrap());
        store.put(&snapshot("snap-1")).unwrap();
        assert!(store.delete("snap-1").unwrap());
        assert!(store.get("snap-1").unwrap().is_none());
    }

    #[test]
    fn generated_ids_are_valid_and_uniqueish() {
        let a = generate_snapshot_id();
        let b = generate_snapshot_id();
        assert!(is_valid_snapshot_id(&a));
        assert!(is_valid_snapshot_id(&b));
        assert_ne!(a, b);
        assert!(a.starts_with("snap-"));
    }

    #[test]
    fn corrupt_snapshot_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        store.put(&snapshot("snap-1")).unwrap();
        let path = store.dir.join("snap-1.json");
        fs::write(&path, "not json").unwrap();
        assert!(store.get("snap-1").is_err());
    }
}
