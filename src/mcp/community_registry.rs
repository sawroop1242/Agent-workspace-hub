use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::custom_mcp::{CustomMcpServerConfig, McpTransport};
use super::global_mcp::{GlobalMcpEntry, GlobalMcpRegistry};

/// A server entry in a community MCP registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityMcpManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
}

/// The index returned by a community MCP registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityRegistryIndex {
    pub mcps: Vec<CommunityMcpManifest>,
}

/// HTTP client for a community MCP registry's index.
pub struct CommunityMcpRegistryClient {
    client: Client,
    index_url: String,
}

impl CommunityMcpRegistryClient {
    /// Creates a client for the registry at `index_url`.
    pub fn new(index_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            index_url: index_url.into(),
        }
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
        McpTransport::Stdio if m.command.as_deref().unwrap_or("").is_empty() => {
            bail!("stdio MCP '{}' is missing command", m.id)
        }
        McpTransport::StreamableHttp if m.url.as_deref().unwrap_or("").is_empty() => {
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
        permissions: Default::default(),
        enabled: true,
    })
}

#[allow(dead_code)]
fn _validate_json(_value: &Value) -> bool {
    true
}
