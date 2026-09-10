//! Transport-agnostic MCP JSON-RPC dispatcher.
//!
//! This module owns the *single* implementation of the MCP tool surface
//! (skills, workspace, memory, tasks, connectors, and dynamic providers). Every
//! transport — stdio, HTTP/SSE, and any future transport — funnels requests
//! through [`McpDispatcher`] so the tool implementations are never duplicated.

use crate::context::{
    ContextEngine, ContextEngineConfig, ContextItem, ContextRequest, ContextScope, ContextSource,
};
use crate::mcp::{
    audit_allow, audit_deny, authorize_mcp_execution, client_name_version, validate_schema,
    validate_schema_syntax, validate_tool_arguments, AuthMethod, CircuitBreakerConfig,
    CircuitBreakerMcpClient, ComposioAccount, ComposioAuth, ComposioProvider, ComposioRegistry,
    Connector, ConnectorsMcp, CustomMcpProvider, CustomMcpRegistry, CustomMcpServerConfig,
    GithubProvider, McpEvent, McpExecutionRequest, McpHook, McpHooks, McpTransport, MemoryMcp,
    MemoryScope, PersistentTrustStore, ProviderRegistry, RepoTarget, ResourceLimits, SkillMcp,
    StdioMcpClient, StreamableHttpMcpClient, TaskPriority, TaskStatus, TasksMcp, ToolMetrics,
    WorkspaceMcp, MAX_TRACKED_TOOLS,
};
use crate::mcp::{permissions, tool_registry};
use crate::services::git::GitService;
use crate::services::terminal::TerminalService;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// JSON-RPC 2.0 standard error codes.
mod codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

/// A dispatch failure carrying the appropriate JSON-RPC error code, so protocol
/// errors and genuine tool-execution failures are reported with the correct
/// (`-32700`/`-32600`/`-32601`/`-32602`/`-32603`) code rather than a blanket
/// "-32600 invalid request".
#[derive(Debug)]
pub struct DispatchError {
    pub code: i64,
    pub message: String,
}

impl DispatchError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn parse(message: impl Into<String>) -> Self {
        Self::new(codes::PARSE_ERROR, message)
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_REQUEST, message)
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_PARAMS, message)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(codes::INTERNAL_ERROR, message)
    }
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DispatchError {}

/// JSON-RPC protocol versions supported by this server, newest first.
///
/// The dispatcher's JSON-RPC method surface (initialize, ping, tools/*,
/// resources/*, prompts/*) is identical across these revisions, so a client
/// requesting any of them gets that exact version echoed back. A client
/// requesting anything else receives the server's latest supported version
/// and decides whether to continue (the MCP negotiation rule).
pub const SUPPORTED_PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// The latest supported protocol version (the server's default answer when
/// a client requests an unsupported or missing version).
pub const MCP_PROTOCOL_VERSION: &str = SUPPORTED_PROTOCOL_VERSIONS[0];

/// JSON-RPC server-error code for requests that arrive before the MCP
/// session is initialized (the same code the MCP reference SDKs borrow
/// from LSP's `ServerNotInitialized`).
pub const SERVER_NOT_INITIALIZED_CODE: i64 = -32002;

/// The lifecycle state of one MCP session (the conceptual `NEW →
/// INITIALIZING → READY → CLOSING/CLOSED/FAILED` machine, reduced to the
/// states the transports actually drive).
///
/// The state flips to initialized exactly once — when a valid `initialize`
/// request has been answered with a success response. `notifications/
/// initialized` is accepted silently (matching the MCP reference servers,
/// which gate on the `initialize` exchange, not the follow-up notification).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Created, waiting for the `initialize` request. Pre-init requests
    /// other than `initialize`/`ping` are rejected with `-32002`.
    New,
    /// The `initialize` exchange completed successfully.
    Ready,
    /// The session is closed; further requests are protocol errors.
    Closed,
    /// The session failed structurally (e.g. the transport died).
    Failed,
}

/// Per-session MCP state, shared between a transport and the dispatcher's
/// lifecycle gate.
///
/// Carries the session identity and protocol metadata (transport, negotiated
/// protocol version, client info) alongside the initialization flag, so
/// observability surfaces can describe a session without exposing secrets.
/// All fields are optional/derived: transports that only need the
/// initialization gate continue to work unchanged.
#[derive(Debug, Default)]
pub struct SessionLifecycle {
    state: std::sync::atomic::AtomicU8,
    /// Stable internal identity (transport-assigned; e.g. the SSE session id).
    session_id: std::sync::Mutex<Option<String>>,
    /// Transport label ("stdio", "sse", "http") for diagnostics.
    transport: std::sync::Mutex<Option<String>>,
    /// Protocol version negotiated during the initialize exchange.
    protocol_version: std::sync::Mutex<Option<String>>,
    /// Client-reported name/version from the initialize request.
    client_info: std::sync::Mutex<Option<Value>>,
    /// Session creation and last-activity timestamps.
    created_at: std::sync::Mutex<Option<std::time::Instant>>,
    last_activity: std::sync::Mutex<Option<std::time::Instant>>,
}

impl SessionLifecycle {
    // The four states the atomic can actually represent. Finer-grained
    // lifecycle phases (`Initializing`, `Closing`) are deliberately NOT
    // modeled in this milestone — they belong to the future Agent Runtime
    // session layer; a not-yet-admitted session is simply `New` here.
    const NEW: u8 = 0;
    const READY: u8 = 1;
    const CLOSED: u8 = 2;
    const FAILED: u8 = 3;

    /// The current session state.
    pub fn state(&self) -> SessionState {
        match self.state.load(std::sync::atomic::Ordering::Acquire) {
            Self::NEW => SessionState::New,
            Self::READY => SessionState::Ready,
            Self::CLOSED => SessionState::Closed,
            _ => SessionState::Failed,
        }
    }

    /// Whether the `initialize` exchange has completed for this session.
    pub fn is_initialized(&self) -> bool {
        self.state() == SessionState::Ready
    }

    /// Marks the session initialized. Returns whether this call performed
    /// the `New → Ready` transition (i.e. whether the session was
    /// previously uninitialized). Atomic (CAS), so exactly one concurrent
    /// caller can win; the losers see `false` and must treat their own
    /// initialize as a duplicate.
    pub fn mark_initialized(&self) -> bool {
        self.state
            .compare_exchange(
                Self::NEW,
                Self::READY,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
    }

    /// Marks the session closed (a graceful shutdown path).
    pub fn mark_closed(&self) {
        self.state
            .store(Self::CLOSED, std::sync::atomic::Ordering::Release);
    }

    /// Marks the session failed (a transport-level failure).
    pub fn mark_failed(&self) {
        self.state
            .store(Self::FAILED, std::sync::atomic::Ordering::Release);
    }

    /// Assigns the stable internal session identity (transport-assigned).
    pub fn set_session_id(&self, id: impl Into<String>) {
        *self.session_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.into());
    }

    /// The transport-assigned session identity, when known.
    pub fn session_id(&self) -> Option<String> {
        self.session_id
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Records the transport label ("stdio", "sse", "http").
    pub fn set_transport(&self, transport: impl Into<String>) {
        *self.transport.lock().unwrap_or_else(|e| e.into_inner()) = Some(transport.into());
    }

    /// Records the protocol version negotiated at initialize time.
    pub fn set_negotiated_version(&self, version: impl Into<String>) {
        *self
            .protocol_version
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(version.into());
    }

    /// The negotiated protocol version, once the initialize exchange ran.
    pub fn negotiated_version(&self) -> Option<String> {
        self.protocol_version
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Records the client info (`clientInfo`) accepted at initialize time.
    pub fn set_client_info(&self, client_info: Value) {
        *self.client_info.lock().unwrap_or_else(|e| e.into_inner()) = Some(client_info);
    }

    /// The client-reported identity, once the initialize exchange ran.
    pub fn client_info(&self) -> Option<Value> {
        self.client_info
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Notes that activity happened on this session (updates `last_activity`).
    pub fn touch(&self) {
        let now = std::time::Instant::now();
        let mut created = self.created_at.lock().unwrap_or_else(|e| e.into_inner());
        if created.is_none() {
            *created = Some(now);
        }
        drop(created);
        *self.last_activity.lock().unwrap_or_else(|e| e.into_inner()) = Some(now);
    }

    /// How long this session has existed, once first touched.
    pub fn age(&self) -> Option<std::time::Duration> {
        self.created_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map(|created| created.elapsed())
    }

    /// How long since the last activity on this session.
    pub fn idle_for(&self) -> Option<std::time::Duration> {
        self.last_activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map(|last| last.elapsed())
    }
}

/// A single parsed JSON-RPC request.
///
/// The distinction between an ABSENT `id` (a notification) and a PRESENT
/// `id` (a request — even when the value is JSON `null`) is protocol
/// semantics per JSON-RPC 2.0 / MCP, so it is preserved here explicitly
/// rather than collapsing both into `id: None`.
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    /// Whether the `id` member was present at all (a request), even when
    /// its value is `null`. `false` means the message is a notification.
    pub id_present: bool,
    pub method: String,
    pub params: Value,
    /// Whether `jsonrpc` was present, a string, and exactly `"2.0"`.
    /// A malformed member must surface as `-32600` with a precise message,
    /// never silently default to `""`.
    pub jsonrpc_valid: bool,
    /// Whether `method` was present, a string, and non-empty.
    pub method_valid: bool,
}

impl<'de> Deserialize<'de> for RpcRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = serde_json::Value::deserialize(deserializer)?;
        let object = raw
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("request must be a JSON object"))?;
        let jsonrpc = object.get("jsonrpc");
        let method = object.get("method");
        Ok(RpcRequest {
            jsonrpc: jsonrpc
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            id: object.get("id").cloned(),
            id_present: object.contains_key("id"),
            method: method
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            params: object.get("params").cloned().unwrap_or_else(|| json!({})),
            // Structural validity is captured at parse time so dispatch can
            // reject malformed members with precise -32600 messages instead
            // of discovering a silently-defaulted "" later.
            jsonrpc_valid: jsonrpc.and_then(Value::as_str) == Some("2.0"),
            method_valid: method
                .and_then(Value::as_str)
                .is_some_and(|m| !m.is_empty()),
        })
    }
}

/// Structural JSON-RPC validation shared by every dispatch path: `jsonrpc`
/// must be the string `"2.0"`, `method` must be a non-empty string, and a
/// present `id` must be a string or number (MCP). Returns the `-32600`
/// error for the first violation, or `None` when the envelope is valid.
///
/// Called after the notification/`id: null` checks (which must keep their
/// own precedence: an absent id is a notification even when the rest of
/// the message is malformed, and the protocol forbids replying to it).
fn validate_envelope(req: &RpcRequest) -> Option<DispatchError> {
    if !req.jsonrpc_valid {
        return Some(DispatchError::invalid_request(
            "'jsonrpc' must be the string \"2.0\"",
        ));
    }
    if !req.method_valid {
        return Some(DispatchError::invalid_request(
            "'method' must be a non-empty string",
        ));
    }
    // MCP request ids: strings or numbers only. Booleans, arrays, and
    // objects are malformed request ids (-32600), never echoed as valid.
    if req.id_present
        && !matches!(
            req.id.as_ref(),
            Some(Value::String(_)) | Some(Value::Number(_))
        )
    {
        return Some(DispatchError::invalid_request(
            "request 'id' must be a string or number",
        ));
    }
    None
}

/// A JSON-RPC response envelope.
#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// The dispatch result: either a response to serialize, or `None` for
/// notifications (which require no response).
pub enum DispatchResult {
    Response(RpcResponse),
    /// A JSON-RPC notification (request without an `id`) requires no reply.
    NoResponse,
}

impl std::fmt::Debug for DispatchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DispatchResult::Response(r) => f.debug_tuple("Response").field(&r.id).finish(),
            DispatchResult::NoResponse => f.write_str("NoResponse"),
        }
    }
}

/// The shared, transport-agnostic MCP request dispatcher.
///
/// [`McpDispatcher`] owns the tool stores and provider registry and exposes the
/// single [`McpDispatcher::dispatch`] entry point consumed by every transport.
/// It is [`Send`] + [`Sync`] and cheap to clone (internally reference-counted),
/// so it can be shared across concurrent MCP sessions.
#[derive(Clone)]
pub struct McpDispatcher {
    skills: Arc<SkillMcp>,
    workspace: Arc<WorkspaceMcp>,
    memory: Arc<MemoryMcp>,
    tasks: Arc<TasksMcp>,
    connectors: Arc<ConnectorsMcp>,
    context_engine: Option<Arc<ContextEngine>>,
    github: Option<Arc<GithubProvider>>,
    providers: Arc<RwLock<ProviderRegistry>>,
    /// Observer-only lifecycle hooks (see [`McpHooks`]).
    hooks: Arc<McpHooks>,
    /// Bounded per-tool call metrics (observability, never a gate).
    metrics: Arc<ToolMetrics>,
    /// When this dispatcher was constructed (health/uptime reporting).
    started_at: std::time::Instant,
}

impl McpDispatcher {
    /// Builds the dispatcher for a project on the caller's thread.
    ///
    /// **Fail-closed contract**: this constructor refuses to run inside an
    /// already-running Tokio runtime. Historically it created an internal
    /// runtime and called `block_on` to spawn trusted custom MCP servers —
    /// calling `block_on` from within a runtime panics ("Cannot start a
    /// runtime from within a runtime"), a latent hazard that struck the
    /// moment a trusted custom stdio server was configured. The refusal is
    /// deterministic and actionable: use [`McpDispatcher::new_async`] from
    /// async contexts.
    pub fn new(project_root: PathBuf) -> Result<Self> {
        if tokio::runtime::Handle::try_current().is_ok() {
            bail!(
                "McpDispatcher::new cannot be called from within a Tokio runtime \
                 (nested runtimes are unsafe); use McpDispatcher::new_async instead"
            );
        }
        let rt = tokio::runtime::Runtime::new()?;
        let dispatcher = rt.block_on(Self::construct(project_root))?;
        // Drop the runtime after construction completes; the dispatcher's
        // providers own their connections and the transports drive dispatch
        // on their own runtimes.
        drop(rt);
        Ok(dispatcher)
    }

    /// Builds the dispatcher for a project from an async context, wiring the
    /// Composio and custom MCP providers without creating any nested runtime.
    pub async fn new_async(project_root: PathBuf) -> Result<Self> {
        Self::construct(project_root).await
    }

    async fn construct(project_root: PathBuf) -> Result<Self> {
        let registry = Arc::new(RwLock::new(ProviderRegistry::default()));

        if std::env::var("COMPOSIO_API_KEY").is_ok() {
            if let Ok(provider) = ComposioProvider::from_env() {
                registry.write().await.register(Box::new(provider));
            }
        }

        // Additional connected accounts registered once via
        // `connector.composio_register` (see `ComposioRegistry`) are global
        // to the machine, not this project, so every project's dispatcher
        // picks them all up here without the human reconnecting per
        // project. All accounts share one `COMPOSIO_API_KEY`; an account is
        // skipped (not an error) if that key isn't set, since it would fail
        // every call anyway.
        if let Ok(api_key) = std::env::var("COMPOSIO_API_KEY") {
            if !api_key.trim().is_empty() {
                if let Ok(accounts) = ComposioRegistry::new().and_then(|r| r.list()) {
                    let mut write = registry.write().await;
                    for account in accounts {
                        if let Ok(provider) = ComposioProvider::new(
                            format!("composio:{}", account.label),
                            api_key.clone(),
                            Some(account.connected_account_id),
                            account.toolkit,
                        ) {
                            write.register(Box::new(provider));
                        }
                    }
                }
            }
        }

        let custom = CustomMcpRegistry::new(project_root.clone())?;
        let trust_store = load_trust_store();
        for cfg in custom.list()? {
            if !cfg.enabled {
                continue;
            }
            // Enforce the centralized execution gate: an enabled custom MCP
            // server is only spawned if it has an explicit, matching trust
            // approval. Missing, blocked, mismatched-version, or over-broad
            // permission requests fail closed (the server is skipped), so a
            // server cannot execute merely because it was registered.
            if !is_authorized(&cfg, trust_store.as_ref()) {
                continue;
            }
            match cfg.transport {
                McpTransport::Stdio => {
                    let client = StdioMcpClient::spawn(&cfg, project_root.clone()).await?;
                    client.initialize().await?;
                    let guarded = CircuitBreakerMcpClient::new(
                        cfg.id.clone(),
                        Arc::new(client),
                        circuit_breaker_config(),
                    );
                    let provider = CustomMcpProvider::new(cfg.id, Arc::new(guarded));
                    registry.write().await.register(Box::new(provider));
                }
                McpTransport::StreamableHttp => {
                    let client = StreamableHttpMcpClient::new(&cfg)?;
                    client.initialize().await?;
                    let guarded = CircuitBreakerMcpClient::new(
                        cfg.id.clone(),
                        Arc::new(client),
                        circuit_breaker_config(),
                    );
                    let provider = CustomMcpProvider::new(cfg.id, Arc::new(guarded));
                    registry.write().await.register(Box::new(provider));
                }
            }
        }

        // The context engine is opt-in per project via `AWH_CONTEXT_ENGINE`
        // (or the config's `enabled` flag, which honors the same env vars).
        // When construction fails the tools surface a clear error instead of
        // taking the whole dispatcher down, so existing behavior is unchanged.
        // The cause (e.g. an invalid AWH_CONTEXT_* value) still reaches
        // stderr via tracing; the JSON-RPC error stays generic.
        let context_engine = ContextEngineConfig::default()
            .with_env_overrides()
            .and_then(|config| ContextEngine::new(&project_root, config).map(Arc::new))
            .map_err(|e| {
                tracing::error!("context engine disabled: {e:#}");
                e
            })
            .ok();

        // GitHub is enabled purely by the presence of GITHUB_TOKEN and fails
        // closed the same way: a missing token simply leaves the github.*
        // tools unadvertised (and their calls rejected) rather than failing
        // dispatcher construction.
        let github = GithubProvider::from_env()
            .map(Arc::new)
            .map_err(|e| tracing::warn!("github provider disabled: {e:#}"))
            .ok();

        Ok(Self {
            skills: Arc::new(SkillMcp::new(project_root.clone())?),
            workspace: Arc::new(WorkspaceMcp::new(project_root.clone())?),
            memory: Arc::new(MemoryMcp::new(project_root.clone())?),
            tasks: Arc::new(TasksMcp::new(project_root.clone())?),
            connectors: Arc::new(ConnectorsMcp::new(project_root)?),
            context_engine,
            github,
            providers: registry,
            hooks: Arc::new(McpHooks::new()),
            metrics: Arc::new(ToolMetrics::new(MAX_TRACKED_TOOLS)),
            started_at: std::time::Instant::now(),
        })
    }

    fn context(&self) -> Result<&ContextEngine> {
        self.context_engine
            .as_ref()
            .map(Arc::as_ref)
            .ok_or_else(|| anyhow::anyhow!("context engine is disabled or failed to initialize"))
    }

    /// The GitHub provider, when `GITHUB_TOKEN` enabled it.
    fn github(&self) -> Result<&GithubProvider> {
        self.github.as_ref().map(Arc::as_ref).ok_or_else(|| {
            anyhow::anyhow!("github tools are disabled: set GITHUB_TOKEN to enable them")
        })
    }

    /// Resolves the `owner/repo` target for a `github.*` call: explicit
    /// arguments win, then the project's `origin` remote, then
    /// `GITHUB_DEFAULT_OWNER`/`GITHUB_DEFAULT_REPO`. Mixed sources are never
    /// combined — each invocation uses exactly one source for both fields.
    async fn resolve_github_target(&self, arguments: &Value) -> Result<RepoTarget> {
        let owner = arguments.get("owner").and_then(Value::as_str);
        let repo = arguments.get("repo").and_then(Value::as_str);
        crate::mcp::github::resolve_repo_target(owner, repo, self.workspace.root()).await
    }

    /// Returns the shared provider registry used to dispatch tool calls.
    pub fn provider_registry(&self) -> Arc<RwLock<ProviderRegistry>> {
        Arc::clone(&self.providers)
    }

    /// The observer-only lifecycle hook registry for this dispatcher.
    ///
    /// Hooks observe events (`initialize` completed, tool calls, resource
    /// reads, notifications); they can neither veto calls nor alter
    /// arguments. Security decisions are made independently of hooks.
    pub fn hooks(&self) -> Arc<McpHooks> {
        Arc::clone(&self.hooks)
    }

    /// The bounded per-tool metrics registry (observability only).
    pub fn metrics(&self) -> Arc<ToolMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Registers an observer hook (bounded at [`MAX_HOOKS`]).
    pub fn register_hook(&self, hook: McpHook) -> Result<(), String> {
        self.hooks.register(hook)
    }

    /// How long this dispatcher has been running (health/uptime reporting).
    pub fn uptime(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Dispatches a single raw JSON-RPC message (request or notification).
    ///
    /// Returns the response to serialize back to the client, or `None` for
    /// notifications. Protocol errors (parse failures, unknown methods, invalid
    /// parameters, unsupported JSON-RPC versions) are converted into JSON-RPC
    /// error objects rather than surfacing as [`Err`], so a single bad message
    /// never tears down the transport loop.
    pub async fn dispatch(&self, input: &str) -> DispatchResult {
        match self.dispatch_strict(input).await {
            Ok(response) => response,
            Err(error) => {
                let id = serde_json::from_str::<Value>(input)
                    .ok()
                    .and_then(|value| value.get("id").cloned());
                DispatchResult::Response(RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({
                        "code": error.code,
                        "message": error.message,
                    })),
                })
            }
        }
    }

    /// Strict variant of [`Self::dispatch`] that propagates protocol and
    /// dispatch failures as [`Err`] instead of converting them into JSON-RPC
    /// error responses.
    ///
    /// This is used by the synchronous stdio adapter (`handle`) to preserve its
    /// historical fail-closed contract: structural/protocol errors surface as
    /// hard errors, while *known* JSON-RPC failure modes (e.g. an unknown
    /// method) still return a well-formed error response.
    pub async fn dispatch_strict(&self, input: &str) -> Result<DispatchResult, DispatchError> {
        self.dispatch_inner(input).await
    }

    /// Dispatches a message on behalf of a transport session, enforcing the
    /// MCP initialization lifecycle (spec §8):
    ///
    /// - Before the `initialize` exchange completes, every request except
    ///   `initialize` and `ping` is rejected with `-32002` (server not
    ///   initialized). Notifications are accepted silently.
    /// - `initialize` parameters are validated and the protocol version is
    ///   negotiated: the client's requested version is echoed when supported,
    ///   otherwise the server's own (supported) version is returned, per the
    ///   MCP specification.
    /// - A duplicate `initialize` on an already-initialized session is a
    ///   deterministic `-32600` protocol error, not a second handshake.
    ///
    /// Structurally invalid messages (bad JSON, unsupported JSON-RPC version)
    /// fail exactly like [`Self::dispatch`]; the lifecycle gate only applies
    /// to well-formed 2.0 messages.
    pub async fn dispatch_with_lifecycle(
        &self,
        input: &str,
        lifecycle: &SessionLifecycle,
    ) -> DispatchResult {
        match self.dispatch_strict_with_lifecycle(input, lifecycle).await {
            Ok(result) => result,
            Err(error) => {
                let id = serde_json::from_str::<Value>(input)
                    .ok()
                    .and_then(|value| value.get("id").cloned());
                DispatchResult::Response(RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({
                        "code": error.code,
                        "message": error.message,
                    })),
                })
            }
        }
    }

    /// Strict variant of [`Self::dispatch_with_lifecycle`]: structural and
    /// protocol failures propagate as [`Err`] (preserving the fail-closed
    /// contract of [`Self::dispatch_strict`]), while lifecycle outcomes
    /// (pre-initialization requests, duplicate initialization) are *known*
    /// JSON-RPC failure modes and come back as well-formed error responses.
    ///
    /// A successful `initialize` exchange flips the session to initialized
    /// and records its negotiated metadata; a failed one leaves it
    /// uninitialized so the client can retry.
    ///
    /// Notification semantics (JSON-RPC 2.0): any parseable message without
    /// an `id` is a notification and never receives a response — even when
    /// it is otherwise invalid (e.g. a wrong `jsonrpc` version) — because
    /// the spec forbids replying to notifications. The method name is still
    /// observed (audited) for diagnostics; its arguments are not.
    pub async fn dispatch_strict_with_lifecycle(
        &self,
        input: &str,
        lifecycle: &SessionLifecycle,
    ) -> Result<DispatchResult, DispatchError> {
        // Parse first so structural errors keep their precise codes.
        let req: RpcRequest = serde_json::from_str(input)
            .map_err(|e| DispatchError::parse(format!("invalid JSON: {e}")))?;

        // Notifications (parseable messages with no `id` member) produce no
        // response at any lifecycle stage — including `notifications/
        // initialized` (state flips on the initialize exchange, matching
        // the MCP reference servers) and notifications that arrive while
        // the session is closed or uninitialized. The method name is
        // recorded for observability; arguments are never logged.
        // A PRESENT `id` makes the message a request — even `id: null`
        // (JSON-RPC 2.0 §Request object: notifications lack the id member;
        // MCP additionally forbids null request ids, handled below).
        let id = req.id.clone();
        if !req.id_present {
            self.hooks.fire(&McpEvent::NotificationReceived {
                method: &req.method,
            });
            return Ok(DispatchResult::NoResponse);
        }
        // MCP (and JSON-RPC interoperability practice) requires request ids
        // to be strings or numbers; a literal `null` id is a malformed
        // request, not a notification. Respond with the id echoed as null.
        if matches!(id.as_ref(), Some(Value::Null)) {
            audit_deny("request_rejected", "null_request_id", &req.method);
            return Ok(DispatchResult::Response(RpcResponse {
                jsonrpc: "2.0",
                id: Some(Value::Null),
                result: None,
                error: Some(json!({
                    "code": codes::INVALID_REQUEST,
                    "message": "request 'id' must not be null (notifications omit 'id' entirely)",
                })),
            }));
        }

        // Full structural validation (jsonrpc member, method member, id
        // type) with precise -32600 messages, replacing the bare
        // version-only check.
        if let Some(error) = validate_envelope(&req) {
            audit_deny("request_rejected", "invalid_envelope", &req.method);
            return Err(error);
        }

        lifecycle.touch();
        match lifecycle.state() {
            // A closed or failed session no longer accepts requests.
            SessionState::Closed | SessionState::Failed => {
                audit_deny("session_closed_rejected", "session_closed", &req.method);
                return Ok(DispatchResult::Response(RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({
                        "code": codes::INVALID_REQUEST,
                        "message": "session is closed",
                    })),
                }));
            }
            SessionState::Ready => {
                if req.method == "initialize" {
                    audit_deny("session_initialize", "duplicate_initialize", "initialize");
                    return Ok(DispatchResult::Response(RpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(json!({
                            "code": codes::INVALID_REQUEST,
                            "message": "session already initialized",
                        })),
                    }));
                }
            }
            SessionState::New => {
                if req.method != "initialize" && req.method != "ping" {
                    audit_deny(
                        "session_preinit_rejected",
                        "server_not_initialized",
                        &req.method,
                    );
                    return Ok(DispatchResult::Response(RpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(json!({
                            "code": SERVER_NOT_INITIALIZED_CODE,
                            "message": "session not initialized: send an initialize request first",
                        })),
                    }));
                }
            }
        }

        let result = self.dispatch_inner(input).await?;
        // The initialize exchange completed successfully: flip the session
        // to initialized and record what was negotiated. (A failure
        // response keeps the session uninitialized so the client can retry.)
        if req.method == "initialize" && result_is_ok(&result) {
            // Atomic CAS: exactly one concurrent initialize can win. If this
            // call did not perform the New→Ready transition, another request
            // initialized the session first (or a sequential duplicate raced
            // past the gate check above) — respond as a duplicate rather
            // than reporting a second success.
            if !lifecycle.mark_initialized() {
                audit_deny("session_initialize", "duplicate_initialize", "initialize");
                return Ok(DispatchResult::Response(RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({
                        "code": codes::INVALID_REQUEST,
                        "message": "session already initialized",
                    })),
                }));
            }
            let negotiated = self
                .initialize_metadata(&req.params)
                .unwrap_or_else(|| MCP_PROTOCOL_VERSION.to_string());
            lifecycle.set_negotiated_version(negotiated.clone());
            lifecycle.set_client_info(req.params.get("clientInfo").cloned().unwrap_or(json!({})));
            lifecycle.touch();
            audit_allow("session_initialize", "initialize", "lifecycle");
            let stored_client_info = lifecycle.client_info();
            let client_info = client_name_version(stored_client_info.as_ref());
            self.hooks.fire(&McpEvent::InitializeCompleted {
                protocol_version: negotiated.as_str(),
                client_info,
            });
        }
        Ok(result)
    }

    /// The protocol version the `initialize` exchange would negotiate for
    /// these parameters (mirrors [`Self::initialize_response`]).
    fn initialize_metadata(&self, params: &Value) -> Option<String> {
        match params.get("protocolVersion").and_then(Value::as_str) {
            Some(version) if SUPPORTED_PROTOCOL_VERSIONS.contains(&version) => {
                Some(version.to_string())
            }
            _ => Some(MCP_PROTOCOL_VERSION.to_string()),
        }
    }

    async fn dispatch_inner(&self, input: &str) -> Result<DispatchResult, DispatchError> {
        let req: RpcRequest = serde_json::from_str(input)
            .map_err(|e| DispatchError::parse(format!("invalid JSON: {e}")))?;

        // Notifications (no `id` member) produce no response.
        let id = req.id.clone();
        let is_notification = !req.id_present;

        // Notifications produce no response (JSON-RPC 2.0). `notifications/
        // initialized` is the standard post-initialize notification; every
        // other notification (known or unknown) is accepted silently too —
        // the protocol forbids replying to notifications.
        if is_notification {
            return Ok(DispatchResult::NoResponse);
        }

        // Structural validation AFTER the notification check: a request
        // (id present) with a malformed `jsonrpc`/`method` member or an
        // invalid id type gets a precise `-32600`, never a silently
        // defaulted "" flowing into method dispatch.
        if let Some(error) = validate_envelope(&req) {
            audit_deny("request_rejected", "invalid_envelope", &req.method);
            return Err(error);
        }

        let started = std::time::Instant::now();
        let result = match req.method.as_str() {
            "initialize" => self.initialize_response(&req.params)?,
            // MCP liveness probe: the protocol requires an empty response.
            "ping" => json!({}),
            "tools/list" => self
                .tools_list_aggregated()
                .await
                .map_err(to_dispatch_error)?,
            "tools/call" => {
                let outcome = self.call_tool(&req.params).await;
                let name = req.params.get("name").and_then(Value::as_str);
                if let Some(name) = name {
                    let ok = outcome.is_ok();
                    let duration = started.elapsed();
                    self.metrics.record(name, ok, duration);
                    self.hooks
                        .fire(&McpEvent::ToolCallCompleted { name, ok, duration });
                    if !ok {
                        // Structured record of the failed dispatch; see
                        // audit_tool_failure for what is (not) captured.
                        audit_tool_failure(name);
                    }
                }
                outcome.map_err(to_dispatch_error)?
            }
            // MCP resources: project context, memory entries, and
            // referenced skills exposed as addressable, readable URIs.
            "resources/list" => self.resources_list().map_err(to_dispatch_error)?,
            "resources/read" => {
                let uri = req.params.get("uri").and_then(Value::as_str);
                let outcome = self.resources_read(&req.params);
                if outcome.is_ok() {
                    if let Some(uri) = uri {
                        audit_allow("mcp_resource_read", uri, "resources/read");
                        self.hooks.fire(&McpEvent::ResourceRead { uri });
                    }
                }
                outcome.map_err(to_dispatch_error)?
            }
            // This server ships no prompt templates; the protocol
            // expects an empty list rather than an error.
            "prompts/list" => json!({"prompts": []}),
            // No prompt templates exist, so every `prompts/get` names an
            // unknown prompt. Answering with a deterministic protocol error
            // (rather than a generic "method not found") matches the
            // MCP error model for unknown prompts and stays consistent
            // with the empty `prompts/list` advertisement.
            "prompts/get" => {
                let name = req
                    .params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                audit_deny("mcp_prompt_requested", "unknown_prompt", name);
                self.hooks.fire(&McpEvent::PromptRequested { name });
                return Err(DispatchError::invalid_params(format!(
                    "unknown prompt: {name} (this server ships no prompt templates)"
                )));
            }
            _ => {
                // Unknown method: a proper JSON-RPC "method not found" error.
                return Ok(DispatchResult::Response(RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({
                        "code": codes::METHOD_NOT_FOUND,
                        "message": "method not found",
                    })),
                }));
            }
        };

        Ok(DispatchResult::Response(RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }))
    }

    /// Builds the `initialize` result: validated parameters, negotiated
    /// protocol version, and honest capability advertisement.
    ///
    /// Validation (fail-closed on anything the protocol types strictly):
    /// - `params`, when present, must be a JSON object;
    /// - `protocolVersion`, when present, must be a string.
    ///
    /// Negotiation follows the MCP specification: if the client requests a
    /// version this server supports, that exact version is echoed; otherwise
    /// the server responds with its own latest supported version and the
    /// client decides whether to continue.
    fn initialize_response(&self, params: &Value) -> Result<Value, DispatchError> {
        if !params.is_null() && !params.is_object() {
            return Err(DispatchError::invalid_params(
                "initialize params must be a JSON object",
            ));
        }
        let requested = params.get("protocolVersion");
        if let Some(requested) = requested {
            if !requested.is_string() {
                return Err(DispatchError::invalid_params(
                    "initialize protocolVersion must be a string",
                ));
            }
        }
        let negotiated = match requested.and_then(Value::as_str) {
            Some(version) if SUPPORTED_PROTOCOL_VERSIONS.contains(&version) => version,
            _ => MCP_PROTOCOL_VERSION,
        };
        Ok(json!({
            "protocolVersion": negotiated,
            // Honest advertisement: tools, resources, and prompts/list are
            // all implemented by this server.
            "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
            "serverInfo": {"name": "agent-workspace-hub", "version": env!("CARGO_PKG_VERSION")}
        }))
    }

    async fn tools_list_aggregated(&self) -> Result<Value> {
        let mut base = self
            .tools_list_static()
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        // Attach the canonical Tool Registry metadata to every static
        // tool entry. Extra fields on a Tool object are ignored by MCP
        // clients (the SDK schemas are permissive), so this is additive.
        for tool in &mut base {
            let Some(name) = tool.get("name").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            let Some(tool_entry) = tool.as_object_mut() else {
                continue;
            };
            match tool_registry::registry_lookup(&name) {
                Some(def) => {
                    tool_entry.insert("category".to_string(), json!(def.category));
                    tool_entry.insert("version".to_string(), json!(def.tool_version));
                    tool_entry.insert("schemaVersion".to_string(), json!(def.schema_version));
                    tool_entry.insert("provider".to_string(), json!(def.provider));
                    tool_entry.insert("risk".to_string(), json!(def.risk.as_str()));
                    let permissions: Vec<&str> = def
                        .required_permissions
                        .iter()
                        .map(|p| p.as_str())
                        .collect();
                    tool_entry.insert("requiredPermissions".to_string(), json!(permissions));
                }
                // A static catalog tool with no registry entry is a bug:
                // fail loudly in tests, never silently emit unclassified
                // metadata. (The exhaustiveness test pins this.)
                None => {
                    let (category, version) = tool_metadata(&name);
                    tool_entry.insert("category".to_string(), json!(category));
                    tool_entry.insert("version".to_string(), json!(version));
                }
            }
        }
        let registry = self.providers.read().await;
        let dynamic = registry.aggregate_tools().await?;
        for tool in dynamic {
            let meta = dynamic_tool_metadata(&tool.name);
            let mut entry = serde_json::to_value(tool)?;
            if let Some(entry) = entry.as_object_mut() {
                entry.insert("category".to_string(), json!(meta.category));
                entry.insert("version".to_string(), json!(meta.tool_version));
                entry.insert("schemaVersion".to_string(), json!(meta.schema_version));
                entry.insert("provider".to_string(), json!(meta.provider));
                // Risk is deliberately NOT emitted for dynamic tools: the
                // provider does not declare it and AWH must not guess.
            }
            base.push(entry);
        }
        Ok(json!({"tools": base}))
    }

    /// The static AWH tool catalog (no registry metadata attached, no
    /// dynamic provider tools). Public so tests and tooling can verify
    /// catalog/registry exhaustiveness.
    pub fn tools_list_static(&self) -> Value {
        // Split into two macro invocations to stay under the macro
        // recursion limit; merged into one array below.
        let core = json!([
            {"name":"skills.list","description":"List project-referenced skills","inputSchema":{"type":"object","properties":{}}},
            {"name":"skills.read","description":"Read a project-referenced skill","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
            {"name":"skills.add","description":"Add an installed global skill","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
            {"name":"skills.remove","description":"Remove a project skill reference","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
            {"name":"skills.search","description":"Search globally installed skills","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}},
            {"name":"workspace.context","description":"Read project agent instructions","inputSchema":{"type":"object","properties":{}}},
            {"name":"workspace.list_files","description":"List workspace files","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}},
            {"name":"workspace.read_file","description":"Read a workspace file","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}},
            {"name":"memory.store","description":"Store project memory","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"content":{"type":"string"},"scope":{"type":"string","enum":["Session","Project","Global"]},"tags":{"type":"array","items":{"type":"string"}}},"required":["id","content","scope"]}},
            {"name":"memory.search","description":"Search memory","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"scope":{"type":"string","enum":["Session","Project","Global"]}},"required":["query"]}},
            {"name":"memory.get","description":"Get memory by id","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"memory.delete","description":"Delete memory","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"memory.update","description":"Update an existing memory entry's content and tags","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"content":{"type":"string"},"scope":{"type":"string","enum":["Session","Project","Global"]},"tags":{"type":"array","items":{"type":"string"}}},"required":["id","content"]}},
            {"name":"tasks.create","description":"Create a task","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"title":{"type":"string"},"description":{"type":"string"},"priority":{"type":"string","enum":["Low","Normal","High","Critical"]},"tags":{"type":"array","items":{"type":"string"}}},"required":["id","title","description"]}},
            {"name":"tasks.list","description":"List tasks","inputSchema":{"type":"object","properties":{"status":{"type":"string","enum":["Todo","InProgress","Blocked","Done"]}}}},
            {"name":"tasks.update","description":"Update a task","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"status":{"type":"string","enum":["Todo","InProgress","Blocked","Done"]},"priority":{"type":"string","enum":["Low","Normal","High","Critical"]},"assignee":{"type":["string","null"]}},"required":["id"]}},
            {"name":"tasks.delete","description":"Delete a task","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"connectors.list","description":"List connector metadata for this project","inputSchema":{"type":"object","properties":{}}},
            {"name":"connectors.add","description":"Register connector metadata; never stores secrets","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"name":{"type":"string"},"provider":{"type":"string"},"auth":{"type":"string","enum":["OAuth","ApiKey","None"]},"scopes":{"type":"array","items":{"type":"string"}},"enabled":{"type":"boolean"}},"required":["id","name","provider","auth"]}},
            {"name":"connectors.enable","description":"Enable a connector","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"connectors.disable","description":"Disable a connector","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"connectors.remove","description":"Remove connector metadata","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"connector.providers","description":"List registered connector and custom MCP providers","inputSchema":{"type":"object","properties":{}}},
            {"name":"connector.tools","description":"List tools exposed by a provider","inputSchema":{"type":"object","properties":{"provider":{"type":"string"}},"required":["provider"]}},
            {"name":"connector.invoke","description":"Invoke a tool exposed by a provider","inputSchema":{"type":"object","properties":{"provider":{"type":"string"},"tool":{"type":"string"},"arguments":{"type":"object"}},"required":["provider","tool"]}},
            {"name":"context.status","description":"Context engine status: items, tokens, budget, offloads, memories","inputSchema":{"type":"object","properties":{}}},
            {"name":"context.insert","description":"Insert a context item into the engine","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"content":{"type":"string"},"source":{"type":"string","enum":["System","User","Assistant","Tool","Skill","File","Memory","Workspace","Search","Summary","Other"]},"relevance":{"type":"number"},"priority":{"type":"number"},"scope":{"type":"string","enum":["Session","Project","Global"]}},"required":["id","content"]}},
            {"name":"context.get","description":"Get a context item by id","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"context.remove","description":"Remove a context item by id","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"context.search","description":"Search active and offloaded context items","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"}},"required":["query"]}},
            {"name":"context.optimize","description":"Run a planning pass: keep, compress, archive, or offload items by score","inputSchema":{"type":"object","properties":{"task":{"type":"string"}},"required":["task"]}},
            {"name":"context.assemble","description":"Assemble budget-constrained context for a task","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"query":{"type":"string"},"token_budget":{"type":"number"}},"required":["task"]}},
            {"name":"context.protect","description":"Protect a context item from offloading","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"context.unprotect","description":"Clear protection for a context item","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"context.offload","description":"Soft-offload a context item (fully recoverable)","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"reason":{"type":"string"}},"required":["id"]}},
            {"name":"context.restore","description":"Restore a soft-offloaded context item to active","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}}
        ]);
        let extended = json!([
            {"name":"connector.composio_link","description":"Start a Composio OAuth link for a new connected account; open the returned redirect_url to finish authorizing, then register the resulting connected_account_id with connector.composio_register","inputSchema":{"type":"object","properties":{"auth_config_id":{"type":"string"},"user_id":{"type":"string"},"alias":{"type":"string"},"callback_url":{"type":"string"}},"required":["auth_config_id","user_id"]}},
            {"name":"connector.composio_accounts","description":"List Composio connected accounts visible to this API key, optionally filtered by user or toolkit","inputSchema":{"type":"object","properties":{"user_id":{"type":"string"},"toolkit":{"type":"string"}}}},
            {"name":"connector.composio_register","description":"Register a Composio connected account globally under a label, so every project and agent on this machine can use it via connector.invoke as 'composio:<label>' without reconnecting","inputSchema":{"type":"object","properties":{"label":{"type":"string"},"connected_account_id":{"type":"string"},"toolkit":{"type":"string"}},"required":["label","connected_account_id"]}},
            {"name":"connector.composio_remove","description":"Remove a globally registered Composio connected account by label","inputSchema":{"type":"object","properties":{"label":{"type":"string"}},"required":["label"]}},
            {"name":"git.status","description":"Git working-tree status (porcelain)","inputSchema":{"type":"object","properties":{}}},
            {"name":"git.branch","description":"Current Git branch","inputSchema":{"type":"object","properties":{}}},
            {"name":"git.log","description":"Recent Git commit log","inputSchema":{"type":"object","properties":{"limit":{"type":"number"}}}},
            {"name":"git.diff","description":"Git unified diff (working tree or staged)","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"staged":{"type":"boolean"}}}},
            {"name":"git.stage","description":"Stage a file or all changes","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}},
            {"name":"git.unstage","description":"Unstage a file, leaving the working tree untouched","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}},
            {"name":"git.commit","description":"Commit staged changes with a message","inputSchema":{"type":"object","properties":{"message":{"type":"string"}},"required":["message"]}},
            {"name":"terminal.run","description":"Run a bounded command in the project workspace (argv form, no shell)","inputSchema":{"type":"object","properties":{"program":{"type":"string"},"args":{"type":"array","items":{"type":"string"}}},"required":["program"]}},
            {"name":"workspace.write_file","description":"Write (create or overwrite) a workspace file","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}},
            {"name":"workspace.delete_file","description":"Delete a workspace file; reports whether it existed","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}},
            {"name":"tasks.get","description":"Get a task by id","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"connectors.get","description":"Get connector metadata by id","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"mcp.status","description":"Read-only MCP server health and status: protocol versions, tool/provider counts, tool-call metrics, uptime. Exposes no secrets.","inputSchema":{"type":"object","properties":{}}}
        ]);
        let mut tools = core;
        if let (Value::Array(core_arr), Value::Array(ext_arr)) = (&mut tools, &extended) {
            core_arr.extend(ext_arr.iter().cloned());
        }
        // The github.* surface is a third macro invocation (the recursion
        // limit documented for the core/extended split binds here too) and is
        // merged only when GITHUB_TOKEN enabled the provider, so clients
        // never see tools that would reject every call.
        if self.github.is_some() {
            if let Value::Array(core_arr) = &mut tools {
                if let Value::Array(gh_arr) = github_tool_schemas() {
                    core_arr.extend(gh_arr);
                }
            }
        }
        json!({"tools": tools})
    }

    /// Looks up one static-catalog tool's `tools/list` envelope by name
    /// (the `{"tools": [...]}` shape `validate_tool_arguments` expects).
    /// Dynamic provider tools are not visible here — their schemas live in
    /// the provider registry and are validated by the provider clients.
    fn static_tool_schema(&self, name: &str) -> Option<Value> {
        let catalog = self.tools_list_static();
        let found = catalog
            .get("tools")?
            .as_array()?
            .iter()
            .any(|tool| tool.get("name").and_then(Value::as_str) == Some(name));
        found.then_some(catalog)
    }

    /// MCP resources exposed by this server: the project's context
    /// files, every memory entry, and each project-referenced skill.
    /// Resources are read-only views over existing services — they add
    /// no new filesystem surface (skills.read already enforces project
    /// references; memory ids are validated by the memory store).
    fn resources_list(&self) -> Result<Value> {
        let mut resources = Vec::new();

        resources.push(json!({
            "uri": "awh://context",
            "name": "Project context",
            "description": "Concatenated AGENTS.md / AGENT.md / README.md",
            "mimeType": "text/markdown",
        }));

        for entry in self.memory.list_all()? {
            resources.push(json!({
                "uri": format!("awh://memory/{}", entry.id),
                "name": format!("Memory {}", entry.id),
                "description": truncate_chars(&entry.content, 60),
                "mimeType": "text/plain",
            }));
        }

        for skill in self.skills.list()? {
            resources.push(json!({
                "uri": format!("awh://skills/{}", skill.name),
                "name": format!("Skill {}", skill.name),
                "description": truncate_chars(&skill.description, 60),
                "mimeType": "text/markdown",
            }));
        }

        Ok(json!({"resources": resources}))
    }

    /// Reads one resource by URI. Unknown or malformed URIs are a
    /// protocol-level error the client can surface. The bare
    /// `awh://context` resource (no id segment) is accepted because
    /// `resources/list` advertises it that way.
    fn resources_read(&self, params: &Value) -> Result<Value> {
        let uri = params.get("uri").and_then(Value::as_str).ok_or_else(|| {
            // A request without a usable uri is a client-side protocol
            // violation, not a server failure: -32602, not -32603.
            DispatchError::invalid_params("resources/read requires a string 'uri' parameter")
        })?;
        // MCP-boundary URI validation (defense in depth — downstream
        // stores also validate, but malformed identifiers must never reach
        // them): a resource uri is `awh://<kind>[/single-segment-id]`.
        // The id may not contain path separators, traversal patterns
        // (literal or percent-encoded), percent signs, control characters,
        // or NUL; `awh://context` is the only kindless form.
        const MAX_URI_LEN: usize = 256;
        if uri.is_empty() {
            return Err(DispatchError::invalid_params("resource uri must not be empty").into());
        }
        if uri.len() > MAX_URI_LEN {
            return Err(DispatchError::invalid_params(format!(
                "resource uri exceeds {MAX_URI_LEN} bytes"
            ))
            .into());
        }
        let path = uri.strip_prefix("awh://").ok_or_else(|| {
            DispatchError::invalid_params(format!("unsupported resource uri scheme: {uri}"))
        })?;
        let (kind, rest) = match path.split_once('/') {
            Some((kind, rest)) => (kind, rest),
            None => (path, ""),
        };
        if kind.is_empty() {
            return Err(DispatchError::invalid_params(format!(
                "resource uri is missing its kind: {uri}"
            ))
            .into());
        }
        let validate_id = |id: &str| -> Result<(), DispatchError> {
            if id.is_empty() {
                return Err(DispatchError::invalid_params(
                    "resource uri is missing its identifier segment",
                ));
            }
            if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('%') {
                return Err(DispatchError::invalid_params(
                    "resource uri identifier must be a single path segment without traversal or percent-encoding",
                ));
            }
            if id.bytes().any(|b| b == 0 || b.is_ascii_control()) {
                return Err(DispatchError::invalid_params(
                    "resource uri identifier must not contain control characters",
                ));
            }
            Ok(())
        };

        let (content, mime) = match kind {
            "context" => {
                if !rest.is_empty() {
                    return Err(DispatchError::invalid_params(
                        "awh://context takes no path segments",
                    )
                    .into());
                }
                (self.workspace.context()?, "text/markdown")
            }
            "memory" => {
                validate_id(rest)?;
                let entry = self.memory.get(rest)?.ok_or_else(|| {
                    DispatchError::invalid_params(format!("memory entry not found: {rest}"))
                })?;
                (entry.content, "text/plain")
            }
            "skills" => {
                validate_id(rest)?;
                let skill = self.skills.read(rest)?;
                (skill.description, "text/markdown")
            }
            other => {
                return Err(DispatchError::invalid_params(format!(
                    "unsupported resource kind: {other}"
                ))
                .into());
            }
        };
        Ok(json!({
            "contents": [{
                "uri": uri,
                "mimeType": mime,
                "text": content,
            }]
        }))
    }

    async fn call_tool(&self, params: &Value) -> Result<Value> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        // Fail closed on a malformed envelope before any dispatch: `name`
        // must be a non-empty string and `arguments`, when present, an
        // object. (An absent `arguments` defaults to `{}` — some clients
        // omit it for no-parameter tools.)
        if name.is_empty() {
            return Err(DispatchError::invalid_params(
                "tools/call requires a non-empty 'name' parameter",
            )
            .into());
        }
        if !arguments.is_object() {
            return Err(DispatchError::invalid_params(
                "tools/call 'arguments' must be a JSON object when present",
            )
            .into());
        }

        // Schema-first validation (before dispatch): a tool executes only
        // when its arguments match the schema advertised in tools/list. This
        // runs for the static catalog below; dynamic provider tools are
        // validated by the provider client itself (see
        // `StdioMcpClient::tools_call`), which fetches the provider's live
        // schema. Either way, no tool runs solely because its name exists.
        if let Some(schema) = self.static_tool_schema(name) {
            if let Err(error) = validate_tool_arguments(&schema, name, &arguments) {
                // Argument-validation failures are invalid-params
                // protocol errors (-32602), not internal errors.
                audit_deny("tool_validation", "schema_mismatch", name);
                return Err(DispatchError::invalid_params(format!(
                    "invalid arguments for '{name}': {error:#}"
                ))
                .into());
            }
        }

        // Audit every tool invocation by name only. Arguments are deliberately
        // never logged: they may contain file contents or secret material.
        audit_allow("tool_invoke", name, "tools/call");

        let value = match name {
            // Read-only MCP health/status: bounded, secret-free snapshot
            // of this server's catalog, providers, and observability state.
            // `status` reports the SERVER PROCESS state only: "running"
            // means this dispatcher is serving requests. It is NOT a
            // subsystem-health verdict — in-process subsystems (filesystem,
            // skills, memory) fail per-call and surface as tool errors.
            "mcp.status" => {
                let registry = self.providers.read().await;
                let provider_ids = registry.providers();
                let static_count = self
                    .tools_list_static()
                    .get("tools")
                    .and_then(Value::as_array)
                    .map(|tools| tools.len())
                    .unwrap_or(0);
                let dynamic_count = registry
                    .aggregate_tools()
                    .await
                    .map(|tools| tools.len())
                    .unwrap_or(0);
                let (tracked, calls, failures) = self.metrics.totals();
                serde_json::to_value(json!({
                    "status": "running",
                    "protocol_versions": SUPPORTED_PROTOCOL_VERSIONS,
                    "tools": {
                        "static": static_count,
                        "dynamic": dynamic_count,
                        "total": static_count + dynamic_count,
                    },
                    "providers": provider_ids,
                    "github_enabled": self.github.is_some(),
                    "context_engine_enabled": self.context_engine.is_some(),
                    "hooks_registered": self.hooks.len(),
                    "metrics": {
                        "tools_tracked": tracked,
                        "tool_calls": calls,
                        "tool_failures": failures,
                    },
                    "uptime_secs": self.uptime().as_secs(),
                }))?
            }
            "skills.list" => serde_json::to_value(self.skills.list()?)?,
            "skills.read" => serde_json::to_value(
                self.skills.read(
                    arguments
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )?,
            )?,
            "skills.add" => {
                self.skills.add(
                    arguments
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )?;
                json!({"ok": true})
            }
            "skills.remove" => json!({
                "removed": self.skills.remove(
                    arguments.get("name").and_then(Value::as_str).unwrap_or_default()
                )?
            }),
            "skills.search" => serde_json::to_value(
                self.skills.search_global(
                    arguments
                        .get("query")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )?,
            )?,
            "workspace.context" => serde_json::to_value(self.workspace.context()?)?,
            "workspace.list_files" => serde_json::to_value(
                self.workspace.list_files(
                    arguments
                        .get("path")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow::anyhow!("missing or non-string 'path' argument"))?,
                )?,
            )?,
            "workspace.read_file" => serde_json::to_value(
                self.workspace.read_file(
                    arguments
                        .get("path")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow::anyhow!("missing or non-string 'path' argument"))?,
                )?,
            )?,
            "workspace.write_file" => {
                let path = strval(&arguments, "path")?;
                let content = strval(&arguments, "content")?;
                // File mutation is exactly what the audit log exists for;
                // the content itself is never logged.
                audit_allow("workspace_write", &path, "");
                self.workspace.write_file(&path, &content)?;
                json!({"written": true})
            }
            "workspace.delete_file" => {
                let path = strval(&arguments, "path")?;
                audit_allow("workspace_delete", &path, "");
                json!({"deleted": self.workspace.delete_file(&path)?})
            }
            "memory.store" => {
                let scope = parse_scope(arguments.get("scope").and_then(Value::as_str))?;
                serde_json::to_value(self.memory.store(
                    strval(&arguments, "id")?,
                    strval(&arguments, "content")?,
                    scope,
                    strings(&arguments, "tags"),
                )?)?
            }
            "memory.search" => serde_json::to_value(
                self.memory.search(
                    arguments
                        .get("query")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    arguments
                        .get("scope")
                        .and_then(Value::as_str)
                        .map(|scope| parse_scope(Some(scope)))
                        .transpose()?,
                )?,
            )?,
            "memory.get" => serde_json::to_value(
                self.memory.get(
                    arguments
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )?,
            )?,
            "memory.delete" => json!({
                "deleted": self.memory.delete(
                    arguments.get("id").and_then(Value::as_str).unwrap_or_default()
                )?
            }),
            "memory.update" => {
                // Updating must not silently create: the entry has to
                // exist, otherwise the caller gets a clear error. The
                // existence check and the write happen under one lock hold
                // inside `update_existing`, so a concurrent delete from
                // another agent can't race a separate get+store into
                // resurrecting the entry.
                let id = strval(&arguments, "id")?;
                let scope = parse_scope(arguments.get("scope").and_then(Value::as_str))?;
                let entry = self.memory.update_existing(
                    id,
                    strval(&arguments, "content")?,
                    scope,
                    strings(&arguments, "tags"),
                )?;
                serde_json::to_value(entry)?
            }
            "tasks.create" => serde_json::to_value(self.tasks.create(
                strval(&arguments, "id")?,
                strval(&arguments, "title")?,
                strval(&arguments, "description")?,
                parse_priority(arguments.get("priority").and_then(Value::as_str))?,
                strings(&arguments, "tags"),
            )?)?,
            "tasks.list" => serde_json::to_value(
                self.tasks.list(
                    arguments
                        .get("status")
                        .and_then(Value::as_str)
                        .map(parse_status)
                        .transpose()?,
                )?,
            )?,
            "tasks.get" => serde_json::to_value(self.tasks.get(&strval(&arguments, "id")?)?)?,
            "tasks.update" => serde_json::to_value(
                self.tasks.update(
                    arguments
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    arguments
                        .get("status")
                        .and_then(Value::as_str)
                        .map(parse_status)
                        .transpose()?,
                    arguments
                        .get("priority")
                        .and_then(Value::as_str)
                        .map(|value| parse_priority(Some(value)))
                        .transpose()?,
                    arguments
                        .get("assignee")
                        .map(|value| value.as_str().map(str::to_string)),
                )?,
            )?,
            "tasks.delete" => json!({
                "deleted": self.tasks.delete(
                    arguments.get("id").and_then(Value::as_str).unwrap_or_default()
                )?
            }),
            "connectors.list" => serde_json::to_value(self.connectors.list()?)?,
            "connectors.get" => {
                serde_json::to_value(self.connectors.get(&strval(&arguments, "id")?)?)?
            }
            "connectors.add" => {
                let connector = Connector {
                    id: strval(&arguments, "id")?,
                    name: strval(&arguments, "name")?,
                    provider: strval(&arguments, "provider")?,
                    auth: parse_auth(arguments.get("auth").and_then(Value::as_str))?,
                    scopes: strings(&arguments, "scopes"),
                    enabled: arguments
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                };
                serde_json::to_value(self.connectors.add(connector)?)?
            }
            "connectors.enable" => serde_json::to_value(
                self.connectors.set_enabled(
                    arguments
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    true,
                )?,
            )?,
            "connectors.disable" => serde_json::to_value(
                self.connectors.set_enabled(
                    arguments
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    false,
                )?,
            )?,
            "connectors.remove" => json!({
                "removed": self.connectors.remove(
                    arguments.get("id").and_then(Value::as_str).unwrap_or_default()
                )?
            }),
            "connector.providers" => {
                let registry = self.providers.read().await;
                json!(registry.providers())
            }
            "connector.tools" => {
                let provider = arguments
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let registry = self.providers.read().await;
                serde_json::to_value(registry.tools(provider).await?)?
            }
            "connector.invoke" => {
                let provider = arguments
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let tool = arguments
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let args = arguments
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                // External connector invocations are security-relevant: log the
                // provider and tool (never the arguments, which may hold secrets).
                audit_allow("connector_invoke", provider, tool);
                // AWH-side validation for the generic invoke path too: the
                // same schema gate as direct `provider.tool` calls. A dynamic
                // tool cannot bypass schema validation by addressing itself
                // through `connector.invoke`.
                let qualified = format!("{provider}.{tool}");
                self.validate_dynamic_arguments(&qualified, &args).await?;
                let registry = self.providers.read().await;
                serde_json::to_value(registry.invoke(provider, tool, args).await?)?
            }
            "connector.composio_link" => {
                let auth_config_id = strval(&arguments, "auth_config_id")?;
                let user_id = strval(&arguments, "user_id")?;
                let alias = arguments.get("alias").and_then(Value::as_str);
                let callback_url = arguments.get("callback_url").and_then(Value::as_str);
                audit_allow("composio_link", &auth_config_id, &user_id);
                let link = ComposioAuth::from_env()?
                    .create_link(&auth_config_id, &user_id, alias, callback_url)
                    .await?;
                serde_json::to_value(link)?
            }
            "connector.composio_accounts" => {
                let user_id = arguments.get("user_id").and_then(Value::as_str);
                let toolkit = arguments.get("toolkit").and_then(Value::as_str);
                let accounts = ComposioAuth::from_env()?
                    .list_accounts(user_id, toolkit)
                    .await?;
                serde_json::to_value(accounts)?
            }
            "connector.composio_register" => {
                let label = strval(&arguments, "label")?;
                let connected_account_id = strval(&arguments, "connected_account_id")?;
                let toolkit = arguments
                    .get("toolkit")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                audit_allow("composio_register", &label, &connected_account_id);
                let account = ComposioRegistry::new()?.register(ComposioAccount {
                    label: label.clone(),
                    connected_account_id: connected_account_id.clone(),
                    toolkit: toolkit.clone(),
                })?;
                // Make the account usable in *this* running session immediately,
                // not just for dispatchers built after this one starts: every
                // other project's next `McpDispatcher::new` will also pick it
                // up from the persistent registry above.
                if let Ok(api_key) = std::env::var("COMPOSIO_API_KEY") {
                    if !api_key.trim().is_empty() {
                        self.providers
                            .write()
                            .await
                            .register(Box::new(ComposioProvider::new(
                                format!("composio:{label}"),
                                api_key,
                                Some(connected_account_id),
                                toolkit,
                            )?));
                    }
                }
                serde_json::to_value(account)?
            }
            "connector.composio_remove" => {
                let label = strval(&arguments, "label")?;
                let removed = ComposioRegistry::new()?.remove(&label)?;
                self.providers
                    .write()
                    .await
                    .unregister(&format!("composio:{label}"));
                json!({ "removed": removed })
            }
            "context.status" => serde_json::to_value(self.context()?.status()?)?,
            "context.insert" => {
                let item = ContextItem::new(
                    strval(&arguments, "id")?,
                    parse_context_source(arguments.get("source").and_then(Value::as_str)),
                    strval(&arguments, "content")?,
                    0,
                );
                let item = with_optional(
                    item,
                    arguments.get("relevance").and_then(Value::as_f64),
                    |mut it, v| {
                        it.relevance = v.clamp(0.0, 1.0) as f32;
                        it
                    },
                );
                let item = with_optional(
                    item,
                    arguments.get("priority").and_then(Value::as_f64),
                    |mut it, v| {
                        it.priority = v.clamp(0.0, 1.0) as f32;
                        it
                    },
                );
                let item = with_optional(
                    item,
                    arguments
                        .get("scope")
                        .and_then(Value::as_str)
                        .map(|v| parse_context_scope(Some(v)))
                        .transpose()?,
                    |mut it, v| {
                        it.scope = v;
                        it
                    },
                );
                serde_json::to_value(self.context()?.insert(item)?)?
            }
            "context.get" => serde_json::to_value(
                self.context()?.get_item(
                    arguments
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
            )?,
            "context.remove" => json!({
                "removed": self.context()?.remove_item(
                    arguments.get("id").and_then(Value::as_str).unwrap_or_default()
                )
            }),
            "context.search" => {
                let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize;
                serde_json::to_value(
                    self.context()?.search(
                        arguments
                            .get("query")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        limit,
                    )?,
                )?
            }
            "context.optimize" => serde_json::to_value(
                self.context()?.optimize(
                    arguments
                        .get("task")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )?,
            )?,
            "context.assemble" => {
                let engine = self.context()?;
                let request = ContextRequest {
                    task: strval(&arguments, "task")?,
                    query: arguments
                        .get("query")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    token_budget: engine.budget().usable_input_tokens(),
                    scope: ContextScope::Project,
                };
                let request = with_optional(
                    request,
                    arguments.get("token_budget").and_then(Value::as_u64),
                    |mut r, v| {
                        r.token_budget = v as usize;
                        r
                    },
                );
                serde_json::to_value(engine.get_context(&request)?)?
            }
            "context.protect" => json!({
                "protected": self.context()?.protect(
                    arguments.get("id").and_then(Value::as_str).unwrap_or_default()
                )
            }),
            "context.unprotect" => json!({
                "unprotected": self.context()?.unprotect(
                    arguments.get("id").and_then(Value::as_str).unwrap_or_default()
                )
            }),
            "context.offload" => {
                self.context()?.offload(
                    arguments
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    arguments
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("manual offload via MCP tool"),
                )?;
                json!({"offloaded": true})
            }
            "context.restore" => {
                let item = self.context()?.restore(
                    arguments
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )?;
                serde_json::to_value(item)?
            }
            "github.pr_list" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.pr_list", &target);
                serde_json::to_value(
                    github
                        .pr_list(
                            &target,
                            arguments.get("state").and_then(Value::as_str),
                            arguments.get("base").and_then(Value::as_str),
                        )
                        .await?,
                )?
            }
            "github.pr_get" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.pr_get", &target);
                serde_json::to_value(
                    github
                        .pr_get(&target, u64val(&arguments, "number")?)
                        .await?,
                )?
            }
            "github.pr_create" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.pr_create", &target);
                serde_json::to_value(
                    github
                        .pr_create(
                            &target,
                            &strval(&arguments, "title")?,
                            &strval(&arguments, "head")?,
                            &strval(&arguments, "base")?,
                            arguments.get("body").and_then(Value::as_str),
                            arguments.get("draft").and_then(Value::as_bool),
                        )
                        .await?,
                )?
            }
            "github.pr_merge" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.pr_merge", &target);
                serde_json::to_value(
                    github
                        .pr_merge(
                            &target,
                            u64val(&arguments, "number")?,
                            arguments.get("merge_method").and_then(Value::as_str),
                        )
                        .await?,
                )?
            }
            "github.pr_review" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.pr_review", &target);
                serde_json::to_value(
                    github
                        .pr_review(
                            &target,
                            u64val(&arguments, "number")?,
                            &strval(&arguments, "event")?,
                            arguments.get("body").and_then(Value::as_str),
                        )
                        .await?,
                )?
            }
            "github.issue_list" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.issue_list", &target);
                serde_json::to_value(
                    github
                        .issue_list(
                            &target,
                            arguments.get("state").and_then(Value::as_str),
                            arguments.get("labels").and_then(Value::as_str),
                        )
                        .await?,
                )?
            }
            "github.issue_get" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.issue_get", &target);
                serde_json::to_value(
                    github
                        .issue_get(&target, u64val(&arguments, "number")?)
                        .await?,
                )?
            }
            "github.issue_create" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.issue_create", &target);
                serde_json::to_value(
                    github
                        .issue_create(
                            &target,
                            &strval(&arguments, "title")?,
                            arguments.get("body").and_then(Value::as_str),
                            arguments
                                .get("labels")
                                .and_then(Value::as_array)
                                .map(|values| {
                                    values
                                        .iter()
                                        .filter_map(Value::as_str)
                                        .map(str::to_string)
                                        .collect()
                                }),
                        )
                        .await?,
                )?
            }
            "github.issue_comment" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.issue_comment", &target);
                serde_json::to_value(
                    github
                        .issue_comment(
                            &target,
                            u64val(&arguments, "number")?,
                            &strval(&arguments, "body")?,
                        )
                        .await?,
                )?
            }
            "github.checks_status" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.checks_status", &target);
                serde_json::to_value(
                    github
                        .checks_status(&target, &strval(&arguments, "ref")?)
                        .await?,
                )?
            }
            "github.workflow_dispatch" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.workflow_dispatch", &target);
                serde_json::to_value(
                    github
                        .workflow_dispatch(
                            &target,
                            &strval(&arguments, "workflow_file")?,
                            &strval(&arguments, "ref")?,
                        )
                        .await?,
                )?
            }
            "github.release_create" => {
                let github = self.github()?;
                let target = self.resolve_github_target(&arguments).await?;
                audit_github_invocation("github.release_create", &target);
                serde_json::to_value(
                    github
                        .release_create(
                            &target,
                            &strval(&arguments, "tag_name")?,
                            arguments.get("name").and_then(Value::as_str),
                            arguments.get("body").and_then(Value::as_str),
                            arguments.get("draft").and_then(Value::as_bool),
                            arguments.get("prerelease").and_then(Value::as_bool),
                        )
                        .await?,
                )?
            }
            "git.status" => {
                let git = GitService::open(self.workspace.root())?;
                serde_json::to_value(git.status().await?)?
            }
            "git.branch" => {
                let git = GitService::open(self.workspace.root())?;
                serde_json::to_value(git.branch().await?)?
            }
            "git.log" => {
                let git = GitService::open(self.workspace.root())?;
                let limit = arguments
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(20)
                    .clamp(1, 200) as usize;
                serde_json::to_value(git.log(limit).await?)?
            }
            "git.diff" => {
                let git = GitService::open(self.workspace.root())?;
                let path = arguments.get("path").and_then(Value::as_str);
                let staged = arguments
                    .get("staged")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let out = if staged {
                    git.diff_staged(path).await?
                } else {
                    git.diff(path).await?
                };
                serde_json::to_value(out)?
            }
            "git.stage" => {
                let git = GitService::open(self.workspace.root())?;
                let path = strval(&arguments, "path")?;
                if path.is_empty() {
                    bail!("git.stage requires a non-empty 'path' (use \".\" to stage all changes)");
                }
                serde_json::to_value(git.stage(&path).await?)?
            }
            "git.unstage" => {
                let git = GitService::open(self.workspace.root())?;
                let path = strval(&arguments, "path")?;
                if path.is_empty() {
                    bail!("git.unstage requires a non-empty 'path' (use \".\" to unstage all changes)");
                }
                serde_json::to_value(git.unstage(&path).await?)?
            }
            "git.commit" => {
                let git = GitService::open(self.workspace.root())?;
                let message = strval(&arguments, "message")?;
                serde_json::to_value(git.commit(&message).await?)?
            }
            "terminal.run" => {
                let program = strval(&arguments, "program")?;
                let args: Vec<String> = arguments
                    .get("args")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let terminal = TerminalService::new(self.workspace.root());
                serde_json::to_value(terminal.run(&program, &args).await?)?
            }
            _ if name.contains('.') => {
                // AWH-side argument validation for dynamic tools: the
                // arguments must match the schema the provider advertised
                // (the same one tools/list gates). Provider-side validation
                // remains as defense-in-depth; AWH does not rely on it.
                self.validate_dynamic_arguments(name, &arguments).await?;
                let registry = self.providers.read().await;
                serde_json::to_value(registry.invoke_qualified(name, arguments).await?)?
            }
            _ => {
                // Unknown tool: fail as an invalid-params error rather than a
                // success envelope containing an error string (MCP conformance).
                return Err(DispatchError::invalid_params(format!("unknown tool: {name}")).into());
            }
        };

        Ok(json!({
            "content": [{"type": "text", "text": serde_json::to_string(&value)?}]
        }))
    }

    /// Looks up a dynamic (`provider.tool`) tool's advertised schema and
    /// validates the call arguments against it BEFORE the provider is
    /// invoked. Malformed advertised schemas and schema mismatches both
    /// fail closed as `-32602` invalid params.
    async fn validate_dynamic_arguments(
        &self,
        qualified_name: &str,
        arguments: &Value,
    ) -> Result<()> {
        let Some((provider, tool)) = qualified_name.split_once('.') else {
            bail!("tool must use provider.tool format");
        };
        let descriptors = {
            let registry = self.providers.read().await;
            registry.tools(provider).await?
        };
        let Some(descriptor) = descriptors.iter().find(|d| d.name == tool) else {
            audit_deny("tool_validation", "unknown_dynamic_tool", qualified_name);
            return Err(
                DispatchError::invalid_params(format!("unknown tool: {qualified_name}")).into(),
            );
        };
        // Both the schema's syntax and the arguments against it: a schema
        // that could not be advertised (tools/list drops it) must also not
        // be invocable directly.
        if let Err(error) = validate_schema_syntax(&descriptor.input_schema, "#") {
            audit_deny("tool_validation", "malformed_tool_schema", qualified_name);
            return Err(DispatchError::invalid_params(format!(
                "tool '{qualified_name}' has a malformed input schema: {error}"
            ))
            .into());
        }
        if let Err(error) = validate_schema(&descriptor.input_schema, arguments, "arguments", "#") {
            audit_deny("tool_validation", "dynamic_schema_mismatch", qualified_name);
            return Err(DispatchError::invalid_params(format!(
                "invalid arguments for '{qualified_name}': {error}"
            ))
            .into());
        }
        Ok(())
    }
}

/// Records a failed tool dispatch in the audit ring with a stable slug and
/// the tool name only — never the error text (it can echo argument or path
/// content) and never the arguments.
fn audit_tool_failure(tool: &str) {
    audit_deny("tool_failure", "dispatch_failed", tool);
}

/// Records a `github.*` tool call in the audit ring. Only the tool name and
/// the `owner/repo` target are recorded — never the token, never any freeform
/// argument (bodies, titles, and reviews can hold user content).
fn audit_github_invocation(tool: &str, target: &RepoTarget) {
    // Mirrors `connector_invoke` (action, provider, tool): the action is the
    // filterable event name, the tool goes in subject, and the repo target in
    // detail. Request bodies/titles are never audited.
    audit_allow(
        "github_invoke",
        tool,
        &format!("{}/{}", target.owner, target.repo),
    );
}

/// The `github.*` tool schemas, kept in their own macro invocation so the
/// `tools_list_static` arrays stay under the macro recursion limit.
fn github_tool_schemas() -> Value {
    json!([
        {"name":"github.pr_list","description":"List pull requests (state defaults to open; owner/repo default to the origin remote or GITHUB_DEFAULT_OWNER/GITHUB_DEFAULT_REPO)","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"state":{"type":"string","enum":["open","closed","all"]},"base":{"type":"string"}}}},
        {"name":"github.pr_get","description":"Get a single pull request","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"}},"required":["number"]}},
        {"name":"github.pr_create","description":"Create a pull request","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"title":{"type":"string"},"head":{"type":"string"},"base":{"type":"string"},"body":{"type":"string"},"draft":{"type":"boolean"}},"required":["title","head","base"]}},
        {"name":"github.pr_merge","description":"Merge a pull request","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"},"merge_method":{"type":"string","enum":["merge","squash","rebase"]}},"required":["number"]}},
        {"name":"github.pr_review","description":"Submit a pull request review","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"},"event":{"type":"string","enum":["APPROVE","REQUEST_CHANGES","COMMENT"]},"body":{"type":"string"}},"required":["number","event"]}},
        {"name":"github.issue_list","description":"List issues (pull requests are filtered out)","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"state":{"type":"string","enum":["open","closed","all"]},"labels":{"type":"string"}}}},
        {"name":"github.issue_get","description":"Get a single issue or pull request","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"}},"required":["number"]}},
        {"name":"github.issue_create","description":"Create an issue","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"title":{"type":"string"},"body":{"type":"string"},"labels":{"type":"array","items":{"type":"string"}}},"required":["title"]}},
        {"name":"github.issue_comment","description":"Comment on an issue or pull request","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"},"body":{"type":"string"}},"required":["number","body"]}},
        {"name":"github.checks_status","description":"Get combined CI/check status (legacy statuses plus check runs) for a commit or branch","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"ref":{"type":"string"}},"required":["ref"]}},
        {"name":"github.workflow_dispatch","description":"Manually trigger a workflow_dispatch run on a branch","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"workflow_file":{"type":"string"},"ref":{"type":"string"}},"required":["workflow_file","ref"]}},
        {"name":"github.release_create","description":"Create a release","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"tag_name":{"type":"string"},"name":{"type":"string"},"body":{"type":"string"},"draft":{"type":"boolean"},"prerelease":{"type":"boolean"}},"required":["tag_name"]}}
    ])
}

/// Catalog metadata for one tool: its category (for discovery and
/// observability — never a security boundary) and its tool version.
///
/// Categories are purely descriptive metadata for clients — they are
/// never capabilities, permissions, or policy.
///
/// The static catalog is resolved EXACTLY by name through the canonical
/// Tool Registry ([`crate::mcp::tool_registry`]). There is deliberately
/// NO name-prefix fallback: a name that is not a registered tool is
/// "uncategorized" rather than silently inheriting a category from its
/// prefix — a future or dynamic tool must never masquerade as, say, a
/// "workspace" tool just because its name starts with `workspace.`.
pub fn tool_metadata(name: &str) -> (&'static str, &'static str) {
    match tool_registry::registry_lookup(name) {
        Some(tool) => (tool.category, tool.tool_version),
        None => ("uncategorized", tool_registry::AWH_TOOL_API_VERSION),
    }
}

/// The full explicit metadata record for a tool: registry entry for
/// static tools; a synthesized record for dynamic (`provider.tool`)
/// tools carrying what the provider declared plus the central AWH tool
/// API version as the tool version fallback.
pub struct DynamicToolMetadata {
    pub category: &'static str,
    pub tool_version: &'static str,
    pub schema_version: u32,
    pub provider: String,
    pub risk: Option<tool_registry::ToolRisk>,
    pub required_permissions: &'static [permissions::Permission],
}

/// Metadata for a dynamic tool: the provider id is explicit from the
/// qualified name; the category is the dedicated `connector` category;
/// risk is NOT asserted (providers do not declare it and AWH must not
/// guess); the tool version defaults to the central AWH tool API version
/// until the provider declares its own.
pub fn dynamic_tool_metadata(qualified_name: &str) -> DynamicToolMetadata {
    DynamicToolMetadata {
        category: "connector",
        tool_version: tool_registry::AWH_TOOL_API_VERSION,
        schema_version: tool_registry::SCHEMA_FORMAT_VERSION,
        provider: qualified_name
            .split_once('.')
            .map(|(provider, _)| provider.to_string())
            .unwrap_or_default(),
        risk: None,
        required_permissions: &[],
    }
}

/// Whether a dispatch result is a success response (no `error` field). Used
/// by the lifecycle gate: only a *successful* `initialize` completes the
/// session handshake; an error response leaves the session uninitialized so
/// the client can retry.
fn result_is_ok(result: &DispatchResult) -> bool {
    matches!(result, DispatchResult::Response(r) if r.error.is_none())
}

/// Converts a dispatch error to a [`DispatchError`], preserving an existing
/// error code (e.g. `-32602` for an unknown tool) and defaulting other failures
/// to `-32603` internal error.
fn to_dispatch_error(error: anyhow::Error) -> DispatchError {
    if let Some(de) = error.downcast_ref::<DispatchError>() {
        return DispatchError::new(de.code, de.message.clone());
    }
    DispatchError::internal(error.to_string())
}

/// Loads the persistent trust store from the user data directory, returning
/// `None` if it cannot be loaded (corrupt or missing). A `None` here means
/// "nothing is approved", so every custom MCP server fails closed.
fn load_trust_store() -> Option<PersistentTrustStore> {
    let dir = dirs::home_dir()?.join(".agent-workspace-hub");
    match PersistentTrustStore::new(dir) {
        Ok(store) => Some(store),
        Err(error) => {
            tracing::warn!(event = "trust_store_unreadable", error = %error);
            None
        }
    }
}

/// Whether a custom MCP server is authorized to execute under the centralized
/// execution gate. Fails closed (and audits) on any denial.
fn is_authorized(cfg: &CustomMcpServerConfig, trust_store: Option<&PersistentTrustStore>) -> bool {
    let Some(store) = trust_store else {
        audit_deny("dispatch_custom_mcp", "trust_store_unavailable", &cfg.id);
        return false;
    };
    // Custom (per-project) servers carry no semantic version; use the same
    // `"local"` marker the CLI `trust` command defaults to.
    let request = McpExecutionRequest {
        id: &cfg.id,
        version: "local",
        permissions: &cfg.permissions,
    };
    match authorize_mcp_execution(&request, &store.to_store()) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(event = "mcp_execution_denied", id = %cfg.id, error = %error);
            false
        }
    }
}

/// Extracts a required string argument. Fails closed when the argument is
/// absent or not a JSON string: silently coercing a non-string to "" would
/// let callers store corrupt state (e.g. an empty-content memory) while
/// still receiving a success response.
fn strval(arguments: &Value, key: &str) -> Result<String> {
    match arguments.get(key).and_then(Value::as_str) {
        Some(value) => Ok(value.to_string()),
        None => bail!("missing or non-string '{key}' argument"),
    }
}

/// Extracts a required non-negative integer argument. Fails closed on
/// absence or non-numeric input, matching `strval`'s fail-closed contract.
fn u64val(arguments: &Value, key: &str) -> Result<u64> {
    match arguments.get(key).and_then(Value::as_u64) {
        Some(value) => Ok(value),
        None => bail!("missing or non-integer '{key}' argument"),
    }
}

/// One-line preview for resource descriptions, cut on a char boundary.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}\u{2026}")
    }
}

fn strings(arguments: &Value, key: &str) -> Vec<String> {
    arguments
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_scope(value: Option<&str>) -> Result<MemoryScope> {
    match value.unwrap_or("Project") {
        "Session" => Ok(MemoryScope::Session),
        "Project" => Ok(MemoryScope::Project),
        "Global" => Ok(MemoryScope::Global),
        other => bail!("invalid scope '{other}': expected Session, Project, or Global"),
    }
}

/// Applies `f` to `value` only when the optional argument is present, so tool
/// callers can leave engine fields at their documented defaults.
fn with_optional<T, V>(value: T, optional: Option<V>, f: impl FnOnce(T, V) -> T) -> T {
    match optional {
        Some(v) => f(value, v),
        None => value,
    }
}

fn parse_context_source(value: Option<&str>) -> ContextSource {
    match value.unwrap_or("Other") {
        "System" => ContextSource::System,
        "User" => ContextSource::User,
        "Assistant" => ContextSource::Assistant,
        "Tool" => ContextSource::Tool,
        "Skill" => ContextSource::Skill,
        "File" => ContextSource::File,
        "Memory" => ContextSource::Memory,
        "Workspace" => ContextSource::Workspace,
        "Search" => ContextSource::Search,
        "Summary" => ContextSource::Summary,
        _ => ContextSource::Other,
    }
}

fn parse_context_scope(value: Option<&str>) -> Result<ContextScope> {
    match value.unwrap_or("Project") {
        "Session" => Ok(ContextScope::Session),
        "Project" => Ok(ContextScope::Project),
        "Global" => Ok(ContextScope::Global),
        other => bail!("invalid scope '{other}': expected Session, Project, or Global"),
    }
}

fn parse_status(value: &str) -> Result<TaskStatus> {
    match value {
        "Todo" => Ok(TaskStatus::Todo),
        "InProgress" => Ok(TaskStatus::InProgress),
        "Blocked" => Ok(TaskStatus::Blocked),
        "Done" => Ok(TaskStatus::Done),
        _ => bail!("invalid status '{value}': expected Todo, InProgress, Blocked, or Done"),
    }
}

fn parse_priority(value: Option<&str>) -> Result<TaskPriority> {
    match value.unwrap_or("Normal") {
        "Low" => Ok(TaskPriority::Low),
        "Normal" => Ok(TaskPriority::Normal),
        "High" => Ok(TaskPriority::High),
        "Critical" => Ok(TaskPriority::Critical),
        other => bail!("invalid priority '{other}': expected Low, Normal, High, or Critical"),
    }
}

fn parse_auth(value: Option<&str>) -> Result<AuthMethod> {
    match value.unwrap_or("None") {
        "OAuth" => Ok(AuthMethod::OAuth),
        "ApiKey" => Ok(AuthMethod::ApiKey),
        "None" => Ok(AuthMethod::None),
        other => bail!("invalid auth '{other}': expected OAuth, ApiKey, or None"),
    }
}

/// Builds the circuit-breaker config from the resolved runtime resource limits.
fn circuit_breaker_config() -> CircuitBreakerConfig {
    let limits = ResourceLimits::default()
        .with_env_overrides()
        .unwrap_or_else(|e| {
            tracing::warn!(event = "config_invalid", error = %e);
            ResourceLimits::default()
        });
    CircuitBreakerConfig {
        failure_threshold: limits.circuit_failure_threshold,
        cooldown: limits.circuit_cooldown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{McpPermissions, TrustLevel, TrustStore};
    use axum::response::IntoResponse;
    use std::sync::Mutex;

    fn config(id: &str, permissions: McpPermissions) -> CustomMcpServerConfig {
        CustomMcpServerConfig {
            id: id.to_string(),
            name: id.to_string(),
            transport: McpTransport::Stdio,
            command: Some("echo".to_string()),
            args: Vec::new(),
            url: None,
            env: Default::default(),
            headers: Default::default(),
            permissions,
            enabled: true,
        }
    }

    /// No trust store at all => every custom MCP server fails closed.
    #[test]
    fn unapproved_server_is_denied_when_trust_store_missing() {
        let cfg = config("server", McpPermissions::default());
        assert!(!is_authorized(&cfg, None));
    }

    /// A server with no approval record is denied.
    #[test]
    fn no_approval_record_is_denied() {
        let store = PersistentTrustStore::from_store(&TrustStore::default());
        let cfg = config("server", McpPermissions::default());
        assert!(!is_authorized(&cfg, Some(&store)));
    }

    /// A server blocked at any trust level is denied.
    #[test]
    fn blocked_server_is_denied() {
        let mut trust = TrustStore::default();
        trust
            .approve(
                "server",
                TrustLevel::Blocked,
                McpPermissions::default(),
                "local",
            )
            .unwrap();
        let store = PersistentTrustStore::from_store(&trust);
        let cfg = config("server", McpPermissions::default());
        assert!(!is_authorized(&cfg, Some(&store)));
    }

    /// A reviewed/trusted server with matching permissions and version is allowed.
    #[test]
    fn approved_server_is_allowed() {
        let mut trust = TrustStore::default();
        trust
            .approve(
                "server",
                TrustLevel::Reviewed,
                McpPermissions::default(),
                "local",
            )
            .unwrap();
        let store = PersistentTrustStore::from_store(&trust);
        let cfg = config("server", McpPermissions::default());
        assert!(is_authorized(&cfg, Some(&store)));
    }

    /// A server requesting broader permissions than approved is denied.
    #[test]
    fn over_broad_permissions_are_denied() {
        let mut trust = TrustStore::default();
        trust
            .approve(
                "server",
                TrustLevel::Reviewed,
                McpPermissions::default(),
                "local",
            )
            .unwrap();
        let store = PersistentTrustStore::from_store(&trust);
        let cfg = config(
            "server",
            McpPermissions {
                network: true,
                ..McpPermissions::default()
            },
        );
        assert!(!is_authorized(&cfg, Some(&store)));
    }

    // ---- context engine MCP tool wiring ---------------------------------

    fn call(dispatcher: &McpDispatcher, name: &str, arguments: Value) -> Result<Value> {
        let rt = tokio::runtime::Runtime::new()?;
        let text = rt.block_on(async {
            let request = serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments}
            }))?;
            match dispatcher.dispatch_strict(&request).await {
                Ok(DispatchResult::Response(response)) => Ok(serde_json::to_string(&response)?),
                Ok(DispatchResult::NoResponse) => Ok(String::new()),
                Err(error) => Err(anyhow::anyhow!(error.message)),
            }
        })?;
        let response: Value = serde_json::from_str(&text)?;
        // Unwrap the standard content envelope into the tool's JSON value.
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no content envelope"))?;
        Ok(serde_json::from_str(text)?)
    }

    /// The MCP protocol requires servers to answer `ping` with an empty
    /// result so clients can probe liveness.
    #[test]
    fn ping_returns_empty_result() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let response = rt
            .block_on(async {
                match dispatcher
                    .dispatch_strict(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#)
                    .await
                {
                    Ok(DispatchResult::Response(response)) => Ok(serde_json::to_value(response)?),
                    other => Err(anyhow::anyhow!("unexpected dispatch result: {other:?}")),
                }
            })
            .unwrap();
        assert_eq!(response["result"], json!({}));
        assert!(response.get("error").is_none());
    }

    /// Enum-typed arguments must be rejected at dispatch when they carry a
    /// value outside the schema's declared enum, instead of being silently
    /// coerced to a default (which would corrupt persisted state while
    /// reporting success to the caller).
    #[test]
    fn enum_arguments_reject_values_outside_schema() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();

        // Task lifecycle must accept the documented values...
        call(
            &dispatcher,
            "tasks.create",
            json!({"id": "t1", "title": "title", "description": "d"}),
        )
        .unwrap();
        let updated = call(
            &dispatcher,
            "tasks.update",
            json!({"id": "t1", "status": "InProgress"}),
        )
        .unwrap();
        assert_eq!(updated["status"], "InProgress");

        // ...and reject anything else — including snake_case look-alikes
        // ("in-progress") and outright garbage ("bogus"). Schema-first
        // validation catches these before dispatch, so the error names the
        // failing argument instead of a tool-specific message.
        for status in ["in-progress", "completed", "bogus", "todo", "done"] {
            let error = call(
                &dispatcher,
                "tasks.update",
                json!({"id": "t1", "status": status}),
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("arguments.status"),
                "status '{status}' should be rejected at the schema layer, got: {error}"
            );
        }

        let error = call(
            &dispatcher,
            "tasks.update",
            json!({"id": "t1", "priority": "urgent"}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("arguments.priority"));

        let error = call(
            &dispatcher,
            "tasks.create",
            json!({"id": "t2", "title": "title", "description": "d", "priority": "mega"}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("arguments.priority"));

        let error = call(&dispatcher, "tasks.list", json!({"status": "done"})).unwrap_err();
        assert!(error.to_string().contains("arguments.status"));

        let error = call(
            &dispatcher,
            "memory.store",
            json!({"id": "m1", "content": "c", "scope": "project"}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("arguments.scope"));

        let error = call(
            &dispatcher,
            "memory.search",
            json!({"query": "q", "scope": "workspace"}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("arguments.scope"));

        let error = call(
            &dispatcher,
            "connectors.add",
            json!({"id": "c1", "name": "n", "provider": "p", "auth": "bogus"}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("arguments.auth"));

        let error = call(
            &dispatcher,
            "context.insert",
            json!({"id": "i1", "content": "c", "source": "Tool", "scope": "workspace"}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("arguments.scope"));

        // Sanity: valid enum values still pass through every one of the
        // parsers exercised above.
        call(
            &dispatcher,
            "memory.store",
            json!({"id": "m1", "content": "c", "scope": "Session"}),
        )
        .unwrap();
        call(
            &dispatcher,
            "connectors.add",
            json!({"id": "c1", "name": "n", "provider": "p", "auth": "OAuth"}),
        )
        .unwrap();

        // Required string arguments must produce a clear invalid-params
        // error at the schema layer (missing required property), not a
        // confusing underlying-tool failure or an internal-error code.
        let error = call(&dispatcher, "git.stage", json!({})).unwrap_err();
        assert!(
            error.to_string().contains("missing required field 'path'"),
            "got: {error}"
        );
        let error = call(&dispatcher, "git.unstage", json!({})).unwrap_err();
        assert!(
            error.to_string().contains("missing required field 'path'"),
            "got: {error}"
        );
        let error = call(&dispatcher, "git.stage", json!({"path": ""})).unwrap_err();
        assert!(error.to_string().contains("non-empty 'path'"));
        let error = call(&dispatcher, "git.unstage", json!({"path": ""})).unwrap_err();
        assert!(error.to_string().contains("non-empty 'path'"));

        // Non-string values for string-typed arguments must be rejected,
        // never silently coerced to empty strings (which would store corrupt
        // state while reporting success). The schema layer catches these
        // before dispatch with an "expected schema type" failure.
        let error = call(
            &dispatcher,
            "memory.store",
            json!({"id": "x", "content": 123, "scope": "Project"}),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("arguments.content")
                && error.to_string().contains("expected schema type"),
            "got: {error}"
        );

        let error = call(
            &dispatcher,
            "tasks.create",
            json!({"id": "t3", "title": ["array"], "description": "d"}),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("arguments.title")
                && error.to_string().contains("expected schema type"),
            "got: {error}"
        );

        let error = call(&dispatcher, "workspace.read_file", json!({"path": 123})).unwrap_err();
        assert!(
            error.to_string().contains("arguments.path")
                && error.to_string().contains("expected schema type"),
            "got: {error}"
        );

        let error = call(&dispatcher, "workspace.read_file", json!({})).unwrap_err();
        assert!(
            error.to_string().contains("missing required field 'path'"),
            "got: {error}"
        );

        let error = call(
            &dispatcher,
            "terminal.run",
            json!({"program": 42, "args": []}),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("arguments.program")
                && error.to_string().contains("expected schema type"),
            "got: {error}"
        );
    }

    #[test]
    fn context_tools_insert_status_offload_restore_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();

        let inserted = call(
            &dispatcher,
            "context.insert",
            json!({"id": "notes", "content": "deploy instructions for api", "source": "Tool"}),
        )
        .unwrap();
        assert_eq!(inserted["id"], "notes");

        let status = call(&dispatcher, "context.status", json!({})).unwrap();
        assert_eq!(status["active_items"], 1);
        assert!(status["active_tokens"].as_u64().unwrap() > 0);

        call(
            &dispatcher,
            "context.offload",
            json!({"id": "notes", "reason": "test"}),
        )
        .unwrap();
        let status = call(&dispatcher, "context.status", json!({})).unwrap();
        assert_eq!(status["active_items"], 0);
        assert_eq!(status["offloaded_items"], 1);

        let restored = call(&dispatcher, "context.restore", json!({"id": "notes"})).unwrap();
        assert_eq!(restored["id"], "notes");
        let status = call(&dispatcher, "context.status", json!({})).unwrap();
        assert_eq!(status["active_items"], 1);
    }

    #[test]
    fn context_search_finds_active_item() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        call(
            &dispatcher,
            "context.insert",
            json!({"id": "q", "content": "kubernetes rollout strategy"}),
        )
        .unwrap();
        let hits = call(
            &dispatcher,
            "context.search",
            json!({"query": "kubernetes", "limit": 5}),
        )
        .unwrap();
        assert!(hits
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| { hit["item"]["id"] == "q" || hit["id"] == "q" }));
    }

    #[test]
    fn context_assemble_respects_budget_argument() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        call(
            &dispatcher,
            "context.insert",
            json!({"id": "small", "content": "tiny relevant snippet"}),
        )
        .unwrap();
        let words: Vec<String> = (0..400).map(|i| format!("filler{i}")).collect();
        call(
            &dispatcher,
            "context.insert",
            json!({"id": "large", "content": words.join(" "), "relevance": 0.1}),
        )
        .unwrap();
        // The explicit token_budget caps selection: with a 20-token budget the
        // 400-token filler item cannot fit, so only the small item is kept.
        let assembled = call(
            &dispatcher,
            "context.assemble",
            json!({"task": "tiny relevant snippet task", "token_budget": 20}),
        )
        .unwrap();
        let ids: Vec<&str> = assembled["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect();
        assert_eq!(ids, vec!["small"]);
        assert!(assembled["rejected_ids"]
            .as_array()
            .unwrap()
            .contains(&json!("large")));
    }

    #[test]
    fn context_get_missing_id_returns_null_like_memory_get() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        let value = call(&dispatcher, "context.get", json!({"id": "nope"})).unwrap();
        assert!(value.is_null());
    }

    #[test]
    fn context_tools_listed_in_tools_list() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let text = rt
            .block_on(async {
                match dispatcher
                    .dispatch_strict(
                        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#,
                    )
                    .await
                {
                    Ok(DispatchResult::Response(response)) => {
                        Ok(serde_json::to_string(&response).unwrap())
                    }
                    Ok(DispatchResult::NoResponse) => Ok(String::new()),
                    Err(error) => Err(error.message),
                }
            })
            .unwrap();
        let response: Value = serde_json::from_str(&text).unwrap();
        let tools: Vec<String> = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect();
        assert!(tools.contains(&"context.status".to_string()));
        assert!(tools.contains(&"context.assemble".to_string()));
    }

    // ---- git/terminal service-backed tools -------------------------------

    /// Initializes a real git repository inside `dir` so git tools can be
    /// exercised against a working tree.
    fn init_git_repo(dir: &std::path::Path) {
        let output = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(dir)
            .output()
            .expect("git init");
        assert!(output.status.success(), "git init failed");
        let output = std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .output()
            .expect("git config");
        assert!(output.status.success());
        let output = std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .expect("git config");
        assert!(output.status.success());
    }

    #[test]
    fn git_tools_round_trip_in_real_repo() {
        let temp = tempfile::tempdir().unwrap();
        init_git_repo(temp.path());
        std::fs::write(temp.path().join("notes.md"), "hello").unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();

        // Stage, commit, then verify status and log reflect the change.
        call(&dispatcher, "git.stage", json!({"path": "."})).unwrap();
        call(
            &dispatcher,
            "git.commit",
            json!({"message": "initial commit"}),
        )
        .unwrap();
        let status = call(&dispatcher, "git.status", json!({})).unwrap();
        let stdout = status["stdout"].as_str().unwrap_or_default();
        assert!(
            stdout.is_empty(),
            "tree should be clean after commit: {stdout}"
        );
        let log = call(&dispatcher, "git.log", json!({"limit": 5})).unwrap();
        assert!(log["stdout"]
            .as_str()
            .unwrap_or_default()
            .contains("initial commit"));
    }

    #[test]
    fn git_log_rejects_absurd_limits() {
        let temp = tempfile::tempdir().unwrap();
        init_git_repo(temp.path());
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        // limit is clamped to [1, 200]; a huge value must not panic.
        let log = call(&dispatcher, "git.log", json!({"limit": 1000000})).unwrap();
        assert!(log["stdout"].as_str().is_some());
    }

    #[test]
    fn terminal_run_executes_argv_without_shell() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        let out = call(
            &dispatcher,
            "terminal.run",
            json!({"program": "printf", "args": ["argv works"]}),
        )
        .unwrap();
        assert_eq!(out["stdout"].as_str().unwrap_or_default(), "argv works");
        assert_eq!(out["exit_code"].as_i64(), Some(0));
    }

    #[test]
    fn terminal_run_rejects_shell_strings() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        // A program name containing whitespace is a shell command line,
        // which the service refuses to interpret.
        let result = call(
            &dispatcher,
            "terminal.run",
            json!({"program": "echo hello; rm -rf /"}),
        );
        assert!(result.is_err());
    }

    // ---- github.* dispatcher wiring ---------------------------------------
    //
    // These build the dispatcher directly with a stubbed provider (a local
    // axum server standing in for api.github.com) so they never touch the
    // network, never require GITHUB_TOKEN, and never race other tests over
    // environment variables.

    /// Builds a dispatcher whose github field points at a local stub server
    /// answering every request with `body`, and records the paths it saw.
    /// The server's tokio runtime is intentionally leaked: it must outlive
    /// this helper (each later `call` runs on its own runtime) for the
    /// lifetime of the test.
    fn dispatcher_with_github_stub(
        temp: &tempfile::TempDir,
        body: serde_json::Value,
    ) -> (McpDispatcher, Arc<Mutex<Vec<String>>>) {
        let paths: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let paths_for_handler = Arc::clone(&paths);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let base_url = rt.block_on(async {
            let router = axum::Router::new().fallback(move |req: axum::extract::Request| {
                let paths = Arc::clone(&paths_for_handler);
                async move {
                    let path = req.uri().path().to_string();
                    paths.lock().unwrap().push(path);
                    axum::Json(body.clone()).into_response()
                }
            });
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap();
            });
            format!("http://{addr}")
        });
        std::mem::forget(rt);

        // Same construction path as `McpDispatcher::new`, then the github
        // provider is swapped for one bound to the stub server.
        let mut dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        dispatcher.github = Some(Arc::new(
            GithubProvider::new("stub-token", &base_url).expect("provider"),
        ));
        (dispatcher, paths)
    }

    #[test]
    fn github_tools_are_listed_when_provider_is_present() {
        let temp = tempfile::tempdir().unwrap();
        let (dispatcher, _paths) = dispatcher_with_github_stub(&temp, serde_json::json!([]));
        let tools = dispatcher.tools_list_static();
        let names: Vec<&str> = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        for expected in [
            "github.pr_list",
            "github.pr_get",
            "github.pr_create",
            "github.pr_merge",
            "github.pr_review",
            "github.issue_list",
            "github.issue_get",
            "github.issue_create",
            "github.issue_comment",
            "github.checks_status",
            "github.workflow_dispatch",
            "github.release_create",
        ] {
            assert!(
                names.contains(&expected),
                "{expected} missing from tools/list ({} advertised)",
                names.len()
            );
        }
    }

    #[test]
    fn github_tools_are_absent_when_provider_is_disabled() {
        let temp = tempfile::tempdir().unwrap();
        // Force the disabled state regardless of the ambient environment:
        // the field is private, but this test module can set it directly.
        let mut dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        dispatcher.github = None;
        let tools = dispatcher.tools_list_static();
        let names: Vec<&str> = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(
            !names.iter().any(|n| n.starts_with("github.")),
            "github tools advertised while disabled: {names:?}"
        );
    }

    #[test]
    fn github_call_round_trips_through_the_stub_server() {
        let temp = tempfile::tempdir().unwrap();
        let (dispatcher, paths) = dispatcher_with_github_stub(
            &temp,
            serde_json::json!([{"number": 7, "title": "stubbed"}]),
        );
        let result = call(
            &dispatcher,
            "github.pr_list",
            json!({"owner": "o", "repo": "r", "state": "open"}),
        )
        .unwrap();
        assert_eq!(result[0]["number"], 7);
        let recorded = paths.lock().unwrap().clone();
        assert_eq!(
            recorded,
            vec!["/repos/o/r/pulls".to_string()],
            "expected exactly one GET /repos/o/r/pulls, saw {recorded:?}"
        );
    }

    #[test]
    fn github_call_fails_closed_when_provider_is_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let mut dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        dispatcher.github = None;
        let error = call(
            &dispatcher,
            "github.pr_list",
            json!({"owner": "o", "repo": "r"}),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("set GITHUB_TOKEN"),
            "unexpected: {error}"
        );
    }

    #[test]
    fn github_half_specified_target_never_mixes_sources() {
        let temp = tempfile::tempdir().unwrap();
        let (dispatcher, _paths) = dispatcher_with_github_stub(&temp, serde_json::json!([]));
        // owner given, repo missing: the call must fail closed rather than
        // pair the explicit owner with a repo from some other source.
        let error = call(&dispatcher, "github.pr_list", json!({"owner": "o"})).unwrap_err();
        assert!(
            error.to_string().contains("owner/repo"),
            "unexpected: {error}"
        );
    }

    #[test]
    fn github_target_resolves_from_the_origin_remote() {
        let temp = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/remote-o/remote-r.git",
            ])
            .current_dir(temp.path())
            .output()
            .unwrap();
        let (dispatcher, paths) = dispatcher_with_github_stub(&temp, serde_json::json!([]));
        call(&dispatcher, "github.pr_list", json!({})).unwrap();
        let recorded = paths.lock().unwrap().clone();
        assert_eq!(
            recorded,
            vec!["/repos/remote-o/remote-r/pulls".to_string()],
            "expected the origin remote to resolve, saw {recorded:?}"
        );
    }

    // ---- workspace.write_file / delete_file dispatcher wiring ------------

    #[test]
    fn workspace_write_and_delete_round_trip_through_tools_call() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();

        let written = call(
            &dispatcher,
            "workspace.write_file",
            json!({"path": "docs/notes.md", "content": "# notes"}),
        )
        .unwrap();
        assert_eq!(written["written"], true);
        assert!(temp.path().join("docs/notes.md").is_file());

        let read = call(
            &dispatcher,
            "workspace.read_file",
            json!({"path": "docs/notes.md"}),
        )
        .unwrap();
        assert!(read.as_str().unwrap_or_default().contains("# notes"));

        let deleted = call(
            &dispatcher,
            "workspace.delete_file",
            json!({"path": "docs/notes.md"}),
        )
        .unwrap();
        assert_eq!(deleted["deleted"], true);
        assert!(!temp.path().join("docs/notes.md").exists());

        // Deleting a missing file reports false, not an error.
        let missing = call(
            &dispatcher,
            "workspace.delete_file",
            json!({"path": "docs/notes.md"}),
        )
        .unwrap();
        assert_eq!(missing["deleted"], false);
    }

    #[test]
    fn workspace_write_rejects_traversal_through_tools_call() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        let error = call(
            &dispatcher,
            "workspace.write_file",
            json!({"path": "../escape.txt", "content": "nope"}),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("unsafe workspace path"),
            "unexpected: {error}"
        );
        assert!(!temp.path().join("../escape.txt").exists());
    }

    // ---- tasks.get / connectors.get dispatcher wiring --------------------

    #[test]
    fn tasks_get_and_connectors_get_serve_single_records() {
        let temp = tempfile::tempdir().unwrap();
        let dispatcher = McpDispatcher::new(temp.path().to_path_buf()).unwrap();
        call(
            &dispatcher,
            "tasks.create",
            json!({"id": "t-1", "title": "title", "description": "d"}),
        )
        .unwrap();
        let task = call(&dispatcher, "tasks.get", json!({"id": "t-1"})).unwrap();
        assert_eq!(task["id"], "t-1");
        let missing_task = call(&dispatcher, "tasks.get", json!({"id": "nope"})).unwrap();
        assert!(missing_task.is_null());

        call(
            &dispatcher,
            "connectors.add",
            json!({"id": "c-1", "name": "n", "provider": "p", "auth": "None"}),
        )
        .unwrap();
        let connector = call(&dispatcher, "connectors.get", json!({"id": "c-1"})).unwrap();
        assert_eq!(connector["id"], "c-1");
        let missing_connector = call(&dispatcher, "connectors.get", json!({"id": "nope"})).unwrap();
        assert!(missing_connector.is_null());
    }
}
