//! Integration tests for the agent-scoped MCP routes (TW-003):
//!
//! * `/{agent}/sse` — session establishment, resolved against the
//!   workspace's canonical agent store. Unknown / non-active / malformed
//!   ids are rejected BEFORE any session exists.
//! * `/{agent}/mcp` — same body/session pipeline as `/mcp`, with one
//!   binding: the presented session must belong to the route's agent AND
//!   workspace. Another agent's session id reads exactly as "unknown
//!   session" — indistinguishable from an id that never existed.
//! * Dispatch — the bound caller drives the per-agent capability gate on
//!   top of the existing SEC-001 trust and policy floors, and `tools/list`
//!   is filtered to the caller's capabilities. Discovery filtering is
//!   separate from authorization: a hidden tool still re-authorizes (and
//!   fails closed) when called directly.

use agent_workspace_hub::core::agents::AgentStore;
use agent_workspace_hub::core::capability_grants::CapabilityGrantStore;
use agent_workspace_hub::core::identity::{AgentId, WorkspaceId};
use agent_workspace_hub::mcp::agent_route::{
    resolve_route_agent, verify_session_binding, RouteError,
};
use agent_workspace_hub::mcp::permissions::Permission;
use agent_workspace_hub::mcp::sse::{Session, SseEvent};
use agent_workspace_hub::mcp::{
    build_router, AppState, HttpServerConfig, McpDispatcher, SessionRegistry,
};
use agent_workspace_hub::models::agent::{Agent, AgentStatus};
use agent_workspace_hub::models::capability_grant::CapabilityGrant;
use agent_workspace_hub::services::init::{initialize_workspace, load_workspace_manifest};
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tower::util::ServiceExt;

const API_KEY: &str = "test-secret";

/// A temp workspace with the on-disk layout AWH uses at runtime.
struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Initializes the runtime boundary (`awh init` equivalent — the
    /// manifest must exist on disk for route resolution), then registers
    /// the requested agents with the given statuses.
    fn new<I: IntoIterator<Item = (&'static str, AgentStatus)>>(agents: I) -> Self {
        let temp = tempdir().expect("tempdir");
        // Keep the temp dir alive for the duration of the test process by
        // leaking it (same pattern as tests/mcp_http.rs).
        let holder = std::mem::ManuallyDrop::new(temp);
        let root = holder.path().to_path_buf();
        initialize_workspace(&root).expect("initialize workspace");
        let store = AgentStore::new(&root);
        for (id, status) in agents {
            store
                .create(&Agent {
                    id: id.into(),
                    name: id.into(),
                    role: "test".into(),
                    status,
                    enabled: true,
                    created_at: chrono::Utc::now().to_rfc3339(),
                })
                .expect("create agent");
        }
        Workspace { root }
    }

    fn workspace_id(&self) -> String {
        load_workspace_manifest(&self.root)
            .expect("manifest exists")
            .workspace_id
            .as_str()
            .to_string()
    }

    fn grants(&self) -> CapabilityGrantStore {
        CapabilityGrantStore::new(&self.root)
    }

    fn grant(
        &self,
        id: &str,
        agent: &str,
        permission: Permission,
        scope: Option<&str>,
        expires_at: Option<&str>,
    ) {
        self.grants()
            .create(&CapabilityGrant {
                id: id.into(),
                agent_id: agent.into(),
                permission,
                scope: scope.map(str::to_string),
                granted_at: chrono::Utc::now().to_rfc3339(),
                expires_at: expires_at.map(str::to_string),
            })
            .expect("create grant");
    }
}

/// Builds a router + inspectable state bound to the workspace.
async fn harness(ws: &Workspace) -> (axum::Router, AppState) {
    let dispatcher = Arc::new(
        McpDispatcher::new_async(ws.root.clone())
            .await
            .expect("build dispatcher"),
    );
    let state = AppState {
        dispatcher,
        sessions: Arc::new(SessionRegistry::new()),
        api_key: Arc::from(API_KEY),
        max_sessions: 8,
        sse_keepalive: Duration::from_secs(15),
        rate_limiter: agent_workspace_hub::services::rate_limit::RateLimiter::default_limiter(),
    };
    let config = HttpServerConfig {
        api_key: API_KEY.to_string(),
        ..HttpServerConfig::default()
    };
    (build_router(state.clone(), &config), state)
}

/// Sends a request through the router, returning status and body text.
/// Only safe for non-SSE responses (SSE streams never terminate).
async fn send(
    router: axum::Router,
    method: &str,
    uri: &str,
    body: String,
    auth: Option<&str>,
) -> (StatusCode, String) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = auth {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = builder
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

/// Creates a session exactly the way `/{agent}/sse` would (resolve →
/// bind → registry), subscribes a receiver BEFORE any dispatch so no
/// result is lost, and runs the MCP initialize exchange through the
/// dispatcher so the lifecycle gate is satisfied.
async fn initialized_bound_session(
    state: &AppState,
    ws: &Workspace,
    agent: &str,
) -> (Session, tokio::sync::broadcast::Receiver<SseEvent>) {
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
                },
            })
            .to_string(),
            &session.lifecycle,
        )
        .await;
    match result {
        agent_workspace_hub::mcp::DispatchResult::Response(_) => {}
        agent_workspace_hub::mcp::DispatchResult::NoResponse => panic!("initialize must respond"),
    }
    // Drain the initialize response so `next_message` sees the next result.
    let _ = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    (session, rx)
}

/// Dispatches a JSON-RPC request through the dispatcher on the session's
/// bound lifecycle (the same entry point the POST handler uses) and
/// collects the answer from the broadcast stream, like a real client
/// would over SSE.
async fn dispatch_on(
    state: &AppState,
    session: &Session,
    _rx: &mut tokio::sync::broadcast::Receiver<SseEvent>,
    request: Value,
) -> Value {
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

// ---------------------------------------------------------------------------
// Route resolution / substitution prevention
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unknown_agent_route_rejects_before_creating_session() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    let (router, state) = harness(&ws).await;

    let (status, body) = send(
        router,
        "GET",
        "/nonexistent/sse",
        String::new(),
        Some(API_KEY),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["error"], RouteError::UnknownAgent.reason_code());
    assert!(
        state.sessions.is_empty().await,
        "no session must be allocated for an unknown agent"
    );
}

#[tokio::test]
async fn malformed_agent_route_rejects_with_bad_request() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    let (router, state) = harness(&ws).await;

    for url in ["/..%2fcreds/sse", "/a..b/sse", "/a%2Fb/sse"] {
        let (status, _body) = send(router.clone(), "GET", url, String::new(), Some(API_KEY)).await;
        assert!(
            matches!(status, StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND),
            "{url} rejected (exact status depends on axum's path decoding order)"
        );
        assert!(state.sessions.is_empty().await, "{url}: session created");
    }
}

#[tokio::test]
async fn non_active_agent_route_rejects_with_forbidden() {
    // The resolver accepts exactly `Active` — every other lifecycle state
    // names a *known* agent but rejects with the dedicated code.
    for status in [
        AgentStatus::Created,
        AgentStatus::Paused,
        AgentStatus::Stopped,
        AgentStatus::Failed,
    ] {
        let ws = Workspace::new([("alpha", status.clone())]);
        let (router, state) = harness(&ws).await;
        let (status_code, body) =
            send(router, "GET", "/alpha/sse", String::new(), Some(API_KEY)).await;
        assert_eq!(
            status_code,
            StatusCode::FORBIDDEN,
            "status {:?} must reject",
            status
        );
        let value: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            value["error"],
            RouteError::AgentInactive.reason_code(),
            "{:?}",
            status
        );
        assert!(
            state.sessions.is_empty().await,
            "{:?}: session created",
            status
        );
    }
}

/// TW-002's `enabled=false` is a deliberate operator kill-switch: the
/// agent's status can still be `Active` on disk, but the profile must
/// never serve callers. The route must reject it exactly like a
/// non-active agent (prompt §27 "disabled agent"), with no session
/// allocated.
#[tokio::test]
async fn disabled_agent_route_rejects_before_creating_session() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    // Flip the profile off through the canonical store, exactly as an
    // operator would (`awh agent disable`).
    agent_workspace_hub::core::agents::AgentStore::new(&ws.root)
        .set_enabled("alpha", false)
        .expect("disable agent");
    let (router, state) = harness(&ws).await;

    let (status, body) = send(router, "GET", "/alpha/sse", String::new(), Some(API_KEY)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        value["error"], "agent_inactive",
        "disabled agents fail closed with the route's inactive code"
    );
    assert!(
        state.sessions.is_empty().await,
        "no session must be allocated for a disabled agent"
    );
}

/// A session created for agent A presented on agent B's route is
/// indistinguishable from an id that never existed: the response is the
/// route's ordinary 404 "unknown session", never a tell-tale hint that a
/// binding mismatch occurred.
#[tokio::test]
async fn session_bound_to_agent_a_read_on_agent_bs_route_is_unknown_session() {
    let ws = Workspace::new([
        ("alpha", AgentStatus::Active),
        ("beta", AgentStatus::Active),
    ]);
    let (router, state) = harness(&ws).await;

    let (alpha_caller, _) = resolve_route_agent(&ws.root, "alpha").unwrap();
    let alpha_session = state
        .sessions
        .create_with_binding("/mcp", &alpha_caller)
        .await;

    let (status, body) = send(
        router.clone(),
        "POST",
        &format!("/beta/mcp?sessionId={}", alpha_session.id),
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string(),
        Some(API_KEY),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        value["error"], "unknown session",
        "beta's route must NOT see alpha's session"
    );

    // Same pairing on alpha's own route passes the binding check (the
    // JSON-RPC layer may then complain about lifecycle, proving the route
    // accepted the session).
    let (status, _) = send(
        router.clone(),
        "POST",
        &format!("/alpha/mcp?sessionId={}", alpha_session.id),
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string(),
        Some(API_KEY),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "alpha route + alpha session is accepted"
    );
}

/// Both directions of cross-route session reuse fail identically — the
/// session never migrates to another agent's context.
#[tokio::test]
async fn sessions_never_migrate_between_agents() {
    let ws = Workspace::new([
        ("alpha", AgentStatus::Active),
        ("beta", AgentStatus::Active),
    ]);
    let (router, state) = harness(&ws).await;

    let alpha = state
        .sessions
        .create_with_binding("/mcp", &resolve_route_agent(&ws.root, "alpha").unwrap().0)
        .await;
    let beta = state
        .sessions
        .create_with_binding("/mcp", &resolve_route_agent(&ws.root, "beta").unwrap().0)
        .await;

    // Route alpha + session beta.
    let (status, _) = send(
        router.clone(),
        "POST",
        &format!("/alpha/mcp?sessionId={}", beta.id),
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string(),
        Some(API_KEY),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Route beta + session alpha.
    let (status, _) = send(
        router.clone(),
        "POST",
        &format!("/beta/mcp?sessionId={}", alpha.id),
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string(),
        Some(API_KEY),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The binding verifier itself: a caller claiming the same agent but a
/// different workspace id must drive a dedicated workspace-mismatch
/// failure, distinct from the agent-mismatch one, so footprints in the
/// audit log identify which substitution was attempted.
#[test]
fn workspace_mismatch_is_a_distinct_failure() {
    let agent = AgentId::new_checked("alpha").unwrap();
    let session_id = agent_workspace_hub::core::identity::SessionId::new();
    let binding = agent_workspace_hub::core::identity::SessionIdentity {
        session_id,
        agent_id: agent.clone(),
        workspace_id: WorkspaceId::new(),
    };
    let caller = agent_workspace_hub::mcp::agent_route::CallerContext {
        agent_id: agent,
        workspace_id: WorkspaceId::new(), // different workspace than the binding
        session_id: None,
    };
    match verify_session_binding(&binding, &caller) {
        Err(RouteError::SessionWorkspaceMismatch) => {}
        other => panic!("expected SessionWorkspaceMismatch, got {other:?}"),
    }
}

#[tokio::test]
async fn session_binding_records_route_agent_and_workspace() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    let (_router, state) = harness(&ws).await;
    let ws_id = ws.workspace_id();
    let (caller, _) = resolve_route_agent(&ws.root, "alpha").unwrap();
    let session = state.sessions.create_with_binding("/mcp", &caller).await;
    let binding = session
        .binding
        .expect("route-bound session records the caller");
    assert_eq!(binding.agent_id.as_str(), "alpha");
    assert_eq!(binding.workspace_id.as_str(), ws_id);
    assert_eq!(binding.session_id.as_str(), session.id);
    verify_session_binding(&binding, &caller).expect("same-context revalidation succeeds");
}

// ---------------------------------------------------------------------------
// Discovery filtering & per-call authorization
// ---------------------------------------------------------------------------

/// tools/list for a bound caller advertises only tools whose declared
/// required permissions are all covered by that caller's live grants.
/// Statically registered tools outside the caller's capability set are
/// omitted; dynamic provider tools are not statically matched and stay.
#[tokio::test]
async fn tools_list_filters_to_callers_capabilities() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    ws.grant("g-fs", "alpha", Permission::Filesystem, None, None);
    let (_router, state) = harness(&ws).await;
    let (session, mut rx) = initialized_bound_session(&state, &ws, "alpha").await;

    let value = dispatch_on(
        &state,
        &session,
        &mut rx,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    let tools: Vec<&str> = value["result"]["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(
        tools.contains(&"workspace.write_file"),
        "alpha holds Filesystem; write_file must appear: {tools:?}"
    );
    assert!(
        !tools.contains(&"connector.composio_accounts"),
        "alpha holds no Network grant; composio_accounts must be hidden: {tools:?}"
    );
}

/// A tool hidden from `tools/list` (because the caller lacks the grants)
/// still re-authorizes — and fails closed — when called directly by name.
/// Discovery filtering is a separate layer from authorization.
#[tokio::test]
async fn hidden_tool_direct_call_still_denied() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    // No grants at all.
    let (_router, state) = harness(&ws).await;
    let (session, mut rx) = initialized_bound_session(&state, &ws, "alpha").await;

    let value = dispatch_on(
        &state,
        &session,
        &mut rx,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "connector.composio_accounts",
                "arguments": {},
            },
        }),
    )
    .await;
    assert_eq!(
        value["error"]["code"], -32005,
        "hidden tool re-authorizes and fails closed: {value}"
    );
    let message = value["error"]["message"].as_str().unwrap();
    // The exact casing comes from Permission::as_str ("network"), so
    // comparisons must not depend on the display name.
    assert!(
        message.contains("alpha")
            && message.contains("connector.composio_accounts")
            && message.to_lowercase().contains("network"),
        "denial names agent, tool, and missing capability: {message}"
    );
}

/// A scoped grant only covers the resource prefix it names — a call
/// against another path prefix is denied even though the grant is valid
/// and unexpired.
#[tokio::test]
async fn scoped_grant_denies_out_of_scope_path() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    ws.grant("g-fs", "alpha", Permission::Filesystem, Some("src"), None);
    let (_router, state) = harness(&ws).await;
    let (session, mut rx) = initialized_bound_session(&state, &ws, "alpha").await;

    let value = dispatch_on(
        &state,
        &session,
        &mut rx,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "workspace.write_file",
                "arguments": {"path": "etc/evil.txt", "content": "x"},
            },
        }),
    )
    .await;
    assert_eq!(
        value["error"]["code"], -32005,
        "out-of-scope path is denied by the capability gate: {value}"
    );
}

/// An expired grant never authorizes: expiry is evaluated at call time.
#[tokio::test]
async fn expired_grant_denies_tools_call() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    ws.grant(
        "g-old",
        "alpha",
        Permission::Filesystem,
        None,
        Some("2001-01-01T00:00:00+00:00"),
    );
    let (_router, state) = harness(&ws).await;
    let (session, mut rx) = initialized_bound_session(&state, &ws, "alpha").await;

    let value = dispatch_on(
        &state,
        &session,
        &mut rx,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "workspace.write_file",
                "arguments": {"path": "src/ok.txt", "content": "x"},
            },
        }),
    )
    .await;
    assert_eq!(
        value["error"]["code"], -32005,
        "expired grant never authorizes: {value}"
    );
}

/// The positive path: an agent holding a valid, unexpired, in-scope grant
/// actually *succeeds* — the capability gate must not over-deny. This
/// also pins the layering: the capability gate passes and the SEC-001
/// trust floor independently allows this Medium-risk built-in (no trust
/// record → no restriction for Medium), so the tool runs and mutates
/// the workspace.
#[tokio::test]
async fn authorized_tool_succeeds_for_bound_caller() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    ws.grant("g-fs", "alpha", Permission::Filesystem, Some("src"), None);
    let (_router, state) = harness(&ws).await;
    let (session, mut rx) = initialized_bound_session(&state, &ws, "alpha").await;

    let value = dispatch_on(
        &state,
        &session,
        &mut rx,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "workspace.write_file",
                "arguments": {"path": "src/ok.txt", "content": "capability-gated write"},
            },
        }),
    )
    .await;
    // The MCP result envelope wraps the tool's JSON as a text content
    // block — unwrap it to assert the tool's own payload.
    let payload: Value = {
        let text = value["result"]["content"][0]["text"]
            .as_str()
            .expect("tool result is a text content block");
        serde_json::from_str(text).expect("tool payload is JSON")
    };
    assert_eq!(payload["written"], json!(true), "{value}");

    // The write really happened on disk — the tool did not merely report
    // success.
    let written = tokio::fs::read_to_string(ws.root.join("src/ok.txt"))
        .await
        .expect("file exists");
    assert_eq!(written, "capability-gated write");
}

/// Prompt §24 concurrency: two agents hold independent sessions over
/// their own routes simultaneously, each dispatch is authorized against
/// exactly its own agent's grants, and neither can see or use the
/// other's session. Both dispatched calls run concurrently and settle
/// independently.
#[tokio::test]
async fn concurrent_agents_keep_independent_sessions_and_decisions() {
    let ws = Workspace::new([
        ("alpha", AgentStatus::Active),
        ("beta", AgentStatus::Active),
    ]);
    // alpha may write under src/; beta holds no Filesystem grant at all.
    ws.grant(
        "g-alpha-fs",
        "alpha",
        Permission::Filesystem,
        Some("src"),
        None,
    );
    let (_router, state) = harness(&ws).await;

    let (alpha_session, mut alpha_rx) = initialized_bound_session(&state, &ws, "alpha").await;
    let (beta_session, mut beta_rx) = initialized_bound_session(&state, &ws, "beta").await;
    assert_ne!(alpha_session.id, beta_session.id);
    assert_ne!(
        alpha_session.binding.as_ref().unwrap().agent_id,
        beta_session.binding.as_ref().unwrap().agent_id
    );

    // Both calls in flight at the same time: alpha's succeeds, beta's is
    // capability-denied. Each decision comes from the caller's own
    // grants — never the other agent's.
    let (alpha_outcome, beta_outcome) = tokio::join!(
        dispatch_on(
            &state,
            &alpha_session,
            &mut alpha_rx,
            json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {"name": "workspace.write_file",
                           "arguments": {"path": "src/alpha.txt", "content": "a"}},
            })
        ),
        dispatch_on(
            &state,
            &beta_session,
            &mut beta_rx,
            json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {"name": "workspace.write_file",
                           "arguments": {"path": "src/beta.txt", "content": "b"}},
            })
        ),
    );
    assert!(
        alpha_outcome["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("\"written\":true"),
        "{alpha_outcome}"
    );
    assert_eq!(
        beta_outcome["error"]["code"], -32005,
        "beta's call is denied by beta's own (missing) grants: {beta_outcome}"
    );
    assert!(
        beta_outcome["error"]["message"]
            .as_str()
            .unwrap()
            .contains("beta"),
        "denial names the resolved agent: {beta_outcome}"
    );

    // Isolation on disk: alpha's write landed, beta's never executed.
    assert!(ws.root.join("src/alpha.txt").exists());
    assert!(!ws.root.join("src/beta.txt").exists());
}

/// Policy denial overrides capability allowance: even with a valid
/// unexpired grant, a workspace deny-rule still wins (SEC-001 deny floor
/// is applied after the capability gate, so both agree — capability is an
/// additional boundary, never a bypass).
#[tokio::test]
async fn policy_deny_overrides_capability_allowance() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    ws.grant("g-fs", "alpha", Permission::Filesystem, None, None);
    // Insert a workspace deny-rule for this tool.
    agent_workspace_hub::core::policy::PolicyStore::new(&ws.root)
        .add(&agent_workspace_hub::models::PolicyRule {
            id: "deny-secret".into(),
            tool: "workspace.write_file".into(),
            pattern: "src/secret.txt".into(),
            reason: Some("policy override test".into()),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .expect("policy deny");
    let (_router, state) = harness(&ws).await;
    let (session, mut rx) = initialized_bound_session(&state, &ws, "alpha").await;

    let value = dispatch_on(
        &state,
        &session,
        &mut rx,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "workspace.write_file",
                "arguments": {"path": "src/secret.txt", "content": "x"},
            },
        }),
    )
    .await;
    assert_eq!(
        value["error"]["code"], -32004,
        "deny-rule wins over capability allowance: {value}"
    );
}

// ---------------------------------------------------------------------------
// Legacy unscoped route invariance
// ---------------------------------------------------------------------------

/// The legacy global `/sse` + `/mcp` route surface is unchanged: a
/// session created through it has no agent binding and is NOT subject to
/// the per-agent capability gate (it relies on the SEC-001 workspace
/// trust floor alone).
#[tokio::test]
async fn legacy_unscoped_session_has_no_binding() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    let (_router, state) = harness(&ws).await;
    let session = state.sessions.create("/mcp").await;
    assert!(session.binding.is_none(), "unscoped session has no binding");
    assert!(session.lifecycle.caller().is_none());
}

/// Sanity of the full route surface: every published route exists (hits
/// auth first → 401 from middleware rather than 404 from the router).
#[tokio::test]
async fn routes_surface_check() {
    let ws = Workspace::new([("alpha", AgentStatus::Active)]);
    let (router, _) = harness(&ws).await;
    for (method, uri) in [
        ("GET", "/sse"),
        ("POST", "/mcp"),
        ("GET", "/alpha/sse"),
        ("POST", "/alpha/mcp"),
    ] {
        let (status, _) = send(router.clone(), method, uri, String::new(), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}
