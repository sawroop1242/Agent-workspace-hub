//! Deterministic relevance scoring for context items.
//!
//! The initial scoring policy is fully deterministic and documented — no
//! reinforcement learning in Phase 1 (see the roadmap in
//! [`crate::context::policy`]). The score combines the item's intrinsic
//! signals with task alignment and a token-cost penalty, and every weight is
//! explicit below so behavior can be reasoned about and later replaced by a
//! learned policy without changing call sites.

/// The documented, normalized scoring weights.
///
/// Each input signal is normalized to `[0.0, 1.0]` before weighting; the
/// token-cost penalty grows with the item's share of the usable budget.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ScoreWeights {
    /// Weight of caller-assigned relevance.
    pub relevance: f32,
    /// Weight of caller-assigned priority.
    pub priority: f32,
    /// Weight of recency.
    pub recency: f32,
    /// Weight of task alignment (lexical overlap with the task/query).
    pub task_alignment: f32,
    /// Weight of the token-cost penalty (applied to `1 - normalized_cost`).
    pub cost_weight: f32,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            relevance: 0.35,
            priority: 0.25,
            recency: 0.15,
            task_alignment: 0.15,
            cost_weight: 0.10,
        }
    }
}

/// Clamps a value into `[0.0, 1.0]`.
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// Lexical task alignment between an item and the current task/query text.
///
/// Deterministic Jaccard-style overlap over lowercase word sets: cheap,
/// dependency-free, and stable. Returns `[0.0, 1.0]`.
pub fn task_alignment(item_content: &str, task: &str) -> f32 {
    let task_lower = task.to_lowercase();
    let item_lower = item_content.to_lowercase();
    let task_words: std::collections::HashSet<&str> = task_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();
    if task_words.is_empty() {
        return 0.0;
    }
    let item_words: std::collections::HashSet<&str> = item_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();
    if item_words.is_empty() {
        return 0.0;
    }
    let shared = task_words.intersection(&item_words).count();
    shared as f32 / task_words.len() as f32
}

/// Scores a single item deterministically.
///
/// Returns a value in `[0.0, 1.0]`; higher is better. `usable_tokens` is the
/// budget's usable window, used to normalize the token cost.
pub fn score_item(
    item: &crate::context::item::ContextItem,
    task: &str,
    usable_tokens: usize,
    weights: &ScoreWeights,
) -> f32 {
    let relevance = clamp01(item.relevance);
    let priority = clamp01(item.priority);
    let recency = clamp01(item.recency);
    let alignment = task_alignment(&item.content, task);

    // Normalized cost: an item consuming its whole usable budget scores 1.0.
    let cost = if usable_tokens == 0 {
        1.0
    } else {
        clamp01(item.token_count as f32 / usable_tokens as f32)
    };

    let score = weights.relevance * relevance
        + weights.priority * priority
        + weights.recency * recency
        + weights.task_alignment * alignment
        + weights.cost_weight * (1.0 - cost);
    clamp01(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::item::{ContextItem, ContextSource};

    fn item(content: &str, relevance: f32, priority: f32, recency: f32) -> ContextItem {
        ContextItem {
            relevance,
            priority,
            recency,
            ..ContextItem::new("id", ContextSource::File, content, 10)
        }
    }

    #[test]
    fn score_is_clamped_to_unit_range() {
        let weights = ScoreWeights::default();
        let it = item("content", 1.0, 1.0, 1.0);
        let score = score_item(&it, "content", 1000, &weights);
        assert!((0.0..=1.0).contains(&score));
        let it = item("content", 7.0, -3.0, 99.0); // out-of-range inputs
        let score = score_item(&it, "content", 1000, &weights);
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn high_relevance_beats_low_relevance() {
        let weights = ScoreWeights::default();
        let high = item("database connection settings", 0.9, 0.5, 0.5);
        let low = item("unrelated rambling", 0.1, 0.5, 0.5);
        assert!(
            score_item(&high, "database", 1000, &weights)
                > score_item(&low, "database", 1000, &weights)
        );
    }

    #[test]
    fn task_alignment_rewards_lexical_overlap() {
        assert!(task_alignment("fix the login bug in auth", "fix login bug") > 0.0);
        assert_eq!(task_alignment("completely different", "fix login bug"), 0.0);
        // Empty task or item means no alignment signal.
        assert_eq!(task_alignment("anything", ""), 0.0);
        assert_eq!(task_alignment("", "some task"), 0.0);
    }

    #[test]
    fn alignment_ignores_short_words() {
        // Words of 1-2 characters are filtered as stopword-length tokens.
        assert_eq!(task_alignment("to go", "so far"), 0.0);
        assert_eq!(task_alignment("ab cd", "cd ab"), 0.0);
    }

    #[test]
    fn expensive_items_are_penalized() {
        let weights = ScoreWeights::default();
        let cheap = ContextItem::new("a", ContextSource::File, "relevant content", 10);
        let expensive = ContextItem::new("b", ContextSource::File, "relevant content", 10_000);
        assert!(
            score_item(&cheap, "relevant", 10_000, &weights)
                > score_item(&expensive, "relevant", 10_000, &weights)
        );
    }

    #[test]
    fn zero_usable_budget_treats_every_item_as_max_cost() {
        let weights = ScoreWeights::default();
        let small = ContextItem::new("a", ContextSource::File, "c", 1);
        let large = ContextItem::new("b", ContextSource::File, "c", 1_000_000);
        assert_eq!(
            score_item(&small, "", 0, &weights),
            score_item(&large, "", 0, &weights)
        );
    }

    #[test]
    fn recency_contributes() {
        let weights = ScoreWeights::default();
        let fresh = item("same content", 0.5, 0.5, 1.0);
        let stale = item("same content", 0.5, 0.5, 0.0);
        assert!(score_item(&fresh, "", 1000, &weights) > score_item(&stale, "", 1000, &weights));
    }
}
