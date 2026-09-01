//! HTTP/SSE transport for remotely exposing the MCP server.
//!
//! This module hosts the MCP [`McpDispatcher`] behind an [`axum`] server with:
//! - `GET /health` — liveness probe (unauthenticated, no secrets).
//! - `GET /sse`    — Server-Sent Events stream carrying an isolated session.
//! - `POST /mcp`   — JSON-RPC request submit for an SSE session.
//!
//! Remote access is mandatory bearer-token authenticated, bounded by request
//! size/time/connection limits, and exposes no secrets or internal paths in
//! error responses.

use crate::mcp::auth;
use crate::mcp::dispatcher::{DispatchResult, McpDispatcher};
use crate::mcp::sse::{SessionRegistry, SseEvent};
use crate::mcp::tls::TlsConfig;
use crate::mcp::{audit_allow, audit_deny};
use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

/// Configuration for the remote HTTP/SSE server.
#[derive(Debug, Clone)]
pub struct HttpServerConfig {
    /// Bind host (e.g. `0.0.0.0`).
    pub host: String,
    /// Bind port (e.g. `8443`).
    pub port: u16,
    /// TLS configuration (`None` disables TLS → plain HTTP).
    pub tls: TlsConfig,
    /// The expected bearer API key (required).
    pub api_key: String,
    /// Restrictive CORS allow-list; empty disables CORS entirely.
    pub allowed_origins: Vec<String>,
    /// Maximum accepted request body size in bytes.
    pub max_body_bytes: usize,
    /// Maximum number of concurrent active SSE sessions.
    pub max_sessions: usize,
    /// Per-request dispatch timeout.
    pub request_timeout: Duration,
    /// SSE stream keep-alive interval.
    pub sse_keepalive: Duration,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8443,
            tls: TlsConfig::default(),
            api_key: String::new(),
            allowed_origins: Vec::new(),
            max_body_bytes: 10 * 1024 * 1024,
            max_sessions: 100,
            request_timeout: Duration::from_secs(30),
            sse_keepalive: Duration::from_secs(15),
        }
    }
}

/// Shared application state passed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub dispatcher: Arc<McpDispatcher>,
    pub sessions: Arc<SessionRegistry>,
    pub api_key: Arc<str>,
    pub max_sessions: usize,
    pub sse_keepalive: Duration,
}

/// Serves the remote MCP server over HTTP (optionally TLS) until shutdown.
pub async fn serve(config: HttpServerConfig, dispatcher: Arc<McpDispatcher>) -> Result<()> {
    config.tls.validate()?;
    if config.api_key.is_empty() {
        anyhow::bail!("refusing to serve remote MCP without an API key");
    }

    let state = AppState {
        dispatcher,
        sessions: Arc::new(SessionRegistry::new()),
        api_key: Arc::from(config.api_key.as_str()),
        max_sessions: config.max_sessions,
        sse_keepalive: config.sse_keepalive,
    };

    let app = build_router(state, &config);

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    // Never log the API key; only log the bind address and TLS status.
    tracing::info!(event = "http_server_started", addr = %addr, tls = config.tls.enabled());

    let acceptor = config.tls.build_acceptor()?;

    match acceptor {
        Some(acceptor) => {
            let listener = TlsListener {
                inner: listener,
                acceptor,
            };
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .context("TLS HTTP server terminated with error")
        }
        None => axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .context("HTTP server terminated with error"),
    }
}

/// A [`axum::serve::Listener`] that wraps accepted TCP streams in TLS.
struct TlsListener {
    inner: tokio::net::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, addr) = match self.inner.accept().await {
                Ok(ok) => ok,
                Err(e) => {
                    tracing::warn!(event = "tls_accept_failed", error = %e);
                    continue;
                }
            };
            match self.acceptor.accept(stream).await {
                Ok(tls) => return (tls, addr),
                // Handshake failure (e.g. plaintext against a TLS port): drop
                // the stream and keep accepting. Never log key material.
                Err(_) => continue,
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

/// Builds the axum router with all middleware and routes.
pub fn build_router(state: AppState, config: &HttpServerConfig) -> Router {
    let cors = build_cors(config);

    // `/sse` and `/mcp` require a valid bearer token; `/health` does not.
    let protected = Router::new()
        .route("/sse", get(sse_handler))
        .route("/mcp", post(mcp_handler))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authenticate,
        ));

    Router::new()
        .route("/health", get(health))
        .merge(protected)
        .layer(cors)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            config.request_timeout,
        ))
        .layer(RequestBodyLimitLayer::new(config.max_body_bytes))
        .with_state(state)
}

/// Builds the CORS layer: restrictive by default, configurable allow-list.
fn build_cors(config: &HttpServerConfig) -> CorsLayer {
    if config.allowed_origins.is_empty() {
        // No CORS headers at all → browsers restrict cross-origin by default.
        CorsLayer::new()
            .allow_origin(AllowOrigin::list([]))
            .allow_methods([])
            .allow_headers([])
    } else {
        let origins: Vec<HeaderValue> = config
            .allowed_origins
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok())
            .collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::CONTENT_TYPE,
                axum::http::header::AUTHORIZATION,
            ])
    }
}

/// `GET /health` — unauthenticated liveness probe exposing no secrets.
async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "agent-workspace-hub",
    }))
}

/// Authentication middleware: rejects requests without a valid bearer token.
async fn authenticate(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| auth::bearer_token(Some(s)));

    match token {
        Some(t) if auth::verify_token(&state.api_key, t) => {
            audit_allow("http_auth", "remote", "success");
            next.run(request).await
        }
        _ => {
            audit_deny("http_auth", "invalid_or_missing_token", "remote");
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
                .into_response()
        }
    }
}

/// `GET /sse` — establishes an isolated SSE session and streams events.
async fn sse_handler(State(state): State<AppState>) -> Response {
    // Enforce the session limit (fail closed on exhaustion).
    if state.sessions.len().await >= state.max_sessions {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "too many sessions"})),
        )
            .into_response();
    }

    let session = state.sessions.create("/mcp").await;
    let session_id = session.id.clone();
    let receiver = session.subscribe();

    // The first event announces the client's dedicated POST endpoint.
    let endpoint_event = Event::default()
        .event("endpoint")
        .data(session.endpoint.clone());

    // Convert broadcast events into SSE frames.
    let stream = BroadcastStream::new(receiver).filter_map(|item| match item {
        Ok(SseEvent::Endpoint(url)) => Some(Ok(Event::default().event("endpoint").data(url))),
        Ok(SseEvent::Message(value)) => Some(Ok(Event::default()
            .event("message")
            .data(value.to_string()))),
        Err(_) => None, // broadcast lag: client must reconnect
    });

    let initial =
        futures_util::stream::once(async move { Ok::<Event, Infallible>(endpoint_event) });

    let keepalive = state.sse_keepalive;

    // Keep the session alive for exactly as long as the SSE stream is being
    // served, then remove it. The stream's `Drop` runs when the response body
    // is dropped — i.e. when the client disconnects or the connection closes —
    // so cleanup is tied to the actual stream lifetime rather than detached.
    let guarded = SessionGuard {
        inner: initial.chain(stream),
        registry: Arc::clone(&state.sessions),
        session_id,
    };

    Sse::new(guarded)
        .keep_alive(KeepAlive::new().interval(keepalive).text("keep-alive"))
        .into_response()
}

/// A [`Stream`] wrapper that removes its SSE session from the registry when the
/// stream is dropped.
///
/// `Sse` (via `axum`) drops the body stream when the client disconnects or the
/// connection is closed, which drops this guard and removes the session. This
/// keeps the session alive for exactly the duration the client is connected —
/// never shorter (the prior detached `tokio::spawn` could remove it before the
/// client used its endpoint) and never leaking a session after disconnect.
struct SessionGuard<S> {
    inner: S,
    registry: Arc<SessionRegistry>,
    session_id: String,
}

impl<S> Drop for SessionGuard<S> {
    fn drop(&mut self) {
        // Removal is async; spawn a task using the registry's own handle since
        // `Drop` cannot await. The registry is `Arc`-owned so it outlives this.
        let registry = Arc::clone(&self.registry);
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            registry.remove(&session_id).await;
        });
    }
}

impl<S> Stream for SessionGuard<S>
where
    S: Stream,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY: `SessionGuard` never structurally pins its fields and is not
        // `Drop`-pin-projected, so it is safe to project to the inner stream.
        unsafe {
            let this = self.get_unchecked_mut();
            Pin::new_unchecked(&mut this.inner).poll_next(cx)
        }
    }
}

/// `POST /mcp` — accepts a JSON-RPC message for an SSE session and dispatches it.
async fn mcp_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<McpQuery>,
    body: axum::body::Bytes,
) -> Response {
    let body = match std::str::from_utf8(&body) {
        Ok(s) => s.to_string(),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "malformed request body"})),
            )
                .into_response();
        }
    };

    // Reject bodies that are not well-formed JSON before dispatching, so a
    // malformed HTTP request fails with a clear 400 rather than an opaque
    // JSON-RPC error pushed over the stream.
    if serde_json::from_str::<Value>(&body).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "malformed JSON-RPC body"})),
        )
            .into_response();
    }

    let session_id = query.session_id.clone().or_else(|| {
        headers
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    });

    let session = match session_id {
        Some(id) => match state.sessions.get(&id).await {
            Some(s) => s,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "unknown session"})),
                )
                    .into_response();
            }
        },
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "missing session id"})),
            )
                .into_response();
        }
    };

    let result = state.dispatcher.dispatch(&body).await;

    match result {
        DispatchResult::Response(response) => {
            let value = serde_json::to_value(&response).unwrap_or(Value::Null);
            session.send(SseEvent::Message(value));
            (StatusCode::ACCEPTED, Json(json!({"ok": true}))).into_response()
        }
        DispatchResult::NoResponse => {
            (StatusCode::ACCEPTED, Json(json!({"ok": true}))).into_response()
        }
    }
}

/// Query parameters for `POST /mcp`.
#[derive(Debug, Deserialize, Default, Clone)]
struct McpQuery {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

/// Signals graceful shutdown on SIGINT/SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!(event = "http_server_shutdown");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::sse::SseEvent;

    /// Verifies the session is retained while the guard's stream is alive and
    /// removed once the guard is dropped (client disconnect). This guards the
    /// regression where a detached `tokio::spawn` removed the session before the
    /// client ever used its endpoint.
    #[tokio::test]
    async fn session_guard_removes_session_on_drop_only() {
        let registry = Arc::new(SessionRegistry::new());
        let session = registry.create("/mcp").await;
        let session_id = session.id.clone();
        assert_eq!(registry.len().await, 1);

        // A finite stream that yields one event then EOF.
        let inner = tokio_stream::iter(vec![SseEvent::Endpoint("/mcp".to_string())]);
        let guarded = SessionGuard {
            inner,
            registry: Arc::clone(&registry),
            session_id: session_id.clone(),
        };

        // While the guard is alive (not dropped), the session must still exist.
        let mut pinned = Box::pin(guarded);
        let first = std::future::poll_fn(|cx| Pin::new(&mut pinned).poll_next(cx)).await;
        assert!(first.is_some());
        assert_eq!(
            registry.len().await,
            1,
            "session must not be removed while alive"
        );

        // Drop the guard to simulate the client disconnecting.
        drop(pinned);

        // Removal is async; yield until the spawned cleanup task runs.
        let mut attempts = 0;
        while registry.len().await != 0 && attempts < 100 {
            tokio::task::yield_now().await;
            attempts += 1;
        }
        assert_eq!(
            registry.len().await,
            0,
            "session must be removed after the guard is dropped"
        );
    }
}
