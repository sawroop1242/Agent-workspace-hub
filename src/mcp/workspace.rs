use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum size, in bytes, of content writable via [`WorkspaceMcp::write_file`].
const MAX_WRITE_FILE_BYTES: usize = 5 * 1024 * 1024;

/// A single file entry within the workspace, with path and byte size.
#[derive(Debug, Serialize)]
pub struct WorkspaceFile {
    /// Path relative to the workspace root.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
}

/// Project-scoped MCP workspace access with path-traversal protection.
pub struct WorkspaceMcp {
    root: PathBuf,
}

impl WorkspaceMcp {
    /// Creates a workspace handle, canonicalizing and validating the root exists.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root
            .into()
            .canonicalize()
            .context("workspace root does not exist")?;
        Ok(Self { root })
    }

    /// Returns the canonical workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reads the concatenated contents of `AGENTS.md`, `AGENT.md`, and `README.md`.
    pub fn context(&self) -> Result<String> {
        let mut out = String::new();
        for name in ["AGENTS.md", "AGENT.md", "README.md"] {
            let path = self.root.join(name);
            if path.is_file() {
                out.push_str(&format!("\n## {name}\n{}\n", fs::read_to_string(path)?));
            }
        }
        Ok(out)
    }

    /// Lists files (not directories) under a workspace subdirectory.
    pub fn list_files(&self, relative: &str) -> Result<Vec<WorkspaceFile>> {
        let dir = self.safe_path(relative)?;
        if !dir.is_dir() {
            bail!("workspace path is not a directory");
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                files.push(WorkspaceFile {
                    path: path.strip_prefix(&self.root)?.display().to_string(),
                    size: fs::metadata(path)?.len(),
                });
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    /// Reads a workspace file as UTF-8, bounded to 2 MiB.
    pub fn read_file(&self, relative: &str) -> Result<String> {
        if relative.trim().is_empty() {
            bail!("workspace path must not be empty");
        }
        let path = self.safe_path(relative)?;
        if !path.is_file() {
            bail!("workspace file not found");
        }
        let metadata = fs::metadata(&path)?;
        if metadata.len() > 2 * 1024 * 1024 {
            bail!("workspace file exceeds 2 MiB limit");
        }
        fs::read_to_string(path).context("workspace file is not valid UTF-8")
    }

    /// Writes (creates or overwrites) a workspace file, bounded to 5 MiB.
    ///
    /// The write is atomic (temp file + rename), the same pattern the JSON
    /// stores use, so a reader never observes a torn write. Unlike those
    /// stores a single file overwrite is not a read-modify-write cycle over
    /// shared state, so no [`crate::mcp::store_lock::StoreLock`] is taken.
    pub fn write_file(&self, relative: &str, content: &str) -> Result<()> {
        if relative.trim().is_empty() {
            bail!("workspace path must not be empty");
        }
        if content.len() > MAX_WRITE_FILE_BYTES {
            bail!("content exceeds {MAX_WRITE_FILE_BYTES} bytes");
        }
        let path = self.safe_new_path(relative)?;
        let parent = path.parent().context("target has no parent directory")?;
        fs::create_dir_all(parent)?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).context("failed to create temp file")?;
        std::io::Write::write_all(&mut temp, content.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&path)
            .map_err(|error| error.error)
            .context("failed to atomically write workspace file")?;
        Ok(())
    }

    /// Deletes a workspace file, returning whether it existed. Traversal and
    /// out-of-root targets are rejected (fail closed); a merely missing file
    /// is a no-op reported as `false`.
    pub fn delete_file(&self, relative: &str) -> Result<bool> {
        if relative.trim().is_empty() {
            bail!("workspace path must not be empty");
        }
        let path = self.safe_new_path(relative)?;
        if !path.is_file() {
            return Ok(false);
        }
        fs::remove_file(&path)?;
        Ok(true)
    }

    fn safe_path(&self, relative: &str) -> Result<PathBuf> {
        let candidate = Path::new(relative);
        if candidate.is_absolute()
            || candidate.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            bail!("unsafe workspace path");
        }
        let joined = self.root.join(candidate);
        let canonical = joined
            .canonicalize()
            .context("workspace path does not exist")?;
        if !canonical.starts_with(&self.root) {
            bail!("workspace path escapes project root");
        }
        Ok(canonical)
    }

    /// [`Self::safe_path`] for a path that need not exist yet (the write
    /// case): traversal components are rejected the same way, then the
    /// deepest *existing* ancestor is canonicalized so a symlinked
    /// directory cannot redirect the write outside the root, while the
    /// still-missing tail is kept as plain components.
    fn safe_new_path(&self, relative: &str) -> Result<PathBuf> {
        let candidate = Path::new(relative);
        if candidate.is_absolute()
            || candidate.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
            || candidate.components().next().is_none()
        {
            bail!("unsafe workspace path");
        }
        let joined = self.root.join(candidate);
        // An existing target must resolve inside the root — this is the
        // symlink-in-final-position check.
        if let Ok(canonical) = joined.canonicalize() {
            if !canonical.starts_with(&self.root) {
                bail!("workspace path escapes project root");
            }
            return Ok(canonical);
        }
        // New target: walk upward to the deepest existing ancestor, keeping
        // the missing components as a relative tail. Joining the
        // canonicalized ancestor with the collected components keeps the
        // result free of trailing-slash artifacts (an empty `PathBuf` join
        // would otherwise introduce one).
        let mut components: Vec<std::ffi::OsString> = Vec::new();
        let mut probe = joined.as_path();
        loop {
            match probe.canonicalize() {
                Ok(existing) => {
                    if !existing.starts_with(&self.root) {
                        bail!("workspace path escapes project root");
                    }
                    let mut result = existing;
                    for component in components.iter().rev() {
                        result = result.join(component);
                    }
                    return Ok(result);
                }
                Err(_) => match probe.file_name() {
                    Some(name) => {
                        components.push(name.to_os_string());
                        probe = probe.parent().unwrap_or(probe);
                    }
                    None => bail!("workspace path does not resolve under the root"),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_with_file(name: &str, contents: &str) -> (WorkspaceMcp, tempfile::TempDir) {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(name), contents).unwrap();
        let workspace = WorkspaceMcp::new(temp.path().to_path_buf()).unwrap();
        (workspace, temp)
    }

    #[test]
    fn write_file_round_trips_through_read_file() {
        let (workspace, _temp) = workspace_with_file("README.md", "old");
        workspace.write_file("README.md", "new contents").unwrap();
        assert_eq!(workspace.read_file("README.md").unwrap(), "new contents");
    }

    #[test]
    fn write_file_creates_missing_parent_directories() {
        let (workspace, _temp) = workspace_with_file("seed.txt", "s");
        workspace
            .write_file("docs/deep/nested/notes.md", "# notes\n")
            .unwrap();
        assert_eq!(
            workspace.read_file("docs/deep/nested/notes.md").unwrap(),
            "# notes\n"
        );
    }

    #[test]
    fn write_file_rejects_oversized_content() {
        let (workspace, _temp) = workspace_with_file("seed.txt", "s");
        let oversized = "x".repeat(MAX_WRITE_FILE_BYTES + 1);
        let error = workspace.write_file("big.txt", &oversized).unwrap_err();
        assert!(error.to_string().contains("exceeds"), "unexpected: {error}");
        // The rejected write must not have left a partial file behind.
        assert!(!workspace.root.join("big.txt").exists());
    }

    #[test]
    fn write_file_accepts_content_at_exactly_the_limit() {
        let (workspace, _temp) = workspace_with_file("seed.txt", "s");
        let at_limit = "x".repeat(MAX_WRITE_FILE_BYTES);
        workspace.write_file("at-limit.txt", &at_limit).unwrap();
        // Verified via filesystem metadata rather than read_file: the read
        // cap (2 MiB) is intentionally tighter than the write cap.
        assert_eq!(
            std::fs::metadata(workspace.root.join("at-limit.txt"))
                .unwrap()
                .len() as usize,
            MAX_WRITE_FILE_BYTES
        );
    }

    #[test]
    fn write_and_read_reject_traversal_and_absolute_paths() {
        let (workspace, temp) = workspace_with_file("seed.txt", "s");
        // A real outside-root file the traversal would otherwise hit.
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("outside.txt");
        std::fs::write(&target, "secret").unwrap();

        for path in ["../escape.txt", "a/../../escape.txt", "/etc/passwd", ""] {
            assert!(
                workspace.write_file(path, "data").is_err(),
                "write_file({path:?}) must be rejected"
            );
            assert!(
                workspace.read_file(path).is_err(),
                "read_file({path:?}) must be rejected"
            );
            assert!(
                workspace.delete_file(path).is_err(),
                "delete_file({path:?}) must be rejected"
            );
        }
        // And nothing escaped the root while checking.
        assert!(!temp.path().join("../escape.txt").exists());
        assert!(!target.parent().unwrap().join("escape.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn write_file_cannot_escape_root_through_a_symlinked_directory() {
        let (workspace, _temp) = workspace_with_file("seed.txt", "s");
        let outside = tempfile::tempdir().unwrap();
        let outside_dir = outside.path().join("outside_dir");
        std::fs::create_dir(&outside_dir).unwrap();

        let link = workspace.root.join("link");
        std::os::unix::fs::symlink(&outside_dir, &link).unwrap();
        let error = workspace.write_file("link/new.txt", "escaped").unwrap_err();
        assert!(
            error.to_string().contains("escapes project root"),
            "unexpected: {error}"
        );
        assert!(!outside_dir.join("new.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn read_and_delete_cannot_follow_symlinks_out_of_the_root() {
        let (workspace, _temp) = workspace_with_file("seed.txt", "s");
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("secret.txt");
        std::fs::write(&outside_file, "secret").unwrap();

        let link = workspace.root.join("leak.txt");
        std::os::unix::fs::symlink(&outside_file, &link).unwrap();
        assert!(workspace.read_file("leak.txt").is_err());
        // Deleting through the symlink must also fail closed, and must not
        // have removed the file it points at.
        assert!(workspace.delete_file("leak.txt").is_err());
        assert!(outside_file.exists());
    }

    #[test]
    fn delete_file_reports_existence_and_removes_the_file() {
        let (workspace, _temp) = workspace_with_file("doomed.txt", "bye");
        assert!(workspace.delete_file("doomed.txt").unwrap());
        assert!(!workspace.root.join("doomed.txt").exists());
        // A second delete of the same (now missing) file reports false.
        assert!(!workspace.delete_file("doomed.txt").unwrap());
        // A directory is not a file; deleting it reports false, not an error.
        std::fs::create_dir(workspace.root.join("dir")).unwrap();
        assert!(!workspace.delete_file("dir").unwrap());
    }
}
