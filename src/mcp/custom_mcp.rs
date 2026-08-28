use anyhow::{bail, Context, Result};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;

use super::audit::audit_secret_deny;
use super::permissions::{is_blocked_environment, is_valid_env_name, McpPermissions};
#[cfg(windows)]
use super::sandbox::apply_windows_job;
use super::sandbox::{wrap_command, SandboxConfig};
use super::schema::validate_tool_arguments;

const MAX_MCP_LINE_BYTES: usize = 10 * 1024 * 1024;
const MAX_HTTP_BODY_BYTES: usize = 10 * 1024 * 1024;
const MCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_MCP_ID_LEN: usize = 128;
const MAX_MCP_NAME_LEN: usize = 256;

/// Transport mechanism used to communicate with an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    /// Launch and talk to the server over stdin/stdout.
    Stdio,
    /// Talk to the server over HTTP (streamable transport).
    StreamableHttp,
}

/// Configuration for a custom MCP server, including permissions and enablement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMcpServerConfig {
    /// Unique server id.
    pub id: String,
    /// Human-facing server name.
    pub name: String,
    /// Transport used to launch the server.
    pub transport: McpTransport,
    /// Launch command (for stdio transport).
    pub command: Option<String>,
    /// Command-line arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Server URL (for HTTP transport).
    pub url: Option<String>,
    /// Environment variables passed to the server.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Permissions granted to the server.
    #[serde(default)]
    pub permissions: McpPermissions,
    /// Whether the server is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// In-memory collection of custom MCP server configs.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CustomMcpStore {
    /// Configured servers.
    pub servers: Vec<CustomMcpServerConfig>,
}

fn filter_env(cfg: &CustomMcpServerConfig) -> impl Iterator<Item = (&String, &String)> {
    cfg.env.iter().filter(move |(key, _)| {
        cfg.permissions
            .environment
            .iter()
            .any(|allowed| allowed == *key)
            && is_valid_env_name(key)
            && !is_blocked_environment(key)
    })
}

fn expand_secret_ref(value: &str, permissions: &McpPermissions) -> Result<String> {
    if let Some(key) = value
        .strip_prefix("${secret:")
        .and_then(|v| v.strip_suffix('}'))
    {
        if !is_valid_env_name(key) {
            audit_secret_deny("invalid_name", "");
            bail!("invalid secret environment variable name");
        }
        if is_blocked_environment(key) {
            audit_secret_deny("blocked_name", key);
            bail!("dangerous secret environment variable is blocked: {key}");
        }
        if !permissions.allows_secret(key) {
            audit_secret_deny("not_approved", key);
            bail!("secret environment variable is not approved: {key}");
        }
        return std::env::var(key)
            .with_context(|| format!("approved secret environment variable is unset: {key}"));
    }
    Ok(value.to_string())
}

fn validate_server(server: &CustomMcpServerConfig) -> Result<()> {
    if server.id.trim().is_empty() || server.name.trim().is_empty() {
        bail!("MCP id and name are required");
    }
    if server.id.len() > MAX_MCP_ID_LEN {
        bail!("MCP id exceeds {MAX_MCP_ID_LEN} characters");
    }
    if server.name.len() > MAX_MCP_NAME_LEN {
        bail!("MCP name exceeds {MAX_MCP_NAME_LEN} characters");
    }
    if server.id != server.id.trim() || server.name != server.name.trim() {
        bail!("MCP id and name must not have leading or trailing whitespace");
    }
    if server
        .id
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    {
        bail!("MCP id may contain only ASCII letters, digits, '-', '_' and '.'");
    }
    server.permissions.validate()?;
    match server.transport {
        McpTransport::Stdio => {
            if server.command.as_deref().unwrap_or("").trim().is_empty() {
                bail!("stdio MCP requires command");
            }
        }
        McpTransport::StreamableHttp => {
            let url = server.url.as_deref().unwrap_or("").trim();
            if url.is_empty() {
                bail!("HTTP MCP requires url");
            }
            let parsed = reqwest::Url::parse(url).context("invalid MCP HTTP URL")?;
            if parsed.scheme() != "https" && parsed.scheme() != "http" {
                bail!("MCP HTTP URL must use http or https");
            }
            if parsed.username() != "" || parsed.password().is_some() {
                bail!("MCP HTTP URL must not contain embedded credentials");
            }
        }
    }
    for key in server.env.keys() {
        if !is_valid_env_name(key) {
            bail!("invalid MCP environment variable name: {key}");
        }
        if is_blocked_environment(key) {
            bail!("dangerous MCP environment variable is blocked: {key}");
        }
    }
    Ok(())
}

/// Persistent per-project store for custom MCP server configs.
pub struct CustomMcpRegistry {
    path: PathBuf,
}

impl CustomMcpRegistry {
    /// Creates a registry backed by `.agent/mcps.json` under the project root.
    pub fn new(project_root: impl Into<PathBuf>) -> Result<Self> {
        let root = project_root.into();
        fs::create_dir_all(root.join(".agent"))?;
        Ok(Self {
            path: root.join(".agent").join("mcps.json"),
        })
    }

    fn load(&self) -> Result<CustomMcpStore> {
        if !self.path.exists() {
            return Ok(CustomMcpStore::default());
        }
        let content =
            fs::read_to_string(&self.path).context("failed to read custom MCP registry")?;
        serde_json::from_str(&content).context("custom MCP registry contains invalid JSON")
    }

    fn save(&self, store: &CustomMcpStore) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("custom MCP registry has no parent directory")?;
        fs::create_dir_all(parent)?;
        let content = serde_json::to_vec_pretty(store)?;
        let mut temp =
            NamedTempFile::new_in(parent).context("failed to create MCP registry temp file")?;
        std::io::Write::write_all(&mut temp, &content)?;
        temp.as_file()
            .sync_all()
            .context("failed to flush MCP registry")?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("failed to atomically replace custom MCP registry")?;
        Ok(())
    }

    /// Lists all configured MCP servers.
    pub fn list(&self) -> Result<Vec<CustomMcpServerConfig>> {
        Ok(self.load()?.servers)
    }

    /// Adds (or replaces) a server after validating its configuration.
    pub fn add(&self, server: CustomMcpServerConfig) -> Result<CustomMcpServerConfig> {
        validate_server(&server)?;
        let mut store = self.load()?;
        store.servers.retain(|entry| entry.id != server.id);
        store.servers.push(server.clone());
        self.save(&store)?;
        Ok(server)
    }

    /// Removes a server, returning whether it existed.
    pub fn remove(&self, id: &str) -> Result<bool> {
        let mut store = self.load()?;
        let before = store.servers.len();
        store.servers.retain(|entry| entry.id != id);
        if before != store.servers.len() {
            self.save(&store)?;
        }
        Ok(before != store.servers.len())
    }

    /// Toggles a server's enabled state, returning the updated config.
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

    /// Returns the config for a specific server id, if present.
    pub fn get(&self, id: &str) -> Result<Option<CustomMcpServerConfig>> {
        Ok(self
            .load()?
            .servers
            .into_iter()
            .find(|entry| entry.id == id))
    }
}

/// A JSON-RPC client for a stdio MCP server process.
pub struct StdioMcpClient {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    next_id: Mutex<u64>,
    #[cfg(windows)]
    _job: super::sandbox::WindowsJob,
}

impl StdioMcpClient {
    /// Spawns the configured command (optionally sandboxed) and wires stdio.
    pub async fn spawn(
        cfg: &CustomMcpServerConfig,
        workspace_root: impl Into<PathBuf>,
    ) -> Result<Self> {
        validate_server(cfg)?;
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

        let msg = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params
        });
        {
            let mut input = self.stdin.lock().await;
            let encoded = serde_json::to_vec(&msg)?;
            if encoded.len() > MAX_MCP_LINE_BYTES {
                bail!("MCP request exceeds 10 MiB limit");
            }
            input.write_all(&encoded).await?;
            input.write_all(b"\n").await?;
            input.flush().await?;
        }

        let mut line = String::new();
        timeout(MCP_REQUEST_TIMEOUT, async {
            self.stdout.lock().await.read_line(&mut line).await
        })
        .await
        .context("MCP request timed out")??;
        if line.len() > MAX_MCP_LINE_BYTES {
            bail!("MCP response exceeds 10 MiB limit");
        }
        jsonrpc_result(serde_json::from_str(line.trim()).context("invalid MCP JSON-RPC response")?)
    }

    /// Sends the MCP `initialize` handshake over stdio.
    pub async fn initialize(&self) -> Result<Value> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {
                    "name": "agent-workspace-hub",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
        .await
    }

    /// Lists tools exposed by the stdio server.
    pub async fn tools_list(&self) -> Result<Value> {
        self.request("tools/list", json!({})).await
    }

    /// Calls a tool on the stdio server, validating arguments first.
    pub async fn tools_call(&self, name: &str, arguments: Value) -> Result<Value> {
        let tools = self.tools_list().await?;
        validate_tool_arguments(&tools, name, &arguments)?;
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
            .await
    }

    /// Terminates the underlying MCP process so it cannot outlive the client.
    pub async fn shutdown(&self) -> Result<()> {
        self.child.lock().await.kill().await?;
        Ok(())
    }
}

/// A JSON-RPC client for a streamable HTTP MCP server.
pub struct StreamableHttpMcpClient {
    client: Client,
    url: String,
    next_id: Mutex<u64>,
    session_id: Mutex<Option<String>>,
    headers: HashMap<String, String>,
}

impl StreamableHttpMcpClient {
    /// Creates a client from a server config, deriving headers from permitted env vars.
    pub fn new(cfg: &CustomMcpServerConfig) -> Result<Self> {
        validate_server(cfg)?;
        let url = cfg.url.clone().context("missing streamable HTTP MCP url")?;
        let mut headers = HashMap::new();
        for (key, value) in filter_env(cfg) {
            headers.insert(key.clone(), expand_secret_ref(value, &cfg.permissions)?);
        }
        Ok(Self {
            client: Client::builder().timeout(MCP_REQUEST_TIMEOUT).build()?,
            url,
            next_id: Mutex::new(1),
            session_id: Mutex::new(None),
            headers,
        })
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let mut id = self.next_id.lock().await;
        let request_id = *id;
        *id += 1;
        drop(id);

        let body = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params
        });
        let mut request = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&body);
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }
        if let Some(session) = self.session_id.lock().await.clone() {
            request = request.header("Mcp-Session-Id", session);
        }
        let response = request.send().await?.error_for_status()?;
        if let Some(session) = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
        {
            *self.session_id.lock().await = Some(session.to_string());
        }
        parse_http_response(response).await
    }

    /// Sends the MCP `initialize` handshake over HTTP.
    pub async fn initialize(&self) -> Result<Value> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {
                    "name": "agent-workspace-hub",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
        .await
    }

    /// Lists tools exposed by the HTTP server.
    pub async fn tools_list(&self) -> Result<Value> {
        self.request("tools/list", json!({})).await
    }

    /// Calls a tool on the HTTP server, validating arguments first.
    pub async fn tools_call(&self, name: &str, arguments: Value) -> Result<Value> {
        let tools = self.tools_list().await?;
        validate_tool_arguments(&tools, name, &arguments)?;
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
            .await
    }
}

async fn parse_http_response(response: Response) -> Result<Value> {
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = response.bytes().await?;
    if bytes.len() > MAX_HTTP_BODY_BYTES {
        bail!("MCP HTTP response exceeds 10 MiB limit");
    }
    if content_type.contains("application/json") {
        return jsonrpc_result(serde_json::from_slice(&bytes)?);
    }
    let text = std::str::from_utf8(&bytes).context("MCP HTTP response is not UTF-8")?;
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            if let Ok(value) = serde_json::from_str::<Value>(data.trim()) {
                return jsonrpc_result(value);
            }
        }
    }
    bail!("MCP HTTP response did not contain a JSON-RPC message")
}

fn jsonrpc_result(response: Value) -> Result<Value> {
    if let Some(error) = response.get("error") {
        bail!("MCP error: {error}");
    }
    Ok(response.get("result").cloned().unwrap_or(response))
}
