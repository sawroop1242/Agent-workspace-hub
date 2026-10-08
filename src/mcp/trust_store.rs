use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

use super::permissions::McpPermissions;
use super::trust::{McpApproval, TrustLevel, TrustStore};

/// Persistent, JSON-backed trust store mirroring the in-memory [`TrustStore`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentTrustStore {
    /// The persisted approvals.
    pub approvals: Vec<McpApproval>,
}

impl PersistentTrustStore {
    /// Loads the trust store from `data_dir/trust.json`, defaulting to empty.
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = data_dir.into();
        fs::create_dir_all(&dir)?;
        let path = dir.join("trust.json");
        if !path.exists() {
            return Ok(Self {
                approvals: Vec::new(),
            });
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    /// Builds a persistent store from an in-memory store.
    pub fn from_store(store: &TrustStore) -> Self {
        Self {
            approvals: store.approvals.clone(),
        }
    }

    /// Converts to the in-memory [`TrustStore`] representation.
    pub fn to_store(&self) -> TrustStore {
        TrustStore {
            approvals: self.approvals.clone(),
        }
    }

    /// Persists the trust store to `data_dir/trust.json`.
    pub fn save(&self, data_dir: impl Into<PathBuf>) -> Result<()> {
        let dir = data_dir.into();
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("trust.json"), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Records (or replaces) an approval in the persistent store.
    pub fn approve(
        &mut self,
        id: impl Into<String>,
        level: TrustLevel,
        permissions: McpPermissions,
        version: impl Into<String>,
    ) -> Result<()> {
        let mut store = self.to_store();
        store.approve(id, level, permissions, version)?;
        self.approvals = store.approvals;
        Ok(())
    }

    /// Revokes a trust record, returning whether one existed.
    pub fn revoke(&mut self, id: &str) -> bool {
        let mut store = self.to_store();
        let changed = store.revoke(id);
        self.approvals = store.approvals;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::permissions::McpPermissions;

    #[test]
    fn new_creates_data_dir_and_defaults_to_empty() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("trust-data");
        assert!(!dir.exists());
        let store = PersistentTrustStore::new(&dir).unwrap();
        assert!(dir.is_dir());
        assert!(store.approvals.is_empty());
    }

    #[test]
    fn save_then_new_round_trips_approvals() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("trust-data");
        let mut store = PersistentTrustStore::new(&dir).unwrap();
        store
            .approve(
                "server-a",
                TrustLevel::Reviewed,
                McpPermissions::default(),
                "1.0",
            )
            .unwrap();
        store.save(&dir).unwrap();

        let reloaded = PersistentTrustStore::new(&dir).unwrap();
        assert_eq!(reloaded.approvals.len(), 1);
        assert_eq!(reloaded.approvals[0].id, "server-a");
        assert_eq!(reloaded.approvals[0].level, TrustLevel::Reviewed);
        assert_eq!(reloaded.approvals[0].approved_version, "1.0");
    }

    #[test]
    fn approve_replaces_and_persists_single_record() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("trust-data");
        let mut store = PersistentTrustStore::new(&dir).unwrap();
        store
            .approve(
                "server-a",
                TrustLevel::Reviewed,
                McpPermissions::default(),
                "1.0",
            )
            .unwrap();
        store
            .approve(
                "server-a",
                TrustLevel::Blocked,
                McpPermissions::default(),
                "2.0",
            )
            .unwrap();
        store.save(&dir).unwrap();
        let reloaded = PersistentTrustStore::new(&dir).unwrap();
        assert_eq!(reloaded.approvals.len(), 1);
        assert_eq!(reloaded.approvals[0].level, TrustLevel::Blocked);
    }

    #[test]
    fn revoke_reports_existence_and_persists() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("trust-data");
        let mut store = PersistentTrustStore::new(&dir).unwrap();
        store
            .approve(
                "server-a",
                TrustLevel::Reviewed,
                McpPermissions::default(),
                "1.0",
            )
            .unwrap();
        store.save(&dir).unwrap();
        let mut reloaded = PersistentTrustStore::new(&dir).unwrap();
        assert!(!reloaded.revoke("missing"));
        assert!(reloaded.revoke("server-a"));
        reloaded.save(&dir).unwrap();
        assert!(PersistentTrustStore::new(&dir)
            .unwrap()
            .approvals
            .is_empty());
    }

    #[test]
    fn store_conversion_preserves_approvals() {
        let mut memory = TrustStore::default();
        memory
            .approve(
                "server-a",
                TrustLevel::Trusted,
                McpPermissions::default(),
                "",
            )
            .unwrap();
        let persistent = PersistentTrustStore::from_store(&memory);
        assert_eq!(persistent.approvals.len(), 1);
        let back = persistent.to_store();
        assert_eq!(back.approvals.len(), 1);
        assert_eq!(back.get("server-a").unwrap().level, TrustLevel::Trusted);
    }

    #[test]
    fn approve_rejects_invalid_permissions_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("trust-data");
        let mut store = PersistentTrustStore::new(&dir).unwrap();
        let mut bad = McpPermissions::default();
        bad.environment.push("PATH".into()); // blocked env var
        assert!(store
            .approve("server-a", TrustLevel::Trusted, bad, "1.0")
            .is_err());
        assert!(store.approvals.is_empty());
    }
}
