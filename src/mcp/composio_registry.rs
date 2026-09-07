//! Global registry of Composio connected accounts, shared across every
//! project and every agent on this machine.
//!
//! [`crate::mcp::composio::ComposioProvider`] wraps exactly one connected
//! account. Before this module, the *only* way to get one running was the
//! `COMPOSIO_API_KEY` / `COMPOSIO_CONNECTED_ACCOUNT_ID` / `COMPOSIO_TOOLKIT`
//! env-var triple read once at dispatcher startup — good for "one account,
//! shared by every project" (the env vars are process-wide), but with no
//! way to add a *second* account (GitHub AND Slack, say) without replacing
//! the first, and no way to add one without a human hand-editing env vars
//! outside the tool entirely. The OAuth link/list-accounts API in
//! `composio_auth.rs` existed to support exactly this but was never wired
//! to anything.
//!
//! `ComposioRegistry` closes that gap the same way
//! [`crate::mcp::global_mcp::GlobalMcpRegistry`] does for custom MCP
//! servers: register an account once (label + the `connected_account_id`
//! obtained via [`crate::mcp::composio_auth::ComposioAuth`]'s link flow),
//! and every project's [`crate::mcp::dispatcher::McpDispatcher`] picks it up
//! from then on as an additional provider (`composio:<label>`), no
//! per-project reconnection required. All accounts registered here still
//! share the single `COMPOSIO_API_KEY` env var, matching how a Composio API
//! key spans many connected accounts.

use crate::mcp::store_lock::StoreLock;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// A single registered Composio connected account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioAccount {
    /// Short label identifying this account, unique within the registry.
    /// Becomes the provider id suffix (`composio:<label>`) an agent uses to
    /// address it via `connector.tools` / `connector.invoke`, and the
    /// argument to [`ComposioRegistry::remove`].
    pub label: String,
    /// The Composio `connected_account_id` for this account, obtained once
    /// the human finishes the OAuth flow started by
    /// `ComposioAuth::create_link` (confirm via `ComposioAuth::list_accounts`
    /// or `get_account`).
    pub connected_account_id: String,
    /// The toolkit this account belongs to (e.g. `"github"`, `"slack"`),
    /// used to scope this account's tool listing. Optional: some setups
    /// don't need the filter and would rather see every tool the connected
    /// account exposes.
    #[serde(default)]
    pub toolkit: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ComposioAccountStore {
    accounts: Vec<ComposioAccount>,
}

/// Persistent, user-wide registry of Composio connected accounts, stored
/// under the platform user data directory alongside
/// [`crate::mcp::global_mcp::GlobalMcpRegistry`]'s `mcps.json`, so it
/// survives independently of any single project checkout.
pub struct ComposioRegistry {
    path: PathBuf,
}

impl ComposioRegistry {
    /// Creates a registry rooted at the platform user data directory.
    pub fn new() -> Result<Self> {
        let root = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("agent-workspace-hub");
        fs::create_dir_all(&root)?;
        Ok(Self {
            path: root.join("composio_accounts.json"),
        })
    }

    /// Creates a registry backed by an explicit file path (used by tests to
    /// avoid touching the real user data directory).
    pub fn with_path(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self { path })
    }

    fn load(&self) -> Result<ComposioAccountStore> {
        if !self.path.exists() {
            return Ok(ComposioAccountStore::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    /// Writes `store` atomically (temp file + rename) so a sibling process
    /// (another agent, or the CLI) never observes a torn write.
    fn save(&self, store: &ComposioAccountStore) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("composio account store path has no parent directory")?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).context("failed to create temp file")?;
        std::io::Write::write_all(&mut temp, serde_json::to_string_pretty(store)?.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("failed to atomically write composio account store")?;
        Ok(())
    }

    /// Lists all registered accounts.
    pub fn list(&self) -> Result<Vec<ComposioAccount>> {
        Ok(self.load()?.accounts)
    }

    /// Registers (or replaces) an account under `account.label`.
    ///
    /// Holds a [`StoreLock`] across the load-modify-save cycle so two agents
    /// (or an agent and the CLI) registering accounts on the same machine at
    /// the same time serialize behind it instead of one silently dropping
    /// the other's registration.
    pub fn register(&self, account: ComposioAccount) -> Result<ComposioAccount> {
        if account.label.trim().is_empty() {
            bail!("account label is required");
        }
        if account.connected_account_id.trim().is_empty() {
            bail!("connected_account_id is required");
        }
        let _lock = StoreLock::acquire(&self.path)?;
        let mut store = self.load()?;
        store.accounts.retain(|a| a.label != account.label);
        store.accounts.push(account.clone());
        self.save(&store)?;
        Ok(account)
    }

    /// Removes an account, returning whether it existed.
    pub fn remove(&self, label: &str) -> Result<bool> {
        let _lock = StoreLock::acquire(&self.path)?;
        let mut store = self.load()?;
        let before = store.accounts.len();
        store.accounts.retain(|a| a.label != label);
        self.save(&store)?;
        Ok(before != store.accounts.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_registry() -> (ComposioRegistry, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            ComposioRegistry::with_path(dir.path().join("composio_accounts.json")).unwrap();
        (registry, dir)
    }

    fn account(label: &str, connected_account_id: &str) -> ComposioAccount {
        ComposioAccount {
            label: label.into(),
            connected_account_id: connected_account_id.into(),
            toolkit: Some("github".into()),
        }
    }

    #[test]
    fn register_rejects_missing_fields() {
        let (registry, _dir) = temp_registry();
        let mut a = account("github", "acc_1");
        a.label = String::new();
        assert!(registry.register(a).is_err());

        let mut a = account("github", "acc_1");
        a.connected_account_id = String::new();
        assert!(registry.register(a).is_err());
    }

    #[test]
    fn register_replace_remove_round_trip() {
        let (registry, _dir) = temp_registry();
        registry.register(account("github", "acc_1")).unwrap();
        assert_eq!(registry.list().unwrap().len(), 1);

        // Re-registering the same label replaces rather than duplicating.
        registry.register(account("github", "acc_2")).unwrap();
        let accounts = registry.list().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].connected_account_id, "acc_2");

        registry.register(account("slack", "acc_3")).unwrap();
        assert_eq!(registry.list().unwrap().len(), 2);

        assert!(registry.remove("github").unwrap());
        assert_eq!(registry.list().unwrap().len(), 1);
        assert!(!registry.remove("github").unwrap());
    }

    #[test]
    fn corrupted_store_fails_closed() {
        let (registry, _dir) = temp_registry();
        fs::write(&registry.path, "not json {").unwrap();
        assert!(registry.register(account("github", "acc_1")).is_err());
        assert!(registry.list().is_err());
    }
}
