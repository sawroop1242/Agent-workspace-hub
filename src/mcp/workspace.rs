use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// A single file entry within the workspace, with path and byte size.
#[derive(Debug, Serialize)]
pub struct WorkspaceFile {
    pub path: String,
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
}
