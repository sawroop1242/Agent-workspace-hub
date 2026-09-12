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

/// A named agent identity within the workspace.
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
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}
