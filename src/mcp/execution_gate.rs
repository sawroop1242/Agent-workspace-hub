use super::{audit_deny, can_enable, McpAuthorizationError, McpPermissions, TrustStore};

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
pub fn authorize(
    request: &McpExecutionRequest<'_>,
    trust: &TrustStore,
) -> Result<(), McpAuthorizationError> {
    if request.id.trim().is_empty() {
        return Err(McpAuthorizationError::MissingId);
    }
    let approval = trust.get(request.id);
    if approval.is_none() {
        audit_deny("authorize_mcp_execution", "no_approval", request.id);
        return Err(McpAuthorizationError::NoApproval {
            id: request.id.to_string(),
        });
    }
    if !can_enable(approval, request.permissions, request.version) {
        audit_deny("authorize_mcp_execution", "trust_mismatch", request.id);
        return Err(McpAuthorizationError::Mismatch {
            id: request.id.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{McpPermissions, TrustLevel};

    fn request<'a>(id: &'a str, permissions: &'a McpPermissions) -> McpExecutionRequest<'a> {
        McpExecutionRequest {
            id,
            version: "1.0",
            permissions,
        }
    }

    #[test]
    fn empty_id_is_missing_id() {
        let trust = TrustStore::default();
        let perms = McpPermissions::default();
        assert_eq!(
            authorize(&request("  ", &perms), &trust),
            Err(McpAuthorizationError::MissingId)
        );
    }

    #[test]
    fn unknown_id_is_no_approval() {
        let mut trust = TrustStore::default();
        trust
            .approve(
                "known",
                TrustLevel::Trusted,
                McpPermissions::default(),
                "1.0",
            )
            .unwrap();
        let perms = McpPermissions::default();
        assert_eq!(
            authorize(&request("missing", &perms), &trust),
            Err(McpAuthorizationError::NoApproval {
                id: "missing".to_string()
            })
        );
    }

    #[test]
    fn mismatched_permissions_are_mismatch() {
        let mut trust = TrustStore::default();
        trust
            .approve(
                "server",
                TrustLevel::Trusted,
                McpPermissions::default(),
                "1.0",
            )
            .unwrap();
        // Request network access that was not approved.
        let perms = McpPermissions {
            network: true,
            ..McpPermissions::default()
        };
        assert_eq!(
            authorize(&request("server", &perms), &trust),
            Err(McpAuthorizationError::Mismatch {
                id: "server".to_string()
            })
        );
    }

    #[test]
    fn matching_approval_is_allowed() {
        let mut trust = TrustStore::default();
        trust
            .approve(
                "server",
                TrustLevel::Trusted,
                McpPermissions::default(),
                "1.0",
            )
            .unwrap();
        let perms = McpPermissions::default();
        assert_eq!(authorize(&request("server", &perms), &trust), Ok(()));
    }
}
