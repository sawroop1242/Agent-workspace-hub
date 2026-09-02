//! The context item: the engine's unit of context.
//!
//! An item is a piece of information with a source, a token cost, and a
//! lifecycle state. Items are inserted by callers (the MCP surface, the CLI)
//! and then scored, selected, compressed, offloaded, restored, and snapshotted
//! by the rest of the engine.

use serde::{Deserialize, Serialize};

/// Where a context item's content came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ContextSource {
    /// Core system-level context.
    System,
    /// A user message.
    User,
    /// An assistant message.
    Assistant,
    /// A tool invocation or its result.
    Tool,
    /// Skill content.
    Skill,
    /// Content read from a workspace file.
    File,
    /// Content recalled from long-term memory.
    Memory,
    /// Workspace-level context (project instructions, listings).
    Workspace,
    /// Content found by a search.
    Search,
    /// A generated summary of other content.
    Summary,
    /// Anything else.
    Other,
}

/// The lifecycle state of a context item.
///
/// The engine only ever moves items `Active -> Offloaded -> Active` (or into
/// `Archived` when explicitly requested); an offloaded item's content is
/// durably preserved and never silently deleted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ContextState {
    /// Currently intended for the LLM.
    Active,
    /// Inactive but durably stored and fully restorable.
    Offloaded,
    /// Explicitly archived; still inspectable, not part of active context.
    Archived,
}

impl ContextState {
    /// Whether this state counts as "active context" for budgeting.
    pub fn is_active(self) -> bool {
        matches!(self, ContextState::Active)
    }
}

/// Which workspace scope an item belongs to.
///
/// `Project` items are only visible to their own project; `Global` items are
/// shared across projects (a small, explicit set such as cross-project
/// conventions). The default is `Project`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextScope {
    /// Scoped to the current project (default).
    #[default]
    Project,
    /// Scoped to the current session only.
    Session,
    /// Visible across projects.
    Global,
}

/// A single unit of context managed by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    /// Stable, caller-supplied unique id (validated by the engine).
    pub id: String,
    /// What produced the item's content.
    pub source: ContextSource,
    /// The item's content, in tokens when active.
    pub content: String,
    /// Token cost of the content, as estimated by the engine's counter.
    pub token_count: usize,
    /// Caller-assigned relevance hint in `[0.0, 1.0]`.
    pub relevance: f32,
    /// Caller-assigned priority in `[0.0, 1.0]` (higher = more important).
    pub priority: f32,
    /// Recency in `[0.0, 1.0]` (1.0 = just seen).
    pub recency: f32,
    /// Lifecycle state.
    pub state: ContextState,
    /// Scope isolation for the item.
    pub scope: ContextScope,
    /// Arbitrary caller metadata (never trusted by the engine).
    pub metadata: serde_json::Value,
}

impl ContextItem {
    /// Builds a new active item, computing `token_count` from `content`.
    pub fn new(
        id: impl Into<String>,
        source: ContextSource,
        content: impl Into<String>,
        token_count: usize,
    ) -> Self {
        Self {
            id: id.into(),
            source,
            content: content.into(),
            token_count,
            relevance: 0.5,
            priority: 0.5,
            recency: 1.0,
            state: ContextState::Active,
            scope: ContextScope::Project,
            metadata: serde_json::Value::Null,
        }
    }
}

/// Validates a context item id: non-empty, at most 256 bytes, and restricted
/// to a filename-safe character set so ids can never traverse paths.
pub fn is_valid_item_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && id != "."
        && id != ".."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_ids_reject_traversal_and_empty() {
        assert!(is_valid_item_id("a"));
        assert!(is_valid_item_id("tool-1_2.3"));
        assert!(!is_valid_item_id(""));
        assert!(!is_valid_item_id(".."));
        assert!(!is_valid_item_id("."));
        assert!(!is_valid_item_id("a/b"));
        assert!(!is_valid_item_id("a\\b"));
        assert!(!is_valid_item_id("has space"));
        assert!(!is_valid_item_id("é"));
    }

    #[test]
    fn item_ids_reject_oversized() {
        let oversized = "i".repeat(257);
        assert!(!is_valid_item_id(&oversized));
    }

    #[test]
    fn new_item_defaults_to_active() {
        let item = ContextItem::new("id", ContextSource::Tool, "content", 3);
        assert_eq!(item.state, ContextState::Active);
        assert!(item.state.is_active());
        assert_eq!(item.scope, ContextScope::Project);
        assert_eq!(item.relevance, 0.5);
    }

    #[test]
    fn states_and_sources_round_trip_through_serde() {
        let item = ContextItem::new("id", ContextSource::Memory, "content", 1);
        let serialized = serde_json::to_string(&item).unwrap();
        assert!(serialized.contains("\"active\""));
        assert!(serialized.contains("\"memory\""));
        assert!(serialized.contains("\"project\""));
        let deserialized: ContextItem = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.source, ContextSource::Memory);
        assert_eq!(deserialized.state, ContextState::Active);
    }

    #[test]
    fn offloaded_state_is_not_active() {
        assert!(!ContextState::Offloaded.is_active());
        assert!(!ContextState::Archived.is_active());
    }
}
