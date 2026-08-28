use crate::mcp::{
    AuthMethod, ComposioProvider, Connector, ConnectorsMcp, CustomMcpProvider, CustomMcpRegistry,
    McpTransport, MemoryMcp, MemoryScope, ProviderRegistry, SkillMcp, StdioMcpClient,
    StreamableHttpMcpClient, TaskPriority, TaskStatus, TasksMcp, WorkspaceMcp,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::runtime::Runtime;

#[derive(Debug, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Option<Value>,
    result: Option<Value>,
    error: Option<Value>,
}

/// The stdio JSON-RPC MCP server exposing project skills, workspace, memory,
/// tasks, and connectors.
pub struct StdioMcpServer {
    skills: SkillMcp,
    workspace: WorkspaceMcp,
    memory: MemoryMcp,
    tasks: TasksMcp,
    connectors: ConnectorsMcp,
    providers: Arc<RwLock<ProviderRegistry>>,
    runtime: Runtime,
}

impl StdioMcpServer {
    /// Builds the server for a project, wiring the Composio and custom MCP providers.
    pub fn new(project_root: PathBuf) -> Result<Self> {
        let runtime = Runtime::new()?;
        let registry = Arc::new(RwLock::new(ProviderRegistry::default()));

        if std::env::var("COMPOSIO_API_KEY").is_ok() {
            if let Ok(provider) = ComposioProvider::from_env() {
                registry
                    .write()
                    .map_err(|_| anyhow::anyhow!("provider registry lock poisoned"))?
                    .register(Box::new(provider));
            }
        }

        let custom = CustomMcpRegistry::new(project_root.clone())?;
        for cfg in custom.list()? {
            if !cfg.enabled {
                continue;
            }
            match cfg.transport {
                McpTransport::Stdio => {
                    let client =
                        runtime.block_on(StdioMcpClient::spawn(&cfg, project_root.clone()))?;
                    runtime.block_on(client.initialize())?;
                    let provider = CustomMcpProvider::new(cfg.id, Arc::new(client));
                    registry
                        .write()
                        .map_err(|_| anyhow::anyhow!("provider registry lock poisoned"))?
                        .register(Box::new(provider));
                }
                McpTransport::StreamableHttp => {
                    let client = StreamableHttpMcpClient::new(&cfg)?;
                    runtime.block_on(client.initialize())?;
                    let provider = CustomMcpProvider::new(cfg.id, Arc::new(client));
                    registry
                        .write()
                        .map_err(|_| anyhow::anyhow!("provider registry lock poisoned"))?
                        .register(Box::new(provider));
                }
            }
        }

        Ok(Self {
            skills: SkillMcp::new(project_root.clone())?,
            workspace: WorkspaceMcp::new(project_root.clone())?,
            memory: MemoryMcp::new(project_root.clone())?,
            tasks: TasksMcp::new(project_root.clone())?,
            connectors: ConnectorsMcp::new(project_root)?,
            providers: registry,
            runtime,
        })
    }

    /// Returns the shared provider registry used to dispatch tool calls.
    pub fn provider_registry(&self) -> Arc<RwLock<ProviderRegistry>> {
        Arc::clone(&self.providers)
    }

    /// Handles a single JSON-RPC request line, returning the JSON response.
    pub fn handle(&self, input: &str) -> Result<String> {
        let req: RpcRequest = serde_json::from_str(input)?;
        if req.jsonrpc != "2.0" {
            anyhow::bail!("unsupported JSON-RPC version: {}", req.jsonrpc);
        }
        let result = match req.method.as_str() {
            "initialize" => json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "agent-workspace-hub", "version": env!("CARGO_PKG_VERSION")}
            }),
            "tools/list" => self.tools_list_aggregated()?,
            "tools/call" => self.call_tool(&req.params)?,
            _ => json!({"error": "method not found"}),
        };

        Ok(serde_json::to_string(&RpcResponse {
            jsonrpc: "2.0",
            id: req.id,
            result: Some(result),
            error: None,
        })?)
    }

    fn tools_list_aggregated(&self) -> Result<Value> {
        let mut base = self
            .tools_list()
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let registry = self
            .providers
            .read()
            .map_err(|_| anyhow::anyhow!("provider registry lock poisoned"))?;
        let dynamic = self.runtime.block_on(registry.aggregate_tools())?;
        for tool in dynamic {
            base.push(serde_json::to_value(tool)?);
        }
        Ok(json!({"tools": base}))
    }

    fn tools_list(&self) -> Value {
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

    fn call_tool(&self, params: &Value) -> Result<Value> {
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
                let registry = self
                    .providers
                    .read()
                    .map_err(|_| anyhow::anyhow!("provider registry lock poisoned"))?;
                json!(registry.providers())
            }
            "connector.tools" => {
                let provider = arguments
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let registry = self
                    .providers
                    .read()
                    .map_err(|_| anyhow::anyhow!("provider registry lock poisoned"))?;
                serde_json::to_value(self.runtime.block_on(registry.tools(provider))?)?
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
                let registry = self
                    .providers
                    .read()
                    .map_err(|_| anyhow::anyhow!("provider registry lock poisoned"))?;
                serde_json::to_value(
                    self.runtime
                        .block_on(registry.invoke(provider, tool, args))?,
                )?
            }
            _ if name.contains('.') => {
                let registry = self
                    .providers
                    .read()
                    .map_err(|_| anyhow::anyhow!("provider registry lock poisoned"))?;
                serde_json::to_value(
                    self.runtime
                        .block_on(registry.invoke_qualified(name, arguments))?,
                )?
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
