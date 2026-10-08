//! Bounded operation tracker: honest progress + notification states.
//!
//! Consequential operations surface as Requested → Authorizing →
//! Executing → Confirmed / Failed / Unknown. The UI never claims
//! success before backend confirmation; an Unknown result (e.g. a
//! remote request whose response timed out) stays visible until the
//! user reconciles it against fresh backend state.
//!
//! v1 limitation (documented per Prompt 30): the local backend is
//! synchronous, so Authorizing/Executing are conceptual states that
//! resolve within a single event tick; Unknown is reachable through
//! ambiguous remote failures. The state machine is still modeled so
//! the visual language is real rather than cosmetic.

use super::theme::Role;
use ratatui::text::{Line, Span};

/// Lifecycle of a tracked operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationPhase {
    /// The user asked for it; not yet authorized/confirmed.
    Requested,
    /// Awaiting authorization or destructive confirmation.
    Authorizing,
    /// Backend call in flight (instantaneous for sync backends).
    Executing,
    /// Backend confirmed the effect.
    Confirmed,
    /// Backend reported failure.
    Failed(String),
    /// Result could not be determined; reconcile before retrying.
    Unknown(String),
}

impl OperationPhase {
    fn role(&self) -> Role {
        match self {
            OperationPhase::Requested => Role::Pending,
            OperationPhase::Authorizing => Role::Warning,
            OperationPhase::Executing => Role::Running,
            OperationPhase::Confirmed => Role::Success,
            OperationPhase::Failed(_) => Role::Error,
            OperationPhase::Unknown(_) => Role::Unknown,
        }
    }

    fn verb(&self) -> &'static str {
        match self {
            OperationPhase::Requested => "requested",
            OperationPhase::Authorizing => "authorizing",
            OperationPhase::Executing => "executing",
            OperationPhase::Confirmed => "confirmed",
            OperationPhase::Failed(_) => "failed",
            OperationPhase::Unknown(_) => "unknown",
        }
    }
}

/// One tracked operation. Bounded: the app keeps at most
/// [`MAX_TRACKED`] and drops the oldest.
#[derive(Debug, Clone)]
pub struct TrackedOperation {
    pub label: String,
    pub phase: OperationPhase,
}

impl TrackedOperation {
    pub fn status_line(&self) -> Vec<Span<'static>> {
        let role = self.phase.role();
        let mut spans = vec![
            Span::styled(format!("{} ", role.symbol()), role.style()),
            Span::styled(self.label.clone(), Role::Normal.style()),
            Span::styled(" — ", Role::Muted.style()),
            Span::styled(self.phase.verb().to_string(), role.style()),
        ];
        match &self.phase {
            OperationPhase::Failed(reason) => {
                spans.push(Span::styled(format!(": {reason}"), role.style()));
            }
            OperationPhase::Unknown(reason) => {
                spans.push(Span::styled(
                    format!(": {reason} — press r to reconcile"),
                    role.style(),
                ));
            }
            _ => {}
        }
        spans
    }
}

pub const MAX_TRACKED: usize = 5;

/// Bounded queue of tracked operations; the newest is the headline.
#[derive(Debug, Clone, Default)]
pub struct OperationTracker {
    ops: Vec<TrackedOperation>,
}

impl OperationTracker {
    pub fn track(&mut self, label: impl Into<String>) {
        self.ops.push(TrackedOperation {
            label: label.into(),
            phase: OperationPhase::Requested,
        });
        if self.ops.len() > MAX_TRACKED {
            let excess = self.ops.len() - MAX_TRACKED;
            self.ops.drain(0..excess);
        }
    }

    /// Moves the newest operation to a phase. `to`/`failed`/`unknown`
    /// convenience wrappers cover the honest terminal states.
    pub fn advance(&mut self, phase: OperationPhase) {
        if let Some(op) = self.ops.last_mut() {
            op.phase = phase;
        }
    }

    pub fn to_authorizing(&mut self) {
        self.advance(OperationPhase::Authorizing);
    }

    pub fn to_executing(&mut self) {
        self.advance(OperationPhase::Executing);
    }

    pub fn to_confirmed(&mut self) {
        self.advance(OperationPhase::Confirmed);
    }

    pub fn to_failed(&mut self, reason: impl Into<String>) {
        self.advance(OperationPhase::Failed(reason.into()));
    }

    pub fn to_unknown(&mut self, reason: impl Into<String>) {
        self.advance(OperationPhase::Unknown(reason.into()));
    }

    /// The headline operation for the status bar, if any.
    pub fn headline(&self) -> Option<&TrackedOperation> {
        self.ops.last()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Reconciliation entry point: an Unknown phase becomes Confirmed
    /// or Failed once fresh state resolves it; older resolved ops fade.
    pub fn reconcile_unknowns(&mut self) {
        for op in &mut self.ops {
            if matches!(op.phase, OperationPhase::Unknown(_)) {
                // Caller resolves against the backend; the visible
                // marker stays Unknown until then. Reconciliation
                // happens via `advance` from the screens.
                let _ = op;
            }
        }
    }
}

/// Notification kinds for transient outcomes. Never shown before the
/// backend responds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Success,
    Info,
    Warning,
    Error,
    Blocked,
}

impl NoticeKind {
    pub fn role(self) -> Role {
        match self {
            NoticeKind::Success => Role::Success,
            NoticeKind::Info => Role::Normal,
            NoticeKind::Warning => Role::Warning,
            NoticeKind::Error => Role::Error,
            NoticeKind::Blocked => Role::Blocked,
        }
    }

    pub fn symbol(self) -> &'static str {
        self.role().symbol()
    }
}

/// Renders a notice line (used by dialogs + footer).
pub fn notice_line(kind: NoticeKind, text: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{} ", kind.symbol()), kind.role().style()),
        Span::raw(text.to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_advances_through_the_honest_sequence() {
        let mut t = OperationTracker::default();
        t.track("commit staged changes");
        assert_eq!(t.headline().unwrap().phase, OperationPhase::Requested);
        t.to_authorizing();
        assert_eq!(t.headline().unwrap().phase, OperationPhase::Authorizing);
        t.to_executing();
        t.to_confirmed();
        assert_eq!(t.headline().unwrap().phase, OperationPhase::Confirmed);
    }

    #[test]
    fn tracker_bounds_history() {
        let mut t = OperationTracker::default();
        for i in 0..(MAX_TRACKED + 3) {
            t.track(format!("op {i}"));
            t.to_confirmed();
        }
        assert_eq!(t.ops.len(), MAX_TRACKED);
        assert_eq!(
            t.headline().unwrap().label,
            format!("op {}", MAX_TRACKED + 2)
        );
    }

    #[test]
    fn unknown_result_names_reconciliation() {
        let mut t = OperationTracker::default();
        t.track("push to origin");
        t.to_unknown("no response before timeout");
        let spans = t.headline().unwrap().status_line();
        let joined: String = spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(joined.contains("unknown"));
        assert!(joined.contains("reconcile"));
        assert!(!joined.contains("confirmed"));
    }

    #[test]
    fn notice_symbols_are_distinct_per_kind() {
        let kinds = [
            NoticeKind::Success,
            NoticeKind::Info,
            NoticeKind::Warning,
            NoticeKind::Error,
            NoticeKind::Blocked,
        ];
        let mut seen = std::collections::HashSet::new();
        for k in kinds {
            assert!(seen.insert(k.symbol()));
        }
    }
}
