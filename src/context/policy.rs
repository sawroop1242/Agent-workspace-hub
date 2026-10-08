//! The replaceable context decision policy.
//!
//! Roadmap (mirroring the upstream research architecture without porting its
//! training stack):
//!
//! - **Phase 1 (now):** [`DeterministicContextPolicy`] — scored, documented,
//!   dependency-free decisions.
//! - **Phase 2:** `ExternalModelContextPolicy` — same trait, decisions
//!   delegated to a configured model endpoint.
//! - **Phase 3:** `LearnedContextPolicy` — decisions from a trained model;
//!   the decision metadata recorded by the engine (see
//!   [`crate::context::engine`]) is shaped to be usable as training signal.
//!
//! No RL training, vLLM, Elasticsearch, or Python is required or included.

use crate::context::budget::ContextBudget;
use crate::context::item::ContextItem;
use crate::context::scoring::{score_item, ScoreWeights};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A single explicit context-management action.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ContextAction {
    /// Keep the item in active context.
    Keep,
    /// Move the item from active to offloaded (content preserved).
    Offload,
    /// Move an offloaded item back to active.
    Restore,
    /// Replace active content with a lossless compressed representation.
    Compress,
    /// Move the item to archived (still inspectable, not active).
    Archive,
    /// Leave the item out of the result set entirely.
    Ignore,
}

/// One policy decision: what to do with one item, and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDecision {
    /// The item the decision applies to.
    pub id: String,
    /// The chosen action.
    pub action: ContextAction,
    /// The item's score at decision time.
    pub score: f32,
    /// Human-readable reason (observability and future RL telemetry).
    pub reason: String,
    /// The item's token cost.
    pub token_cost: usize,
    /// The item's priority.
    pub priority: f32,
}

/// The policy abstraction: turns scored items into explicit actions.
///
/// Implementations must be deterministic unless they intentionally delegate
/// to an external model. [`DeterministicContextPolicy`] is the Phase 1
/// implementation.
pub trait ContextPolicy: Send + Sync {
    /// Decides the action for every active item given the current task.
    fn decide(
        &self,
        items: &[ContextItem],
        task: &str,
        budget: &ContextBudget,
        protected_ids: &HashSet<String>,
    ) -> Vec<ContextDecision>;
}

/// The deterministic Phase 1 policy.
///
/// Decision rules, in order:
///
/// 1. **Protected** items are always `Keep`.
/// 2. Items already `Offloaded`/`Archived` are `Keep` in place (the engine
///    restores explicitly, never implicitly).
/// 3. Items scoring at or above `keep_threshold` are `Keep`.
/// 4. Items scoring below `offload_threshold` are `Offload` (soft — content
///    preserved and restorable).
/// 5. Expensive mid-score items are `Compress` (lossless compression keeps
///    the information but lowers cost).
/// 6. Remaining low-value items are `Archive`.
///
/// Between the thresholds, "cheap" means `token_count <= cheap_token_limit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicContextPolicy {
    /// Scores at or above this are always kept.
    pub keep_threshold: f32,
    /// Scores below this are offloaded.
    pub offload_threshold: f32,
    /// Items at or under this token count are considered cheap.
    pub cheap_token_limit: usize,
    /// Scoring weights (documented defaults; see [`ScoreWeights`]).
    pub weights: ScoreWeights,
}

impl Default for DeterministicContextPolicy {
    fn default() -> Self {
        Self {
            keep_threshold: 0.45,
            offload_threshold: 0.15,
            cheap_token_limit: 512,
            weights: ScoreWeights::default(),
        }
    }
}

impl ContextPolicy for DeterministicContextPolicy {
    fn decide(
        &self,
        items: &[ContextItem],
        task: &str,
        budget: &ContextBudget,
        protected_ids: &HashSet<String>,
    ) -> Vec<ContextDecision> {
        let usable = budget.usable_input_tokens();
        let mut scored: Vec<(f32, &ContextItem)> = items
            .iter()
            .map(|item| (score_item(item, task, usable, &self.weights), item))
            .collect();
        // Deterministic ordering: score descending, then id.
        scored.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .reverse()
                .then_with(|| a.1.id.cmp(&b.1.id))
        });

        scored
            .into_iter()
            .map(|(score, item)| {
                let (action, reason) = if protected_ids.contains(&item.id) {
                    (ContextAction::Keep, "protected item".to_string())
                } else if !item.state.is_active() {
                    (
                        ContextAction::Keep,
                        "item is inactive; restore explicitly".to_string(),
                    )
                } else if score >= self.keep_threshold {
                    (
                        ContextAction::Keep,
                        format!("score {score:.3} >= keep threshold"),
                    )
                } else if score < self.offload_threshold {
                    (
                        ContextAction::Offload,
                        format!("score {score:.3} < offload threshold; soft offload"),
                    )
                } else if item.token_count > self.cheap_token_limit {
                    (
                        ContextAction::Compress,
                        format!(
                            "mid score {score:.3} and {token} tokens > cheap limit {limit}; compress",
                            token = item.token_count,
                            limit = self.cheap_token_limit
                        ),
                    )
                } else {
                    (
                        ContextAction::Archive,
                        format!(
                            "low score {score:.3} but small ({token} tokens); archive",
                            token = item.token_count
                        ),
                    )
                };
                ContextDecision {
                    id: item.id.clone(),
                    action,
                    score,
                    reason,
                    token_cost: item.token_count,
                    priority: item.priority,
                }
            })
            .collect()
    }
}

/// Convenience: maps decisions by item id.
pub fn decisions_by_id(decisions: &[ContextDecision]) -> HashMap<String, &ContextDecision> {
    decisions.iter().map(|d| (d.id.clone(), d)).collect()
}

/// A test-only helper asserting every returned decision id exists in items.
#[cfg(test)]
fn assert_all_decisions_cover_items(decisions: &[ContextDecision], items: &[ContextItem]) {
    let mut decision_ids: HashSet<String> = decisions.iter().map(|d| d.id.clone()).collect();
    for item in items {
        assert!(decision_ids.remove(&item.id), "no decision for {}", item.id);
    }
    assert!(
        decision_ids.is_empty(),
        "decisions for unknown ids: {decision_ids:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::item::{ContextItem, ContextSource, ContextState};

    fn policy() -> DeterministicContextPolicy {
        DeterministicContextPolicy::default()
    }

    fn budget(usable: usize) -> ContextBudget {
        ContextBudget {
            max_input_tokens: usable,
            reserved_output_tokens: 0,
            safety_margin_tokens: 0,
        }
    }

    fn item(id: &str, relevance: f32, tokens: usize) -> ContextItem {
        // Content deliberately lexical-disjoint from the test task so scores
        // are driven by the relevance hint, not task alignment.
        let mut item = ContextItem::new(id, ContextSource::Tool, "zzz qqq xxx", tokens);
        item.relevance = relevance;
        item.priority = 0.5;
        item.recency = 0.5;
        item
    }

    #[test]
    fn protected_items_are_kept_regardless_of_score() {
        let items = vec![item("p", 0.0, 10_000)];
        let protected = HashSet::from(["p".to_string()]);
        let decisions = policy().decide(&items, "unrelated", &budget(1000), &protected);
        assert_eq!(decisions[0].action, ContextAction::Keep);
        assert!(decisions[0].reason.contains("protected"));
    }

    #[test]
    fn high_score_is_kept() {
        let items = vec![item("h", 0.9, 100)];
        let decisions = policy().decide(&items, "unrelated", &budget(1000), &HashSet::new());
        assert_eq!(decisions[0].action, ContextAction::Keep);
    }

    #[test]
    fn very_low_score_is_offloaded_with_reason() {
        // Stale, unprioritized, irrelevant, and cheap: every score signal is
        // near zero, so the item lands below the offload threshold.
        let mut junk = item("l", 0.0, 50);
        junk.recency = 0.0;
        junk.priority = 0.0;
        let decisions = policy().decide(&[junk], "unrelated", &budget(1000), &HashSet::new());
        assert_eq!(decisions[0].action, ContextAction::Offload);
        assert!(decisions[0].reason.contains("offload"));
    }

    #[test]
    fn low_score_cheap_item_is_archived() {
        // Same low relevance but small: the cost weight pushes the score
        // just above the offload threshold, so the item is archived (kept out
        // of active context without a durable offload record).
        let items = vec![item("l", 0.01, 10)];
        let decisions = policy().decide(&items, "unrelated", &budget(1000), &HashSet::new());
        assert_eq!(decisions[0].action, ContextAction::Archive);
    }

    #[test]
    fn mid_score_expensive_item_is_compressed() {
        let items = vec![item("m", 0.3, 5_000)];
        let decisions = policy().decide(&items, "unrelated", &budget(1000), &HashSet::new());
        assert_eq!(decisions[0].action, ContextAction::Compress);
    }

    #[test]
    fn mid_score_cheap_item_is_archived() {
        let items = vec![item("m", 0.3, 10)];
        let decisions = policy().decide(&items, "unrelated", &budget(1000), &HashSet::new());
        assert_eq!(decisions[0].action, ContextAction::Archive);
    }

    #[test]
    fn inactive_items_are_kept_in_place() {
        let mut offloaded = item("o", 0.0, 100);
        offloaded.state = ContextState::Offloaded;
        let decisions = policy().decide(&[offloaded], "unrelated", &budget(1000), &HashSet::new());
        assert_eq!(decisions[0].action, ContextAction::Keep);
        assert!(decisions[0].reason.contains("inactive"));
    }

    #[test]
    fn every_item_gets_exactly_one_decision() {
        let items = vec![item("a", 0.9, 10), item("b", 0.2, 10), item("c", 0.01, 10)];
        let decisions = policy().decide(&items, "unrelated", &budget(1000), &HashSet::new());
        assert_all_decisions_cover_items(&decisions, &items);
        assert_eq!(decisions.len(), items.len());
    }

    #[test]
    fn decisions_include_score_token_cost_and_priority() {
        let items = vec![item("x", 0.7, 42)];
        let decisions = policy().decide(&items, "unrelated", &budget(1000), &HashSet::new());
        assert_eq!(decisions[0].token_cost, 42);
        assert_eq!(decisions[0].priority, 0.5);
        assert!(decisions[0].score >= 0.0 && decisions[0].score <= 1.0);
    }

    #[test]
    fn empty_input_yields_no_decisions() {
        let decisions = policy().decide(&[], "unrelated", &budget(1000), &HashSet::new());
        assert!(decisions.is_empty());
    }
}
