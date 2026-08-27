use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// A skill pinned in the lockfile with its source and integrity digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedSkill {
    pub name: String,
    pub version: String,
    pub source: String,
    pub sha256: Option<String>,
}

/// The project skill lockfile, recording pinned skill versions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillLockfile {
    pub version: u32,
    pub skills: Vec<LockedSkill>,
}

/// Persistent store for a project's skill lockfile.
pub struct LockfileStore {
    path: PathBuf,
}

impl LockfileStore {
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
