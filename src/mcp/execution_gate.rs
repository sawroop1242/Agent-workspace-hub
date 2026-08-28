use super::{can_enable, McpPermissions, TrustStore};
use anyhow::{bail, Result};

/// A request to execute an MCP server, carrying the identity, version, and
/// requested permissions that must be authorized against a [`TrustStore`].
#[derive(Debug, Clone)]
pub struct McpExecutionRequest<'a> {
    /// MCP server id.
    pub id: &'a str,
    /// Server version requesting execution.
    pub version: &'a str,
    /// Permissions requested by the server.
    pub permissions: &'a McpPermissions,
}

/// Authorizes an MCP execution request against the trust store, failing closed
/// for missing, blocked, mismatched-version, or over-broad permission requests.
pub fn authorize(request: &McpExecutionRequest<'_>, trust: &TrustStore) -> Result<()> {
    if request.id.trim().is_empty() {
        bail!("MCP id is required");
    }
    let approval = trust.get(request.id);
    if approval.is_none() {
        bail!("MCP execution denied: no approval for {}", request.id);
    }
    if !can_enable(approval, request.permissions, request.version) {
        bail!("MCP execution denied: trust, version, or permissions do not match approval");
    }
    Ok(())
}
