use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Configured skill registry URLs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryConfig {
    pub registries: Vec<String>,
}

/// Persistent store for the list of configured skill registries.
pub struct RegistryStore {
    path: PathBuf,
}

impl RegistryStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            path: root.into().join("registries.json"),
        }
    }

    /// Loads the registry configuration, defaulting to an empty list.
    pub fn load(&self) -> Result<RegistryConfig> {
        if !self.path.exists() {
            return Ok(RegistryConfig::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    /// Adds a registry URL, returning whether it was newly added.
    pub fn add(&self, url: &str) -> Result<bool> {
        let mut config = self.load()?;
        if config.registries.iter().any(|u| u == url) {
            return Ok(false);
        }
        config.registries.push(url.trim_end_matches('/').to_owned());
        config.registries.sort();
        self.save(&config)?;
        Ok(true)
    }

    /// Removes a registry URL, returning whether it was present.
    pub fn remove(&self, url: &str) -> Result<bool> {
        let mut config = self.load()?;
        let old = config.registries.len();
        config.registries.retain(|u| u != url);
        if old == config.registries.len() {
            return Ok(false);
        }
        self.save(&config)?;
        Ok(true)
    }

    /// Persists the registry configuration.
    pub fn save(&self, config: &RegistryConfig) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(config)?)?;
        Ok(())
    }
}
