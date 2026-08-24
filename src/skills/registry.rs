use crate::skills::{parse_skill, Skill};
use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Global, user-owned skill registry. Projects reference skills by name instead
/// of copying their SKILL.md files into every repository.
pub struct GlobalSkillRegistry {
    root: PathBuf,
}

impl GlobalSkillRegistry {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        Ok(Self::new(home.join(".agent-workspace-hub").join("skills")))
    }

    pub fn skills_dir(&self) -> &Path {
        &self.root
    }

    pub fn create(&self, name: &str, description: &str) -> Result<Skill> {
        validate_name(name)?;
        fs::create_dir_all(&self.root)?;
        let dir = self.root.join(name);
        if dir.exists() {
            bail!("global skill already exists: {name}");
        }
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\ndescription: {description}\nversion: 0.1.0\n---\n\n# {name}\n\n## When to use\n\nDescribe when an agent should use this skill.\n\n## Workflow\n\n1. Describe the first step.\n2. Describe the second step.\n\n## Rules\n\n- Add important rules here.\n"))?;
        parse_skill(dir)
    }

    pub fn get(&self, name: &str) -> Result<Option<Skill>> {
        validate_name(name)?;
        let dir = self.root.join(name);
        if !dir.is_dir() {
            return Ok(None);
        }
        Ok(Some(parse_skill(dir)?))
    }

    pub fn list(&self) -> Result<Vec<Skill>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut skills = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.is_dir() && path.join("SKILL.md").is_file() {
                skills.push(parse_skill(path)?);
            }
        }
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(skills)
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 100 || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        bail!("invalid skill name: {name}");
    }
    Ok(())
}
