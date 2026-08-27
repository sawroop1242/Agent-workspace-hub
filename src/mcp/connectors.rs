use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Authentication method required by a connector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthMethod {
    OAuth,
    ApiKey,
    None,
}

/// Metadata for an external service connector. Holds no secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub auth: AuthMethod,
    pub scopes: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConnectorStore {
    connectors: Vec<Connector>,
}

/// Stores connector metadata and references only. OAuth tokens/API secrets must live
/// in an OS credential store or external secret manager, never in this JSON file.
pub struct ConnectorsMcp {
    path: PathBuf,
}

impl ConnectorsMcp {
    pub fn new(project_root: impl Into<PathBuf>) -> Result<Self> {
        let root = project_root.into();
        fs::create_dir_all(root.join(".agent"))?;
        Ok(Self {
            path: root.join(".agent").join("connectors.json"),
        })
    }

    fn load(&self) -> Result<ConnectorStore> {
        if !self.path.exists() {
            return Ok(ConnectorStore::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    fn save(&self, store: &ConnectorStore) -> Result<()> {
        fs::write(&self.path, serde_json::to_string_pretty(store)?)?;
        Ok(())
    }

    /// Lists all connectors.
    pub fn list(&self) -> Result<Vec<Connector>> {
        Ok(self.load()?.connectors)
    }

    /// Adds (or replaces) a connector after validating its required fields.
    pub fn add(&self, connector: Connector) -> Result<Connector> {
        if connector.id.is_empty() || connector.name.is_empty() || connector.provider.is_empty() {
            bail!("connector id, name and provider are required");
        }
        let mut store = self.load()?;
        store.connectors.retain(|c| c.id != connector.id);
        store.connectors.push(connector.clone());
        self.save(&store)?;
        Ok(connector)
    }

    /// Removes a connector, returning whether it existed.
    pub fn remove(&self, id: &str) -> Result<bool> {
        let mut store = self.load()?;
        let before = store.connectors.len();
        store.connectors.retain(|c| c.id != id);
        self.save(&store)?;
        Ok(before != store.connectors.len())
    }

    /// Toggles a connector's enabled state, returning the updated connector.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<Option<Connector>> {
        let mut store = self.load()?;
        let connector = match store.connectors.iter_mut().find(|c| c.id == id) {
            Some(c) => c,
            None => return Ok(None),
        };
        connector.enabled = enabled;
        let result = connector.clone();
        self.save(&store)?;
        Ok(Some(result))
    }
}
