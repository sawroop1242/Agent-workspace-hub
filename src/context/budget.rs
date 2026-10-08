//! Explicit context token budget.
//!
//! The budget is the engine's hard contract: the sum of active item tokens
//! plus the reserved output tokens plus the safety margin must never exceed
//! the configured model/context limit. All budget arithmetic happens here so
//! the selector, policy, and MCP surface agree on a single definition.

use serde::{Deserialize, Serialize};

/// An explicit token budget with reserved output and safety margin.
///
/// `max_input_tokens` is the model/context limit (configurable, see
/// [`crate::context::engine::ContextEngineConfig`]); `reserved_output_tokens`
/// is headroom kept for the model's reply; `safety_margin_tokens` absorbs
/// token-estimate error (the engine's counter is approximate).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextBudget {
    /// The model/context token limit.
    pub max_input_tokens: usize,
    /// Tokens reserved for model output.
    pub reserved_output_tokens: usize,
    /// Extra headroom absorbing estimation error.
    pub safety_margin_tokens: usize,
}

impl ContextBudget {
    /// The largest number of input tokens that may be active at once.
    ///
    /// Saturates at zero rather than underflowing when the reservations
    /// consume the entire limit.
    pub fn usable_input_tokens(&self) -> usize {
        self.max_input_tokens
            .saturating_sub(self.reserved_output_tokens)
            .saturating_sub(self.safety_margin_tokens)
    }

    /// Whether `tokens` fit within the usable input budget.
    pub fn fits(&self, tokens: usize) -> bool {
        tokens <= self.usable_input_tokens()
    }

    /// Remaining usable input tokens given `tokens` already active.
    pub fn remaining(&self, tokens: usize) -> usize {
        self.usable_input_tokens().saturating_sub(tokens)
    }

    /// Whether `tokens` overflows the usable budget, and by how much.
    pub fn overflow(&self, tokens: usize) -> Option<usize> {
        let usable = self.usable_input_tokens();
        (tokens > usable).then(|| tokens - usable)
    }
}

impl Default for ContextBudget {
    fn default() -> Self {
        // A conservative default usable window for a typical 128k-class model
        // after reserving output and safety headroom.
        Self {
            max_input_tokens: 128_000,
            reserved_output_tokens: 8_192,
            safety_margin_tokens: 4_096,
        }
    }
}

/// A budget snapshot for observability (see `context.status`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextBudgetStatus {
    /// Configured model/context limit.
    pub max_input_tokens: usize,
    /// Tokens reserved for output.
    pub reserved_output_tokens: usize,
    /// Safety headroom tokens.
    pub safety_margin_tokens: usize,
    /// Usable input tokens (limit minus reservations).
    pub usable_input_tokens: usize,
    /// Tokens currently active.
    pub active_tokens: usize,
    /// Remaining usable input tokens.
    pub remaining_tokens: usize,
    /// Whether the active context fits the budget.
    pub within_budget: bool,
}

impl ContextBudget {
    /// Builds a status view for `active_tokens`.
    pub fn status(&self, active_tokens: usize) -> ContextBudgetStatus {
        ContextBudgetStatus {
            max_input_tokens: self.max_input_tokens,
            reserved_output_tokens: self.reserved_output_tokens,
            safety_margin_tokens: self.safety_margin_tokens,
            usable_input_tokens: self.usable_input_tokens(),
            active_tokens,
            remaining_tokens: self.remaining(active_tokens),
            within_budget: self.fits(active_tokens),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget() -> ContextBudget {
        ContextBudget {
            max_input_tokens: 1000,
            reserved_output_tokens: 200,
            safety_margin_tokens: 100,
        }
    }

    #[test]
    fn usable_subtracts_reservations() {
        assert_eq!(budget().usable_input_tokens(), 700);
    }

    #[test]
    fn fits_and_remaining_agree() {
        let b = budget();
        assert!(b.fits(700));
        assert!(!b.fits(701));
        assert_eq!(b.remaining(500), 200);
        assert_eq!(b.remaining(700), 0);
        assert_eq!(b.remaining(800), 0);
    }

    #[test]
    fn overflow_reports_excess() {
        let b = budget();
        assert_eq!(b.overflow(701), Some(1));
        assert_eq!(b.overflow(1000), Some(300));
        assert_eq!(b.overflow(700), None);
        assert_eq!(b.overflow(0), None);
    }

    #[test]
    fn over_reserved_budget_saturates_to_zero() {
        let b = ContextBudget {
            max_input_tokens: 100,
            reserved_output_tokens: 200,
            safety_margin_tokens: 100,
        };
        assert_eq!(b.usable_input_tokens(), 0);
        assert!(b.fits(0));
        assert!(!b.fits(1));
    }

    #[test]
    fn status_reports_within_budget() {
        let b = budget();
        let status = b.status(650);
        assert!(status.within_budget);
        assert_eq!(status.remaining_tokens, 50);
        let status = b.status(701);
        assert!(!status.within_budget);
        assert_eq!(status.remaining_tokens, 0);
    }

    #[test]
    fn default_budget_is_positive_and_conservative() {
        let b = ContextBudget::default();
        assert!(b.usable_input_tokens() > 0);
        assert!(b.reserved_output_tokens > 0);
        assert!(b.safety_margin_tokens > 0);
    }
}
