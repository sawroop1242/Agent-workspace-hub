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
    ///
    /// Defense in depth: subject/detail are redacted here (the single
    /// choke point every audit call site funnels through) so that
    /// token-shaped material can never enter the audit ring, even by
    /// accident at a future call site. Bearer tokens follow RFC 6750
    /// (>=16 chars of base62); AWH keys match. Very short strings are
    /// left alone: a 5-char rate-limit key like `9.9.9.9` is not a
    /// secret and redacting it would erase useful context.
    pub fn record(&self, kind: &'static str, action: &str, subject: &str, detail: &str) {
        let entry = AuditEntry {
            ts_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
            kind,
            action: action.to_owned(),
            subject: redact_token_like(subject),
            detail: redact_token_like(detail),
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

/// Masks token-shaped segments in free-form audit text. Any run of 16
/// or more base62 characters (`A-Za-z0-9_-`) looks like a bearer/API
/// key (RFC 6750 opaque tokens, AWH keys, ngrok authtokens) and carries
/// no diagnostic value, so it is replaced wholesale. All other
/// characters (dots, slashes, `=`) pass through, so client IPs, project
/// paths, and prefixes like `token=` stay intact and the audit trail
/// remains useful. Opaque identifiers (session ids, UUIDs) may also
/// match and be masked — an acceptable trade: identifiers are cheap,
/// secrets are not.
pub fn redact_token_like(text: &str) -> String {
    const MIN_SECRET_LEN: usize = 16;
    let mut out = String::with_capacity(text.len());
    let mut seg = String::new();
    let flush = |out: &mut String, seg: &mut String| {
        if seg.chars().count() >= MIN_SECRET_LEN {
            out.push_str("[redacted]");
        } else {
            out.push_str(seg);
        }
        seg.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
            seg.push(c);
        } else {
            flush(&mut out, &mut seg);
            out.push(c);
        }
    }
    flush(&mut out, &mut seg);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_token_like_masks_long_base62_runs() {
        let secret = "Xq3vJ7Lb2mPz9KwR4TnE";
        assert_eq!(redact_token_like(secret), "[redacted]");
        assert_eq!(
            redact_token_like(&format!("Bearer {secret} on file")),
            "Bearer [redacted] on file"
        );
    }

    #[test]
    fn redact_token_like_preserves_short_identifiers_and_paths() {
        // Client IPs (rate-limit subjects) and project names stay intact.
        assert_eq!(redact_token_like("9.9.9.9"), "9.9.9.9");
        assert_eq!(redact_token_like("203.0.113.77"), "203.0.113.77");
        assert_eq!(redact_token_like("direct"), "direct");
        assert_eq!(
            redact_token_like("projects/demo/.agent/context.md"),
            "projects/demo/.agent/context.md"
        );
        assert_eq!(redact_token_like(""), "");
    }

    #[test]
    fn ring_buffer_caps_at_max_entries() {
        let log = AuditLog::new();
        for i in 0..(MAX_ENTRIES + 50) {
            log.record("allow", "test", "subj", &format!("detail-{i}"));
        }
        assert_eq!(log.len(), MAX_ENTRIES);
        let recent = log.recent(3);
        // Newest first; the first pushed items were dropped.
        assert!(recent[0]
            .detail
            .ends_with(&format!("-{}", MAX_ENTRIES + 49)));
    }

    #[test]
    fn recorded_subjects_and_details_are_redacted_at_the_choke_point() {
        let log = AuditLog::new();
        // A future call site that (incorrectly) passes a bearer token
        // as subject or embeds one in detail must not leak it.
        log.record(
            "allow",
            "some_action",
            "Xq3vJ7Lb2mPz9KwR4TnE",
            "token=Xq3vJ7Lb2mPz9KwR4TnE ok",
        );
        let entry = &log.recent(1)[0];
        assert_eq!(entry.subject, "[redacted]");
        assert_eq!(entry.detail, "token=[redacted] ok");
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
