use crate::skills::{parse_skill, Skill};
use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;

/// Project-scoped skill store (`SkillStore`), managing skills under a
/// `.agent/skills` directory.
pub struct SkillStore {
    root: PathBuf,
}

impl SkillStore {
    /// Creates a store rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn skills_dir(&self) -> PathBuf {
        self.root.join(".agent").join("skills")
    }

    /// Creates a new project skill with a starter `SKILL.md` template.
    pub fn create(&self, name: &str, description: &str) -> Result<Skill> {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
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

    /// Returns the project skill named `name`, if present.
    pub fn get(&self, name: &str) -> Result<Option<Skill>> {
        let dir = self.skills_dir().join(name);
        if !dir.exists() {
            return Ok(None);
        }
        Ok(Some(parse_skill(dir)?))
    }

    /// Lists all project skills, sorted by name.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_writes_parseable_skill_md_and_returns_skill() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::new(temp.path());
        let skill = store.create("my-skill", "does things").unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "does things");
        assert_eq!(skill.version.as_deref(), Some("0.1.0"));
        assert!(store
            .skills_dir()
            .join("my-skill")
            .join("SKILL.md")
            .is_file());
    }

    #[test]
    fn create_rejects_invalid_names() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::new(temp.path());
        for name in ["", "UPPER", "with space", "with/slash"] {
            assert!(
                store.create(name, "d").is_err(),
                "{name:?} must be rejected"
            );
        }
        // nothing was created for any invalid name
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn create_rejects_duplicate_skill() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::new(temp.path());
        store.create("dup", "first").unwrap();
        assert!(store.create("dup", "second").is_err());
    }

    #[test]
    fn get_returns_none_for_missing_and_some_for_present() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::new(temp.path());
        assert!(store.get("missing").unwrap().is_none());
        store.create("present", "d").unwrap();
        let skill = store.get("present").unwrap().unwrap();
        assert_eq!(skill.name, "present");
    }

    #[test]
    fn list_is_sorted_and_ignores_dirs_without_skill_md() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::new(temp.path());
        store.create("b-skill", "b").unwrap();
        store.create("a-skill", "a").unwrap();
        fs::create_dir_all(store.skills_dir().join("not-a-skill")).unwrap();

        let skills = store.list().unwrap();
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a-skill", "b-skill"]);
    }

    #[test]
    fn list_on_missing_skills_dir_is_empty() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::new(temp.path());
        assert!(store.list().unwrap().is_empty());
    }
}
