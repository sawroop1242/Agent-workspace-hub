use crate::models::{Agent, AgentStatus};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Persistent agent identity storage under `.agent/agents` as per-agent JSON files.
pub struct AgentStore {
    root: PathBuf,
}

impl AgentStore {
    /// Creates an `AgentStore` rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn agents_dir(&self) -> PathBuf {
        self.root.join(".agent").join("agents")
    }

    /// Creates (or overwrites) an agent, keyed by its `id`.
    pub fn create(&self, agent: &Agent) -> Result<()> {
        if !is_safe_agent_id(&agent.id) {
            return Err(anyhow::anyhow!(
                "invalid agent id: {:?} (must not be empty or contain path separators)",
                agent.id
            ));
        }
        fs::create_dir_all(self.agents_dir())?;
        let path = self.agents_dir().join(format!("{}.json", agent.id));
        let data = serde_json::to_string_pretty(agent)?;
        fs::write(path, data)?;
        Ok(())
    }

    /// Returns the agent with the given `id`, or `None` if it does not exist.
    pub fn get(&self, id: &str) -> Result<Option<Agent>> {
        let path = self.agents_dir().join(format!("{}.json", id));
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    /// Lists all stored agents sorted by `id`.
    pub fn list(&self) -> Result<Vec<Agent>> {
        let dir = self.agents_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut agents = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let agent: Agent = serde_json::from_str(&fs::read_to_string(path)?)?;
            agents.push(agent);
        }
        agents.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(agents)
    }

    /// Updates an agent's status, returning `false` if the agent does not exist.
    pub fn set_status(&self, id: &str, status: AgentStatus) -> Result<bool> {
        let Some(mut agent) = self.get(id)? else {
            return Ok(false);
        };
        agent.status = status;
        self.create(&agent)
            .with_context(|| format!("failed to update status of agent {id}"))?;
        Ok(true)
    }
}

/// Returns whether `id` is safe to use as an agent filename (no path separators or traversal).
pub fn is_safe_agent_id(id: &str) -> bool {
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

    fn agent(id: &str, name: &str, role: &str) -> Agent {
        Agent {
            id: id.into(),
            name: name.into(),
            role: role.into(),
            status: AgentStatus::Created,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn create_then_get_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        let original = agent("writer", "Writer Agent", "writer");
        store.create(&original).unwrap();
        let loaded = store.get("writer").unwrap().expect("agent persisted");
        assert_eq!(loaded, original);
    }

    #[test]
    fn get_returns_none_for_missing_agent() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        assert!(store.get("missing").unwrap().is_none());
    }

    #[test]
    fn list_is_sorted_by_id() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        store.create(&agent("zeta", "Zeta", "reviewer")).unwrap();
        store
            .create(&agent("alpha", "Alpha", "researcher"))
            .unwrap();
        store.create(&agent("mid", "Mid", "implementer")).unwrap();
        let ids: Vec<String> = store.list().unwrap().into_iter().map(|a| a.id).collect();
        assert_eq!(ids, vec!["alpha", "mid", "zeta"]);
    }

    #[test]
    fn list_on_missing_directory_is_empty() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn set_status_changes_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        store.create(&agent("worker", "Worker", "worker")).unwrap();
        assert!(store.set_status("worker", AgentStatus::Active).unwrap());
        let loaded = store.get("worker").unwrap().expect("agent exists");
        assert_eq!(loaded.status, AgentStatus::Active);
        assert!(store.set_status("worker", AgentStatus::Failed).unwrap());
        let loaded = store.get("worker").unwrap().expect("agent exists");
        assert_eq!(loaded.status, AgentStatus::Failed);
    }

    #[test]
    fn set_status_returns_false_for_missing_agent() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        assert!(!store.set_status("missing", AgentStatus::Active).unwrap());
    }

    #[test]
    fn create_rejects_unsafe_ids() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        for bad in ["../escape", "a/b", "a\\b"] {
            assert!(
                store.create(&agent(bad, "Bad", "role")).is_err(),
                "id {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn is_safe_agent_id_rejects_traversal_and_separators() {
        for bad in ["../x", "a/b", "", ".", "..", "a\\b"] {
            assert!(!is_safe_agent_id(bad), "id {bad:?} must be rejected");
        }
        assert!(is_safe_agent_id("agent-1"));
        assert!(is_safe_agent_id("agent_2"));
    }

    #[test]
    fn persists_under_agent_directory() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        store
            .create(&agent("writer", "Writer Agent", "writer"))
            .unwrap();
        let path = temp
            .path()
            .join(".agent")
            .join("agents")
            .join("writer.json");
        assert!(path.exists(), "agent file must live under .agent/agents");
    }
}
