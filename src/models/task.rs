use serde::{Deserialize, Serialize};

/// Lifecycle state of a [`Task`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Not yet started.
    Pending,
    /// Currently being worked on.
    InProgress,
    /// Finished successfully.
    Completed,
    /// No longer relevant.
    Cancelled,
}

/// A unit of work tracked within a project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    /// Unique task id.
    pub id: String,
    /// Short human-readable summary.
    pub title: String,
    /// Current lifecycle state.
    pub status: TaskStatus,
}
