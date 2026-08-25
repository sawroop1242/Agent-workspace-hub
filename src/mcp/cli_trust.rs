use anyhow::{bail, Result};
use super::{McpPermissions, PersistentTrustStore, TrustLevel};

pub fn trust_mcp(data_dir: impl Into<std::path::PathBuf>, id: &str, version: &str, permissions: McpPermissions) -> Result<()> {
    let data_dir = data_dir.into();
    let mut store = PersistentTrustStore::new(&data_dir)?;
    store.approve(id.to_string(), TrustLevel::Reviewed, permissions, version.to_string())?;
    store.save(data_dir)
}

pub fn block_mcp(data_dir: impl Into<std::path::PathBuf>, id: &str, version: &str) -> Result<()> {
    let data_dir = data_dir.into();
    let mut store = PersistentTrustStore::new(&data_dir)?;
    store.approve(id.to_string(), TrustLevel::Blocked, McpPermissions::default(), version.to_string())?;
    store.save(data_dir)
}

pub fn revoke_mcp(data_dir: impl Into<std::path::PathBuf>, id: &str) -> Result<()> {
    let data_dir = data_dir.into();
    let mut store = PersistentTrustStore::new(&data_dir)?;
    if !store.revoke(id) { bail!("no trust record found for {id}"); }
    store.save(data_dir)
}
