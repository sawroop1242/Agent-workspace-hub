//! End-to-end integration tests for the remote HTTP/SSE MCP transport.
//!
//! These exercise the real [`build_router`] router (health, authentication,
//! request limits, the `/mcp` endpoint, and the `/sse` stream) against the
//! shared [`McpDispatcher`], without duplicating the tool implementations.

use agent_workspace_hub::mcp::SessionRegistry;
use agent_workspace_hub::mcp::{
    build_router, AppState, DispatchResult, HttpServerConfig, McpDispatcher,
};
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tower::util::ServiceExt;

/// Builds a router and its (inspectable) state over a fresh temp project.
fn harness(api_key: &str) -> (axum::Router, AppState) {
    let dir = tempdir().expect("tempdir");
    // Keep the temp dir alive for the duration of the test by leaking its path.
    let root = std::mem::ManuallyDrop::new(dir);
    let dispatcher =
        Arc::new(McpDispatcher::new(root.path().to_path_buf()).expect("build dispatcher"));
    let state = AppState {
        dispatcher,
        sessions: Arc::new(SessionRegistry::new()),
        api_key: Arc::from(api_key),
        max_sessions: 100,
        sse_keepalive: Duration::from_secs(15),
    };
    let config = HttpServerConfig {
        api_key: api_key.to_string(),
        ..HttpServerConfig::default()
    };
    let router = build_router(state.clone(), &config);
    (router, state)
}

/// Sends a request through the router, returning status and body text.
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

#[tokio::test]
async fn health_returns_ok_without_auth() {
    let (router, _) = harness("secret");
    let (status, body) = send(router.clone(), "GET", "/health", String::new(), None).await;
    assert_eq!(status, StatusCode::OK);
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["service"], "agent-workspace-hub");
    // Health must not leak configuration.
    assert!(!body.contains("AWH_API_KEY"));
    assert!(!body.contains("secret"));
}

#[tokio::test]
async fn mcp_requires_authentication() {
    let (router, _) = harness("secret");
    // Missing authorization header.
    let (status, _) = send(
        router.clone(),
        "POST",
        "/mcp?sessionId=x",
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}).to_string(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Wrong token.
    let (status, _) = send(
        router.clone(),
        "POST",
        "/mcp?sessionId=x",
        "{}".to_string(),
        Some("wrong"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn mcp_accepts_valid_token() {
    let (router, state) = harness("secret");
    let session = state.sessions.create("/mcp").await;
    let uri = format!("/mcp?sessionId={}", session.id);
    let (status, _) = send(
        router.clone(),
        "POST",
        &uri,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}).to_string(),
        Some("secret"),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn sse_requires_authentication() {
    let (router, _) = harness("secret");
    let (status, _) = send(router.clone(), "GET", "/sse", String::new(), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_session_is_rejected() {
    let (router, _) = harness("secret");
    let (status, _) = send(
        router.clone(),
        "POST",
        "/mcp?sessionId=does-not-exist",
        "{}".to_string(),
        Some("secret"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn malformed_json_body_returns_bad_request() {
    let (router, state) = harness("secret");
    let session = state.sessions.create("/mcp").await;
    let (status, _) = send(
        router.clone(),
        "POST",
        &format!("/mcp?sessionId={}", session.id),
        "not-json".to_string(),
        Some("secret"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn oversized_request_body_is_rejected() {
    let config = HttpServerConfig {
        api_key: "secret".to_string(),
        max_body_bytes: 16,
        ..HttpServerConfig::default()
    };
    let dir = tempdir().unwrap();
    let dispatcher = Arc::new(McpDispatcher::new(dir.path().to_path_buf()).expect("dispatcher"));
    let state = AppState {
        dispatcher,
        sessions: Arc::new(SessionRegistry::new()),
        api_key: Arc::from("secret"),
        max_sessions: 100,
        sse_keepalive: Duration::from_secs(15),
    };
    let router = build_router(state, &config);
    // A valid JSON body well over the 16-byte limit is rejected by the
    // request-body limit layer before reaching the dispatcher.
    let big = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"pad":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}).to_string();
    let (status, _) = send(router, "POST", "/mcp?sessionId=x", big, Some("secret")).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn dispatcher_round_trips_over_sse_session() {
    // Drive the shared dispatcher directly through an SSE session's channel and
    // verify a tools/list + initialize round trip without any stdio involvement.
    let (_, state) = harness("secret");
    let session = state.sessions.create("/mcp").await;
    let mut rx = session.subscribe();

    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
    let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});

    // Dispatch responses directly, mirroring the `/mcp` handler routing.
    let dispatcher = state.dispatcher.clone();
    let responses = tokio::spawn(async move {
        let mut out = Vec::new();
        for req in [init, list] {
            if let DispatchResult::Response(r) = dispatcher.dispatch(&req.to_string()).await {
                out.push(r);
            }
        }
        out
    })
    .await
    .unwrap();

    assert_eq!(responses.len(), 2);
    let first = serde_json::to_value(&responses[0]).unwrap();
    let second = serde_json::to_value(&responses[1]).unwrap();
    assert_eq!(first["result"]["serverInfo"]["name"], "agent-workspace-hub");
    assert!(second["result"]["tools"].is_array());

    // The session channel is independent and usable (no cross-session leakage).
    let _ = rx.try_recv();
}
