//! Structured audit logging for the MCP security boundaries.
//!
//! Every fail-closed denial in the trust/permission/sandbox/transport chain
//! emits a structured audit event so unattended agent operation can be
//! reconstructed and audited. Audit events never include secret values.

/// Emits a structured audit event for a denied security decision.
///
/// `reason` is a stable machine-readable slug; `subject` identifies the
/// affected actor (MCP id, tool name, path, etc.). No secret values are logged.
pub fn audit_deny(action: &str, reason: &str, subject: &str) {
    tracing::warn!(event = "mcp_security_denied", action, reason, subject,);
}

/// Emits a structured audit event for a secret-resolution denial, logging only
/// the secret *name* (never its value).
pub fn audit_secret_deny(reason: &str, name: &str) {
    tracing::warn!(event = "mcp_secret_denied", reason, name,);
}

/// Emits a structured audit event for a circuit-breaker trip.
pub fn audit_circuit_open(provider: &str) {
    tracing::warn!(event = "mcp_circuit_open", provider,);
}

/// Emits a structured audit event for a successful security-relevant action
/// (authentication success, session creation, tool invocation). `subject`
/// identifies the actor or resource; `detail` is free-form, non-secret context.
///
/// Info-level because allow-side events are not violations, but they are
/// still required for incident reconstruction (who did what, when).
pub fn audit_allow(action: &str, subject: &str, detail: &str) {
    tracing::info!(event = "mcp_audit", action, subject, detail,);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn audit_helpers_do_not_panic() {
        audit_deny("authorize", "no_approval", "mcp-1");
        audit_secret_deny("not_approved", "my_secret");
        audit_circuit_open("provider-1");
        audit_allow("tool_invoke", "memory.store", "tools/call");
    }

    /// Extracts the value of the `event` field from an audit event.
    struct EventNameVisitor(Option<String>);

    impl tracing::field::Visit for EventNameVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == "event" {
                self.0 = Some(value.to_string());
            }
        }

        fn record_debug(&mut self, _field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {}
    }

    /// A [`tracing_subscriber::Layer`] that records `event` field values.
    #[derive(Clone, Default)]
    struct EventRecorder(Arc<Mutex<Vec<String>>>);

    impl<S> tracing_subscriber::Layer<S> for EventRecorder
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut visitor = EventNameVisitor(None);
            event.record(&mut visitor);
            if let Some(name) = visitor.0 {
                self.0.lock().unwrap().push(name);
            }
        }
    }

    /// Proves audit events actually reach an installed subscriber: the audit
    /// helpers are not compiled-out no-ops and carry the right event names.
    #[test]
    fn audit_events_reach_the_subscriber() {
        use tracing_subscriber::layer::SubscriberExt;

        let recorder = EventRecorder::default();
        // `set_default` is scoped to this thread (guard on drop), so it cannot
        // leak into other tests running on their own threads.
        let subscriber = tracing_subscriber::registry().with(recorder.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        audit_allow("session_create", "sid-1", "/sse");
        audit_deny("http_auth", "invalid_or_missing_token", "remote");
        audit_secret_deny("not_approved", "my_secret");
        audit_circuit_open("provider-1");
        audit_allow("tool_invoke", "memory.store", "tools/call");

        let recorded = recorder.0.lock().unwrap().clone();
        assert!(recorded.contains(&"mcp_audit".to_string()), "{recorded:?}");
        assert!(
            recorded.contains(&"mcp_security_denied".to_string()),
            "{recorded:?}"
        );
        assert!(
            recorded.contains(&"mcp_secret_denied".to_string()),
            "{recorded:?}"
        );
        assert!(
            recorded.contains(&"mcp_circuit_open".to_string()),
            "{recorded:?}"
        );
        // Allow-side events fire once per call: two audit_allow calls above.
        assert_eq!(
            recorded.iter().filter(|e| *e == "mcp_audit").count(),
            2,
            "{recorded:?}"
        );
    }
}
