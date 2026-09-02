//! Soft context offloading.
//!
//! **Offload never means delete.** An offloaded item's content is written
//! durably under the project's `.agent/context-engine/offloads/` directory as
//! an individual JSON record keyed by its validated, filename-safe id. The
//! original item stays fully recoverable through [`OffloadStore::restore`],
//! and offloaded content inherits the same project isolation as every other
//! `.agent` store: one project's offloads are physically unable to appear in
//! another project's engine because each engine is rooted at that project.

use crate::context::item::{ContextItem, ContextState};
use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum offloaded records per project (bounded memory on disk).
const MAX_OFFLOADS: usize = 10_000;

/// A durable offload record: the full item plus when and why it moved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffloadRecord {
    /// The complete item at offload time (content included).
    pub item: ContextItem,
    /// RFC 3339 timestamp of the offload.
    pub offloaded_at: String,
    /// Optional reason (policy decision text).
    pub reason: Option<String>,
}

/// Durable storage for offloaded context items.
pub struct OffloadStore {
    dir: PathBuf,
}

impl OffloadStore {
    /// Creates an offload store under `project_root/.agent/context-engine/offloads`.
    pub fn new(project_root: &Path) -> Result<Self> {
        let dir = project_root
            .join(".agent")
            .join("context-engine")
            .join("offloads");
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create offload dir {}", dir.display()))?;
        Ok(Self { dir })
    }

    fn record_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Durably writes an offload record.
    ///
    /// Fails closed on invalid ids, invalid state, or store overflow. Failure
    /// leaves the item active (callers must keep the in-memory item on error
    /// — the engine enforces this).
    pub fn put(&self, record: &OffloadRecord) -> Result<()> {
        let item = &record.item;
        if !crate::context::item::is_valid_item_id(&item.id) {
            bail!("invalid context item id: {:?}", item.id);
        }
        if item.state != ContextState::Offloaded {
            bail!("can only offload items in the Offloaded state");
        }
        if record.offloaded_at.trim().is_empty() {
            bail!("offload record must carry a timestamp");
        }
        if self.list_ids()?.len() >= MAX_OFFLOADS && !self.record_path(&item.id).exists() {
            bail!("offload store is full (max {MAX_OFFLOADS} records)");
        }
        let path = self.record_path(&item.id);
        let data = serde_json::to_string_pretty(record)?;
        fs::write(&path, data)
            .with_context(|| format!("failed to write offload {}", path.display()))?;
        Ok(())
    }

    /// Reads back one offloaded item, `None` when the id is unknown.
    pub fn get(&self, id: &str) -> Result<Option<ContextItem>> {
        if !crate::context::item::is_valid_item_id(id) {
            return Ok(None);
        }
        let path = self.record_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let record: OffloadRecord = serde_json::from_str(
            &fs::read_to_string(&path)
                .with_context(|| format!("failed to read offload {}", path.display()))?,
        )
        .with_context(|| format!("corrupt offload record {}", path.display()))?;
        Ok(Some(record.item))
    }

    /// Reads back the full record (including reason/timestamp), `None` if absent.
    pub fn get_record(&self, id: &str) -> Result<Option<OffloadRecord>> {
        if !crate::context::item::is_valid_item_id(id) {
            return Ok(None);
        }
        let path = self.record_path(id);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(
            &fs::read_to_string(&path)
                .with_context(|| format!("failed to read offload {}", path.display()))?,
        )?))
    }

    /// Deletes an offload record *only after it has been restored*: this is
    /// the exclusive deletion path, and callers must only invoke it when the
    /// item is active again. Keeps offloaded content from leaking after
    /// restore while guaranteeing it was recoverable the whole time.
    pub fn remove_restored(&self, id: &str) -> Result<bool> {
        if !crate::context::item::is_valid_item_id(id) {
            return Ok(false);
        }
        let path = self.record_path(id);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove offload {}", path.display()))?;
        Ok(true)
    }

    /// Lists all offloaded item ids, sorted.
    ///
    /// Uses the directory listing (indexed metadata), never a full scan of
    /// record contents.
    pub fn list_ids(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
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

    /// Returns the number of offloaded records.
    pub fn len(&self) -> Result<usize> {
        Ok(self.list_ids()?.len())
    }

    /// Whether the store holds no records.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.list_ids()?.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::item::ContextSource;
    use chrono::Utc;

    fn store_in(temp: &tempfile::TempDir) -> OffloadStore {
        OffloadStore::new(temp.path()).unwrap()
    }

    fn offloaded_item(id: &str, content: &str) -> ContextItem {
        let mut item = ContextItem::new(id, ContextSource::Tool, content, 5);
        item.state = ContextState::Offloaded;
        item
    }

    fn record(item: ContextItem) -> OffloadRecord {
        OffloadRecord {
            item,
            offloaded_at: Utc::now().to_rfc3339(),
            reason: Some("low score".to_string()),
        }
    }

    #[test]
    fn active_to_offloaded_to_restored_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp);

        let item = offloaded_item("tool-1", "original tool output");
        store.put(&record(item.clone())).unwrap();

        let restored = store.get("tool-1").unwrap().unwrap();
        assert_eq!(restored.content, "original tool output");
        assert_eq!(restored.id, "tool-1");

        // Only deletion after restore.
        assert!(store.remove_restored("tool-1").unwrap());
        assert!(store.get("tool-1").unwrap().is_none());
    }

    #[test]
    fn source_content_is_preserved_during_offload() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp);
        let content = "very long tool output\nwith multiple\nlines of data";
        store.put(&record(offloaded_item("x", content))).unwrap();
        assert_eq!(store.get("x").unwrap().unwrap().content, content);
    }

    #[test]
    fn record_carries_reason_and_timestamp() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp);
        let mut rec = record(offloaded_item("y", "content"));
        rec.reason = Some("policy: score 0.02 < offload threshold".to_string());
        store.put(&rec).unwrap();
        let stored = store.get_record("y").unwrap().unwrap();
        assert_eq!(
            stored.reason.as_deref(),
            Some("policy: score 0.02 < offload threshold")
        );
        assert!(!stored.offloaded_at.is_empty());
    }

    #[test]
    fn invalid_ids_are_rejected_not_written() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp);
        for bad in ["", "..", "a/b", "has space"] {
            assert!(
                store.put(&record(offloaded_item(bad, "c"))).is_err(),
                "{bad:?}"
            );
        }
        assert!(store.list_ids().unwrap().is_empty());
    }

    #[test]
    fn active_state_cannot_be_offloaded() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp);
        let mut item = ContextItem::new("active", ContextSource::Tool, "c", 1);
        item.state = ContextState::Active;
        assert!(store.put(&record(item)).is_err());
    }

    #[test]
    fn missing_timestamp_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp);
        let mut rec = record(offloaded_item("z", "c"));
        rec.offloaded_at = String::new();
        assert!(store.put(&rec).is_err());
    }

    #[test]
    fn unknown_ids_return_none_not_errors() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp);
        assert!(store.get("absent").unwrap().is_none());
        assert!(store.get("../escape").unwrap().is_none());
        assert!(!store.remove_restored("absent").unwrap());
    }

    #[test]
    fn projects_are_isolated_by_root() {
        let temp_a = tempfile::tempdir().unwrap();
        let temp_b = tempfile::tempdir().unwrap();
        let a = OffloadStore::new(temp_a.path()).unwrap();
        let b = OffloadStore::new(temp_b.path()).unwrap();
        a.put(&record(offloaded_item("shared", "from a"))).unwrap();
        assert!(b.get("shared").unwrap().is_none());
        assert!(a.get("shared").unwrap().is_some());
    }

    #[test]
    fn list_ids_are_sorted_and_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp);
        for id in ["c", "a", "b"] {
            store.put(&record(offloaded_item(id, "c"))).unwrap();
        }
        assert_eq!(store.list_ids().unwrap(), vec!["a", "b", "c"]);
        assert_eq!(store.len().unwrap(), 3);
        assert!(!store.is_empty().unwrap());
    }

    #[test]
    fn offload_dir_is_under_agent_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp);
        let dir = store.dir.display().to_string();
        let root = temp.path().display().to_string();
        assert!(dir.contains(".agent/context-engine/offloads"));
        assert!(dir.starts_with(&root));
    }
}
