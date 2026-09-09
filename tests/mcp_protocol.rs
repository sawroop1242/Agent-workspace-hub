//! Protocol-correctness tests for the MCP plane: JSON-RPC 2.0 notification
//! semantics, MCP version negotiation, error-code mapping, and the session
//! state machine (NEW → INITIALIZING → READY → CLOSING/CLOSED/FAILED).
//!
//! These pin behaviors the master prompt requires explicit tests for:
//! - Requests missing `id` are notifications: the method is observed but NO
//!   response is ever emitted — including for `tools/call`.
//! - Unsupported/missing/malformed protocol versions negotiate per spec.
//! - Session gating: pre-init requests are rejected, duplicate `initialize`
//!   is rejected, post-close requests are rejected, and no state leaks
//!   between sessions.

use agent_workspace_hub::mcp::{
    McpDispatcher, SessionLifecycle, SessionState, SUPPORTED_PROTOCOL_VERSIONS,
};
use serde_json::{json, Value};
use tempfile::tempdir;

async fn new_dispatcher() -> (McpDispatcher, tempfile::TempDir) {
    let dir = tempdir().expect("tempdir");
    let dispatcher = McpDispatcher::new_async(dir.path().to_path_buf())
        .await
        .expect("dispatcher");
    (dispatcher, dir)
}

fn request(id: Option<Value>, method: &str, params: Value) -> String {
    let mut body = json!({"jsonrpc": "2.0", "method": method, "params": params});
    if let Some(id) = id {
        body["id"] = id;
    } else {
        body.as_object_mut().unwrap().remove("id");
    }
    body.to_string()
}

async fn dispatch(dispatcher: &McpDispatcher, input: &str, lifecycle: &SessionLifecycle) -> Value {
    use agent_workspace_hub::mcp::DispatchResult;
    match dispatcher.dispatch_with_lifecycle(input, lifecycle).await {
        DispatchResult::Response(resp) => serde_json::to_value(&resp).expect("serialize response"),
        DispatchResult::NoResponse => Value::Null,
    }
}

fn init_params() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "protocol-test", "version": "0.0.1"}
    })
}

// --------------------------------------------------------------------------
// Notification semantics (JSON-RPC 2.0 §"Notification")
// --------------------------------------------------------------------------

#[tokio::test]
async fn tools_call_notification_produces_no_response() {
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input = request(
        None,
        "tools/call",
        json!({"name": "memory.search", "arguments": {"query": "x"}}),
    );
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert!(
        response.is_null(),
        "a tools/call notification must not emit a response, got: {response}"
    );
    // The notification must not have initialized the session either.
    assert_eq!(lifecycle.state(), SessionState::New);
}

#[tokio::test]
async fn notifications_never_get_responses_across_the_method_surface() {
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    // The exact method does not matter: anything without an `id` is a
    // notification and can never be answered.
    for method in [
        "notifications/initialized",
        "notifications/cancelled",
        "tools/call",
        "resources/read",
        "resources/list",
        "prompts/get",
        "prompts/list",
        "ping",
        "completely/unknown",
    ] {
        let params = if method == "resources/read" {
            json!({"uri": "awh://memory"})
        } else {
            json!({})
        };
        let input = request(None, method, params);
        let response = dispatch(&dispatcher, &input, &lifecycle).await;
        assert!(
            response.is_null(),
            "notification '{method}' must not emit a response, got: {response}"
        );
    }
}

#[tokio::test]
async fn invalid_notification_is_still_silent() {
    // A parseable message with a WRONG jsonrpc version but no `id` is a
    // notification; the spec forbids replying to notifications, so the
    // wrong version must not trigger an error response either.
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input = r#"{"jsonrpc":"1.0","method":"tools/call","params":{"name":"memory.list"}}"#;
    let response = dispatch(&dispatcher, input, &lifecycle).await;
    assert!(
        response.is_null(),
        "invalid notification must not emit a response, got: {response}"
    );
}

#[tokio::test]
async fn unparseable_input_yields_parse_error_with_null_id() {
    // Not JSON at all: the server cannot know whether it was a notification,
    // so a -32700 response with id null is the spec-compliant answer.
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let response = dispatch(&dispatcher, "{ not json", &lifecycle).await;
    assert_eq!(response["id"], Value::Null);
    assert_eq!(response["error"]["code"], -32700);
}

#[tokio::test]
async fn invalid_request_version_with_id_gets_invalid_request_code() {
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input = r#"{"jsonrpc":"1.0","id":7,"method":"initialize","params":{}}"#;
    let response = dispatch(&dispatcher, input, &lifecycle).await;
    assert_eq!(response["id"], 7);
    assert_eq!(response["error"]["code"], -32600);
}

// --------------------------------------------------------------------------
// Version negotiation matrix
// --------------------------------------------------------------------------

#[tokio::test]
async fn version_negotiation_matrix() {
    for (requested, expected) in [
        // Supported versions are echoed exactly (older-compatible included).
        ("2025-06-18", "2025-06-18"),
        ("2025-03-26", "2025-03-26"),
        ("2024-11-05", "2024-11-05"),
        // Unsupported/newer/garbage → server's latest supported version.
        ("2030-01-01", "2025-06-18"),
        ("2023-01-01", "2025-06-18"),
        ("totally-not-a-version", "2025-06-18"),
    ] {
        let (dispatcher, _dir) = new_dispatcher().await;
        let lifecycle = SessionLifecycle::default();
        let input = request(
            Some(json!(1)),
            "initialize",
            json!({"protocolVersion": requested}),
        );
        let response = dispatch(&dispatcher, &input, &lifecycle).await;
        assert_eq!(
            response["result"]["protocolVersion"], expected,
            "requesting {requested} should negotiate {expected}, got: {response}"
        );
    }
    // Missing protocolVersion → server's latest supported version.
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input = request(Some(json!(1)), "initialize", json!({}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
    assert!(!SUPPORTED_PROTOCOL_VERSIONS.is_empty());

    // Malformed (non-string) protocolVersion is a -32602 invalid-params error.
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input = request(Some(json!(1)), "initialize", json!({"protocolVersion": 42}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32602);
}

// --------------------------------------------------------------------------
// Session state machine
// --------------------------------------------------------------------------

#[tokio::test]
async fn session_state_machine_transitions() {
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();

    // NEW: pre-init requests are rejected with -32002 (initialize and ping
    // are the only pre-init requests allowed).
    assert_eq!(lifecycle.state(), SessionState::New);
    let input = request(Some(json!(1)), "tools/list", json!({}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32002);

    // initialize → READY, and session metadata is captured.
    let input = request(Some(json!(2)), "initialize", init_params());
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert!(response["result"].is_object(), "got: {response}");
    assert_eq!(lifecycle.state(), SessionState::Ready);
    assert_eq!(
        lifecycle.negotiated_version().as_deref(),
        Some("2025-06-18")
    );
    assert_eq!(lifecycle.client_info().unwrap()["name"], "protocol-test");

    // READY: tools/list now works.
    let input = request(Some(json!(3)), "tools/list", json!({}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert!(response["result"]["tools"].is_array());

    // Duplicate initialize is rejected, state stays READY.
    let input = request(Some(json!(4)), "initialize", init_params());
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32600);
    assert_eq!(lifecycle.state(), SessionState::Ready);

    // CLOSING is still admitted (graceful drain), then CLOSED rejects.
    lifecycle.touch();
    let input = request(Some(json!(5)), "ping", json!({}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert!(response["result"].is_object());
    lifecycle.mark_closed();
    assert_eq!(lifecycle.state(), SessionState::Closed);
    let input = request(Some(json!(6)), "tools/list", json!({}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32600);
    assert_eq!(response["error"]["message"], "session is closed");

    // FAILED also rejects.
    let lifecycle = SessionLifecycle::default();
    lifecycle.mark_failed();
    assert_eq!(lifecycle.state(), SessionState::Failed);
    let input = request(Some(json!(1)), "initialize", init_params());
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32600);
}

#[tokio::test]
async fn failed_initialize_leaves_session_retryable() {
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    // Malformed initialize (non-object params) fails the exchange...
    let input = request(Some(json!(1)), "initialize", json!([1, 2, 3]));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32602);
    // ...and the session stays uninitialized, so the client can retry.
    assert_eq!(lifecycle.state(), SessionState::New);
    let input = request(Some(json!(2)), "initialize", init_params());
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert!(response["result"].is_object());
    assert_eq!(lifecycle.state(), SessionState::Ready);
}

#[tokio::test]
async fn sessions_are_isolated_no_state_leakage() {
    // Two concurrent sessions over one dispatcher: each has its own
    // lifecycle; initializing one must not admit the other's requests.
    let (dispatcher, _dir) = new_dispatcher().await;
    let session_a = SessionLifecycle::default();
    let session_b = SessionLifecycle::default();

    // Initialize only session A.
    let input = request(Some(json!(1)), "initialize", init_params());
    let response = dispatch(&dispatcher, &input, &session_a).await;
    assert!(response["result"].is_object());

    // Session B is still NEW: its tools/list must be -32002.
    let input = request(Some(json!(2)), "tools/list", json!({}));
    let response = dispatch(&dispatcher, &input, &session_b).await;
    assert_eq!(
        response["error"]["code"], -32002,
        "session B must not inherit session A's initialization"
    );
    assert_eq!(session_a.state(), SessionState::Ready);
    assert_eq!(session_b.state(), SessionState::New);

    // Closing session A must not affect session B.
    session_a.mark_closed();
    let input = request(Some(json!(3)), "tools/list", json!({}));
    let response = dispatch(&dispatcher, &input, &session_b).await;
    assert_eq!(response["error"]["code"], -32002);
}

// --------------------------------------------------------------------------
// tools/call argument validation (schema-first)
// --------------------------------------------------------------------------

#[tokio::test]
async fn tools_call_argument_failures_are_invalid_params_not_internal() {
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input = request(Some(json!(1)), "initialize", init_params());
    dispatch(&dispatcher, &input, &lifecycle).await;

    // Missing required argument → -32602 invalid params (was -32603 before
    // schema-first validation).
    let input = request(
        Some(json!(2)),
        "tools/call",
        json!({"name": "workspace.read_file", "arguments": {}}),
    );
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(
        response["error"]["code"], -32602,
        "schema mismatch must map to invalid params, got: {response}"
    );
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("missing required field 'path'"));

    // Wrong type for a typed argument → -32602.
    let input = request(
        Some(json!(3)),
        "tools/call",
        json!({"name": "workspace.read_file", "arguments": {"path": 42}}),
    );
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32602);

    // Non-object arguments → -32602.
    let input = request(
        Some(json!(4)),
        "tools/call",
        json!({"name": "workspace.read_file", "arguments": ["not", "an", "object"]}),
    );
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32602);

    // Empty/missing name → -32602, before any dispatch.
    let input = request(Some(json!(5)), "tools/call", json!({"arguments": {}}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32602);

    // Unknown tool stays -32602 (existing behavior).
    let input = request(
        Some(json!(6)),
        "tools/call",
        json!({"name": "doesnotexist", "arguments": {}}),
    );
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert_eq!(response["error"]["code"], -32602);

    // A valid call still succeeds and reports tool metrics.
    let input = request(
        Some(json!(7)),
        "tools/call",
        json!({"name": "memory.store", "arguments": {"id": "m", "content": "c", "scope": "Project"}}),
    );
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert!(response["result"].is_object(), "got: {response}");
}

// --------------------------------------------------------------------------
// mcp.status health tool + metrics
// --------------------------------------------------------------------------

#[tokio::test]
async fn mcp_status_reports_bounded_health_snapshot() {
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input = request(Some(json!(1)), "initialize", init_params());
    dispatch(&dispatcher, &input, &lifecycle).await;

    // Successful and failed calls populate metrics.
    let input = request(
        Some(json!(2)),
        "tools/call",
        json!({"name": "memory.search", "arguments": {"query": "anything"}}),
    );
    dispatch(&dispatcher, &input, &lifecycle).await;
    let input = request(
        Some(json!(3)),
        "tools/call",
        json!({"name": "workspace.read_file", "arguments": {}}),
    );
    dispatch(&dispatcher, &input, &lifecycle).await;

    let input = request(Some(json!(4)), "tools/call", json!({"name": "mcp.status"}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    // tools/call results arrive in an MCP content envelope; unwrap the
    // JSON text blob.
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("mcp.status must return text content, got: {response}"));
    let status: Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("mcp.status text is not JSON ({e}): {text}"));
    assert_eq!(status["status"], "healthy");
    assert_eq!(status["protocol_versions"].as_array().unwrap().len(), 3);
    assert!(status["tools"]["static"].as_u64().unwrap() >= 53);
    assert!(
        status["tools"]["total"].as_u64().unwrap() >= status["tools"]["static"].as_u64().unwrap()
    );
    assert!(status["metrics"]["tool_calls"].as_u64().unwrap() >= 2);
    assert!(status["metrics"]["tool_failures"].as_u64().unwrap() >= 1);
    // No secrets: the status payload is small and fixed-shape.
    assert!(text.len() < 2048, "status must stay bounded");
}

// --------------------------------------------------------------------------
// Hooks (observer-only lifecycle events)
// --------------------------------------------------------------------------

#[tokio::test]
async fn hooks_observe_lifecycle_without_authority() {
    use agent_workspace_hub::mcp::{McpEvent, ToolMetricsSnapshot};
    use std::sync::{Arc, Mutex};

    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&events);
    dispatcher
        .register_hook(Arc::new(move |event: &McpEvent<'_>| {
            let mut seen = seen.lock().unwrap();
            match event {
                McpEvent::NotificationReceived { method } => seen.push(format!("note:{method}")),
                McpEvent::InitializeCompleted {
                    protocol_version, ..
                } => seen.push(format!("init:{protocol_version}")),
                McpEvent::ToolCallCompleted { name, ok, .. } => {
                    seen.push(format!("tool:{name}:{}", ok))
                }
                McpEvent::ResourceRead { uri } => seen.push(format!("resource:{uri}")),
                McpEvent::PromptRequested { name } => seen.push(format!("prompt:{name}")),
            }
        }))
        .expect("hook registration");

    // initialize → InitializeCompleted hook fires with the negotiated version.
    let input = request(Some(json!(1)), "initialize", init_params());
    dispatch(&dispatcher, &input, &lifecycle).await;
    // notification → NotificationReceived hook fires, still no response.
    let input = request(None, "notifications/initialized", json!({}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert!(response.is_null());
    // tool call → ToolCallCompleted (ok).
    let input = request(
        Some(json!(2)),
        "tools/call",
        json!({"name": "memory.search", "arguments": {"query": "anything"}}),
    );
    dispatch(&dispatcher, &input, &lifecycle).await;
    // tool call failure → ToolCallCompleted (!ok).
    let input = request(
        Some(json!(3)),
        "tools/call",
        json!({"name": "workspace.read_file", "arguments": {}}),
    );
    dispatch(&dispatcher, &input, &lifecycle).await;
    // resource read → ResourceRead hook.
    let input = request(
        Some(json!(4)),
        "resources/read",
        json!({"uri": "awh://context"}),
    );
    dispatch(&dispatcher, &input, &lifecycle).await;
    // prompt request → PromptRequested hook.
    let input = request(Some(json!(5)), "prompts/get", json!({"name": "summarize"}));
    dispatch(&dispatcher, &input, &lifecycle).await;

    let seen = events.lock().unwrap().clone();
    assert!(
        seen.contains(&"init:2025-06-18".to_string()),
        "got: {seen:?}"
    );
    assert!(
        seen.contains(&"note:notifications/initialized".to_string()),
        "got: {seen:?}"
    );
    assert!(seen.contains(&"tool:memory.search:true".to_string()));
    assert!(seen.contains(&"tool:workspace.read_file:false".to_string()));
    assert!(seen.contains(&"resource:awh://context".to_string()));
    assert!(seen.contains(&"prompt:summarize".to_string()));

    // Hooks never block or change dispatch: metrics still recorded.
    let metrics: Vec<ToolMetricsSnapshot> = dispatcher.metrics().snapshot();
    assert!(metrics
        .iter()
        .any(|m| m.name == "memory.search" && m.calls == 1));
    assert!(metrics
        .iter()
        .any(|m| m.name == "workspace.read_file" && m.failures == 1));
}

// --------------------------------------------------------------------------
// Batch behavior (documented pin)
// --------------------------------------------------------------------------

#[tokio::test]
async fn json_rpc_batches_are_rejected_as_parse_errors() {
    // MCP (2025-06-18) does not use JSON-RPC batching. A JSON array is not
    // a valid single RpcRequest, so it surfaces as -32700 parse error with
    // id null. This pins the behavior explicitly rather than leaving it
    // undefined.
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input =
        r#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","id":2,"method":"ping"}]"#;
    let response = dispatch(&dispatcher, input, &lifecycle).await;
    assert_eq!(response["id"], Value::Null);
    assert_eq!(response["error"]["code"], -32700);
}

// --------------------------------------------------------------------------
// tools/list carries catalog metadata
// --------------------------------------------------------------------------

#[tokio::test]
async fn tools_list_carries_category_and_version_metadata() {
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input = request(Some(json!(1)), "initialize", init_params());
    dispatch(&dispatcher, &input, &lifecycle).await;

    let input = request(Some(json!(2)), "tools/list", json!({}));
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    let tools = response["result"]["tools"].as_array().unwrap().clone();
    assert!(tools.len() >= 53);
    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        assert!(
            tool["category"].as_str().is_some(),
            "tool {name} must carry a category"
        );
        assert!(
            tool["version"].as_str().is_some(),
            "tool {name} must carry a version"
        );
    }
    // Spot-check category assignments on static catalog entries.
    let by_name = |name: &str| {
        tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("tool {name} missing"))
            .clone()
    };
    assert_eq!(by_name("mcp.status")["category"], "system");
    assert_eq!(by_name("git.status")["category"], "git");
    assert_eq!(by_name("memory.store")["category"], "memory");
    assert_eq!(by_name("skills.list")["category"], "skills");
    // github.* tools appear only when a token is configured; if present
    // they must still carry their category.
    if let Some(github_tool) = tools
        .iter()
        .find(|t| t["name"].as_str().is_some_and(|n| n.starts_with("github.")))
    {
        assert_eq!(github_tool["category"], "github");
    }
}

#[tokio::test]
async fn resources_read_errors_are_invalid_params_not_internal() {
    // resources/read has no JSON schema (it is a protocol method, not a
    // tool), so the -32602 mapping for client-side addressing mistakes is
    // pinned by hand: missing uri, wrong-typed uri, non-awh:// scheme,
    // unknown kind, and missing memory entry are all client errors.
    let (dispatcher, _dir) = new_dispatcher().await;
    let lifecycle = SessionLifecycle::default();
    let input = request(Some(json!(0)), "initialize", init_params());
    dispatch(&dispatcher, &input, &lifecycle).await;

    for (label, params, expect_message) in [
        ("missing uri", json!({}), "requires a string 'uri'"),
        (
            "non-string uri",
            json!({"uri": 42}),
            "requires a string 'uri'",
        ),
        (
            "foreign scheme",
            json!({"uri": "https://example.com/x"}),
            "unsupported resource uri",
        ),
        (
            "unknown kind",
            json!({"uri": "awh://nope"}),
            "unsupported resource kind",
        ),
        (
            "missing memory entry",
            json!({"uri": "awh://memory/does-not-exist"}),
            "memory entry not found",
        ),
    ] {
        let input = request(Some(json!(1)), "resources/read", params);
        let response = dispatch(&dispatcher, &input, &lifecycle).await;
        assert_eq!(
            response["error"]["code"], -32602,
            "{label} must map to invalid params, got: {response}"
        );
        let message = response["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains(expect_message),
            "{label}: message {message:?} must mention {expect_message:?}"
        );
    }
}
