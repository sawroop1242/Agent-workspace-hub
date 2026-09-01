use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Maximum number of connectors a single project may register.
const MAX_CONNECTORS: usize = 500;
/// Maximum length of connector id, name, or provider.
const MAX_CONNECTOR_FIELD_LEN: usize = 256;
/// Maximum number of OAuth scopes per connector.
const MAX_CONNECTOR_SCOPES: usize = 64;
/// Maximum length of a single OAuth scope.
const MAX_CONNECTOR_SCOPE_LEN: usize = 256;

/// Authentication method required by a connector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthMethod {
    /// OAuth 2.0 authorization.
    OAuth,
    /// Static API key.
    ApiKey,
    /// No authentication.
    None,
}

/// Metadata for an external service connector. Holds no secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    /// Connector id.
    pub id: String,
    /// Human-facing name.
    pub name: String,
    /// Backing provider/service name.
    pub provider: String,
    /// Authentication method used.
    pub auth: AuthMethod,
    /// OAuth scopes requested by the connector.
    pub scopes: Vec<String>,
    /// Whether the connector is enabled.
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
    /// Creates a connector store backed by `.agent/connectors.json` under the project root.
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

    /// Adds (or replaces) a connector after validating its required fields
    /// and enforcing the store's size limits.
    pub fn add(&self, connector: Connector) -> Result<Connector> {
        validate_connector(&connector)?;
        let mut store = self.load()?;
        let exists = store.connectors.iter().any(|c| c.id == connector.id);
        if !exists && store.connectors.len() >= MAX_CONNECTORS {
            bail!("connector store is full (max {MAX_CONNECTORS} connectors)");
        }
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

/// Rejects connector definitions that would violate the store's limits or
/// required-field rules.
fn validate_connector(connector: &Connector) -> Result<()> {
    if connector.id.trim().is_empty() {
        bail!("connector id is required");
    }
    if connector.name.trim().is_empty() {
        bail!("connector name is required");
    }
    if connector.provider.trim().is_empty() {
        bail!("connector provider is required");
    }
    for (field, value) in [("id", &connector.id), ("name", &connector.name)] {
        if value.len() > MAX_CONNECTOR_FIELD_LEN {
            bail!("connector {field} exceeds {MAX_CONNECTOR_FIELD_LEN} bytes");
        }
    }
    if connector.provider.len() > MAX_CONNECTOR_FIELD_LEN {
        bail!("connector provider exceeds {MAX_CONNECTOR_FIELD_LEN} bytes");
    }
    if connector.scopes.len() > MAX_CONNECTOR_SCOPES {
        bail!("connector exceeds {MAX_CONNECTOR_SCOPES} scopes");
    }
    if let Some(scope) = connector
        .scopes
        .iter()
        .find(|s| s.len() > MAX_CONNECTOR_SCOPE_LEN)
    {
        bail!("connector scope exceeds {MAX_CONNECTOR_SCOPE_LEN} bytes: {scope:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (ConnectorsMcp, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = ConnectorsMcp::new(dir.path()).unwrap();
        (store, dir)
    }

    fn connector(id: &str) -> Connector {
        Connector {
            id: id.into(),
            name: "name".into(),
            provider: "provider".into(),
            auth: AuthMethod::None,
            scopes: vec![],
            enabled: false,
        }
    }

    #[test]
    fn add_rejects_missing_required_fields() {
        let (store, _dir) = temp_store();
        let mut c = connector("id");
        c.name = String::new();
        assert!(store.add(c).is_err());

        let mut c = connector("id");
        c.provider = String::new();
        assert!(store.add(c).is_err());

        let mut c = connector("");
        c.id = "   ".into();
        assert!(store.add(c).is_err());
    }

    #[test]
    fn add_rejects_oversized_fields_and_scopes() {
        let (store, _dir) = temp_store();
        let mut c = connector("id");
        c.id = "i".repeat(MAX_CONNECTOR_FIELD_LEN + 1);
        assert!(store.add(c).is_err());

        let mut c = connector("id");
        c.provider = "p".repeat(MAX_CONNECTOR_FIELD_LEN + 1);
        assert!(store.add(c).is_err());

        let mut c = connector("id");
        c.scopes = (0..MAX_CONNECTOR_SCOPES + 1)
            .map(|i| i.to_string())
            .collect();
        assert!(store.add(c).is_err());

        let mut c = connector("id");
        c.scopes = vec!["s".repeat(MAX_CONNECTOR_SCOPE_LEN + 1)];
        assert!(store.add(c).is_err());
    }

    #[test]
    fn add_enforces_connector_count_limit() {
        let (store, _dir) = temp_store();
        let connectors: Vec<Connector> = (0..MAX_CONNECTORS as u64)
            .map(|i| connector(&format!("c-{i}")))
            .collect();
        fs::write(
            store.path.clone(),
            serde_json::to_string(&ConnectorStore { connectors }).unwrap(),
        )
        .unwrap();

        // Adding one more must fail.
        assert!(store.add(connector("overflow")).is_err());

        // Replacing an existing connector must still succeed.
        assert!(store.add(connector("c-0")).is_ok());
    }

    #[test]
    fn add_replace_remove_set_enabled_round_trip() {
        let (store, _dir) = temp_store();
        assert!(store.add(connector("c1")).is_ok());
        // Replace keeps a single entry.
        assert!(store.add(connector("c1")).is_ok());
        assert_eq!(store.list().unwrap().len(), 1);

        let enabled = store.set_enabled("c1", true).unwrap().unwrap();
        assert!(enabled.enabled);

        assert!(store.remove("c1").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn corrupted_store_fails_closed() {
        let (store, _dir) = temp_store();
        fs::write(&store.path, "not json {").unwrap();
        assert!(store.add(connector("c1")).is_err());
        assert!(store.list().is_err());
    }
}
