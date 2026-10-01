//! Durable active-window persistence for the Context Engine.
//!
//! The active context window is the engine's primary index: the items a
//! task's assembled context is selected from. CTX-001 §6 requires every
//! durable context-owned state to have an explicit schema/version, atomic
//! publication, and fail-closed corruption handling — this store provides
//! exactly that for the window, so a saved item survives process
//! termination (proving `write → terminate → reload → verify`), which is
//! what makes the one-shot `awh context save|update|clear` CLI contract
//! implementable at all.
//!
//! Scope note: the window persists items of ALL scopes exactly as the
//! in-memory engine holds them (mirroring the offload store, which also
//! persists session-scoped content); scope filtering stays a read-time
//! concern in the selector/planner, never a storage concern.

use crate::context::item::{is_valid_item_id, ContextItem};
use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The window file schema version. A mismatch fails closed rather than
/// being silently reinterpreted.
pub const WINDOW_SCHEMA_VERSION: u32 = 1;

/// The durable shape of the window: schema version, items, protected ids.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowRecord {
    pub schema_version: u32,
    pub items: Vec<ContextItem>,
    #[serde(default)]
    pub protected: Vec<String>,
}

/// Durable storage for the active context window.
pub struct WindowStore {
    path: PathBuf,
}

impl WindowStore {
    /// Creates the window store at
    /// `project_root/.agent/context-engine/active.json`.
    pub fn new(project_root: &Path) -> Result<Self> {
        let dir = project_root.join(".agent").join("context-engine");
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create context-engine dir {}", dir.display()))?;
        Ok(Self {
            path: dir.join("active.json"),
        })
    }

    /// Loads the persisted window. A missing file is an empty window;
    /// anything unreadable, malformed, version-mismatched, or carrying an
    /// invalid item id fails closed with a clear error.
    pub fn load(&self) -> Result<WindowRecord> {
        if !self.path.exists() {
            return Ok(WindowRecord {
                schema_version: WINDOW_SCHEMA_VERSION,
                items: Vec::new(),
                protected: Vec::new(),
            });
        }
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read context window {}", self.path.display()))?;
        let record: WindowRecord = serde_json::from_str(&raw).with_context(|| {
            format!(
                "corrupt context window {} — remove or repair the file before running context operations",
                self.path.display()
            )
        })?;
        if record.schema_version != WINDOW_SCHEMA_VERSION {
            bail!(
                "unsupported context window schema version {} in {} (expected {WINDOW_SCHEMA_VERSION})",
                record.schema_version,
                self.path.display()
            );
        }
        for item in &record.items {
            if !is_valid_item_id(&item.id) {
                bail!(
                    "context window {} contains an invalid item id {:?}",
                    self.path.display(),
                    item.id
                );
            }
        }
        Ok(record)
    }

    /// Atomically publishes the window (temp file + rename in the same
    /// directory). Failure leaves the previous window intact.
    pub fn save(&self, record: &WindowRecord) -> Result<()> {
        let parent = self.path.parent().context("window path has no parent")?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).with_context(|| {
            format!("failed to create window temp file in {}", parent.display())
        })?;
        let data = serde_json::to_string_pretty(record)?;
        temp.write_all(data.as_bytes())
            .and_then(|_| temp.flush())
            .with_context(|| "failed to write the context window")?;
        // persist to disk before the rename so the published file is
        // durable, matching the manifest/policy write pattern.
        let file = temp.persist(&self.path).map_err(|error| {
            anyhow::anyhow!(
                "failed to publish the context window to {}: {error}",
                self.path.display()
            )
        })?;
        file.sync_all()
            .with_context(|| "failed to fsync the context window")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_window_loads_empty() {
        let temp = tempfile::tempdir().unwrap();
        let store = WindowStore::new(temp.path()).unwrap();
        let record = store.load().unwrap();
        assert!(record.items.is_empty());
        assert_eq!(record.schema_version, WINDOW_SCHEMA_VERSION);
    }

    #[test]
    fn save_then_reload_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = WindowStore::new(temp.path()).unwrap();
        let mut item = ContextItem::new("a", crate::context::item::ContextSource::Tool, "hello", 1);
        item.state = crate::context::item::ContextState::Offloaded;
        let record = WindowRecord {
            schema_version: WINDOW_SCHEMA_VERSION,
            items: vec![item],
            protected: vec!["a".to_string()],
        };
        store.save(&record).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].id, "a");
        assert_eq!(loaded.protected, vec!["a".to_string()]);
    }

    #[test]
    fn corrupt_window_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = WindowStore::new(temp.path()).unwrap();
        std::fs::write(&store.path, "{ not json").unwrap();
        assert!(store.load().is_err());
    }

    #[test]
    fn unsupported_version_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = WindowStore::new(temp.path()).unwrap();
        std::fs::write(&store.path, r#"{"schema_version":99,"items":[]}"#).unwrap();
        assert!(store.load().is_err());
    }

    #[test]
    fn invalid_item_id_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = WindowStore::new(temp.path()).unwrap();
        let item = ContextItem::new("../evil", crate::context::item::ContextSource::Tool, "x", 1);
        let record = WindowRecord {
            schema_version: WINDOW_SCHEMA_VERSION,
            items: vec![item],
            protected: Vec::new(),
        };
        // Saving is the engine's responsibility to prevent; a hand-crafted
        // file with a traversal id must still fail on load.
        std::fs::write(&store.path, serde_json::to_string(&record).unwrap()).unwrap();
        assert!(store.load().is_err());
    }
}
