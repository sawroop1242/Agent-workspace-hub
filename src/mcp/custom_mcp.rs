use anyhow::{bail, Context, Result};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use super::permissions::{is_blocked_environment, is_valid_env_name, McpPermissions};
use super::sandbox::{wrap_command, SandboxConfig};
#[cfg(windows)]
use super::sandbox::apply_windows_job;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    StreamableHttp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMcpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub permissions: McpPermissions,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CustomMcpStore {
    pub servers: Vec<CustomMcpServerConfig>,
}

fn filter_env(cfg: &CustomMcpServerConfig) -> impl Iterator<Item = (&String, &String)> {
    cfg.env.iter().filter(move |(key, _)| {
        cfg.permissions.environment.iter().any(|allowed| allowed == *key)
            && is_valid_env_name(key)
            && !is_blocked_environment(key)
    })
}

fn expand_secret_ref(value: &str, permissions: &McpPermissions) -> Result<String> {
    if let Some(key) = value
        .strip_prefix("${secret:")
        .and_then(|value| value.strip_suffix('}'))
    {
        if !is_valid_env_name(key) {
            tracing::warn!(event = "mcp_secret_denied", reason = "invalid_name");
            bail!("invalid secret environment variable name");
        }
        if is_blocked_environment(key) {
            tracing::warn!(event = "mcp_secret_denied", reason = "blocked_name", name = key);
            bail!("dangerous secret environment variable is blocked: {key}");
        }
        if !permissions.allows_secret(key) {
            tracing::warn!(event = "mcp_secret_denied", reason = "not_approved", name = key);
            bail!("secret environment variable is not approved: {key}");
        }
        return std::env::var(key)
            .with_context(|| format!("approved secret environment variable is unset: {key}"));
    }
    Ok(value.to_string())
}

pub struct CustomMcpRegistry {
    path: PathBuf,
}

impl CustomMcpRegistry {
    pub fn new(project_root: impl Into<PathBuf>) -> Result<Self> {
        let root = project_root.into();
        fs::create_dir_all(root.join(".agent"))?;
        Ok(Self { path: root.join(".agent").join("mcps.json") })
    }

    fn load(&self) -> Result<CustomMcpStore> {
        if !self.path.exists() {
            return Ok(CustomMcpStore::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    fn save(&self, store: &CustomMcpStore) -> Result<()> {
        fs::write(&self.path, serde_json::to_string_pretty(store)?)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<CustomMcpServerConfig>> { Ok(self.load()?.servers) }

    pub fn add(&self, server: CustomMcpServerConfig) -> Result<CustomMcpServerConfig> {
        if server.id.is_empty() || server.name.is_empty() {
            bail!("id and name are required");
        }
        server.permissions.validate()?;
        match server.transport {
            McpTransport::Stdio if server.command.as_deref().unwrap_or("").is_empty() => {
                bail!("stdio MCP requires command")
            }
            McpTransport::StreamableHttp if server.url.as_deref().unwrap_or("").is_empty() => {
                bail!("HTTP MCP requires url")
            }
            _ => {}
        }
        let mut store = self.load()?;
        store.servers.retain(|entry| entry.id != server.id);
        store.servers.push(server.clone());
        self.save(&store)?;
        Ok(server)
    }

    pub fn remove(&self, id: &str) -> Result<bool> {
        let mut store = self.load()?;
        let before = store.servers.len();
        store.servers.retain(|entry| entry.id != id);
        self.save(&store)?;
        Ok(before != store.servers.len())
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<Option<CustomMcpServerConfig>> {
        let mut store = self.load()?;
        let entry = match store.servers.iter_mut().find(|entry| entry.id == id) {
            Some(entry) => entry,
            None => return Ok(None),
        };
        entry.enabled = enabled;
        let result = entry.clone();
        self.save(&store)?;
        Ok(Some(result))
    }

    pub fn get(&self, id: &str) -> Result<Option<CustomMcpServerConfig>> {
        Ok(self.load()?.servers.into_iter().find(|entry| entry.id == id))
    }
}

pub struct StdioMcpClient {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    next_id: Mutex<u64>,
    #[cfg(windows)]
    _job: super::sandbox::WindowsJob,
}

impl StdioMcpClient {
    pub async fn spawn(cfg: &CustomMcpServerConfig, workspace_root: impl Into<PathBuf>) -> Result<Self> {
        let command = cfg.command.as_ref().context("missing stdio command")?;
        let sandbox = SandboxConfig::new(workspace_root, cfg.permissions.clone())?;
        let (program, args) = wrap_command(&sandbox, command, &cfg.args)?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());
        for (key, value) in filter_env(cfg) {
            cmd.env(key, expand_secret_ref(value, &cfg.permissions)?);
        }

        let mut child = cmd.spawn().context("failed to start MCP server")?;
        #[cfg(windows)]
        let job = match apply_windows_job(&child, &sandbox.limits) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill().await;
                return Err(error).context("failed to sandbox MCP process with Windows Job Object");
            }
        };

        let stdin = child.stdin.take().context("MCP stdin unavailable")?;
        let stdout = child.stdout.take().context("MCP stdout unavailable")?;
        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            next_id: Mutex::new(1),
            #[cfg(windows)]
            _job: job,
        })
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let mut id = self.next_id.lock().await;
        let request_id = *id;
        *id += 1;
        drop(id);
        let msg = json!({"jsonrpc":"2.0","id":request_id,"method":method,"params":params});
        {
            let mut input = self.stdin.lock().await;
            input.write_all((serde_json::to_string(&msg)? + "\n").as_bytes()).await?;
            input.flush().await?;
        }
        let mut line = String::new();
        self.stdout.lock().await.read_line(&mut line).await?;
        jsonrpc_result(serde_json::from_str(line.trim()).context("invalid MCP JSON-RPC response")?)
    }

    pub async fn initialize(&self) -> Result<Value> {
        self.request("initialize", json!({
            "protocolVersion":"2025-06-18",
            "capabilities":{},
            "clientInfo":{"name":"agent-workspace-hub","version":env!("CARGO_PKG_VERSION")}
        })).await
    }
    pub async fn tools_list(&self) -> Result<Value> { self.request("tools/list", json!({})).await }
    pub async fn tools_call(&self, name: &str, arguments: Value) -> Result<Value> {
        self.request("tools/call", json!({"name":name,"arguments":arguments})).await
    }
}

pub struct StreamableHttpMcpClient {
    client: Client,
    url: String,
    next_id: Mutex<u64>,
    session_id: Mutex<Option<String>>,
    headers: HashMap<String, String>,
}

impl StreamableHttpMcpClient {
    pub fn new(cfg: &CustomMcpServerConfig) -> Result<Self> {
        let url = cfg.url.clone().context("missing streamable HTTP MCP url")?;
        let mut headers = HashMap::new();
        for (key, value) in filter_env(cfg) {
            headers.insert(key.clone(), expand_secret_ref(value, &cfg.permissions)?);
        }
        Ok(Self { client: Client::new(), url, next_id: Mutex::new(1), session_id: Mutex::new(None), headers })
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let mut id = self.next_id.lock().await;
        let request_id = *id;
        *id += 1;
        drop(id);
        let body = json!({"jsonrpc":"2.0","id":request_id,"method":method,"params":params});
        let mut request = self.client.post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&body);
        for (key, value) in &self.headers { request = request.header(key, value); }
        if let Some(session) = self.session_id.lock().await.clone() { request = request.header("Mcp-Session-Id", session); }
        let response = request.send().await?.error_for_status()?;
        if let Some(session) = response.headers().get("Mcp-Session-Id").and_then(|value| value.to_str().ok()) {
            *self.session_id.lock().await = Some(session.to_string());
        }
        parse_http_response(response).await
    }

    pub async fn initialize(&self) -> Result<Value> {
        self.request("initialize", json!({
            "protocolVersion":"2025-06-18",
            "capabilities":{},
            "clientInfo":{"name":"agent-workspace-hub","version":env!("CARGO_PKG_VERSION")}
        })).await
    }
    pub async fn tools_list(&self) -> Result<Value> { self.request("tools/list", json!({})).await }
    pub async fn tools_call(&self, name: &str, arguments: Value) -> Result<Value> {
        self.request("tools/call", json!({"name":name,"arguments":arguments})).await
    }
}

async fn parse_http_response(response: Response) -> Result<Value> {
    let content_type = response.headers().get("content-type").and_then(|value| value.to_str().ok()).unwrap_or("").to_ascii_lowercase();
    if content_type.contains("application/json") { return jsonrpc_result(response.json().await?); }
    let text = response.text().await?;
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            if let Ok(value) = serde_json::from_str::<Value>(data.trim()) { return jsonrpc_result(value); }
        }
    }
    bail!("MCP HTTP response did not contain a JSON-RPC message")
}

fn jsonrpc_result(response: Value) -> Result<Value> {
    if let Some(error) = response.get("error") { bail!("MCP error: {error}"); }
    Ok(response.get("result").cloned().unwrap_or(response))
}
