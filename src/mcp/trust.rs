use super::permissions::McpPermissions;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// The trust level assigned to an MCP server. `Unknown` (untrusted) and
/// `Blocked` deny execution; `Review` and `Trusted` permit it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Explicitly trusted.
    Trusted,
    /// Reviewed and approved.
    Reviewed,
    /// Untrusted (fail closed).
    Unknown,
    /// Explicitly blocked.
    Blocked,
}

/// A persisted approval for a specific MCP server id, version, and permission set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpApproval {
    /// MCP server id being approved.
    pub id: String,
    /// Assigned trust level.
    #[serde(default)]
    pub level: TrustLevel,
    /// The permission set explicitly approved for the server.
    #[serde(default)]
    pub approved_permissions: McpPermissions,
    /// The server version this approval applies to (empty means any).
    #[serde(default)]
    pub approved_version: String,
}
/// `Unknown` is deliberately not the first variant, so `Default` cannot be
/// derived: a new approval must start untrusted (fail closed) rather than
/// implicitly trusted.
#[allow(clippy::derivable_impls)]
impl Default for TrustLevel {
    fn default() -> Self {
        Self::Unknown
    }
}
/// In-memory collection of MCP approvals.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TrustStore {
    /// The stored approvals.
    pub approvals: Vec<McpApproval>,
}
impl TrustStore {
    /// Returns the approval for `id`, if any.
    pub fn get(&self, id: &str) -> Option<&McpApproval> {
        self.approvals.iter().find(|x| x.id == id)
    }
    /// Records (or replaces) an approval after validating the permission set.
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
    /// Removes an approval, returning whether one existed for `id`.
    pub fn revoke(&mut self, id: &str) -> bool {
        let before = self.approvals.len();
        self.approvals.retain(|x| x.id != id);
        before != self.approvals.len()
    }
}
/// Whether an approval authorizes the requested permissions and version,
/// denying blocked/unknown levels and over-broad requests.
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
