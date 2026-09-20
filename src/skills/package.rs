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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_file_matches_known_digest() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("data.bin");
        // sha256("awh") = 04fd58696ae34bb287f...
        fs::write(&path, b"awh").unwrap();
        let digest = sha256_file(&path).unwrap();
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
        // known digest of the empty file for an empty file
        fs::write(temp.path().join("empty.bin"), b"").unwrap();
        assert_eq!(
            sha256_file(temp.path().join("empty.bin")).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_file_fails_on_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        assert!(sha256_file(temp.path().join("missing.bin")).is_err());
    }

    #[test]
    fn validate_skill_package_accepts_dir_with_skill_md() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("skill");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "---\nname: x\ndescription: d\n---\n").unwrap();
        assert!(validate_skill_package(&dir).is_ok());
    }

    #[test]
    fn validate_skill_package_rejects_missing_and_wrong_types() {
        let temp = tempfile::tempdir().unwrap();
        // not a directory
        let file = temp.path().join("plain.txt");
        fs::write(&file, "hi").unwrap();
        assert!(validate_skill_package(&file).is_err());
        // directory without SKILL.md
        let empty = temp.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(validate_skill_package(&empty).is_err());
    }

    #[test]
    fn validate_skill_package_rejects_oversized_skill_md() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("big");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
        let error = validate_skill_package(&dir).unwrap_err();
        assert!(error.to_string().contains("1 MiB"));
    }

    #[test]
    fn validate_skill_package_accepts_exactly_one_mib() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("edge");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), vec![b'x'; 1024 * 1024]).unwrap();
        assert!(validate_skill_package(&dir).is_ok());
    }

    #[test]
    fn safe_package_path_joins_relative_paths() {
        assert_eq!(
            safe_package_path("/pkg", "skills/alpha/SKILL.md").unwrap(),
            PathBuf::from("/pkg/skills/alpha/SKILL.md")
        );
        // plain filename is fine
        assert_eq!(
            safe_package_path("/pkg", "SKILL.md").unwrap(),
            PathBuf::from("/pkg/SKILL.md")
        );
        // curiously-named files that only look like traversal are fine
        assert_eq!(
            safe_package_path("/pkg", "..hidden").unwrap(),
            PathBuf::from("/pkg/..hidden")
        );
    }

    #[test]
    fn safe_package_path_rejects_traversal_and_absolute() {
        for relative in [
            "../escape",
            "a/../../escape",
            "/absolute/path",
            "a/../../../escape",
        ] {
            assert!(
                safe_package_path("/pkg", relative).is_err(),
                "{relative:?} must be rejected"
            );
        }
    }
}
