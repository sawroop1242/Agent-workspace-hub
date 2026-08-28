//! Structured error types for the MCP security/authorization boundary.
//!
//! The execution gate distinguishes fail-closed reasons so callers can make
//! precise policy decisions (for example, treating an unknown-id denial
//! differently from a mismatch) without parsing strings.

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
