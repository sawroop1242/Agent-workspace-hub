use anyhow::{bail, Result};
use super::{can_enable, McpPermissions, TrustStore};

#[derive(Debug, Clone)]
pub struct McpExecutionRequest<'a> { pub id: &'a str, pub version: &'a str, pub permissions: &'a McpPermissions }

pub fn authorize(request: &McpExecutionRequest<'_>, trust: &TrustStore) -> Result<()> {
    if request.id.trim().is_empty() { bail!("MCP id is required"); }
    let approval = trust.get(request.id);
    if approval.is_none() { bail!("MCP execution denied: no approval for {}", request.id); }
    if !can_enable(approval, request.permissions, request.version) {
        bail!("MCP execution denied: trust, version, or permissions do not match approval");
    }
    Ok(())
}
