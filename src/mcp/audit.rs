//! Structured audit logging for the MCP security boundaries.
//!
//! Every fail-closed denial in the trust/permission/sandbox/transport chain
//! emits a structured audit event so unattended agent operation can be
//! reconstructed and audited. Audit events never include secret values.

/// Emits a structured audit event for a denied security decision.
///
/// `reason` is a stable machine-readable slug; `subject` identifies the
/// affected actor (MCP id, tool name, path, etc.). No secret values are logged.
pub fn audit_deny(action: &str, reason: &str, subject: &str) {
    tracing::warn!(event = "mcp_security_denied", action, reason, subject,);
}

/// Emits a structured audit event for a secret-resolution denial, logging only
/// the secret *name* (never its value).
pub fn audit_secret_deny(reason: &str, name: &str) {
    tracing::warn!(event = "mcp_secret_denied", reason, name,);
}

/// Emits a structured audit event for a circuit-breaker trip.
pub fn audit_circuit_open(provider: &str) {
    tracing::warn!(event = "mcp_circuit_open", provider,);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_helpers_do_not_panic() {
        audit_deny("authorize", "no_approval", "mcp-1");
        audit_secret_deny("not_approved", "my_secret");
        audit_circuit_open("provider-1");
    }
}
