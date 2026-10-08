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
    /// Creates a registry rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Locates the user-global registry under `~/.agent-workspace-hub/skills`.
    pub fn discover() -> Result<Self> {
        // Test/embedding seam (mirrors the Control API's
        // `global_skills_root` injection): an explicit root keeps tests
        // off the real home registry without HOME mutation, which
        // silently no-ops on Windows and races parallel tests.
        if let Some(root) = std::env::var_os("AWH_GLOBAL_SKILLS_ROOT") {
            return Ok(Self::new(root));
        }
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        Ok(Self::new(home.join(".agent-workspace-hub").join("skills")))
    }

    /// Returns the on-disk skills directory backing this registry.
    pub fn skills_dir(&self) -> &Path {
        &self.root
    }

    /// Creates a new global skill with a starter `SKILL.md` template.
    pub fn create(&self, name: &str, description: &str) -> Result<Skill> {
        validate_skill_name(name)?;
        fs::create_dir_all(&self.root)?;
        let dir = self.root.join(name);
        if dir.exists() {
            bail!("global skill already exists: {name}");
        }
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\ndescription: {description}\nversion: 0.1.0\n---\n\n# {name}\n\n## When to use\n\nDescribe when an agent should use this skill.\n\n## Workflow\n\n1. Describe the first step.\n2. Describe the second step.\n\n## Rules\n\n- Add important rules here.\n"))?;
        parse_skill(dir)
    }

    /// Returns the global skill named `name`, if installed.
    pub fn get(&self, name: &str) -> Result<Option<Skill>> {
        validate_skill_name(name)?;
        let dir = self.root.join(name);
        if !dir.is_dir() {
            return Ok(None);
        }
        Ok(Some(parse_skill(dir)?))
    }

    /// Lists all installed global skills, sorted by name.
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

fn validate_skill_name(name: &str) -> Result<()> {
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

/// Validates a skill name for any path join under the canonical skills
/// root. This is the boundary that keeps a caller-supplied or
/// registry-advertised name from escaping the registry: `..`, path
/// separators, absolute paths, and drive prefixes are all rejected before
/// any `join`, so no mutation can address a directory outside the skills
/// root. Public so install/uninstall paths share the registry's own
/// validation instead of trusting their caller.
pub fn validate_name(name: &str) -> Result<()> {
    validate_skill_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_name_rejects_path_shapes_and_accepts_safe_names() {
        for evil in [
            "",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "/etc/passwd",
            "C:\\windows",
            "UPPER",
            "with space",
            "d\u{f6}t",
        ] {
            assert!(validate_name(evil).is_err(), "{evil:?} must be rejected");
        }
        for ok in ["alpha", "my-skill", "a_b", "s1", &"a".repeat(100)] {
            assert!(validate_name(ok).is_ok(), "{ok:?} must be accepted");
        }
        // one over the length limit is rejected
        assert!(validate_name(&"a".repeat(101)).is_err());
    }

    #[test]
    fn registry_get_and_create_share_the_same_validation() {
        let temp = tempfile::tempdir().unwrap();
        let reg = GlobalSkillRegistry::new(temp.path());
        assert!(reg.get("../escape").is_err());
        assert!(reg.create("../escape", "d").is_err());
        // neither touched the filesystem above the root
        assert!(!temp.path().join("escape").exists());
    }
}
