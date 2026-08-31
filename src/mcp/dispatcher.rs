//! Transport-agnostic MCP JSON-RPC dispatcher.
//!
//! This module owns the *single* implementation of the MCP tool surface
//! (skills, workspace, memory, tasks, connectors, and dynamic providers). Every
//! transport — stdio, HTTP/SSE, and any future transport — funnels requests
//! through [`McpDispatcher`] so the tool implementations are never duplicated.

use crate::mcp::{
    AuthMethod, CircuitBreakerConfig, CircuitBreakerMcpClient, ComposioProvider, Connector,
    ConnectorsMcp, CustomMcpProvider, CustomMcpRegistry, McpTransport, MemoryMcp, MemoryScope,
    ProviderRegistry, ResourceLimits, SkillMcp, StdioMcpClient, StreamableHttpMcpClient,
    TaskPriority, TaskStatus, TasksMcp, WorkspaceMcp,
};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

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
        for cfg in custom.list()? {
            if !cfg.enabled {
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

        Ok(Self {
            skills: Arc::new(SkillMcp::new(project_root.clone())?),
            workspace: Arc::new(WorkspaceMcp::new(project_root.clone())?),
            memory: Arc::new(MemoryMcp::new(project_root.clone())?),
            tasks: Arc::new(TasksMcp::new(project_root.clone())?),
            connectors: Arc::new(ConnectorsMcp::new(project_root)?),
            providers: registry,
        })
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
                        "code": -32600,
                        "message": error.to_string(),
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
    pub async fn dispatch_strict(&self, input: &str) -> Result<DispatchResult> {
        self.dispatch_inner(input).await
    }

    async fn dispatch_inner(&self, input: &str) -> Result<DispatchResult> {
        let req: RpcRequest = serde_json::from_str(input)?;
        if req.jsonrpc != "2.0" {
            bail!("unsupported JSON-RPC version: {}", req.jsonrpc);
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
            "tools/list" => self.tools_list_aggregated().await?,
            "tools/call" => self.call_tool(&req.params).await?,
            _ => {
                // Unknown method: a proper JSON-RPC "method not found" error.
                return Ok(DispatchResult::Response(RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({
                        "code": -32601,
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
            {"name":"connector.invoke","description":"Invoke a tool exposed by a provider","inputSchema":{"type":"object","properties":{"provider":{"type":"string"},"tool":{"type":"string"},"arguments":{"type":"object"}},"required":["provider","tool"]}}
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
                let registry = self.providers.read().await;
                serde_json::to_value(registry.invoke(provider, tool, args).await?)?
            }
            _ if name.contains('.') => {
                let registry = self.providers.read().await;
                serde_json::to_value(registry.invoke_qualified(name, arguments).await?)?
            }
            _ => json!({"error": "unknown tool"}),
        };

        Ok(json!({
            "content": [{"type": "text", "text": serde_json::to_string(&value)?}]
        }))
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
