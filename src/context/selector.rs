//! Budget-constrained context selection.
//!
//! Given a scored set of candidate items, the selector picks the highest-value
//! subset that fits the usable budget. Protected items are always kept (even
//! if that overflows — protected content must never be dropped); everything
//! else is greedily packed in score order, and anything that does not fit is
//! reported as rejected so callers can offload it explicitly (never silently
//! discard it).

use crate::context::budget::ContextBudget;
use crate::context::item::ContextItem;
use serde::Serialize;

/// The result of a selection pass.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SelectionOutcome {
    /// The ids kept in active context, in final assembly order.
    pub kept_ids: Vec<String>,
    /// Ids rejected because the budget was exhausted, lowest score first.
    pub rejected_ids: Vec<String>,
    /// Total tokens of the kept items.
    pub kept_tokens: usize,
    /// The usable budget the selection targeted.
    pub usable_tokens: usize,
}

/// Selects the highest-scoring subset of `items` that fits the budget.
///
/// Items whose ids appear in `protected_ids` are always selected first and are
/// exempt from the budget (protected content is never dropped by the engine).
/// Remaining items are packed greedily in descending score order, preserving
/// insertion order for ties so selection is fully deterministic.
pub fn select_within_budget(
    items: &[ContextItem],
    scores: &std::collections::HashMap<String, f32>,
    budget: &ContextBudget,
    protected_ids: &std::collections::HashSet<String>,
) -> SelectionOutcome {
    let usable = budget.usable_input_tokens();

    let mut kept_ids: Vec<String> = Vec::new();
    let mut rejected_ids: Vec<String> = Vec::new();
    let mut kept_tokens = 0usize;

    // Protected items first, in original order.
    for item in items {
        if protected_ids.contains(&item.id) {
            kept_ids.push(item.id.clone());
            kept_tokens += item.token_count;
        }
    }

    // Everyone else, best-score first; stable for equal scores.
    let mut candidates: Vec<&ContextItem> = items
        .iter()
        .filter(|i| !protected_ids.contains(&i.id))
        .collect();
    candidates.sort_by(|a, b| {
        scores
            .get(&b.id)
            .unwrap_or(&0.0)
            .partial_cmp(scores.get(&a.id).unwrap_or(&0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });

    for item in candidates {
        if kept_tokens + item.token_count <= usable {
            kept_ids.push(item.id.clone());
            kept_tokens += item.token_count;
        } else {
            rejected_ids.push(item.id.clone());
        }
    }

    // Rejected items are reported in ascending score order (worst first) so
    // the cheapest-to-drop candidates are obvious.
    rejected_ids.sort_by_key(|id| {
        let score = scores.get(id).copied().unwrap_or(0.0);
        // bits-ordered float key: deterministic and total.
        score.to_bits()
    });

    SelectionOutcome {
        kept_ids,
        rejected_ids,
        kept_tokens,
        usable_tokens: usable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::item::{ContextItem, ContextSource};
    use std::collections::{HashMap, HashSet};

    fn item(id: &str, tokens: usize) -> ContextItem {
        ContextItem::new(id, ContextSource::Tool, "content", tokens)
    }

    fn budget(usable: usize) -> ContextBudget {
        ContextBudget {
            max_input_tokens: usable,
            reserved_output_tokens: 0,
            safety_margin_tokens: 0,
        }
    }

    #[test]
    fn everything_fits_when_under_budget() {
        let items = vec![item("a", 10), item("b", 20), item("c", 30)];
        let scores = HashMap::from([("a".into(), 0.9), ("b".into(), 0.8), ("c".into(), 0.7)]);
        let outcome = select_within_budget(&items, &scores, &budget(1000), &HashSet::new());
        assert_eq!(outcome.kept_ids, vec!["a", "b", "c"]);
        assert!(outcome.rejected_ids.is_empty());
        assert_eq!(outcome.kept_tokens, 60);
    }

    #[test]
    fn high_relevance_wins_over_low() {
        let items = vec![item("low", 10), item("high", 10)];
        let scores = HashMap::from([("low".into(), 0.1), ("high".into(), 0.9)]);
        let outcome = select_within_budget(&items, &scores, &budget(15), &HashSet::new());
        assert_eq!(outcome.kept_ids, vec!["high"]);
        assert_eq!(outcome.rejected_ids, vec!["low"]);
    }

    #[test]
    fn budget_is_respected() {
        let items = vec![item("a", 50), item("b", 40), item("c", 30)];
        let scores = HashMap::from([("a".into(), 0.9), ("b".into(), 0.8), ("c".into(), 0.7)]);
        let outcome = select_within_budget(&items, &scores, &budget(100), &HashSet::new());
        // a (50) + b (40) = 90 fit; c (30) would overflow.
        assert_eq!(outcome.kept_ids, vec!["a", "b"]);
        assert_eq!(outcome.rejected_ids, vec!["c"]);
        assert_eq!(outcome.kept_tokens, 90);
        assert!(outcome.kept_tokens <= outcome.usable_tokens);
    }

    #[test]
    fn protected_items_are_always_kept() {
        let items = vec![item("vital", 500), item("a", 30)];
        let scores = HashMap::from([("vital".into(), 0.05), ("a".into(), 0.9)]);
        let protected = HashSet::from(["vital".to_string()]);
        let outcome = select_within_budget(&items, &scores, &budget(100), &protected);
        assert!(outcome.kept_ids.contains(&"vital".to_string()));
        // Protected items may exceed the budget by design.
        assert!(outcome.kept_tokens > outcome.usable_tokens);
        assert_eq!(outcome.rejected_ids, vec!["a"]);
    }

    #[test]
    fn ties_preserve_deterministic_id_order() {
        let items = vec![item("c", 10), item("a", 10), item("b", 10)];
        let scores = HashMap::from([("a".into(), 0.5), ("b".into(), 0.5), ("c".into(), 0.5)]);
        let first = select_within_budget(&items, &scores, &budget(1000), &HashSet::new());
        let second = select_within_budget(&items, &scores, &budget(1000), &HashSet::new());
        assert_eq!(first, second);
        assert_eq!(first.kept_ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn empty_input_selects_nothing() {
        let outcome = select_within_budget(&[], &HashMap::new(), &budget(100), &HashSet::new());
        assert!(outcome.kept_ids.is_empty());
        assert_eq!(outcome.kept_tokens, 0);
    }

    #[test]
    fn expensive_irrelevant_context_is_deprioritized() {
        // High score but enormous cost that cannot fit at all.
        let items = vec![item("huge", 10_000), item("small", 10)];
        let scores = HashMap::from([("huge".into(), 0.99), ("small".into(), 0.10)]);
        let outcome = select_within_budget(&items, &scores, &budget(100), &HashSet::new());
        assert!(outcome.kept_ids.contains(&"small".to_string()));
        assert!(outcome.rejected_ids.contains(&"huge".to_string()));
    }
}
