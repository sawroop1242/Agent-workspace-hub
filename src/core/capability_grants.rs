use crate::models::CapabilityGrant;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Persistent capability grant storage under `.agent/capabilities` as
/// per-grant JSON files.
pub struct CapabilityGrantStore {
    root: PathBuf,
}

impl CapabilityGrantStore {
    /// Creates a `CapabilityGrantStore` rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn capabilities_dir(&self) -> PathBuf {
        self.root.join(".agent").join("capabilities")
    }

    /// Creates (or overwrites) a grant, keyed by its `id`.
    pub fn create(&self, grant: &CapabilityGrant) -> Result<()> {
        fs::create_dir_all(self.capabilities_dir())?;
        let path = self.capabilities_dir().join(format!("{}.json", grant.id));
        let data = serde_json::to_string_pretty(grant)?;
        fs::write(path, data)?;
        Ok(())
    }

    /// Returns the grant with the given `id`, or `None` if it does not exist.
    pub fn get(&self, id: &str) -> Result<Option<CapabilityGrant>> {
        let path = self.capabilities_dir().join(format!("{}.json", id));
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    /// Lists all grants belonging to `agent_id`, sorted by grant `id`.
    pub fn list_for_agent(&self, agent_id: &str) -> Result<Vec<CapabilityGrant>> {
        let dir = self.capabilities_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut grants = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let grant: CapabilityGrant = serde_json::from_str(&fs::read_to_string(path)?)?;
            if grant.agent_id == agent_id {
                grants.push(grant);
            }
        }
        grants.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(grants)
    }

    /// Revokes (deletes) the grant with the given `id`, returning `false` if
    /// it did not exist.
    pub fn revoke(&self, id: &str) -> Result<bool> {
        let path = self.capabilities_dir().join(format!("{}.json", id));
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(path).with_context(|| format!("failed to revoke capability grant {id}"))?;
        Ok(true)
    }
}

/// Returns whether `id` is safe to use as a grant filename (no path separators or traversal).
pub fn is_safe_grant_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains('/')
        && !id.contains('\\')
        && Path::new(id).file_name().and_then(|x| x.to_str()) == Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::permissions::Permission;

    fn grant(id: &str, agent_id: &str, permission: Permission) -> CapabilityGrant {
        CapabilityGrant {
            id: id.into(),
            agent_id: agent_id.into(),
            permission,
            scope: None,
            granted_at: chrono::Utc::now().to_rfc3339(),
            expires_at: None,
        }
    }

    #[test]
    fn create_then_get_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = CapabilityGrantStore::new(temp.path());
        let original = grant("g1", "writer", Permission::Filesystem);
        store.create(&original).unwrap();
        let loaded = store.get("g1").unwrap().expect("grant persisted");
        assert_eq!(loaded, original);
        assert_eq!(loaded.permission, Permission::Filesystem);
    }

    #[test]
    fn get_returns_none_for_missing_grant() {
        let temp = tempfile::tempdir().unwrap();
        let store = CapabilityGrantStore::new(temp.path());
        assert!(store.get("missing").unwrap().is_none());
    }

    #[test]
    fn list_for_agent_returns_only_that_agents_grants() {
        let temp = tempfile::tempdir().unwrap();
        let store = CapabilityGrantStore::new(temp.path());
        store
            .create(&grant("z1", "writer", Permission::Filesystem))
            .unwrap();
        store
            .create(&grant("a1", "writer", Permission::Network))
            .unwrap();
        store
            .create(&grant("b1", "reader", Permission::Process))
            .unwrap();
        let ids: Vec<String> = store
            .list_for_agent("writer")
            .unwrap()
            .into_iter()
            .map(|g| g.id)
            .collect();
        assert_eq!(ids, vec!["a1", "z1"]);
        let reader = store.list_for_agent("reader").unwrap();
        assert_eq!(reader.len(), 1);
        assert_eq!(reader[0].permission, Permission::Process);
        assert!(store.list_for_agent("unknown").unwrap().is_empty());
    }

    #[test]
    fn revoke_removes_grant_and_reports_missing() {
        let temp = tempfile::tempdir().unwrap();
        let store = CapabilityGrantStore::new(temp.path());
        store
            .create(&grant("g1", "writer", Permission::Network))
            .unwrap();
        assert!(store.revoke("g1").unwrap(), "existing grant revokes");
        assert!(store.get("g1").unwrap().is_none(), "grant is gone");
        assert!(
            !store.revoke("g1").unwrap(),
            "revoking a missing grant returns false"
        );
    }

    #[test]
    fn list_for_agent_on_missing_directory_is_empty() {
        let temp = tempfile::tempdir().unwrap();
        let store = CapabilityGrantStore::new(temp.path());
        assert!(store.list_for_agent("writer").unwrap().is_empty());
    }

    #[test]
    fn persists_under_capabilities_directory() {
        let temp = tempfile::tempdir().unwrap();
        let store = CapabilityGrantStore::new(temp.path());
        store
            .create(&grant("g1", "writer", Permission::Secrets))
            .unwrap();
        let path = temp
            .path()
            .join(".agent")
            .join("capabilities")
            .join("g1.json");
        assert!(
            path.exists(),
            "grant file must live under .agent/capabilities"
        );
    }

    #[test]
    fn scope_and_expiry_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let store = CapabilityGrantStore::new(temp.path());
        let original = CapabilityGrant {
            id: "g2".into(),
            agent_id: "writer".into(),
            permission: Permission::Environment,
            scope: Some("src/".into()),
            granted_at: "2026-01-01T00:00:00+00:00".into(),
            expires_at: Some("2027-01-01T00:00:00+00:00".into()),
        };
        store.create(&original).unwrap();
        assert_eq!(store.get("g2").unwrap().unwrap(), original);
    }
}
