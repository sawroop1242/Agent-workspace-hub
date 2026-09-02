//! Shared in-memory audit trail (spec sections 16, 25, 26).
//!
//! Both the Control API and the MCP plane record security-relevant
//! events here through the shared service layer. The store is a
//! bounded ring buffer: audit history must never grow without limit,
//! and older events are dropped first. Entries never contain secret
//! values — call sites pass actor identifiers, action slugs, and
//! non-secret context only.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum retained entries; the ring drops the oldest first.
const MAX_ENTRIES: usize = 1000;

/// One security-relevant event.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    /// Unix epoch milliseconds.
    pub ts_ms: u128,
    /// `allow` or `deny`.
    pub kind: &'static str,
    /// Stable action slug (`control_auth`, `tool_invoke`, ...).
    pub action: String,
    /// Affected actor or resource (never a secret value).
    pub subject: String,
    /// Free-form non-secret context.
    pub detail: String,
}

/// Bounded audit ring buffer.
pub struct AuditLog {
    entries: Mutex<VecDeque<AuditEntry>>,
}

impl AuditLog {
    fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_ENTRIES)),
        }
    }

    /// Appends an entry, dropping the oldest when at capacity.
    pub fn record(&self, kind: &'static str, action: &str, subject: &str, detail: &str) {
        let entry = AuditEntry {
            ts_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
            kind,
            action: action.to_owned(),
            subject: subject.to_owned(),
            detail: detail.to_owned(),
        };
        let mut guard = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        if guard.len() == MAX_ENTRIES {
            guard.pop_front();
        }
        guard.push_back(entry);
    }

    /// Returns the most recent `limit` entries, newest first.
    pub fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        let guard = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        guard.iter().rev().take(limit).cloned().collect()
    }

    /// Number of retained entries.
    pub fn len(&self) -> usize {
        let guard = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        guard.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Process-wide store shared by both transport planes.
pub fn global() -> &'static AuditLog {
    static STORE: OnceLock<AuditLog> = OnceLock::new();
    STORE.get_or_init(AuditLog::new)
}

/// Records an allow-side event in the global store.
pub fn record_allow(action: &str, subject: &str, detail: &str) {
    global().record("allow", action, subject, detail);
}

/// Records a deny-side event in the global store.
pub fn record_deny(action: &str, reason: &str, subject: &str) {
    global().record("deny", action, subject, reason);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_caps_at_max_entries() {
        let log = AuditLog::new();
        for i in 0..(MAX_ENTRIES + 50) {
            log.record("allow", "test", "subj", &format!("detail-{i}"));
        }
        assert_eq!(log.len(), MAX_ENTRIES);
        let recent = log.recent(3);
        // Newest first; the first pushed items were dropped.
        assert_eq!(recent[0].detail, format!("detail-{}", MAX_ENTRIES + 49));
        assert_eq!(recent[2].detail, format!("detail-{}", MAX_ENTRIES + 47));
    }

    #[test]
    fn recent_returns_newest_first() {
        let log = AuditLog::new();
        log.record("allow", "a1", "s", "first");
        log.record("deny", "a2", "s", "second");
        let recent = log.recent(10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].action, "a2");
        assert_eq!(recent[1].action, "a1");
        assert_eq!(recent[0].kind, "deny");
    }

    #[test]
    fn ts_is_epoch_millis() {
        let log = AuditLog::new();
        log.record("allow", "a", "s", "d");
        let e = &log.recent(1)[0];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        assert!(e.ts_ms <= now && now - e.ts_ms < 5_000);
    }
}
