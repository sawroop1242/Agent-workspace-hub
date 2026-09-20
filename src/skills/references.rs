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

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with(temp: &tempfile::TempDir, names: &[&str]) -> GlobalSkillRegistry {
        let registry = GlobalSkillRegistry::new(temp.path().join("global-skills"));
        for name in names {
            registry.create(name, "test skill").unwrap();
        }
        registry
    }

    #[test]
    fn new_points_at_agent_skills_json() {
        let refs = ProjectSkillReferences::new("/proj");
        assert_eq!(refs.path(), Path::new("/proj/.agent/skills.json"));
    }

    #[test]
    fn load_defaults_to_empty_when_missing() {
        let temp = tempfile::tempdir().unwrap();
        let refs = ProjectSkillReferences::new(temp.path());
        assert!(refs.load().unwrap().skills.is_empty());
    }

    #[test]
    fn add_requires_skill_installed_globally() {
        let temp = tempfile::tempdir().unwrap();
        let registry = registry_with(&temp, &[]);
        let refs = ProjectSkillReferences::new(temp.path().join("proj"));
        let error = refs.add("missing-skill", &registry).unwrap_err();
        assert!(error.to_string().contains("not installed"));
    }

    #[test]
    fn add_persists_sorted_unique_references() {
        let temp = tempfile::tempdir().unwrap();
        let registry = registry_with(&temp, &["alpha", "beta"]);
        let proj = temp.path().join("proj");
        let refs = ProjectSkillReferences::new(&proj);
        refs.add("beta", &registry).unwrap();
        refs.add("alpha", &registry).unwrap();
        refs.add("alpha", &registry).unwrap(); // duplicate add is a no-op

        let loaded = refs.load().unwrap();
        assert_eq!(loaded.skills, vec!["alpha", "beta"]);
        assert!(refs.path().is_file());
    }

    #[test]
    fn remove_reports_existence_and_persists() {
        let temp = tempfile::tempdir().unwrap();
        let registry = registry_with(&temp, &["alpha"]);
        let refs = ProjectSkillReferences::new(temp.path().join("proj"));
        assert!(!refs.remove("alpha").unwrap()); // not referenced yet
        refs.add("alpha", &registry).unwrap();
        assert!(refs.remove("alpha").unwrap());
        assert!(refs.load().unwrap().skills.is_empty());
        assert!(!refs.remove("alpha").unwrap()); // already removed
    }

    #[test]
    fn resolve_returns_installed_skills_in_reference_order() {
        let temp = tempfile::tempdir().unwrap();
        let registry = registry_with(&temp, &["alpha", "beta"]);
        let refs = ProjectSkillReferences::new(temp.path().join("proj"));
        refs.add("beta", &registry).unwrap();
        refs.add("alpha", &registry).unwrap();
        let resolved = refs.resolve(&registry).unwrap();
        let names: Vec<&str> = resolved.iter().map(|s| s.name.as_str()).collect();
        // stored sorted, so resolution is deterministic
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn resolve_fails_when_referenced_skill_is_missing_globally() {
        let temp = tempfile::tempdir().unwrap();
        let registry = registry_with(&temp, &["alpha"]);
        let proj = temp.path().join("proj");
        let refs = ProjectSkillReferences::new(&proj);
        refs.add("alpha", &registry).unwrap();
        // simulate the skill being uninstalled after being referenced
        fs::remove_dir_all(registry.skills_dir().join("alpha")).unwrap();
        let error = refs.resolve(&registry).unwrap_err();
        assert!(error.to_string().contains("missing global skill"));
    }
}
