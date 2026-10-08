use crate::models::{Agent, AgentStatus};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Persistent agent profile/identity storage under `.agent/agents` as
/// per-agent JSON files.
pub struct AgentStore {
    root: PathBuf,
}

/// Maximum accepted length for an agent id (bounded input, TW-002 §15).
pub const MAX_AGENT_ID_LEN: usize = 64;

impl AgentStore {
    /// Creates an `AgentStore` rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn agents_dir(&self) -> PathBuf {
        self.root.join(".agent").join("agents")
    }

    fn record_path(&self, id: &str) -> PathBuf {
        self.agents_dir().join(format!("{}.json", id))
    }

    /// Writes the agent record with a durability point before the rename
    /// (matching the manifest/policy/session stores: tempfile + fsync +
    /// rename under `StoreLock`).
    fn write_record(&self, agent: &Agent) -> Result<()> {
        let path = self.record_path(&agent.id);
        let mut tmp = tempfile::NamedTempFile::new_in(&self.root)?;
        serde_json::to_writer_pretty(tmp.as_file_mut(), agent)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path)?;
        Ok(())
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
        let path = self.record_path(&agent.id);
        let _lock = crate::mcp::store_lock::StoreLock::acquire(&path)?;
        self.write_record(agent)
    }

    /// Registers a new agent profile, failing if the id is already taken.
    /// Unlike [`create`](Self::create) (a compatibility upsert), this is
    /// the canonical registry operation: a duplicate identity is an error,
    /// never a silent overwrite of another agent's record (TW-002 §5).
    /// The check and the write run under one `StoreLock` on the record
    /// path, so two concurrent registrations cannot both win (one gets
    /// an explicit duplicate error).
    pub fn register(&self, agent: &Agent) -> Result<()> {
        if !is_safe_agent_id(&agent.id) {
            return Err(anyhow::anyhow!(
                "invalid agent id: {:?} (must not be empty or contain path separators)",
                agent.id
            ));
        }
        fs::create_dir_all(self.agents_dir())?;
        let path = self.record_path(&agent.id);
        let _lock = crate::mcp::store_lock::StoreLock::acquire(&path)?;
        if self.get_locked(&path)?.is_some() {
            return Err(anyhow::anyhow!(
                "agent id already registered: {} (use a different id)",
                agent.id
            ));
        }
        self.write_record(agent)
    }

    /// Unlocked-path record read used while a `StoreLock` on `path` is
    /// already held by the caller (reads never need the lock; this exists
    /// so `register` performs its duplicate check against the exact file
    /// it is about to write).
    fn get_locked(&self, path: &Path) -> Result<Option<Agent>> {
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    /// Returns the agent with the given `id`, or `None` if it does not
    /// exist. Unsafe ids fail closed as `None` — an id that cannot name a
    /// store file can never escape `.agent/agents/` via a read (TW-002 §15).
    pub fn get(&self, id: &str) -> Result<Option<Agent>> {
        if !is_safe_agent_id(id) {
            return Ok(None);
        }
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

    /// Updates an agent's `enabled` flag, returning `false` if the agent
    /// does not exist. Disabling is the profile-level switch the runtime
    /// consults before resolving any session for the agent (TW-002 §5).
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let Some(mut agent) = self.get(id)? else {
            return Ok(false);
        };
        agent.enabled = enabled;
        self.create(&agent)
            .with_context(|| format!("failed to update enabled flag of agent {id}"))?;
        Ok(true)
    }
}

/// Returns whether `id` is safe to use as an agent filename (no path
/// separators, no traversal, bounded length, no control characters).
pub fn is_safe_agent_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_AGENT_ID_LEN
        && id != "."
        && id != ".."
        && !id.contains('/')
        && !id.contains('\\')
        && !id.chars().any(|c| c.is_control())
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
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn register_rejects_duplicate_ids() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        store.create(&agent("writer", "Writer", "writer")).unwrap();
        let err = store
            .register(&agent("writer", "Impostor", "writer"))
            .unwrap_err();
        assert!(err.to_string().contains("already registered"), "{err}");
        // The original record was not overwritten.
        let loaded = store.get("writer").unwrap().unwrap();
        assert_eq!(loaded.name, "Writer");
    }

    #[test]
    fn register_accepts_fresh_ids() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        store
            .register(&agent("writer", "Writer", "writer"))
            .unwrap();
        assert!(store.get("writer").unwrap().is_some());
    }

    #[test]
    fn concurrent_registration_never_silently_overwrites() {
        // The duplicate-identity guarantee must hold across processes, not
        // just sequential calls: N threads racing `register` on the same
        // id must yield exactly one winner; every loser gets an explicit
        // duplicate error (never a silent overwrite of the winner's
        // record).
        use std::sync::Arc;
        let temp = tempfile::tempdir().unwrap();
        let root = Arc::new(temp.path().to_path_buf());

        const THREADS: usize = 8;
        let mut handles = Vec::new();
        for t in 0..THREADS {
            let root = root.clone();
            handles.push(std::thread::spawn(move || {
                let store = AgentStore::new(root.as_path());
                store
                    .register(&agent("writer", &format!("Writer {t}"), "writer"))
                    .is_ok()
            }));
        }

        let winners = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|ok| *ok)
            .count();
        assert_eq!(winners, 1, "exactly one concurrent registrant may win");
        let store = AgentStore::new(root.as_path());
        let loaded = store.get("writer").unwrap().expect("winner persisted");
        // The surviving record is the winner's, byte-intact.
        assert_eq!(loaded.role, "writer");
        assert!(loaded.name.starts_with("Writer "), "{}", loaded.name);
    }

    #[test]
    fn get_fails_closed_on_unsafe_ids() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        store.create(&agent("writer", "Writer", "writer")).unwrap();
        // Even with a crafted file outside the store, a traversal id must
        // resolve to no agent rather than read outside `.agent/agents/`.
        for bad in ["../workspace", "a/b", "..", "", "a\\b", "."] {
            assert!(store.get(bad).unwrap().is_none(), "{bad:?} must be None");
        }
    }

    #[test]
    fn set_enabled_toggles_profile_switch() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentStore::new(temp.path());
        store.create(&agent("writer", "Writer", "writer")).unwrap();
        assert!(store.set_enabled("writer", false).unwrap());
        assert!(!store.get("writer").unwrap().unwrap().enabled);
        assert!(store.set_enabled("writer", true).unwrap());
        assert!(store.get("writer").unwrap().unwrap().enabled);
        assert!(!store.set_enabled("ghost", false).unwrap());
    }

    #[test]
    fn legacy_records_without_enabled_field_default_to_enabled() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join(".agent").join("agents");
        fs::create_dir_all(&dir).unwrap();
        let legacy = r#"{
            "id": "legacy",
            "name": "Legacy Agent",
            "role": "writer",
            "status": "created",
            "created_at": "2024-01-01T00:00:00Z"
        }"#;
        fs::write(dir.join("legacy.json"), legacy).unwrap();
        let store = AgentStore::new(temp.path());
        let loaded = store.get("legacy").unwrap().expect("legacy agent loads");
        assert!(loaded.enabled, "legacy record must stay enabled");
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
