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
    audit_allow, audit_deny, authorize_mcp_execution, AuthMethod, CircuitBreakerConfig,
    CircuitBreakerMcpClient, ComposioProvider, Connector, ConnectorsMcp, CustomMcpProvider,
    CustomMcpRegistry, CustomMcpServerConfig, McpExecutionRequest, McpTransport, MemoryMcp,
    MemoryScope, PersistentTrustStore, ProviderRegistry, ResourceLimits, SkillMcp, StdioMcpClient,
    StreamableHttpMcpClient, TaskPriority, TaskStatus, TasksMcp, WorkspaceMcp,
};
use anyhow::Result;
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

/// JSON-RPC protocol version negotiated by this server.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// A single parsed JSON-RPC request.
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
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
pub struct McpDispatcher {
    skills: Arc<SkillMcp>,
    workspace: Arc<WorkspaceMcp>,
    memory: Arc<MemoryMcp>,
    tasks: Arc<TasksMcp>,
    connectors: Arc<ConnectorsMcp>,
    context_engine: Option<Arc<ContextEngine>>,
    providers: Arc<RwLock<ProviderRegistry>>,
}

impl McpDispatcher {
    /// Builds the dispatcher for a project, wiring the Composio and custom MCP
    /// providers.
    pub fn new(project_root: PathBuf) -> Result<Self> {
        let registry = Arc::new(RwLock::new(ProviderRegistry::default()));

        if std::env::var("COMPOSIO_API_KEY").is_ok() {
            if let Ok(provider) = ComposioProvider::from_env() {
                registry.blocking_write().register(Box::new(provider));
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
                    let rt = tokio::runtime::Runtime::new()?;
                    let client = rt.block_on(StdioMcpClient::spawn(&cfg, project_root.clone()))?;
                    rt.block_on(client.initialize())?;
                    let guarded = CircuitBreakerMcpClient::new(
                        cfg.id.clone(),
                        Arc::new(client),
                        circuit_breaker_config(),
                    );
                    let provider = CustomMcpProvider::new(cfg.id, Arc::new(guarded));
                    registry.blocking_write().register(Box::new(provider));
                }
                McpTransport::StreamableHttp => {
                    let rt = tokio::runtime::Runtime::new()?;
                    let client = StreamableHttpMcpClient::new(&cfg)?;
                    rt.block_on(client.initialize())?;
                    let guarded = CircuitBreakerMcpClient::new(
                        cfg.id.clone(),
                        Arc::new(client),
                        circuit_breaker_config(),
                    );
                    let provider = CustomMcpProvider::new(cfg.id, Arc::new(guarded));
                    registry.blocking_write().register(Box::new(provider));
                }
            }
        }

        // The context engine is opt-in per project via `AWH_CONTEXT_ENGINE`
        // (or the config's `enabled` flag, which honors the same env vars).
        // When construction fails the tools surface a clear error instead of
        // taking the whole dispatcher down, so existing behavior is unchanged.
        let context_engine = ContextEngineConfig::default()
            .with_env_overrides()
            .and_then(|config| ContextEngine::new(&project_root, config).map(Arc::new))
            .ok();

        Ok(Self {
            skills: Arc::new(SkillMcp::new(project_root.clone())?),
            workspace: Arc::new(WorkspaceMcp::new(project_root.clone())?),
            memory: Arc::new(MemoryMcp::new(project_root.clone())?),
            tasks: Arc::new(TasksMcp::new(project_root.clone())?),
            connectors: Arc::new(ConnectorsMcp::new(project_root)?),
            context_engine,
            providers: registry,
        })
    }

    fn context(&self) -> Result<&ContextEngine> {
        self.context_engine
            .as_ref()
            .map(Arc::as_ref)
            .ok_or_else(|| anyhow::anyhow!("context engine is disabled or failed to initialize"))
    }

    /// Returns the shared provider registry used to dispatch tool calls.
    pub fn provider_registry(&self) -> Arc<RwLock<ProviderRegistry>> {
        Arc::clone(&self.providers)
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

    async fn dispatch_inner(&self, input: &str) -> Result<DispatchResult, DispatchError> {
        let req: RpcRequest = serde_json::from_str(input)
            .map_err(|e| DispatchError::parse(format!("invalid JSON: {e}")))?;
        if req.jsonrpc != "2.0" {
            return Err(DispatchError::invalid_request(format!(
                "unsupported JSON-RPC version: {}",
                req.jsonrpc
            )));
        }

        // Notifications (no `id`) produce no response.
        let id = req.id;
        let is_notification = id.is_none();

        // `notifications/initialized` is the standard post-initialize
        // notification; accept it (and any other notification) silently.
        if is_notification {
            return Ok(DispatchResult::NoResponse);
        }

        let result = match req.method.as_str() {
            "initialize" => json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "agent-workspace-hub", "version": env!("CARGO_PKG_VERSION")}
            }),
            "tools/list" => self
                .tools_list_aggregated()
                .await
                .map_err(to_dispatch_error)?,
            "tools/call" => self
                .call_tool(&req.params)
                .await
                .map_err(to_dispatch_error)?,
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

    async fn tools_list_aggregated(&self) -> Result<Value> {
        let mut base = self
            .tools_list_static()
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let registry = self.providers.read().await;
        let dynamic = registry.aggregate_tools().await?;
        for tool in dynamic {
            base.push(serde_json::to_value(tool)?);
        }
        Ok(json!({"tools": base}))
    }

    fn tools_list_static(&self) -> Value {
        json!({"tools": [
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
        ]})
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

        // Audit every tool invocation by name only. Arguments are deliberately
        // never logged: they may contain file contents or secret material.
        audit_allow("tool_invoke", name, "tools/call");

        let value = match name {
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
                self.workspace
                    .list_files(arguments.get("path").and_then(Value::as_str).unwrap_or("."))?,
            )?,
            "workspace.read_file" => serde_json::to_value(
                self.workspace.read_file(
                    arguments
                        .get("path")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )?,
            )?,
            "memory.store" => {
                let scope = parse_scope(arguments.get("scope").and_then(Value::as_str));
                serde_json::to_value(self.memory.store(
                    strval(&arguments, "id"),
                    strval(&arguments, "content"),
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
                        .map(|scope| parse_scope(Some(scope))),
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
            "tasks.create" => serde_json::to_value(self.tasks.create(
                strval(&arguments, "id"),
                strval(&arguments, "title"),
                strval(&arguments, "description"),
                parse_priority(arguments.get("priority").and_then(Value::as_str)),
                strings(&arguments, "tags"),
            )?)?,
            "tasks.list" => serde_json::to_value(
                self.tasks.list(
                    arguments
                        .get("status")
                        .and_then(Value::as_str)
                        .map(parse_status),
                )?,
            )?,
            "tasks.update" => serde_json::to_value(
                self.tasks.update(
                    arguments
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    arguments
                        .get("status")
                        .and_then(Value::as_str)
                        .map(parse_status),
                    arguments
                        .get("priority")
                        .and_then(Value::as_str)
                        .map(|value| parse_priority(Some(value))),
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
            "connectors.add" => {
                let connector = Connector {
                    id: strval(&arguments, "id"),
                    name: strval(&arguments, "name"),
                    provider: strval(&arguments, "provider"),
                    auth: parse_auth(arguments.get("auth").and_then(Value::as_str)),
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
                let registry = self.providers.read().await;
                serde_json::to_value(registry.invoke(provider, tool, args).await?)?
            }
            "context.status" => serde_json::to_value(self.context()?.status()?)?,
            "context.insert" => {
                let item = ContextItem::new(
                    strval(&arguments, "id"),
                    parse_context_source(arguments.get("source").and_then(Value::as_str)),
                    strval(&arguments, "content"),
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
                    arguments.get("scope").and_then(Value::as_str),
                    |mut it, v| {
                        it.scope = parse_context_scope(Some(v));
                        it
                    },
                );
                serde_json::to_value(self.context()?.insert(item)?)?
            }
            "context.get" => {
                let id = arguments
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                match self.context()?.get_item(id) {
                    Some(item) => serde_json::to_value(item)?,
                    None => json!({"error": format!("no context item: {id}")}),
                }
            }
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
                    task: strval(&arguments, "task"),
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
            _ if name.contains('.') => {
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

fn strval(arguments: &Value, key: &str) -> String {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
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

fn parse_scope(value: Option<&str>) -> MemoryScope {
    match value.unwrap_or("Project") {
        "Session" => MemoryScope::Session,
        "Global" => MemoryScope::Global,
        _ => MemoryScope::Project,
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

fn parse_context_scope(value: Option<&str>) -> ContextScope {
    match value.unwrap_or("Project") {
        "Session" => ContextScope::Session,
        "Global" => ContextScope::Global,
        _ => ContextScope::Project,
    }
}

fn parse_status(value: &str) -> TaskStatus {
    match value {
        "InProgress" => TaskStatus::InProgress,
        "Blocked" => TaskStatus::Blocked,
        "Done" => TaskStatus::Done,
        _ => TaskStatus::Todo,
    }
}

fn parse_priority(value: Option<&str>) -> TaskPriority {
    match value.unwrap_or("Normal") {
        "Low" => TaskPriority::Low,
        "High" => TaskPriority::High,
        "Critical" => TaskPriority::Critical,
        _ => TaskPriority::Normal,
    }
}

fn parse_auth(value: Option<&str>) -> AuthMethod {
    match value.unwrap_or("None") {
        "OAuth" => AuthMethod::OAuth,
        "ApiKey" => AuthMethod::ApiKey,
        _ => AuthMethod::None,
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

    fn config(id: &str, permissions: McpPermissions) -> CustomMcpServerConfig {
        CustomMcpServerConfig {
            id: id.to_string(),
            name: id.to_string(),
            transport: McpTransport::Stdio,
            command: Some("echo".to_string()),
            args: Vec::new(),
            url: None,
            env: Default::default(),
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
}
