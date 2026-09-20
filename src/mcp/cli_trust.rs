use super::{McpPermissions, PersistentTrustStore, TrustLevel};
use anyhow::{bail, Result};

/// Approves an MCP server (trust level `Reviewed`) for the given permissions and version.
pub fn trust_mcp(
    data_dir: impl Into<std::path::PathBuf>,
    id: &str,
    version: &str,
    permissions: McpPermissions,
) -> Result<()> {
    let data_dir = data_dir.into();
    let mut store = PersistentTrustStore::new(&data_dir)?;
    store.approve(
        id.to_string(),
        TrustLevel::Reviewed,
        permissions,
        version.to_string(),
    )?;
    store.save(data_dir)
}

/// Blocks an MCP server (trust level `Blocked`) for the given version.
pub fn block_mcp(data_dir: impl Into<std::path::PathBuf>, id: &str, version: &str) -> Result<()> {
    let data_dir = data_dir.into();
    let mut store = PersistentTrustStore::new(&data_dir)?;
    store.approve(
        id.to_string(),
        TrustLevel::Blocked,
        McpPermissions::default(),
        version.to_string(),
    )?;
    store.save(data_dir)
}

/// Revokes any trust record for an MCP server, failing if none exists.
pub fn revoke_mcp(data_dir: impl Into<std::path::PathBuf>, id: &str) -> Result<()> {
    let data_dir = data_dir.into();
    let mut store = PersistentTrustStore::new(&data_dir)?;
    if !store.revoke(id) {
        bail!("no trust record found for {id}");
    }
    store.save(data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_mcp_persists_reviewed_approval() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("data");
        trust_mcp(&data_dir, "server-a", "1.0", McpPermissions::default()).unwrap();
        let store = PersistentTrustStore::new(&data_dir).unwrap();
        let approval = store
            .approvals
            .iter()
            .find(|a| a.id == "server-a")
            .expect("approval must be persisted");
        assert_eq!(approval.level, TrustLevel::Reviewed);
        assert_eq!(approval.approved_version, "1.0");
    }

    #[test]
    fn block_mcp_persists_blocked_approval_with_default_permissions() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("data");
        block_mcp(&data_dir, "server-b", "2.0").unwrap();
        let store = PersistentTrustStore::new(&data_dir).unwrap();
        let approval = store
            .approvals
            .iter()
            .find(|a| a.id == "server-b")
            .expect("approval must be persisted");
        assert_eq!(approval.level, TrustLevel::Blocked);
        // blocking never grants capabilities: every permission field is off/empty
        assert!(!approval.approved_permissions.network);
        assert!(!approval.approved_permissions.process);
        assert!(approval.approved_permissions.filesystem.is_empty());
        assert!(approval.approved_permissions.environment.is_empty());
        assert!(approval.approved_permissions.secrets.is_empty());
        assert_eq!(approval.approved_version, "2.0");
    }

    #[test]
    fn block_replaces_previous_trust_level() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("data");
        trust_mcp(&data_dir, "server-a", "1.0", McpPermissions::default()).unwrap();
        block_mcp(&data_dir, "server-a", "1.0").unwrap();
        let store = PersistentTrustStore::new(&data_dir).unwrap();
        assert_eq!(store.approvals.len(), 1);
        assert_eq!(store.approvals[0].level, TrustLevel::Blocked);
    }

    #[test]
    fn revoke_mcp_removes_record_and_fails_when_absent() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("data");
        // revoking an unknown id fails
        assert!(revoke_mcp(&data_dir, "server-a").is_err());
        trust_mcp(&data_dir, "server-a", "1.0", McpPermissions::default()).unwrap();
        revoke_mcp(&data_dir, "server-a").unwrap();
        let store = PersistentTrustStore::new(&data_dir).unwrap();
        assert!(store.approvals.is_empty());
        // a second revoke of the same id fails because the record is gone
        assert!(revoke_mcp(&data_dir, "server-a").is_err());
    }
}
