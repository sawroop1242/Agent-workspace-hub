use serde::{Deserialize, Serialize};

/// A single timestamped memory record persisted as JSONL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    pub timestamp: String,
    pub content: String,
}
