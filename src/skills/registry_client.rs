use crate::skills::{RegistryManifest, RegistrySkill};
use anyhow::{bail, Context, Result};
use serde_json::Value;

pub struct RegistryClient {
    pub base_url: String,
}

impl RegistryClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
        }
    }

    /// Fetch a registry manifest. The HTTP transport is intentionally kept behind
    /// this boundary so the CLI/MCP layer does not depend on registry internals.
    pub async fn fetch_manifest(&self) -> Result<RegistryManifest> {
        let url = format!("{}/registry.json", self.base_url);
        let response = reqwest::get(&url)
            .await
            .context("failed to contact skill registry")?;
        if !response.status().is_success() {
            bail!("skill registry returned HTTP {}", response.status());
        }
        response
            .json::<RegistryManifest>()
            .await
            .context("invalid registry.json")
    }

    pub async fn search(&self, query: &str) -> Result<Vec<RegistrySkill>> {
        let manifest = self.fetch_manifest().await?;
        let query = query.to_ascii_lowercase();
        Ok(manifest
            .skills
            .into_iter()
            .filter(|s| {
                s.name.to_ascii_lowercase().contains(&query)
                    || s.description.to_ascii_lowercase().contains(&query)
            })
            .collect())
    }
}

#[allow(dead_code)]
fn _json_shape_check(_: Value) {}
