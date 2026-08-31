use crate::skills::{GlobalSkillRegistry, Skill};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Names of skills a project references from the global registry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillReferences {
    /// Referenced skill names.
    pub skills: Vec<String>,
}

/// Per-project skill references persisted under `.agent/skills.json`.
pub struct ProjectSkillReferences {
    project_root: PathBuf,
}

impl ProjectSkillReferences {
    /// Creates reference storage for a project root.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: root.into(),
        }
    }

    /// Returns the on-disk path of the references file.
    pub fn path(&self) -> PathBuf {
        self.project_root.join(".agent").join("skills.json")
    }

    /// Loads the project's skill references, defaulting to empty.
    pub fn load(&self) -> Result<SkillReferences> {
        let path = self.path();
        if !path.exists() {
            return Ok(SkillReferences::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    fn validate_name(name: &str) -> Result<()> {
        if name.is_empty()
            || name.len() > 100
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            bail!("invalid skill name: {name}");
        }
        Ok(())
    }

    /// Adds a skill reference, returning whether it was newly added.
    pub fn add(&self, name: &str, registry: &GlobalSkillRegistry) -> Result<bool> {
        Self::validate_name(name)?;
        if registry.get(name)?.is_none() {
            bail!("skill is not installed globally: {name}");
        }
        let mut refs = self.load()?;
        if refs.skills.iter().any(|s| s == name) {
            return Ok(false);
        }
        refs.skills.push(name.to_owned());
        refs.skills.sort();
        refs.skills.dedup();
        self.save(&refs)?;
        Ok(true)
    }

    /// Removes a skill reference, returning whether it was present.
    pub fn remove(&self, name: &str) -> Result<bool> {
        let mut refs = self.load()?;
        let old_len = refs.skills.len();
        refs.skills.retain(|s| s != name);
        if refs.skills.len() == old_len {
            return Ok(false);
        }
        self.save(&refs)?;
        Ok(true)
    }

    /// Resolves referenced skill names to installed skills, skipping missing ones.
    pub fn resolve(&self, registry: &GlobalSkillRegistry) -> Result<Vec<Skill>> {
        let refs = self.load()?;
        let mut resolved = Vec::with_capacity(refs.skills.len());
        for name in refs.skills {
            if let Some(skill) = registry.get(&name)? {
                resolved.push(skill);
            }
        }
        Ok(resolved)
    }

    /// Persists the project's skill references.
    pub fn save(&self, refs: &SkillReferences) -> Result<()> {
        let path = self.path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(refs)?)?;
        Ok(())
    }
}
