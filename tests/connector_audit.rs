//! §9 Connectors × audit (Prompt 18): consequential connector
//! registration mutations must be attributable and redacted end to end.
//!
//! A dedicated test binary (own process) so the process-wide audit store
//! has no sibling-test noise: the durable readback below is exactly
//! what a restarted auditor reconstructs, deterministically.

use agent_workspace_hub::mcp::connectors::{AuthMethod, Connector, ConnectorsMcp};
use agent_workspace_hub::services::audit::{init_global, AuditLog};
use agent_workspace_hub::services::init::initialize_workspace;

fn connector(id: &str) -> Connector {
    Connector {
        id: id.into(),
        // Short labels: `redact_token_like` masks long base62 runs, and
        // redaction behavior itself is pinned by the Phase 10 tests —
        // this test pins the attribution FORMAT (name / provider).
        name: "probe-name".into(),
        provider: "probe-provider".into(),
        auth: AuthMethod::None,
        scopes: vec![],
        enabled: false,
    }
}

#[test]
fn connector_mutations_are_attributable_and_redacted_in_audit() {
    let dir = tempfile::tempdir().expect("tempdir");
    initialize_workspace(dir.path()).expect("initialize workspace");
    init_global(dir.path()).expect("bind durable audit root");
    let store = ConnectorsMcp::new(dir.path()).expect("connector store");

    // The full consequential-mutation surface of a connector registration.
    store.add(connector("conn-e2e")).expect("add");
    store
        .set_enabled("conn-e2e", true)
        .expect("enable")
        .expect("present");
    store
        .set_enabled("conn-e2e", false)
        .expect("disable")
        .expect("present");
    assert!(store.remove("conn-e2e").expect("remove"));
    // Removing a missing connector is a no-op: no fabricated event.
    assert!(!store.remove("conn-e2e").expect("remove again"));

    // A rejected add is a consequential DENY: it must be attributed too.
    let mut invalid = connector("conn-e2e");
    invalid.scopes = vec!["scope-leak-probe".into()];
    invalid.auth = AuthMethod::ApiKey;
    invalid.name = String::new();
    assert!(
        store.add(invalid).is_err(),
        "empty-name re-add must be rejected"
    );

    // Restarted-auditor readback: a fresh AuditLog over the same root.
    let log = AuditLog::open(dir.path()).expect("durable audit store opens");
    let entries: Vec<_> = log
        .recent(100)
        .into_iter()
        .filter(|entry| entry.subject == "conn-e2e")
        .collect();
    let actions: Vec<&str> = entries.iter().map(|e| e.action.as_str()).collect();
    for expected in [
        "connector_added",
        "connector_enabled",
        "connector_disabled",
        "connector_removed",
        "connector_add_rejected",
    ] {
        assert!(
            actions.contains(&expected),
            "missing {expected} in {actions:?}"
        );
    }

    let added = entries
        .iter()
        .find(|e| e.action == "connector_added")
        .expect("connector_added present");
    assert_eq!(added.kind, "allow");
    assert_eq!(added.detail, "probe-name / probe-provider");

    let rejected = entries
        .iter()
        .find(|e| e.action == "connector_add_rejected")
        .expect("connector_add_rejected present");
    assert_eq!(rejected.kind, "deny");

    // Redaction: OAuth scopes and auth-method material never land in
    // the audit trail, and the no-op second remove fabricated nothing.
    assert!(
        !entries
            .iter()
            .any(|e| e.detail.contains("scope") || e.detail.contains("ApiKey")),
        "audit detail leaked scope/auth material: {entries:?}"
    );
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.action == "connector_removed")
            .count(),
        1,
        "second remove of a missing connector must not audit again"
    );
}
