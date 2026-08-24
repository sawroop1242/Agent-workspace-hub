use crate::skills::{parse_skill, Skill};
use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;

pub struct SkillStore {
    root: PathBuf,
}

impl SkillStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn skills_dir(&self) -> PathBuf {
        self.root.join(".agent").join("skills")
    }

    pub fn create(&self, name: &str, description: &str) -> Result<Skill> {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
            bail!("invalid skill name: {name}");
        }
        let dir = self.skills_dir().join(name);
        if dir.exists() {
            bail!("skill already exists: {name}");
        }
        fs::create_dir_all(&dir)?;
        let content = format!("---\nname: {name}\ndescription: {description}\nversion: 0.1.0\n---\n\n# {name}\n\n## When to use\n\nDescribe when an agent should use this skill.\n\n## Workflow\n\n1. Describe the first step.\n2. Describe the second step.\n\n## Rules\n\n- Add important rules here.\n");
        fs::write(dir.join("SKILL.md"), content)?;
        parse_skill(dir)
    }

    pub fn get(&self, name: &str) -> Result<Option<Skill>> {
        let dir = self.skills_dir().join(name);
        if !dir.exists() {
            return Ok(None);
        }
        Ok(Some(parse_skill(dir)?))
    }

    pub fn list(&self) -> Result<Vec<Skill>> {
        let dir = self.skills_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut skills = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() && path.join("SKILL.md").is_file() {
                skills.push(parse_skill(path)?);
            }
        }
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(skills)
    }
}
