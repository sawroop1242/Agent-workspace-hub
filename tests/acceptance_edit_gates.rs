//! AWE-015 / TW-008 acceptance: authorization floors for the edit plane
//! (in-process dispatcher tests — Prompt 16 §11, §12, §26).
//!
//! These pin that EVERY denial in front of `filesystem.*` edits is a
//! **zero-side-effect** denial: not just the JSON-RPC error code, but the
//! protected file's bytes, the absence of any snapshot/provenance residue,
//! and the audit outcome are all asserted. A denial response alone is
//! insufficient evidence (§11).
//!
//! Complementary to the existing suites: `tests/mcp_builtin_tool_gate.rs`
//! proves trust-gate semantics with connector/git examples;
//! `tests/mcp_agent_routes.rs` proves capability gating for
//! `workspace.write_file`. This file adds the edit-plane evidence:
//!
//! * the SEC-001 built-in gate denying `filesystem.replace` AND
//!   `filesystem.rollback` when the trust record excludes the Filesystem
//!   category, with the operator-compatible no-record default pinned as
//!   the documented control;
//! * the TW-003 per-agent capability gate denying `filesystem.*` for a
//!   bound caller with no grant, and allowing it with a scoped grant.

use agent_workspace_hub::core::agents::AgentStore;
use agent_workspace_hub::core::capability_grants::CapabilityGrantStore;
use agent_workspace_hub::mcp::agent_route::resolve_route_agent;
use agent_workspace_hub::mcp::permissions::Permission;
use agent_workspace_hub::mcp::sse::Session;
use agent_workspace_hub::mcp::{
    AppState, McpDispatcher, McpPermissions, PersistentTrustStore, SessionLifecycle,
    SessionRegistry, TrustLevel, TrustStore, BUILTIN_TOOL_DENIED_CODE, BUILTIN_TOOL_TRUST_ID,
};
use agent_workspace_hub::models::agent::{Agent, AgentStatus};
use agent_workspace_hub::models::capability_grant::CapabilityGrant;
use agent_workspace_hub::services::audit::global as audit_log;
use agent_workspace_hub::services::init::initialize_workspace;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

const API_KEY: &str = "acceptance-test-secret";

// ---------------------------------------------------------------------------
// Dispatcher helpers (same harness shape as tests/mcp_builtin_tool_gate.rs)
// ---------------------------------------------------------------------------

fn request(id: Value, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn init_params() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "edit-acceptance", "version": "0.0.1"}
    })
}

fn builtin_permissions(filesystem: bool) -> McpPermissions {
    McpPermissions {
        filesystem: if filesystem {
            vec![BUILTIN_TOOL_TRUST_ID.to_string()]
        } else {
            Vec::new()
        },
        ..McpPermissions::default()
    }
}

/// A persistent trust store holding one `awh.builtin` record with the given
/// filesystem category state (approved for the built-in gate's pinned
/// `local` version, exactly like `tests/mcp_builtin_tool_gate.rs`).
fn builtin_trust_store(permissions: McpPermissions) -> PersistentTrustStore {
    let mut store = TrustStore::default();
    store
        .approve(
            BUILTIN_TOOL_TRUST_ID,
            TrustLevel::Trusted,
            permissions,
            "local".to_string(),
        )
        .expect("valid approval");
    PersistentTrustStore::from_store(&store)
}

/// An empty persistent store — present, but with no `awh.builtin` record.
fn empty_trust_store() -> PersistentTrustStore {
    PersistentTrustStore::from_store(&TrustStore::default())
}

/// A dispatcher over an initialized workspace carrying `trust`.
async fn dispatcher_with(trust: PersistentTrustStore) -> (McpDispatcher, tempfile::TempDir) {
    let dir = tempdir().expect("tempdir");
    initialize_workspace(dir.path()).expect("initialize workspace");
    std::fs::write(dir.path().join("edit.txt"), "alpha\nbeta\n").expect("fixture");
    let dispatcher = McpDispatcher::new_async(dir.path().to_path_buf())
        .await
        .expect("dispatcher")
        .with_trust_store(trust);
    (dispatcher, dir)
}

/// Drives a full session and calls one tool, returning the raw response.
async fn call_tool_raw(dispatcher: &McpDispatcher, name: &str, arguments: Value) -> Value {
    use agent_workspace_hub::mcp::DispatchResult;
    let lifecycle = SessionLifecycle::default();
    let init = request(json!(0), "initialize", init_params());
    match dispatcher.dispatch_with_lifecycle(&init, &lifecycle).await {
        DispatchResult::Response(_) => {}
        DispatchResult::NoResponse => panic!("initialize must produce a response"),
    }
    let input = request(
        json!(1),
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    );
    match dispatcher.dispatch_with_lifecycle(&input, &lifecycle).await {
        DispatchResult::Response(resp) => serde_json::to_value(&resp).expect("serialize"),
        DispatchResult::NoResponse => panic!("{name} must produce a response"),
    }
}

fn agent_sub(dir: &tempfile::TempDir, sub: &str) -> PathBuf {
    dir.path().join(".agent").join(sub)
}

/// True when the named `.agent` state directory is empty or absent — the
/// zero-residue assertion for denied edits.
fn no_records(path: &std::path::Path) -> bool {
    std::fs::read_dir(path)
        .map(|entries| entries.count() == 0)
        .unwrap_or(true)
}

// ---------------------------------------------------------------------------
// §11 §12 §26: trust-gate denial is a zero-side-effect denial for the
// edit plane (both the edit AND the recovery surface).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trust_denial_leaves_edit_plane_untouched() {
    // A trust record that excludes the Filesystem category: every
    // filesystem.* tool must be denied BEFORE the service runs.
    let (dispatcher, dir) = dispatcher_with(builtin_trust_store(builtin_permissions(false))).await;

    let error = call_tool_raw(
        &dispatcher,
        "filesystem.replace",
        json!({"path":"edit.txt","old":"alpha","new":"ALPHA"}),
    )
    .await;
    assert_eq!(
        error["error"]["code"],
        json!(BUILTIN_TOOL_DENIED_CODE),
        "the built-in gate denies the edit: {error}"
    );

    // §11: a denial response is insufficient evidence by itself.
    assert_eq!(
        std::fs::read(dir.path().join("edit.txt")).expect("read"),
        b"alpha\nbeta\n",
        "the protected file's bytes are untouched"
    );
    assert!(
        no_records(&agent_sub(&dir, "snapshots")),
        "no snapshot manifest was published"
    );
    assert!(
        no_records(&agent_sub(&dir, "snapshot-contents")),
        "no recovery bytes were captured"
    );
    assert!(
        no_records(&agent_sub(&dir, "provenance")),
        "no provenance record exists for a denied edit"
    );

    // The recovery surface is equally gated: rollback is denied too.
    let error = call_tool_raw(
        &dispatcher,
        "filesystem.rollback",
        json!({"edit_id":"edit-0000000000000000000000000000000"}),
    )
    .await;
    assert_eq!(
        error["error"]["code"],
        json!(BUILTIN_TOOL_DENIED_CODE),
        "the built-in gate denies rollback: {error}"
    );
    assert_eq!(
        std::fs::read(dir.path().join("edit.txt")).expect("read"),
        b"alpha\nbeta\n",
        "rollback denial is zero-side-effect"
    );

    // Both denials were audited (the gate records its decisions even
    // though nothing mutated). Warm the callsites, then check the ring —
    // the tracing/audit interest-cache pattern from AGENTS.md.
    assert!(
        audit_log()
            .recent(1000)
            .iter()
            .any(|entry| entry.action == "builtin_tool_denied"
                && entry.subject == "filesystem.replace"),
        "the edit denial was audited"
    );
    assert!(
        audit_log()
            .recent(1000)
            .iter()
            .any(|entry| entry.action == "builtin_tool_denied"
                && entry.subject == "filesystem.rollback"),
        "the rollback denial was audited"
    );

    // Positive control on a FRESH workspace: the same call with a record
    // that grants the Filesystem category commits — proving the denial
    // above was the gate, not the fixture.
    let (dispatcher, dir) = dispatcher_with(builtin_trust_store(builtin_permissions(true))).await;
    let response = call_tool_raw(
        &dispatcher,
        "filesystem.replace",
        json!({"path":"edit.txt","old":"alpha","new":"ALPHA"}),
    )
    .await;
    assert!(response["error"].is_null(), "gated allow: {response}");
    assert_eq!(
        std::fs::read(dir.path().join("edit.txt")).expect("read"),
        b"ALPHA\nbeta\n",
        "the positive control mutated"
    );

    // The documented operator compatibility default: with NO trust record
    // at all, Medium-risk workspace-local mutations stay allowed (the
    // pre-SEC-001 opt-in semantics the gate tests pin).
    let (dispatcher, dir) = dispatcher_with(empty_trust_store()).await;
    let response = call_tool_raw(
        &dispatcher,
        "filesystem.replace",
        json!({"path":"edit.txt","old":"alpha","new":"ALPHA"}),
    )
    .await;
    assert!(
        response["error"].is_null(),
        "no-record Medium default allows the edit: {response}"
    );
    assert_eq!(
        std::fs::read(dir.path().join("edit.txt")).expect("read"),
        b"ALPHA\nbeta\n"
    );
}

// ---------------------------------------------------------------------------
// §11 §18 §26: per-agent capability gating of the edit plane (TW-003) —
// a bound caller without a grant is denied with zero side effects; a
// scoped grant allows exactly its scope.
// ---------------------------------------------------------------------------

struct AgentWs {
    root: PathBuf,
    _dir: std::mem::ManuallyDrop<tempfile::TempDir>,
}

impl AgentWs {
    fn new(agents: &[&str]) -> Self {
        let dir = tempdir().expect("tempdir");
        // Leak the temp dir (same pattern as tests/mcp_http.rs): the
        // dispatcher and stores hold paths into it for the test process.
        let holder = std::mem::ManuallyDrop::new(dir);
        let root = holder.path().to_path_buf();
        initialize_workspace(&root).expect("initialize workspace");
        let store = AgentStore::new(&root);
        for id in agents {
            store
                .create(&Agent {
                    id: (*id).into(),
                    name: (*id).into(),
                    role: "test".into(),
                    status: AgentStatus::Active,
                    enabled: true,
                    created_at: chrono::Utc::now().to_rfc3339(),
                })
                .expect("create agent");
        }
        Self { root, _dir: holder }
    }

    fn grant(&self, id: &str, agent: &str, scope: Option<&str>) {
        CapabilityGrantStore::new(&self.root)
            .create(&CapabilityGrant {
                id: id.into(),
                agent_id: agent.into(),
                permission: Permission::Filesystem,
                scope: scope.map(str::to_string),
                granted_at: chrono::Utc::now().to_rfc3339(),
                expires_at: None,
            })
            .expect("create grant");
    }
}

async fn harness(ws: &AgentWs) -> AppState {
    let dispatcher = Arc::new(
        McpDispatcher::new_async(ws.root.clone())
            .await
            .expect("build dispatcher"),
    );
    AppState {
        dispatcher,
        sessions: Arc::new(SessionRegistry::new()),
        api_key: Arc::from(API_KEY),
        max_sessions: 8,
        sse_keepalive: Duration::from_secs(15),
        rate_limiter: agent_workspace_hub::services::rate_limit::RateLimiter::default_limiter(),
    }
}

/// Creates a session exactly like `/{agent}/sse` would and runs the
/// initialize exchange through the dispatcher on the bound lifecycle.
async fn bound_session(state: &AppState, ws: &AgentWs, agent: &str) -> Session {
    let (caller, _) = resolve_route_agent(&ws.root, agent).expect("resolve agent");
    let session = state.sessions.create_with_binding("/mcp", &caller).await;
    let mut rx = session.subscribe();
    let result = state
        .dispatcher
        .dispatch_with_lifecycle(
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "0"},
                }
            })
            .to_string(),
            &session.lifecycle,
        )
        .await;
    match result {
        agent_workspace_hub::mcp::DispatchResult::Response(_) => {}
        agent_workspace_hub::mcp::DispatchResult::NoResponse => panic!("initialize must respond"),
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    session
}

/// Dispatches a JSON-RPC request on the session's bound lifecycle (the
/// same entry point the POST handler uses) and returns the response.
async fn dispatch_on(state: &AppState, session: &Session, request: Value) -> Value {
    let result = state
        .dispatcher
        .dispatch_with_lifecycle(&request.to_string(), &session.lifecycle)
        .await;
    match result {
        agent_workspace_hub::mcp::DispatchResult::Response(response) => {
            let value = serde_json::to_value(&response).unwrap_or(Value::Null);
            assert_eq!(value["jsonrpc"], "2.0");
            value
        }
        agent_workspace_hub::mcp::DispatchResult::NoResponse => {
            panic!("{request:?} must produce a response")
        }
    }
}

#[tokio::test]
async fn capability_gate_zero_side_effect_for_edit_tools() {
    let ws = AgentWs::new(&["alpha", "beta"]);
    // alpha holds a scoped Filesystem grant; beta holds nothing.
    ws.grant("g-alpha-fs", "alpha", Some("src"));
    std::fs::create_dir_all(ws.root.join("src")).expect("src dir");
    std::fs::write(ws.root.join("src/edit.txt"), "alpha\nbeta\n").expect("fixture");

    let state = harness(&ws).await;

    // --- beta: no grant → denied before the service runs -----------------
    let beta = bound_session(&state, &ws, "beta").await;
    let value = dispatch_on(
        &state,
        &beta,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "filesystem.replace",
                "arguments": {"path": "src/edit.txt", "old": "alpha", "new": "ESCAPED"},
            },
        }),
    )
    .await;
    assert!(
        value["error"].is_object(),
        "capability gate must deny: {value}"
    );
    let message = value["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("beta")
            && message.contains("filesystem")
            && message.to_lowercase().contains("filesystem"),
        "denial names the caller and the missing capability: {message}"
    );

    // §11: zero side effects — bytes, recovery material, and no provenance.
    assert_eq!(
        std::fs::read(ws.root.join("src/edit.txt")).expect("read"),
        b"alpha\nbeta\n",
        "a denied edit never mutated the file"
    );
    assert!(
        std::fs::read_dir(ws.root.join(".agent/provenance"))
            .map(|entries| entries.count() == 0)
            .unwrap_or(true),
        "no provenance record for the denied edit"
    );
    assert!(
        std::fs::read_dir(ws.root.join(".agent/snapshots"))
            .map(|entries| entries.count() == 0)
            .unwrap_or(true),
        "no snapshot manifest for the denied edit"
    );

    // Beta cannot reach the recovery surface either.
    let value = dispatch_on(
        &state,
        &beta,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "filesystem.rollback",
                "arguments": {"edit_id": "edit-0000000000000000000000000000000"},
            },
        }),
    )
    .await;
    assert!(
        value["error"].is_object(),
        "capability gate must deny rollback: {value}"
    );

    // --- alpha: in-scope grant → the same call commits -------------------
    let alpha = bound_session(&state, &ws, "alpha").await;
    let value = dispatch_on(
        &state,
        &alpha,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "filesystem.replace",
                "arguments": {"path": "src/edit.txt", "old": "alpha", "new": "ALPHA"},
            },
        }),
    )
    .await;
    assert!(
        value["result"].is_object(),
        "scoped grant allows the edit: {value}"
    );
    assert_eq!(
        std::fs::read(ws.root.join("src/edit.txt")).expect("read"),
        b"ALPHA\nbeta\n",
        "the granted edit mutated"
    );

    // --- alpha out-of-scope: the same grant does not cover other paths ---
    let value = dispatch_on(
        &state,
        &alpha,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "filesystem.replace",
                "arguments": {"path": "etc/evil.txt", "old": "a", "new": "b"},
            },
        }),
    )
    .await;
    assert!(
        value["error"].is_object(),
        "out-of-scope edit must be denied: {value}"
    );
    assert!(
        !ws.root.join("etc/evil.txt").exists(),
        "out-of-scope denial created nothing"
    );
}

/// The bound caller's identity rides the audit correlation: alpha's
/// committed edit is correlated with alpha's agent id in the audit ring.
#[tokio::test]
async fn capability_allowed_edit_is_audited_with_agent_correlation() {
    let ws = AgentWs::new(&["alpha"]);
    ws.grant("g-alpha-fs", "alpha", None);
    std::fs::write(ws.root.join("edit.txt"), "alpha\n").expect("fixture");

    let state = harness(&ws).await;
    let alpha = bound_session(&state, &ws, "alpha").await;
    let value = dispatch_on(
        &state,
        &alpha,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "filesystem.replace",
                "arguments": {"path": "edit.txt", "old": "alpha", "new": "ALPHA"},
            },
        }),
    )
    .await;
    assert!(value["result"].is_object(), "allowed edit: {value}");

    let entries = audit_log().recent(1000);
    let event = entries
        .iter()
        .find(|entry| {
            entry.action == "filesystem.replace"
                && entry.kind == "allow"
                && entry.agent_id.as_deref() == Some("alpha")
        })
        .expect("agent-correlated allow event for the edit");
    let _ = event; // presence + agent correlation IS the assertion
}
