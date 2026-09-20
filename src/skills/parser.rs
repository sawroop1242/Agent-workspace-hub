use crate::skills::model::Skill;
use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

/// Parses a `SKILL.md` file into a [`Skill`], validating its YAML front matter
/// and name.
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
        path,
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
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key.trim() {
            "name" => name = Some(value.to_owned()),
            "description" => description = Some(value.to_owned()),
            "version" => version = Some(value.to_owned()),
            _ => {}
        }
    }

    let name = name.ok_or_else(|| anyhow::anyhow!("missing skill metadata: name"))?;
    let description =
        description.ok_or_else(|| anyhow::anyhow!("missing skill metadata: description"))?;
    Ok((name, description, version))
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill_dir(temp: &tempfile::TempDir, name: &str) -> std::path::PathBuf {
        temp.path().join(name)
    }

    fn write_skill(
        temp: &tempfile::TempDir,
        name: &str,
        front_matter: &str,
        body: &str,
    ) -> std::path::PathBuf {
        let dir = skill_dir(temp, name);
        fs::create_dir_all(&dir).unwrap();
        let content = if front_matter.is_empty() {
            body.to_string()
        } else {
            format!("---\n{front_matter}\n---\n{body}")
        };
        fs::write(dir.join("SKILL.md"), content).unwrap();
        dir
    }

    #[test]
    fn parses_valid_skill_with_name_description_and_version() {
        let temp = tempfile::tempdir().unwrap();
        let dir = write_skill(
            &temp,
            "my-skill",
            "name: my-skill\ndescription: Does a thing\nversion: 1.2.3",
            "# My Skill\n",
        );
        let skill = parse_skill(&dir).unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "Does a thing");
        assert_eq!(skill.version.as_deref(), Some("1.2.3"));
        assert_eq!(skill.path, dir);
    }

    #[test]
    fn parses_skill_without_version() {
        let temp = tempfile::tempdir().unwrap();
        let dir = write_skill(
            &temp,
            "no-version",
            "name: no-version\ndescription: Missing version is allowed",
            "",
        );
        let skill = parse_skill(&dir).unwrap();
        assert_eq!(skill.name, "no-version");
        assert_eq!(skill.version, None);
    }

    #[test]
    fn quoted_metadata_values_are_unquoted() {
        let temp = tempfile::tempdir().unwrap();
        let dir = write_skill(
            &temp,
            "quoted",
            "name: quoted\ndescription: \"double quoted\"",
            "",
        );
        let skill = parse_skill(&dir).unwrap();
        assert_eq!(skill.description, "double quoted");
    }

    #[test]
    fn missing_skill_md_fails() {
        let temp = tempfile::tempdir().unwrap();
        let dir = skill_dir(&temp, "empty");
        fs::create_dir_all(&dir).unwrap();
        assert!(parse_skill(&dir).is_err());
    }

    #[test]
    fn missing_front_matter_delimiter_fails() {
        let temp = tempfile::tempdir().unwrap();
        let dir = skill_dir(&temp, "raw");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "no front matter here\n").unwrap();
        assert!(parse_skill(&dir).is_err());
    }

    #[test]
    fn missing_required_metadata_fails() {
        let temp = tempfile::tempdir().unwrap();
        // no name
        let no_name = write_skill(&temp, "no-name", "description: d", "");
        assert!(parse_skill(&no_name).is_err());
        // no description
        let no_desc = write_skill(&temp, "no-desc", "name: no-desc", "");
        assert!(parse_skill(&no_desc).is_err());
    }

    #[test]
    fn invalid_names_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["", "UPPER", "with space", "with/slash", "d\u{f6}t"] {
            let dir = write_skill(
                &temp,
                "holder",
                &format!("name: {name}\ndescription: d"),
                "",
            );
            let skill = parse_skill(&dir);
            if name.is_empty() {
                assert!(skill.is_err(), "empty name must fail");
            } else {
                let error = skill.expect_err("invalid name must fail");
                assert!(
                    error.to_string().contains("invalid skill name"),
                    "unexpected error: {error}"
                );
            }
        }
    }

    #[test]
    fn long_names_over_limit_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let long = "a".repeat(101);
        let dir = write_skill(&temp, "long", &format!("name: {long}\ndescription: d"), "");
        assert!(parse_skill(&dir).is_err());
    }

    #[test]
    fn front_matter_without_closing_delimiter_still_reads_to_eof() {
        let temp = tempfile::tempdir().unwrap();
        let dir = skill_dir(&temp, "unterminated");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            "---\nname: unterminated\ndescription: d",
        )
        .unwrap();
        // keys are still collected because parsing scans lines to EOF
        let skill = parse_skill(&dir).unwrap();
        assert_eq!(skill.name, "unterminated");
    }

    #[test]
    fn unknown_metadata_keys_are_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let dir = write_skill(
            &temp,
            "extra-keys",
            "name: extra-keys\ndescription: d\nauthor: someone\nunknown_key: value",
            "",
        );
        let skill = parse_skill(&dir).unwrap();
        assert_eq!(skill.name, "extra-keys");
        assert_eq!(skill.description, "d");
    }
}
