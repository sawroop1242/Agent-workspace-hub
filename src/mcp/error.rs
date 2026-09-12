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
            | Self::PermissionDenied { tool, .. }
            | Self::TrustLevelDenied { tool, .. } => tool,
        }
    }
}
