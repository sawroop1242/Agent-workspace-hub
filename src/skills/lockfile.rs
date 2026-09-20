use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// A skill pinned in the lockfile with its source and integrity digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedSkill {
    /// Skill name.
    pub name: String,
    /// Pinned version.
    pub version: String,
    /// Source (registry or repository) of the skill.
    pub source: String,
    /// Optional SHA-256 digest of the skill package.
    pub sha256: Option<String>,
}

/// The project skill lockfile, recording pinned skill versions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillLockfile {
    /// Lockfile schema version.
    pub version: u32,
    /// Pinned skills.
    pub skills: Vec<LockedSkill>,
}

/// Persistent store for a project's skill lockfile.
pub struct LockfileStore {
    path: PathBuf,
}

impl LockfileStore {
    /// Creates a store backed by `project_root/.agent/skills.lock.json`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            path: project_root.into().join(".agent").join("skills.lock.json"),
        }
    }

    /// Loads the lockfile, defaulting to an empty v1 lockfile.
    pub fn load(&self) -> Result<SkillLockfile> {
        if !self.path.exists() {
            return Ok(SkillLockfile {
                version: 1,
                skills: Vec::new(),
            });
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    /// Persists the lockfile.
    pub fn save(&self, lock: &SkillLockfile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(lock)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locked(name: &str, version: &str, source: &str) -> LockedSkill {
        LockedSkill {
            name: name.into(),
            version: version.into(),
            source: source.into(),
            sha256: None,
        }
    }

    #[test]
    fn new_points_at_agent_skills_lock_json() {
        let store = LockfileStore::new("/proj");
        assert_eq!(store.path, PathBuf::from("/proj/.agent/skills.lock.json"));
    }

    #[test]
    fn load_defaults_to_empty_v1_lockfile_when_missing() {
        let temp = tempfile::tempdir().unwrap();
        let store = LockfileStore::new(temp.path());
        let lock = store.load().unwrap();
        assert_eq!(lock.version, 1);
        assert!(lock.skills.is_empty());
    }

    #[test]
    fn save_then_load_round_trips_pinned_skills() {
        let temp = tempfile::tempdir().unwrap();
        let store = LockfileStore::new(temp.path());
        let lock = SkillLockfile {
            version: 1,
            skills: vec![
                locked("alpha", "1.0.0", "registry"),
                locked("beta", "2.0.0", "repo"),
            ],
        };
        store.save(&lock).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.skills.len(), 2);
        assert_eq!(loaded.skills[0].name, "alpha");
        assert_eq!(loaded.skills[0].version, "1.0.0");
        assert_eq!(loaded.skills[1].source, "repo");
        assert_eq!(loaded.skills[1].sha256, None);
    }

    #[test]
    fn save_creates_missing_parent_directories() {
        let temp = tempfile::tempdir().unwrap();
        let store = LockfileStore::new(temp.path().join("deep/nested/project"));
        store
            .save(&SkillLockfile {
                version: 1,
                skills: vec![],
            })
            .unwrap();
        assert!(temp
            .path()
            .join("deep/nested/project/.agent/skills.lock.json")
            .is_file());
    }

    #[test]
    fn load_fails_on_corrupt_json() {
        let temp = tempfile::tempdir().unwrap();
        let store = LockfileStore::new(temp.path());
        fs::create_dir_all(temp.path().join(".agent")).unwrap();
        fs::write(&store.path, "not json").unwrap();
        assert!(store.load().is_err());
    }

    #[test]
    fn sha256_digest_is_round_tripped_when_present() {
        let temp = tempfile::tempdir().unwrap();
        let store = LockfileStore::new(temp.path());
        let mut skill = locked("alpha", "1.0.0", "registry");
        skill.sha256 = Some("a".repeat(64));
        store
            .save(&SkillLockfile {
                version: 1,
                skills: vec![skill],
            })
            .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(
            loaded.skills[0].sha256.as_deref(),
            Some("a".repeat(64).as_str())
        );
    }
}
