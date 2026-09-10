use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

use super::custom_mcp::{CustomMcpServerConfig, McpTransport};
use super::store_lock::StoreLock;

/// A globally installed MCP server entry with its version and source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalMcpEntry {
    /// The server configuration.
    pub config: CustomMcpServerConfig,
    /// Installed version.
    #[serde(default)]
    pub version: String,
    /// Originating source (registry or repository).
    #[serde(default)]
    pub source: String,
}

/// In-memory collection of globally installed MCP servers.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GlobalMcpStore {
    /// Installed servers.
    pub servers: Vec<GlobalMcpEntry>,
}

/// Persistent store for globally installed MCP servers under the user data directory.
pub struct GlobalMcpRegistry {
    path: PathBuf,
}
impl GlobalMcpRegistry {
    /// Creates a registry rooted at the platform user data directory.
    pub fn new() -> Result<Self> {
        let root = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("agent-workspace-hub");
        fs::create_dir_all(&root)?;
        Ok(Self {
            path: root.join("mcps.json"),
        })
    }
    /// Creates a registry backed by an explicit file path.
    pub fn with_path(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self { path })
    }
    fn load(&self) -> Result<GlobalMcpStore> {
        if !self.path.exists() {
            return Ok(GlobalMcpStore::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }
    fn save(&self, store: &GlobalMcpStore) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("global MCP store path has no parent directory")?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).context("failed to create temp file")?;
        std::io::Write::write_all(&mut temp, serde_json::to_string_pretty(store)?.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("failed to atomically write global MCP store")?;
        Ok(())
    }
    /// Lists all globally installed MCP servers.
    pub fn list(&self) -> Result<Vec<GlobalMcpEntry>> {
        Ok(self.load()?.servers)
    }

    /// Returns the MCP entry with the given id, if present.
    pub fn get(&self, id: &str) -> Result<Option<GlobalMcpEntry>> {
        Ok(self.load()?.servers.into_iter().find(|x| x.config.id == id))
    }
    /// Installs (or replaces) an MCP server with the given version and source.
    ///
    /// Holds a [`StoreLock`] across the load-modify-save cycle: this store is
    /// user-wide (not per-project), so it can be touched concurrently by the
    /// CLI and by any agent's dispatcher across every project on the machine.
    pub fn install(
        &self,
        config: CustomMcpServerConfig,
        version: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<GlobalMcpEntry> {
        if config.id.is_empty() {
            bail!("MCP id is required");
        }
        let entry = GlobalMcpEntry {
            config,
            version: version.into(),
            source: source.into(),
        };
        let _lock = StoreLock::acquire(&self.path)?;
        let mut store = self.load()?;
        store.servers.retain(|x| x.config.id != entry.config.id);
        store.servers.push(entry.clone());
        self.save(&store)?;
        Ok(entry)
    }
    /// Removes an MCP server, returning whether it existed.
    pub fn remove(&self, id: &str) -> Result<bool> {
        let _lock = StoreLock::acquire(&self.path)?;
        let mut store = self.load()?;
        let before = store.servers.len();
        store.servers.retain(|x| x.config.id != id);
        self.save(&store)?;
        Ok(before != store.servers.len())
    }
    /// Toggles an MCP server's enabled state, returning the updated entry.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<Option<GlobalMcpEntry>> {
        let _lock = StoreLock::acquire(&self.path)?;
        let mut store = self.load()?;
        let item = match store.servers.iter_mut().find(|x| x.config.id == id) {
            Some(x) => x,
            None => return Ok(None),
        };
        item.config.enabled = enabled;
        let result = item.clone();
        self.save(&store)?;
        Ok(Some(result))
    }
}

/// Names of MCP servers a project references from the global registry.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectMcpRefs {
    /// Referenced server ids.
    pub mcps: Vec<String>,
}

/// Per-project references to globally installed MCP servers.
pub struct ProjectMcpReferences {
    path: PathBuf,
}
impl ProjectMcpReferences {
    /// Creates a project MCP-reference store under `.agent/mcp_refs.json`.
    pub fn new(project_root: impl Into<PathBuf>) -> Result<Self> {
        let root = project_root.into();
        fs::create_dir_all(root.join(".agent"))?;
        Ok(Self {
            path: root.join(".agent").join("mcp_refs.json"),
        })
    }
    fn load(&self) -> Result<ProjectMcpRefs> {
        if !self.path.exists() {
            return Ok(ProjectMcpRefs::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }
    fn save(&self, refs: &ProjectMcpRefs) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("project MCP refs path has no parent directory")?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).context("failed to create temp file")?;
        std::io::Write::write_all(&mut temp, serde_json::to_string_pretty(refs)?.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("failed to atomically write project MCP refs")?;
        Ok(())
    }
    /// Lists the referenced MCP ids.
    pub fn list(&self) -> Result<Vec<String>> {
        Ok(self.load()?.mcps)
    }

    /// Adds an MCP reference, returning whether it was newly added.
    pub fn add(&self, id: &str) -> Result<bool> {
        if id.is_empty() {
            bail!("MCP id is required");
        }
        let _lock = StoreLock::acquire(&self.path)?;
        let mut refs = self.load()?;
        if refs.mcps.iter().any(|x| x == id) {
            return Ok(false);
        }
        refs.mcps.push(id.to_string());
        refs.mcps.sort();
        self.save(&refs)?;
        Ok(true)
    }
    /// Removes an MCP reference, returning whether it was present.
    pub fn remove(&self, id: &str) -> Result<bool> {
        let _lock = StoreLock::acquire(&self.path)?;
        let mut refs = self.load()?;
        let before = refs.mcps.len();
        refs.mcps.retain(|x| x != id);
        self.save(&refs)?;
        Ok(before != refs.mcps.len())
    }
    /// Resolves referenced ids to enabled MCP server configs.
    pub fn resolve(&self, global: &GlobalMcpRegistry) -> Result<Vec<CustomMcpServerConfig>> {
        let mut result = Vec::new();
        for id in self.list()? {
            if let Some(entry) = global.get(&id)? {
                if entry.config.enabled {
                    result.push(entry.config);
                }
            }
        }
        Ok(result)
    }
}

#[allow(dead_code)]
fn _transport_is_exhaustive(t: McpTransport) -> bool {
    matches!(t, McpTransport::Stdio | McpTransport::StreamableHttp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::McpPermissions;

    fn config(id: &str) -> CustomMcpServerConfig {
        CustomMcpServerConfig {
            id: id.into(),
            name: "name".into(),
            transport: McpTransport::Stdio,
            command: Some("true".into()),
            args: vec![],
            url: None,
            env: Default::default(),
            headers: Default::default(),
            permissions: McpPermissions::default(),
            enabled: true,
        }
    }

    fn temp_registry() -> (GlobalMcpRegistry, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let registry = GlobalMcpRegistry::with_path(dir.path().join("mcps.json")).unwrap();
        (registry, dir)
    }

    #[test]
    fn install_get_remove_round_trip() {
        let (registry, _dir) = temp_registry();
        registry.install(config("srv"), "1.0.0", "test").unwrap();
        assert!(registry.get("srv").unwrap().is_some());
        assert_eq!(registry.list().unwrap().len(), 1);

        assert!(
            !registry
                .set_enabled("srv", false)
                .unwrap()
                .unwrap()
                .config
                .enabled
        );

        assert!(registry.remove("srv").unwrap());
        assert!(registry.get("srv").unwrap().is_none());
        assert!(!registry.remove("srv").unwrap());
    }

    #[test]
    fn project_refs_add_remove_and_resolve_enabled_only() {
        let (registry, _dir) = temp_registry();
        registry
            .install(config("enabled-srv"), "1.0.0", "test")
            .unwrap();
        let mut disabled = config("disabled-srv");
        disabled.enabled = false;
        registry.install(disabled, "1.0.0", "test").unwrap();

        let project_dir = tempfile::tempdir().unwrap();
        let refs = ProjectMcpReferences::new(project_dir.path()).unwrap();
        assert!(refs.add("enabled-srv").unwrap());
        assert!(refs.add("disabled-srv").unwrap());
        // Adding the same id twice reports "not newly added".
        assert!(!refs.add("enabled-srv").unwrap());

        let resolved = refs.resolve(&registry).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].id, "enabled-srv");

        assert!(refs.remove("enabled-srv").unwrap());
        assert!(refs.resolve(&registry).unwrap().is_empty());
    }
}
