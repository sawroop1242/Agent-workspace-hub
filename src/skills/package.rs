use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub fn sha256_file(path: impl AsRef<Path>) -> Result<String> {
    let data = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(data)))
}

pub fn validate_skill_package(dir: impl AsRef<Path>) -> Result<()> {
    let dir = dir.as_ref();
    if !dir.is_dir() { bail!("skill package is not a directory"); }
    let skill_md = dir.join("SKILL.md");
    if !skill_md.is_file() { bail!("skill package must contain SKILL.md"); }
    let metadata = fs::metadata(&skill_md).context("cannot inspect SKILL.md")?;
    if metadata.len() > 1024 * 1024 { bail!("SKILL.md exceeds 1 MiB limit"); }
    Ok(())
}

pub fn safe_package_path(root: impl AsRef<Path>, relative: impl AsRef<Path>) -> Result<PathBuf> {
    let relative = relative.as_ref();
    if relative.is_absolute() || relative.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Root | std::path::Component::Prefix(_))) {
        bail!("unsafe package path");
    }
    Ok(root.as_ref().join(relative))
}
