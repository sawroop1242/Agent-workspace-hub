use serde::{Deserialize, Serialize};

/// Lifecycle state of an [`Agent`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Registered but not yet running.
    Created,
    /// Currently running.
    Active,
    /// Running but temporarily suspended.
    Paused,
    /// Deliberately halted; will not restart.
    Stopped,
    /// Terminated by an error.
    Failed,
}

impl AgentStatus {
    /// Human-readable label matching the serde wire names.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

/// Whether a missing `enabled` field deserializes as `true` (legacy records).
fn default_enabled() -> bool {
    true
}

/// A named agent identity within the workspace.
///
/// TW-002: this record doubles as the minimal [`AgentProfile`] — the
/// declarative identity/configuration the runtime instantiates sessions
/// from. It deliberately carries no capability or policy fields: a profile
/// can never authorize anything by itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Agent {
    /// Unique agent id.
    pub id: String,
    /// Human-readable agent name.
    pub name: String,
    /// The agent's role (e.g. `researcher`, `reviewer`).
    pub role: String,
    /// Current lifecycle state.
    pub status: AgentStatus,
    /// Whether the profile may serve callers at all. Defaults to `true` so
    /// agents persisted before this field existed keep working unchanged.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}
