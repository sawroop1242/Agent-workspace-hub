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
//!
//! ## Security: SEC-002 TLS Guard
//!
//! Non-loopback MCP HTTP/SSE binds require TLS. Plaintext is only allowed for
//! loopback addresses (`127.0.0.1`, `::1`). This prevents accidental exposure
//! of bearer tokens and MCP traffic on public interfaces.

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
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
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
    /// Sliding-window limiter for authenticated MCP traffic (spec §25
    /// chain applies to both planes). Keyed like the Control API:
    /// first `X-Forwarded-For` value, else the shared "direct" bucket.
    pub rate_limiter: Arc<crate::services::rate_limit::RateLimiter>,
}

/// Serves the remote MCP server over HTTP (optionally TLS) until shutdown.
///
/// ## SEC-002 Security Enforcement
///
/// This function enforces the SEC-002 policy: non-loopback binds require TLS.
/// The check happens before binding, ensuring no plaintext listener starts on
/// public interfaces.
pub async fn serve(config: HttpServerConfig, dispatcher: Arc<McpDispatcher>) -> Result<()> {
    config.tls.validate()?;
    if config.api_key.is_empty() {
        anyhow::bail!("refusing to serve remote MCP without an API key");
    }

    // SEC-002: Enforce TLS for non-loopback binds
    validate_sec_002_policy(&config.host, config.tls.enabled())?;

    // SEC-002: build the TLS acceptor BEFORE binding any socket. With the
    // acceptor built first, invalid TLS material on a non-loopback bind
    // fails closed before a single public TCP listener exists — a
    // misconfigured public server can never briefly open a plaintext-
    // capable socket while the error surfaces.
    let acceptor = config.tls.build_acceptor()?;

    let state = AppState {
        dispatcher,
        sessions: Arc::new(SessionRegistry::new()),
        api_key: Arc::from(config.api_key.as_str()),
        max_sessions: config.max_sessions,
        sse_keepalive: config.sse_keepalive,
        rate_limiter: crate::services::rate_limit::RateLimiter::default_limiter(),
    };

    let app = build_router(state, &config);

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    // Never log the API key; only log the bind address and TLS status.
    tracing::info!(event = "http_server_started", addr = %addr, tls = config.tls.enabled());
    crate::mcp::audit::audit_allow("server_start", "http", &addr);

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

/// SEC-002: Validates that non-loopback binds have TLS enabled.
///
/// This is the core security enforcement for SEC-002. It checks the actual
/// bind address semantics rather than relying on string matching alone.
///
/// # Policy Matrix
///
/// | Bind Address | TLS Disabled | TLS Enabled |
/// |--------------|--------------|-------------|
/// | 127.0.0.1    | ALLOW        | ALLOW       |
/// | ::1          | ALLOW        | ALLOW       |
/// | localhost    | ALLOW*       | ALLOW       |
/// | 0.0.0.0      | REJECT       | ALLOW       |
/// | ::           | REJECT       | ALLOW       |
/// | non-loopback | REJECT       | ALLOW       |
///
/// *localhost is resolved and checked against actual loopback addresses.
/// If resolution fails, we fail closed (reject).
pub fn validate_sec_002_policy(host: &str, tls_enabled: bool) -> Result<()> {
    if tls_enabled {
        return Ok(());
    }

    let is_loopback = is_loopback_host(host)?;

    if !is_loopback {
        audit_deny("sec_002_tls_guard", "non_loopback_plaintext_rejected", host);
        anyhow::bail!(
            "SEC-002 violation: TLS is required for non-loopback MCP HTTP/SSE binds. \
             Attempted plaintext bind on '{}' which is not a loopback address. \
             Enable TLS or use a loopback address (127.0.0.1, ::1).",
            host
        );
    }

    Ok(())
}

/// Determines if a host string represents a loopback address.
///
/// Uses actual IP address semantics via std::net types rather than string
/// matching. Handles direct IP addresses and `localhost` via resolution.
/// Fails closed if resolution is ambiguous or fails.
fn is_loopback_host(host: &str) -> Result<bool> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(ip.is_loopback());
    }

    if host.eq_ignore_ascii_case("localhost") {
        let addrs: Vec<SocketAddr> = (host, 80)
            .to_socket_addrs()
            .with_context(|| format!("failed to resolve '{}'", host))?
            .collect();

        if addrs.is_empty() {
            anyhow::bail!(
                "SEC-002: failed to resolve '{}' to any addresses, rejecting as non-loopback",
                host
            );
        }

        return Ok(addrs.iter().all(|addr| addr.ip().is_loopback()));
    }

    anyhow::bail!(
        "SEC-002: host '{}' is neither a recognized IP address nor 'localhost'. \
         For security, only explicit loopback IPs (127.0.0.1, ::1) or 'localhost' are allowed for plaintext.",
        host
    );
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

    let protected = Router::new()
        .route("/sse", get(sse_handler))
        .route("/mcp", post(mcp_handler))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_guard,
        ))
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

/// Rate-limit middleware keyed on the first `X-Forwarded-For` value, or the
/// shared direct bucket when no proxy header is present.
async fn rate_limit_guard(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let key = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("direct")
        .to_owned();

    match state.rate_limiter.check(&key) {
        Ok(_) => next.run(request).await,
        Err(retry_after) => {
            audit_deny("mcp_rate_limit", "rate_limited", &key);
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "error": "rate_limited",
                    "retry_after_secs": retry_after,
                })),
            )
                .into_response();
            if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after.to_string()) {
                response
                    .headers_mut()
                    .insert(axum::http::header::RETRY_AFTER, value);
            }
            response
        }
    }
}

/// `GET /sse` — establishes an isolated SSE session and streams events.
async fn sse_handler(State(state): State<AppState>) -> Response {
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

    let endpoint_event = Event::default()
        .event("endpoint")
        .data(session.endpoint.clone());

    let stream = BroadcastStream::new(receiver).filter_map(|item| match item {
        Ok(SseEvent::Endpoint(url)) => Some(Ok(Event::default().event("endpoint").data(url))),
        Ok(SseEvent::Message(value)) => Some(Ok(Event::default()
            .event("message")
            .data(value.to_string()))),
        Err(_) => None,
    });

    let initial =
        futures_util::stream::once(async move { Ok::<Event, Infallible>(endpoint_event) });

    let keepalive = state.sse_keepalive;

    let guarded = SessionGuard {
        inner: initial.chain(stream),
        registry: Arc::clone(&state.sessions),
        session_id,
    };

    Sse::new(guarded)
        .keep_alive(KeepAlive::new().interval(keepalive).text("keep-alive"))
        .into_response()
}

/// A stream wrapper that removes its SSE session from the registry when dropped.
struct SessionGuard<S> {
    inner: S,
    registry: Arc<SessionRegistry>,
    session_id: String,
}

impl<S> Drop for SessionGuard<S> {
    fn drop(&mut self) {
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

    let result = state
        .dispatcher
        .dispatch_with_lifecycle(&body, &session.lifecycle)
        .await;

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
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(event = "ctrl_c_handler_failed", error = %error);
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(event = "terminate_handler_failed", error = %error);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!(event = "http_server_shutdown");
    crate::mcp::audit::audit_allow("server_stop", "http", "signal");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::sse::SseEvent;

    #[tokio::test]
    async fn session_guard_removes_session_on_drop_only() {
        let registry = Arc::new(SessionRegistry::new());
        let session = registry.create("/mcp").await;
        let session_id = session.id.clone();
        assert_eq!(registry.len().await, 1);

        let inner = tokio_stream::iter(vec![SseEvent::Endpoint("/mcp".to_string())]);
        let guarded = SessionGuard {
            inner,
            registry: Arc::clone(&registry),
            session_id: session_id.clone(),
        };

        let mut pinned = Box::pin(guarded);
        let first = std::future::poll_fn(|cx| Pin::new(&mut pinned).poll_next(cx)).await;
        assert!(first.is_some());
        assert_eq!(registry.len().await, 1);

        drop(pinned);

        let mut attempts = 0;
        while registry.len().await != 0 && attempts < 100 {
            tokio::task::yield_now().await;
            attempts += 1;
        }
        assert_eq!(registry.len().await, 0);
    }
}
