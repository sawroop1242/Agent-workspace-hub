use crate::skills::{GlobalSkillRegistry, Skill};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Names of skills a project references from the global registry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillReferences {
    /// Referenced skill names, sorted and deduplicated.
    pub skills: Vec<String>,
}

/// Per-project skill references persisted under `.agent/skills.json`.
pub struct ProjectSkillReferences {
    path: PathBuf,
}

impl ProjectSkillReferences {
    /// Creates reference storage for a project root.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            path: project_root.into().join(".agent").join("skills.json"),
        }
    }

    /// Loads the project's skill references, defaulting to empty.
    pub fn load(&self) -> Result<SkillReferences> {
        if !self.path.exists() {
            return Ok(SkillReferences::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    /// Adds a skill reference, requiring the skill to be installed globally.
    pub fn add(&self, name: &str, registry: &GlobalSkillRegistry) -> Result<()> {
        if registry.get(name)?.is_none() {
            bail!("global skill is not installed: {name}");
        }
        let mut refs = self.load()?;
        if !refs.skills.iter().any(|x| x == name) {
            refs.skills.push(name.to_owned());
            refs.skills.sort();
        }
        self.save(&refs)
    }

    /// Removes a skill reference, returning whether it was present.
    pub fn remove(&self, name: &str) -> Result<bool> {
        let mut refs = self.load()?;
        let before = refs.skills.len();
        refs.skills.retain(|x| x != name);
        if refs.skills.len() == before {
            return Ok(false);
        }
        self.save(&refs)?;
        Ok(true)
    }

    /// Resolves referenced skill names to installed skills, failing if one is missing.
    pub fn resolve(&self, registry: &GlobalSkillRegistry) -> Result<Vec<Skill>> {
        let refs = self.load()?;
        let mut resolved = Vec::new();
        for name in refs.skills {
            let Some(skill) = registry.get(&name)? else {
                bail!("project references missing global skill: {name}");
            };
            resolved.push(skill);
        }
        Ok(resolved)
    }

    fn save(&self, refs: &SkillReferences) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(refs)?)?;
        Ok(())
    }

    /// Returns the on-disk path of the references file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}
