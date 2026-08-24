use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillReferences {
    pub skills: Vec<String>,
}

pub struct ProjectSkillReferences {
    project_root: PathBuf,
}

impl ProjectSkillReferences {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { project_root: root.into() }
    }

    fn path(&self) -> PathBuf {
        self.project_root.join(".agent").join("skills.json")
    }

    pub fn load(&self) -> Result<SkillReferences> {
        let path = self.path();
        if !path.exists() {
            return Ok(SkillReferences::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    fn validate_name(name: &str) -> Result<()> {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
            bail!("invalid skill name: {name}");
        }
        Ok(())
    }

    pub fn add(&self, name: &str) -> Result<bool> {
        Self::validate_name(name)?;
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

    pub fn save(&self, refs: &SkillReferences) -> Result<()> {
        let path = self.path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(refs)?)?;
        Ok(())
    }
}

pub fn project_skill_path(project_root: &Path) -> PathBuf {
    project_root.join(".agent").join("skills.json")
}
