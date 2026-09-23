use serde::{Deserialize, Serialize};

/// Lifecycle state of an [`AgentSessionRecord`] — the AWH-native runtime
/// session (TW-002). This is deliberately distinct from the MCP protocol
/// [`SessionLifecycle`](crate::mcp::dispatcher::SessionLifecycle): a protocol
/// session tracks one transport connection; an AWH session is the
/// application-level identity binding one caller to one agent within one
/// workspace.
///
/// Terminal states (`stopped`, `failed`) are never usable: presenting them
/// always fails closed; sessions are never auto-reactivated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Opened and usable.
    Active,
    /// Temporarily suspended; can be resumed.
    Paused,
    /// Terminated deliberately; terminal state.
    Stopped,
    /// Terminated by an error; terminal state.
    Failed,
}

impl SessionStatus {
    /// Whether a session in this state may serve as a caller identity.
    /// `active` and `paused` resolve (paused callers must resume before
    /// consequential use is expected); terminal states never do.
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Active | Self::Paused)
    }

    /// Human-readable label matching the serde wire names (CLI output).
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

/// One AWH runtime session: the association between an external caller, one
/// agent, and exactly one workspace (TW-002 §7).
///
/// Identity only — never authority: a session proves *who* the caller
/// resolved to; every consequential operation still has to pass the
/// capability/policy boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionRecord {
    /// Typed session identity (`sess-<nanos>-<pid>-<seq>`).
    pub session_id: String,
    /// The agent that owns this session.
    pub agent_id: String,
    /// The workspace this session is bound to (workspace manifest identity).
    pub workspace_id: String,
    /// Current lifecycle state.
    pub status: SessionStatus,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 timestamp of the last lifecycle transition.
    pub last_activity_at: String,
}
