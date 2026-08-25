use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

use super::trust::{McpApproval, TrustLevel, TrustStore};
use super::permissions::McpPermissions;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentTrustStore { pub approvals: Vec<McpApproval> }

impl PersistentTrustStore {
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = data_dir.into();
        fs::create_dir_all(&dir)?;
        let path = dir.join("trust.json");
        if !path.exists() { return Ok(Self { approvals: Vec::new() }); }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    pub fn from_store(store: &TrustStore) -> Self { Self { approvals: store.approvals.clone() } }

    pub fn to_store(&self) -> TrustStore { TrustStore { approvals: self.approvals.clone() } }

    pub fn save(&self, data_dir: impl Into<PathBuf>) -> Result<()> {
        let dir = data_dir.into();
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("trust.json"), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn approve(&mut self, id: impl Into<String>, level: TrustLevel, permissions: McpPermissions, version: impl Into<String>) -> Result<()> {
        let mut store = self.to_store();
        store.approve(id, level, permissions, version)?;
        self.approvals = store.approvals;
        Ok(())
    }

    pub fn revoke(&mut self, id: &str) -> bool {
        let mut store = self.to_store();
        let changed = store.revoke(id);
        self.approvals = store.approvals;
        changed
    }
}
