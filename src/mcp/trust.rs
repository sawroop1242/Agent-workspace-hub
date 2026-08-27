use super::permissions::McpPermissions;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    Trusted,
    Reviewed,
    Unknown,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpApproval {
    pub id: String,
    #[serde(default)]
    pub level: TrustLevel,
    #[serde(default)]
    pub approved_permissions: McpPermissions,
    #[serde(default)]
    pub approved_version: String,
}
impl Default for TrustLevel {
    fn default() -> Self {
        Self::Unknown
    }
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TrustStore {
    pub approvals: Vec<McpApproval>,
}
impl TrustStore {
    pub fn get(&self, id: &str) -> Option<&McpApproval> {
        self.approvals.iter().find(|x| x.id == id)
    }
    pub fn approve(
        &mut self,
        id: impl Into<String>,
        level: TrustLevel,
        permissions: McpPermissions,
        version: impl Into<String>,
    ) -> Result<()> {
        let id = id.into();
        if id.trim().is_empty() {
            bail!("MCP id is required")
        }
        permissions.validate()?;
        self.approvals.retain(|x| x.id != id);
        self.approvals.push(McpApproval {
            id,
            level,
            approved_permissions: permissions,
            approved_version: version.into(),
        });
        Ok(())
    }
    pub fn revoke(&mut self, id: &str) -> bool {
        let before = self.approvals.len();
        self.approvals.retain(|x| x.id != id);
        before != self.approvals.len()
    }
}
pub fn can_enable(
    approval: Option<&McpApproval>,
    requested: &McpPermissions,
    version: &str,
) -> bool {
    let Some(a) = approval else { return false };
    if matches!(a.level, TrustLevel::Blocked | TrustLevel::Unknown) {
        return false;
    }
    if !a.approved_version.is_empty() && a.approved_version != version {
        return false;
    }
    requested.network <= a.approved_permissions.network
        && requested.process <= a.approved_permissions.process
        && requested
            .filesystem
            .iter()
            .all(|p| a.approved_permissions.filesystem.contains(p))
        && requested
            .environment
            .iter()
            .all(|v| a.approved_permissions.environment.contains(v))
        && requested
            .secrets
            .iter()
            .all(|v| a.approved_permissions.secrets.contains(v))
}
