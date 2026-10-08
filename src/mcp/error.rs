//! Structured error types for the MCP security/authorization boundary.
//!
//! The execution gate distinguishes fail-closed reasons so callers can make
//! precise policy decisions (for example, treating an unknown-id denial
//! differently from a mismatch) without parsing strings.

use super::permissions::Permission;
use thiserror::Error;

/// Authorization failure for an MCP execution request, failing closed with a
/// specific, distinguishable cause.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum McpAuthorizationError {
    /// The request carried an empty or whitespace-only MCP id.
    #[error("MCP id is required")]
    MissingId,

    /// No approval exists in the trust store for the requested MCP id.
    #[error("MCP execution denied: no approval for {id}")]
    NoApproval { id: String },

    /// An approval exists but does not match the requested trust, version, or
    /// permissions.
    #[error("MCP execution denied: trust, version, or permissions do not match approval for {id}")]
    Mismatch { id: String },
}

/// Authorization failure for a built-in (static) tool invocation gated under
/// the reserved [`crate::mcp::execution_gate::BUILTIN_TOOL_TRUST_ID`] trust
/// identity, failing closed with a specific, distinguishable cause.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BuiltinToolAuthorizationError {
    /// The tool name has no entry in the tool registry. The registry is the
    /// single source of truth for a tool's risk level and required
    /// permissions, so a name that cannot be resolved cannot be gated and
    /// fails closed rather than executing ungated.
    #[error("built-in tool '{tool}' is not in the tool registry")]
    Unregistered { tool: String },

    /// The trust store could not be loaded (for example a corrupt
    /// `trust.json`), so the built-in gate cannot prove the call is permitted
    /// and denies it.
    #[error("built-in tool '{tool}' denied: trust store unavailable (fail closed)")]
    StoreUnavailable { tool: String },

    /// A High-risk built-in tool was called with no trust record for the
    /// built-in identity at all. SEC-001: High-risk built-ins are
    /// deny-by-default, so absence of a record is denial — the operator must
    /// explicitly grant the required capability first.
    #[error(
        "built-in tool '{tool}' denied: High-risk tools require explicit authorization \
         (no trust record for '{id}'); grant it with: awh mcp trust {id} \
         --network --process --filesystem"
    )]
    AuthorizationRequired { tool: String, id: String },

    /// A trust record for the built-in identity exists, but it does not grant
    /// a permission the tool's registry entry requires.
    #[error("built-in tool '{tool}' denied: permission '{permission}' not granted to {id}")]
    PermissionDenied {
        /// The gated tool name.
        tool: String,
        /// The required permission the approval does not grant.
        permission: Permission,
        /// The reserved trust identity the record was checked against.
        id: String,
    },

    /// A trust record for the built-in identity exists but its trust level
    /// (blocked/unknown) or approved version does not permit built-in tool
    /// execution at all, regardless of individual permissions.
    #[error(
        "built-in tool '{tool}' denied: trust level or approved version for '{id}' \
         does not permit built-in tool execution"
    )]
    TrustLevelDenied { tool: String, id: String },
}

impl BuiltinToolAuthorizationError {
    /// The gated tool this denial is about.
    pub fn tool(&self) -> &str {
        match self {
            Self::Unregistered { tool, .. }
            | Self::StoreUnavailable { tool, .. }
            | Self::AuthorizationRequired { tool, .. }
            | Self::PermissionDenied { tool, .. }
            | Self::TrustLevelDenied { tool, .. } => tool,
        }
    }
}

/// Denial of a built-in tool call by a workspace-local policy rule.
///
/// Unlike [`BuiltinToolAuthorizationError`] (which answers "is this tool
/// category allowed at all for this machine"), this error names the
/// *specific* rule that rejected a *specific* resource (a path or program).
/// It is produced only when the coarse gate has already allowed the call, so
/// a policy denial always narrows — never widens — the trust decision.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyDenialError {
    /// A policy rule matched the call's resource, and its deny applies.
    #[error(
        "'{tool}' denied by policy rule '{rule_id}' matching '{pattern}'{}",
        reason.as_ref().map(|r| format!(": {r}")).unwrap_or_default()
    )]
    Denied {
        /// The gated tool name.
        tool: String,
        /// The id of the rule that matched.
        rule_id: String,
        /// The pattern that matched.
        pattern: String,
        /// Optional human-readable reason, prefixed with `": "` when present.
        reason: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_error_display_messages_are_distinct_and_stable() {
        assert_eq!(
            McpAuthorizationError::MissingId.to_string(),
            "MCP id is required"
        );
        assert_eq!(
            McpAuthorizationError::NoApproval {
                id: "server-a".into()
            }
            .to_string(),
            "MCP execution denied: no approval for server-a"
        );
        assert_eq!(
            McpAuthorizationError::Mismatch {
                id: "server-a".into()
            }
            .to_string(),
            "MCP execution denied: trust, version, or permissions do not match approval for server-a"
        );
    }

    #[test]
    fn authorization_error_equality_distinguishes_variants() {
        let no_approval = McpAuthorizationError::NoApproval {
            id: "server-a".into(),
        };
        let mismatch = McpAuthorizationError::Mismatch {
            id: "server-a".into(),
        };
        assert_ne!(no_approval, mismatch);
        // each constructed value equals an identically-constructed value
        assert_eq!(
            no_approval,
            McpAuthorizationError::NoApproval {
                id: "server-a".into()
            }
        );
    }

    #[test]
    fn builtin_tool_error_tool_returns_gated_tool_for_every_variant() {
        let cases = [
            (
                "workspace.write_file",
                BuiltinToolAuthorizationError::Unregistered {
                    tool: "workspace.write_file".into(),
                },
            ),
            (
                "workspace.delete_file",
                BuiltinToolAuthorizationError::StoreUnavailable {
                    tool: "workspace.delete_file".into(),
                },
            ),
            (
                "terminal.run",
                BuiltinToolAuthorizationError::AuthorizationRequired {
                    tool: "terminal.run".into(),
                    id: "awh-builtin-tools".into(),
                },
            ),
            (
                "workspace.read_file",
                BuiltinToolAuthorizationError::PermissionDenied {
                    tool: "workspace.read_file".into(),
                    permission: Permission::Network,
                    id: "awh-builtin-tools".into(),
                },
            ),
            (
                "filesystem.patch",
                BuiltinToolAuthorizationError::TrustLevelDenied {
                    tool: "filesystem.patch".into(),
                    id: "awh-builtin-tools".into(),
                },
            ),
        ];
        for (expected, error) in &cases {
            assert_eq!(error.tool(), *expected);
        }
    }

    #[test]
    fn builtin_permission_denied_error_is_eq_comparable() {
        // `Permission` must participate in PartialEq so callers can match on
        // the specific missing capability (SEC-001 fail-closed reporting).
        let a = BuiltinToolAuthorizationError::PermissionDenied {
            tool: "terminal.run".into(),
            permission: Permission::Process,
            id: "awh-builtin-tools".into(),
        };
        let b = BuiltinToolAuthorizationError::PermissionDenied {
            tool: "terminal.run".into(),
            permission: Permission::Process,
            id: "awh-builtin-tools".into(),
        };
        assert_eq!(a, b);
        let c = BuiltinToolAuthorizationError::PermissionDenied {
            tool: "terminal.run".into(),
            permission: Permission::Filesystem,
            id: "awh-builtin-tools".into(),
        };
        assert_ne!(a, c);
    }

    #[test]
    fn policy_denial_display_includes_rule_and_optional_reason() {
        let plain = PolicyDenialError::Denied {
            tool: "workspace.write_file".into(),
            rule_id: "deny-env".into(),
            pattern: "src/env/".into(),
            reason: None,
        };
        assert_eq!(
            plain.to_string(),
            "'workspace.write_file' denied by policy rule 'deny-env' matching 'src/env/'"
        );
        let explained = PolicyDenialError::Denied {
            tool: "terminal.run".into(),
            rule_id: "deny-rm".into(),
            pattern: "rm".into(),
            reason: Some("destructive command".into()),
        };
        assert_eq!(
            explained.to_string(),
            "'terminal.run' denied by policy rule 'deny-rm' matching 'rm': destructive command"
        );
        assert_eq!(
            plain,
            PolicyDenialError::Denied {
                tool: "workspace.write_file".into(),
                rule_id: "deny-env".into(),
                pattern: "src/env/".into(),
                reason: None,
            }
        );
    }
}
