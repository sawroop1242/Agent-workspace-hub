use serde::{Deserialize, Serialize};

/// A single timestamped memory record persisted as JSONL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    /// RFC 3339 timestamp of the record.
    pub timestamp: String,
    /// The memory content.
    pub content: String,
}
