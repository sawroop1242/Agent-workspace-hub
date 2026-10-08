//! Agent-scoped MCP route resolution (TW-003).
//!
//! The path segment in `/{agent}/sse` and `/{agent}/mcp` is a *routing
//! selector, never authorization*: resolving a route only binds the request
//! to the canonical [`Agent`] record registered for the workspace, which is
//! then resolved with the existing [`SessionIdentity`] machinery into a
//! transport-independent [`CallerContext`]. Bearer authentication and the
//! existing execution gates continue to gate consequential execution exactly
//! as on the unscoped `/mcp`/`/sse` endpoints — the agent route adds the
//! *per-agent* capability boundary on top of the shared ones.
//!
//! Resolution aborts with a structured [`RouteError`] on:
//!
//! * a malformed or unsafe agent id (empty, `.`/`..`, separators, control
//!   characters, over-length input),
//! * an unknown agent,
//! * an agent disabled or not active in this workspace (the canonical
//!   TW-002 relation semantics),
//! * a session key that does not match the resolved route context.

use crate::core::agents::{is_safe_agent_id, AgentStore};
use crate::core::identity::{
    resolve_session_relation, AgentId, IdentityRelation, SessionId, SessionIdentity, WorkspaceId,
};

/// Reason a `/​{agent}` route or session-binding verification failed.
///
/// Codes follow the prompt §19 error vocabulary; each variant maps to a
/// stable `as_str` identifier so the wire surface is machine-readable
/// without exposing internal state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    /// The path segment itself failed structural validation.
    #[error("invalid agent id")]
    InvalidAgentId,
    /// No agent record exists for the id in this workspace.
    #[error("unknown agent")]
    UnknownAgent,
    /// The agent record exists but is disabled or not active in this
    /// workspace.
    #[error("agent inactive or disabled")]
    AgentInactive,
    /// The session id presented does not match the agent context.
    #[error("session not bound to agent")]
    SessionAgentMismatch,
    /// The session id presented does not match the workspace context.
    #[error("session not bound to workspace")]
    SessionWorkspaceMismatch,
    /// No session exists for the id, or it has already been closed.
    #[error("unknown or closed session")]
    UnknownSession,
}

/// Stable, machine-readable reason codes (prompt §19) used in error bodies.
impl RouteError {
    pub fn reason_code(&self) -> &'static str {
        match self {
            RouteError::InvalidAgentId => "invalid_agent_id",
            RouteError::UnknownAgent => "unknown_agent",
            RouteError::AgentInactive => "agent_inactive",
            RouteError::SessionAgentMismatch => "session_agent_mismatch",
            RouteError::SessionWorkspaceMismatch => "session_workspace_mismatch",
            RouteError::UnknownSession => "unknown_session",
        }
    }
}

/// The transport-independent trusted caller context for one bound MCP route.
///
/// Holds only identity primitives — never credentials, never raw transport
/// metadata. Constructed exclusively by [`resolve_route_agent`] on the
/// transport, then consumed by the dispatcher's per-agent capability gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerContext {
    /// The agent resolved from the route.
    pub agent_id: AgentId,
    /// The workspace whose manifest the agent record comes from.
    pub workspace_id: WorkspaceId,
    /// The MCP protocol session the request runs under, when one exists
    /// (always present after SSE session establishment; `None` only while
    /// a handler is still binding the just-created session).
    pub session_id: Option<SessionId>,
}

impl CallerContext {
    /// Builds the authoritative [`SessionIdentity`] for this caller once
    /// the MCP session id is known (after SSE session creation).
    pub fn to_session_identity(&self, session_id: SessionId) -> SessionIdentity {
        SessionIdentity {
            session_id,
            agent_id: self.agent_id.clone(),
            workspace_id: self.workspace_id.clone(),
        }
    }
}

/// Validates the raw route path segment and resolves the canonical agent
/// record for the workspace whose `.agent` directory `root` contains.
///
/// This is the single authoritative route-resolution entry point for both
/// `/{agent}/sse` and `/{agent}/mcp`: it never accepts a client-supplied
/// workspace, treats the segment purely as a lookup key against the agent
/// store, and fails closed (agent must exist, be enabled, *and* be active
/// in this workspace). The activation/enabled/workspace semantics are the
/// canonical ones owned by [`resolve_session_relation`]
/// (TW-002) — this function is a thin transport adapter over them, not a
/// second authority. The returned [`CallerContext`] carries the workspace
/// id of the resolved workspace so downstream binding checks never
/// re-derive identity from the raw request.
pub fn resolve_route_agent(
    root: &std::path::Path,
    segment: &str,
) -> Result<(CallerContext, crate::models::Agent), RouteError> {
    // Structural validation first (is_safe_agent_id enforces the canonical
    // MAX_AGENT_ID_LEN, separators, control chars, `.`/`..`, traversal).
    if !is_safe_agent_id(segment) {
        return Err(RouteError::InvalidAgentId);
    }

    let store = AgentStore::new(root);
    let Some(agent) = store.get(segment).map_err(|_| RouteError::InvalidAgentId)? else {
        return Err(RouteError::UnknownAgent);
    };

    // The canonical activation check (TW-002): disabled and non-active
    // agents fail closed with the workspace's own relation semantics, so
    // this module can never accept an agent the runtime service would
    // reject. Disabled and inactive map to the same route-level response
    // (they both mean "this route may not serve callers"), but the audit
    // log records the exact relation.
    let workspace = crate::services::init::load_workspace_manifest(root)
        .map_err(|_| RouteError::InvalidAgentId)?;
    let identity = SessionIdentity::new(
        AgentId::new_checked(agent.id.clone()).map_err(|_| RouteError::InvalidAgentId)?,
        workspace.workspace_id.clone(),
    );
    match resolve_session_relation(root, &identity).map_err(|_| RouteError::UnknownAgent)? {
        IdentityRelation::AgentActiveInWorkspace => {}
        relation => {
            audit_deny_route_relation(segment, &relation);
            return Err(RouteError::AgentInactive);
        }
    }

    let caller = CallerContext {
        agent_id: identity.agent_id().clone(),
        workspace_id: identity.workspace_id().clone(),
        session_id: None,
    };
    Ok((caller, agent))
}

/// Records the exact relation that closed a route resolution. Only the
/// relation name reaches the audit log — never agent metadata or request
/// payloads.
fn audit_deny_route_relation(segment: &str, relation: &IdentityRelation) {
    crate::mcp::audit_deny("agent_route_resolve", relation_name(relation), segment);
}

/// Stable audit name for an identity relation.
fn relation_name(relation: &IdentityRelation) -> &'static str {
    match relation {
        IdentityRelation::AgentActiveInWorkspace => "agent_active",
        IdentityRelation::AgentDisabled => "agent_disabled",
        IdentityRelation::AgentInactive => "agent_inactive",
        IdentityRelation::UnknownAgent => "unknown_agent",
        IdentityRelation::WrongWorkspace => "wrong_workspace",
    }
}

/// Verifies that an existing SSE-session binding matches the caller context
/// of the current request route. A session created for agent A under
/// workspace X must never answer over agent B's route or workspace Y.
pub fn verify_session_binding(
    binding: &SessionIdentity,
    caller: &CallerContext,
) -> Result<(), RouteError> {
    if binding.agent_id != caller.agent_id {
        return Err(RouteError::SessionAgentMismatch);
    }
    if binding.workspace_id != caller.workspace_id {
        return Err(RouteError::SessionWorkspaceMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Agent, AgentStatus};

    fn active_agent(store: &AgentStore, id: &str) {
        let agent = Agent {
            id: id.to_string(),
            name: id.to_string(),
            role: "writer".to_string(),
            status: AgentStatus::Active,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        store.create(&agent).unwrap();
    }

    #[test]
    fn resolves_active_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        crate::services::init::initialize_workspace(root).unwrap();
        active_agent(&AgentStore::new(root), "claude");
        let (caller, agent) = resolve_route_agent(root, "claude").unwrap();
        assert_eq!(agent.id, "claude");
        assert_eq!(caller.agent_id.as_str(), "claude");
        assert!(caller.session_id.is_none());
    }

    #[test]
    fn rejects_unknown_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        crate::services::init::initialize_workspace(root).unwrap();
        assert_eq!(
            resolve_route_agent(root, "ghost").unwrap_err(),
            RouteError::UnknownAgent
        );
    }

    #[test]
    fn rejects_inactive_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        crate::services::init::initialize_workspace(root).unwrap();
        let store = AgentStore::new(root);
        let agent = Agent {
            id: "idle".to_string(),
            name: "idle".to_string(),
            role: "writer".to_string(),
            status: AgentStatus::Created,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        store.create(&agent).unwrap();
        assert_eq!(
            resolve_route_agent(root, "idle").unwrap_err(),
            RouteError::AgentInactive
        );
    }

    /// TW-002 added `enabled` as a deliberate operator kill-switch that is
    /// distinct from lifecycle state: a disabled profile must never serve
    /// callers, even while its status remains `Active`.
    #[test]
    fn rejects_disabled_agent_even_when_active() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        crate::services::init::initialize_workspace(root).unwrap();
        let store = AgentStore::new(root);
        let agent = Agent {
            id: "off".to_string(),
            name: "off".to_string(),
            role: "writer".to_string(),
            status: AgentStatus::Active,
            enabled: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        store.create(&agent).unwrap();
        assert_eq!(
            resolve_route_agent(root, "off").unwrap_err(),
            RouteError::AgentInactive
        );
    }

    #[test]
    fn rejects_malformed_id() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        crate::services::init::initialize_workspace(root).unwrap();
        for seg in [
            "",
            "..",
            ".",
            "a/b",
            "a\\b",
            "a\0b",
            // Over the canonical MAX_AGENT_ID_LEN (64): both a slightly
            // over-long id and an obviously absurd one.
            "x".repeat(65).as_str(),
            "x".repeat(129).as_str(),
        ] {
            assert!(
                resolve_route_agent(root, seg).is_err(),
                "segment {seg:?} must be rejected"
            );
        }
    }
}
