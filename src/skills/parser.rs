use crate::skills::model::Skill;
use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn parse_skill(path: impl AsRef<Path>) -> Result<Skill> {
    let path = path.as_ref().to_path_buf();
    let skill_file = path.join("SKILL.md");
    if !skill_file.is_file() {
        bail!("SKILL.md not found: {}", skill_file.display());
    }

    let content = fs::read_to_string(&skill_file)?;
    let (name, description, version) = parse_front_matter(&content)?;

    if !is_valid_name(&name) {
        bail!("invalid skill name: {name}");
    }

    Ok(Skill {
        name,
        description,
        version,
        path: PathBuf::from(path),
    })
}

fn parse_front_matter(content: &str) -> Result<(String, String, Option<String>)> {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        bail!("SKILL.md must start with YAML front matter");
    }

    let mut name = None;
    let mut description = None;
    let mut version = None;

    for line in lines.by_ref() {
        if line.trim() == "---" {
            break;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key.trim() {
            "name" => name = Some(value.to_owned()),
            "description" => description = Some(value.to_owned()),
            "version" => version = Some(value.to_owned()),
            _ => {}
        }
    }

    let name = name.ok_or_else(|| anyhow::anyhow!("missing skill metadata: name"))?;
    let description = description.ok_or_else(|| anyhow::anyhow!("missing skill metadata: description"))?;
    Ok((name, description, version))
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}
