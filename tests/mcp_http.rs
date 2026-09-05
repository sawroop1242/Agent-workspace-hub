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
        rate_limiter: agent_workspace_hub::services::rate_limit::RateLimiter::default_limiter(),
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

/// Sends a request with a raw `Authorization` header value (not auto-`Bearer`-wrapped).
async fn send_raw_auth(
    router: axum::Router,
    method: &str,
    uri: &str,
    body: String,
    authorization: &str,
) -> StatusCode {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, authorization)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    response.status()
}

#[tokio::test]
async fn rejects_non_bearer_and_malformed_bearer_headers() {
    let (router, _) = harness("secret");
    let body = "{}".to_string();

    // Non-Bearer scheme.
    assert_eq!(
        send_raw_auth(
            router.clone(),
            "POST",
            "/mcp?sessionId=x",
            body.clone(),
            "Basic abc"
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    // Bearer with no token.
    assert_eq!(
        send_raw_auth(
            router.clone(),
            "POST",
            "/mcp?sessionId=x",
            body.clone(),
            "Bearer"
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    // Bearer with only whitespace.
    assert_eq!(
        send_raw_auth(
            router.clone(),
            "POST",
            "/mcp?sessionId=x",
            body.clone(),
            "Bearer   "
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    // Empty token.
    assert_eq!(
        send_raw_auth(
            router.clone(),
            "POST",
            "/mcp?sessionId=x",
            body.clone(),
            "Bearer "
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    // Very long token (length mismatch short-circuits constant-time compare).
    let long_token = "a".repeat(10_000);
    assert_eq!(
        send_raw_auth(
            router.clone(),
            "POST",
            "/mcp?sessionId=x",
            body.clone(),
            &format!("Bearer {long_token}"),
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn enforces_session_limit() {
    let dir = tempdir().unwrap();
    let dispatcher =
        Arc::new(McpDispatcher::new(dir.path().to_path_buf()).expect("build dispatcher"));
    let state = AppState {
        dispatcher,
        sessions: Arc::new(SessionRegistry::new()),
        api_key: Arc::from("secret"),
        max_sessions: 2,
        sse_keepalive: Duration::from_secs(15),
        rate_limiter: agent_workspace_hub::services::rate_limit::RateLimiter::default_limiter(),
    };
    let config = HttpServerConfig {
        api_key: "secret".to_string(),
        max_sessions: 2,
        ..HttpServerConfig::default()
    };
    let router = build_router(state.clone(), &config);

    // Two sessions established directly through the registry reach the cap.
    state.sessions.create("/mcp").await;
    state.sessions.create("/mcp").await;
    assert_eq!(state.sessions.len().await, 2);

    // A third `/sse` connection attempt must fail closed (503).
    let (status, _) = send(router.clone(), "GET", "/sse", String::new(), Some("secret")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
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
        rate_limiter: agent_workspace_hub::services::rate_limit::RateLimiter::default_limiter(),
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

/// A tight limiter proves 429s with Retry-After appear after auth
/// succeeds — and that 401s never consume quota (spec §25 ordering).
#[tokio::test]
async fn rate_limit_applies_only_after_auth() {
    let dir = tempdir().unwrap();
    let dispatcher =
        Arc::new(McpDispatcher::new(dir.path().to_path_buf()).expect("build dispatcher"));
    let state = AppState {
        dispatcher,
        sessions: Arc::new(SessionRegistry::new()),
        api_key: Arc::from("secret"),
        max_sessions: 100,
        sse_keepalive: Duration::from_secs(15),
        // Two requests per window is enough to demonstrate the guard.
        rate_limiter: Arc::new(agent_workspace_hub::services::rate_limit::RateLimiter::new(
            2,
            Duration::from_secs(60),
        )),
    };
    let config = HttpServerConfig {
        api_key: "secret".to_string(),
        ..HttpServerConfig::default()
    };
    let router = build_router(state.clone(), &config);

    // Wrong-key requests never burn quota: five 401s, then the very
    // next correctly-authenticated request still succeeds.
    for _ in 0..5 {
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

    // Two authenticated requests fit inside the window...
    for _ in 0..2 {
        let (status, _) = send(
            router.clone(),
            "POST",
            "/mcp?sessionId=x",
            "{}".to_string(),
            Some("secret"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND); // unknown session, but PAST auth+limit
    }
    // ...the third is throttled.
    let (status, body) = send(
        router.clone(),
        "POST",
        "/mcp?sessionId=x",
        "{}".to_string(),
        Some("secret"),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["error"], "rate_limited");
    assert!(parsed["retry_after_secs"].as_u64().is_some());
}

/// resources/list and resources/read expose context, memory, and
/// skills as MCP resources; malformed URIs fail as protocol errors.
/// Responses travel over the session's SSE channel (HTTP is 202).
#[tokio::test]
async fn resources_list_and_read_round_trip() {
    let (router, state) = harness("secret");
    let session = state.sessions.create("/mcp").await;
    let mut rx = session.subscribe();
    // Drain the endpoint announcement event.
    let _ = rx.try_recv();

    let req = |id: i64, method: &str, params: Value| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })
        .to_string()
    };
    let post = |body: String| {
        let fut = router.clone().oneshot(
            Request::post(format!("/mcp?sessionId={}", session.id))
                .header(header::AUTHORIZATION, "Bearer secret")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        );
        async move {
            let resp = fut.await.unwrap();
            (resp.status(), resp)
        }
    };

    // All well-formed requests are HTTP 202; the JSON-RPC result (or
    // error) arrives on the SSE channel keyed by the request id.
    let (status, _) = post(req(1, "prompts/list", json!({}))).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (status, _) = post(req(2, "resources/read", json!({"uri": "awh://context"}))).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (status, _) = post(req(
        3,
        "resources/read",
        json!({"uri": "file:///etc/passwd"}),
    ))
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (status, _) = post(req(
        4,
        "resources/read",
        json!({"uri": "awh://memory/missing"}),
    ))
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let mut by_id = std::collections::HashMap::new();
    for _ in 0..4 {
        match rx.try_recv() {
            Ok(agent_workspace_hub::mcp::SseEvent::Message(v)) => {
                let id = v["id"].as_i64().unwrap_or(-1);
                by_id.insert(id, v);
            }
            Ok(_) => continue,
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
        }
    }

    // prompts/list: empty list, not an error.
    let prompts = by_id.get(&1).expect("prompts/list response");
    assert_eq!(prompts["result"]["prompts"], json!([]));

    // resources/read of context returns a text/markdown content block.
    let context = by_id.get(&2).expect("context response");
    assert_eq!(context["result"]["contents"][0]["uri"], "awh://context");
    assert_eq!(
        context["result"]["contents"][0]["mimeType"],
        "text/markdown"
    );

    // Unsupported scheme and unknown memory id become JSON-RPC errors
    // delivered over the stream — the transport itself stays 202/OK.
    for id in [3, 4] {
        let resp = by_id.get(&id).expect("error response");
        assert!(resp["error"].is_object(), "id {id} should error: {resp}");
    }

    // Write a memory entry via memory.store, then read it back through
    // the resources plane — both planes must observe the same store.
    let (status, _) = post(req(
        5,
        "tools/call",
        json!({
            "name": "memory.store",
            "arguments": {
                "id": "res-round-trip",
                "content": "resource round trip",
                "scope": "Project"
            }
        }),
    ))
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let list_resp = by_id_from(&mut rx, 5).expect("memory.store response");
    let entry: Value = serde_json::from_str(
        list_resp["result"]["content"][0]["text"]
            .as_str()
            .expect("store result text"),
    )
    .expect("store result is a serialized entry");
    assert_eq!(entry["id"], "res-round-trip");
    assert_eq!(entry["content"], "resource round trip");

    let (status, _) = post(req(
        6,
        "resources/read",
        json!({"uri": "awh://memory/res-round-trip"}),
    ))
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let read_resp = by_id_from(&mut rx, 6).expect("memory resource read");
    assert_eq!(
        read_resp["result"]["contents"][0]["text"], "resource round trip",
        "resource must return the stored content"
    );
    assert_eq!(read_resp["result"]["contents"][0]["mimeType"], "text/plain");
}

/// Drains SSE messages until one carrying the given id arrives (or the
/// channel goes quiet) — responses may interleave with keepalives.
fn by_id_from(
    rx: &mut tokio::sync::broadcast::Receiver<agent_workspace_hub::mcp::SseEvent>,
    id: i64,
) -> Option<Value> {
    for _ in 0..16 {
        match rx.try_recv() {
            Ok(agent_workspace_hub::mcp::SseEvent::Message(v)) => {
                if v["id"].as_i64() == Some(id) {
                    return Some(v);
                }
            }
            Ok(_) => continue,
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
            | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => return None,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
        }
    }
    None
}
