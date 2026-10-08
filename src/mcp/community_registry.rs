use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::custom_mcp::{CustomMcpServerConfig, McpTransport};
use super::global_mcp::{GlobalMcpEntry, GlobalMcpRegistry};

/// A server entry in a community MCP registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityMcpManifest {
    /// Unique server id.
    pub id: String,
    /// Human-facing server name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
    /// Optional version.
    #[serde(default)]
    pub version: String,
    /// Optional author.
    #[serde(default)]
    pub author: String,
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
    pub env: std::collections::HashMap<String, String>,
    /// Optional project homepage.
    #[serde(default)]
    pub homepage: Option<String>,
    /// Optional source repository URL.
    #[serde(default)]
    pub repository: Option<String>,
}

/// The index returned by a community MCP registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityRegistryIndex {
    /// Servers advertised by the registry.
    pub mcps: Vec<CommunityMcpManifest>,
}

/// HTTP client for a community MCP registry's index.
pub struct CommunityMcpRegistryClient {
    client: Client,
    index_url: String,
}

impl CommunityMcpRegistryClient {
    /// Creates a client for the registry at `index_url`.
    ///
    /// Fails only if the shared HTTP client cannot be constructed —
    /// fail-closed, not panic (§20).
    pub fn new(index_url: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client: super::config::build_http_client()?,
            index_url: index_url.into(),
        })
    }

    /// Fetches the registry index.
    pub async fn index(&self) -> Result<CommunityRegistryIndex> {
        let response = self
            .client
            .get(&self.index_url)
            .send()
            .await?
            .error_for_status()?;
        response
            .json()
            .await
            .context("invalid community MCP registry index")
    }

    /// Searches the registry by id, name, description, or author (case-insensitive).
    pub async fn search(&self, query: &str) -> Result<Vec<CommunityMcpManifest>> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Ok(self.index().await?.mcps);
        }
        Ok(self
            .index()
            .await?
            .mcps
            .into_iter()
            .filter(|m| {
                [
                    m.id.as_str(),
                    m.name.as_str(),
                    m.description.as_str(),
                    m.author.as_str(),
                ]
                .iter()
                .any(|v| v.to_lowercase().contains(&query))
            })
            .collect())
    }

    /// Returns the manifest for a specific MCP `id`.
    pub async fn get(&self, id: &str) -> Result<CommunityMcpManifest> {
        self.index()
            .await?
            .mcps
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| anyhow!("MCP '{}' not found in community registry", id))
    }

    /// Installs an MCP by id into the global registry.
    pub async fn install(&self, global: &GlobalMcpRegistry, id: &str) -> Result<GlobalMcpEntry> {
        let manifest = self.get(id).await?;
        let config = manifest_to_config(&manifest)?;
        global.install(
            config,
            manifest.version,
            format!("community:{}", self.index_url),
        )
    }

    /// Updates an already-installed MCP to the latest registry version.
    pub async fn update(&self, global: &GlobalMcpRegistry, id: &str) -> Result<GlobalMcpEntry> {
        let manifest = self.get(id).await?;
        let current = global
            .get(id)?
            .ok_or_else(|| anyhow!("MCP '{}' is not installed globally", id))?;
        if !current.version.is_empty() && current.version == manifest.version {
            return Ok(current);
        }
        self.install(global, id).await
    }
}

fn manifest_to_config(m: &CommunityMcpManifest) -> Result<CustomMcpServerConfig> {
    if m.id.trim().is_empty() || m.name.trim().is_empty() {
        bail!("registry MCP must have id and name");
    }
    match m.transport {
        // Trim consistently with the id/name checks above: a whitespace-only
        // command or URL is just as unusable as a missing one.
        McpTransport::Stdio if m.command.as_deref().unwrap_or("").trim().is_empty() => {
            bail!("stdio MCP '{}' is missing command", m.id)
        }
        McpTransport::StreamableHttp if m.url.as_deref().unwrap_or("").trim().is_empty() => {
            bail!("HTTP MCP '{}' is missing url", m.id)
        }
        _ => {}
    }
    Ok(CustomMcpServerConfig {
        id: m.id.clone(),
        name: m.name.clone(),
        transport: m.transport.clone(),
        command: m.command.clone(),
        args: m.args.clone(),
        url: m.url.clone(),
        env: m.env.clone(),
        headers: Default::default(),
        permissions: Default::default(),
        enabled: true,
    })
}

#[allow(dead_code)]
fn _validate_json(_value: &Value) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, transport: McpTransport) -> CommunityMcpManifest {
        CommunityMcpManifest {
            id: id.into(),
            name: format!("{id} server"),
            description: String::new(),
            version: "1.0".into(),
            author: String::new(),
            transport,
            command: None,
            args: Vec::new(),
            url: None,
            env: Default::default(),
            homepage: None,
            repository: None,
        }
    }

    #[test]
    fn manifest_to_config_rejects_missing_id_or_name() {
        let mut m = manifest("mcp-a", McpTransport::Stdio);
        m.command = Some("run".into());
        {
            let mut bad = m.clone();
            bad.id = "   ".into();
            assert!(manifest_to_config(&bad).is_err());
        }
        {
            let mut bad = m.clone();
            bad.name = String::new();
            assert!(manifest_to_config(&bad).is_err());
        }
    }

    #[test]
    fn manifest_to_config_rejects_stdio_without_command() {
        let m = manifest("mcp-a", McpTransport::Stdio);
        let error = manifest_to_config(&m).unwrap_err();
        assert!(error.to_string().contains("missing command"));
        // whitespace-only command is treated as missing
        let mut m = m;
        m.command = Some("   ".into());
        assert!(manifest_to_config(&m).is_err());
    }

    #[test]
    fn manifest_to_config_rejects_http_without_url() {
        let m = manifest("mcp-a", McpTransport::StreamableHttp);
        let error = manifest_to_config(&m).unwrap_err();
        assert!(error.to_string().contains("missing url"));
    }

    #[test]
    fn manifest_to_config_maps_stdio_manifest_to_enabled_config() {
        let mut m = manifest("mcp-a", McpTransport::Stdio);
        m.command = Some("node".into());
        m.args = vec!["server.js".into()];
        m.env.insert("API_KEY".into(), "value".into());

        let config = manifest_to_config(&m).unwrap();
        assert_eq!(config.id, "mcp-a");
        assert_eq!(config.name, "mcp-a server");
        assert!(matches!(config.transport, McpTransport::Stdio));
        assert_eq!(config.command.as_deref(), Some("node"));
        assert_eq!(config.args, vec!["server.js".to_string()]);
        assert_eq!(config.env.get("API_KEY").map(String::as_str), Some("value"));
        // community installs start enabled with no extra permissions
        assert!(config.enabled);
        assert_eq!(config.headers.len(), 0);
        assert!(!config.permissions.network);
        assert!(!config.permissions.process);
    }

    #[test]
    fn manifest_to_config_maps_http_manifest_to_enabled_config() {
        let mut m = manifest("mcp-b", McpTransport::StreamableHttp);
        m.url = Some("https://example.com/mcp".into());

        let config = manifest_to_config(&m).unwrap();
        assert!(matches!(config.transport, McpTransport::StreamableHttp));
        assert_eq!(config.url.as_deref(), Some("https://example.com/mcp"));
        assert!(config.enabled);
    }

    #[test]
    fn index_and_manifests_deserialize_from_json() {
        let json = r#"{
            "mcps": [
                {
                    "id": "mcp-a",
                    "name": "A",
                    "transport": "stdio",
                    "command": "node",
                    "args": [],
                    "url": null,
                    "env": {}
                }
            ]
        }"#;
        let index: CommunityRegistryIndex = serde_json::from_str(json).unwrap();
        assert_eq!(index.mcps.len(), 1);
        assert_eq!(index.mcps[0].id, "mcp-a");
        // optional fields default rather than fail
        assert_eq!(index.mcps[0].version, "");
        assert_eq!(index.mcps[0].author, "");
        assert_eq!(index.mcps[0].description, "");
        assert!(index.mcps[0].homepage.is_none());
    }
}
