use crate::skills::{GlobalSkillRegistry, Skill};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub enum SkillRegistrySource {
    GitHub { repository: String, reference: String },
    Community { url: String },
}

impl SkillRegistrySource {
    pub fn parse(value: &str) -> Result<Self> {
        if let Some(repo) = value.strip_prefix("github:") {
            let mut parts = repo.splitn(2, '#');
            let repository = parts.next().unwrap_or_default().trim();
            let reference = parts.next().unwrap_or("main").trim();
            if repository.split('/').count() != 2 || repository.contains("..") {
                bail!("invalid GitHub skill repository: {repository}");
            }
            return Ok(Self::GitHub {
                repository: repository.to_owned(),
                reference: reference.to_owned(),
            });
        }
        if let Some(url) = value.strip_prefix("community:") {
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                bail!("community registry must use http(s)");
            }
            return Ok(Self::Community {
                url: url.trim_end_matches('/').to_owned(),
            });
        }
        bail!("registry must start with github: or community:")
    }
}

pub struct RemoteSkillRegistry {
    pub cache_dir: PathBuf,
}

impl RemoteSkillRegistry {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    pub fn install_github(
        &self,
        repository: &str,
        reference: &str,
        skill_name: &str,
        global: &GlobalSkillRegistry,
    ) -> Result<Skill> {
        let target = self
            .cache_dir
            .join("github")
            .join(repository.replace('/', "__"))
            .join(reference)
            .join(skill_name);
        if target.exists() {
            fs::remove_dir_all(&target)?;
        }
        fs::create_dir_all(&target)?;
        let url = format!("https://github.com/{repository}.git");
        let parent = target.parent().context("invalid cache target")?;
        let clone_target = parent.join("repo");
        if clone_target.exists() {
            fs::remove_dir_all(&clone_target)?;
        }
        let status = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--branch",
                reference,
                &url,
                clone_target.to_str().unwrap(),
            ])
            .status()?;
        if !status.success() {
            bail!("git clone failed for {repository}#{reference}");
        }
        let source = clone_target.join(skill_name);
        if !source.join("SKILL.md").is_file() {
            bail!("skill not found in repository: {skill_name}");
        }
        let installed = global.skills_dir().join(skill_name);
        if installed.exists() {
            fs::remove_dir_all(&installed)?;
        }
        copy_dir(&source, &installed)?;
        global
            .get(skill_name)?
            .context("installed skill could not be parsed")
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(from, to)?;
        }
    }
    Ok(())
}
