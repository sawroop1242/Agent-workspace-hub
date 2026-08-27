use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Expected SHA-256 digest used to integrity-check downloaded packages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageIntegrity {
    pub sha256: String,
}

/// Validates an MCP id: 1-128 chars of `[A-Za-z0-9._-]`.
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 128 {
        bail!("MCP id must contain 1-128 characters");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        bail!("MCP id contains unsupported characters");
    }
    Ok(())
}

/// Validates an MCP command string: non-empty and free of control characters.
pub fn validate_command(command: Option<&str>) -> Result<()> {
    if let Some(command) = command {
        let command = command.trim();
        if command.is_empty() {
            bail!("MCP command cannot be empty");
        }
        if command.contains(['\n', '\r', '\0']) {
            bail!("MCP command contains invalid control characters");
        }
    }
    Ok(())
}

/// Validates an MCP URL, permitting only `http`/`https` schemes.
pub fn validate_url(url: Option<&str>) -> Result<()> {
    if let Some(url) = url {
        let parsed = reqwest::Url::parse(url)?;
        match parsed.scheme() {
            "http" | "https" => {}
            other => bail!("unsupported MCP URL scheme: {other}"),
        }
    }
    Ok(())
}

/// Resolve a user-controlled path beneath `base` without allowing traversal or symlink escape.
pub fn secure_path(base: impl AsRef<Path>, candidate: impl AsRef<Path>) -> Result<PathBuf> {
    let base = fs::canonicalize(base.as_ref()).context("failed to canonicalize security base")?;
    let candidate = candidate.as_ref();
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    };
    let resolved = if joined.exists() {
        fs::canonicalize(&joined).context("failed to canonicalize target path")?
    } else {
        let parent = joined.parent().context("target has no parent directory")?;
        let parent = fs::canonicalize(parent).context("failed to canonicalize target parent")?;
        parent.join(joined.file_name().context("target has no file name")?)
    };
    if !resolved.starts_with(&base) {
        bail!("path escapes allowed security directory");
    }
    Ok(resolved)
}

/// Validate a destination beneath `base`, including canonicalization of its parent.
pub fn secure_destination(
    base: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<PathBuf> {
    let base = fs::canonicalize(base.as_ref()).context("failed to canonicalize security base")?;
    let resolved = secure_path(&base, destination)?;
    if let Some(parent) = resolved.parent() {
        let parent =
            fs::canonicalize(parent).context("failed to canonicalize destination parent")?;
        if !parent.starts_with(&base) {
            bail!("destination parent escapes allowed security directory");
        }
    }
    Ok(resolved)
}

/// Atomically write a file inside `base` using a temporary file in the validated parent.
pub fn atomic_write(
    base: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    contents: &[u8],
) -> Result<()> {
    let target = secure_destination(&base, destination)?;
    let parent = target.parent().context("destination has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).context("failed to create temporary file")?;
    std::io::Write::write_all(&mut temp, contents)?;
    temp.as_file().sync_all()?;
    temp.persist(&target)
        .map_err(|error| error.error)
        .context("failed to atomically install file")?;
    Ok(())
}

pub fn sha256_file(path: impl AsRef<Path>) -> Result<String> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Verifies a file's SHA-256 digest matches `expected` (case-insensitive).
pub fn verify_sha256(path: impl AsRef<Path>, expected: &str) -> Result<()> {
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("MCP integrity check failed: SHA-256 mismatch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_parent_traversal() {
        let dir = tempdir().unwrap();
        assert!(secure_path(dir.path(), "../outside.txt").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        assert!(secure_path(dir.path(), link.join("secret.txt")).is_err());
    }

    #[test]
    fn allows_nested_destination_and_writes_atomically() {
        let dir = tempdir().unwrap();
        let destination = dir.path().join("skills/example/SKILL.md");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        atomic_write(dir.path(), "skills/example/SKILL.md", b"safe").unwrap();
        assert_eq!(fs::read_to_string(destination).unwrap(), "safe");
    }
}
