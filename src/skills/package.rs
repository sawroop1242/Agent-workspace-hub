use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Computes the SHA-256 digest of a file as hex.
pub fn sha256_file(path: impl AsRef<Path>) -> Result<String> {
    let data = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(data)))
}

/// Validates a skill package directory: it must contain a `SKILL.md` bounded
/// to 1 MiB.
pub fn validate_skill_package(dir: impl AsRef<Path>) -> Result<()> {
    let dir = dir.as_ref();
    if !dir.is_dir() {
        bail!("skill package is not a directory");
    }
    let skill_md = dir.join("SKILL.md");
    if !skill_md.is_file() {
        bail!("skill package must contain SKILL.md");
    }
    let metadata = fs::metadata(&skill_md).context("cannot inspect SKILL.md")?;
    if metadata.len() > 1024 * 1024 {
        bail!("SKILL.md exceeds 1 MiB limit");
    }
    Ok(())
}

/// Joins a relative path beneath `root`, rejecting absolute paths and
/// parent-directory traversal.
pub fn safe_package_path(root: impl AsRef<Path>, relative: impl AsRef<Path>) -> Result<PathBuf> {
    let relative = relative.as_ref();
    if relative.is_absolute()
        || relative.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        bail!("unsafe package path");
    }
    Ok(root.as_ref().join(relative))
}
