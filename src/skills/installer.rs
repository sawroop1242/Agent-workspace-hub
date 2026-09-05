use crate::skills::{
    validate_sha256, validate_skill_package, GlobalSkillRegistry, RegistryClient, RegistrySkill,
};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Downloads and installs skills into the global registry from a remote
/// registry or a local directory.
pub struct SkillInstaller {
    cache_dir: PathBuf,
}

impl SkillInstaller {
    /// Creates an installer using the given cache directory.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    /// Downloads a skill from a remote registry and installs it globally,
    /// after validating the package and a safe download path.
    pub async fn install_from_registry(
        &self,
        client: &RegistryClient,
        skill_name: &str,
        registry: &GlobalSkillRegistry,
    ) -> Result<()> {
        let manifest = client.fetch_manifest().await?;
        let entry = manifest
            .skills
            .into_iter()
            .find(|s| s.name == skill_name)
            .context("skill not found in registry")?;

        if entry.path.starts_with('/') || entry.path.contains("..") {
            bail!("registry returned unsafe skill path");
        }

        let source_url = format!("{}/{}", client.base_url, entry.path.trim_start_matches('/'));
        let response = reqwest::get(&source_url)
            .await
            .context("failed to download skill")?;
        if !response.status().is_success() {
            bail!("skill download returned HTTP {}", response.status());
        }

        let bytes = response.bytes().await?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        validate_sha256(entry.sha256.as_deref(), &digest)?;
        let package_dir = self.cache_dir.join(&entry.name);
        if package_dir.exists() {
            fs::remove_dir_all(&package_dir)?;
        }
        fs::create_dir_all(&package_dir)?;
        let file = package_dir.join("SKILL.md");
        fs::write(&file, &bytes)?;
        validate_skill_package(&package_dir)?;

        let installed = registry.skills_dir().join(&entry.name);
        if installed.exists() {
            fs::remove_dir_all(&installed)?;
        }
        copy_dir(&package_dir, &installed)?;
        Ok(())
    }

    /// Installs a skill from a local directory into the global registry.
    pub fn install_from_local(
        &self,
        source: impl AsRef<Path>,
        registry: &GlobalSkillRegistry,
        name: &str,
    ) -> Result<()> {
        let source = source.as_ref();
        validate_skill_package(source)?;
        let installed = registry.skills_dir().join(name);
        if installed.exists() {
            fs::remove_dir_all(&installed)?;
        }
        copy_dir(source, &installed)
    }

    /// Returns the registry entry's skill name.
    pub fn entry_name(entry: &RegistrySkill) -> &str {
        &entry.name
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
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
