//! MCP workspace plane (ARCH-001): a thin adapter over the canonical
//! [`FilesService`].
//!
//! This module owns no filesystem business rules. Containment, size caps,
//! atomicity, and listing semantics live in [`crate::services::files`];
//! the MCP tool handlers in the dispatcher call these methods, which only
//! translate protocol-level concerns (response size bounds, the
//! `WorkspaceFile` wire shape) onto canonical service operations. Two
//! read-only discovery helpers that have no service-level counterpart
//! (`context`, `root`) stay here because they touch no persistent state.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::services::files::FilesService;

/// Protocol-level response bound for `workspace.read_file`: 2 MiB of
/// UTF-8 text. The canonical service still enforces its own (larger)
/// read cap underneath; this bound keeps MCP responses bounded for
/// JSON-RPC transports.
const MAX_READ_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Protocol-level request bound for `workspace.write_file`: 5 MiB of
/// content. The canonical service's own cap applies underneath as well.
pub const MAX_WRITE_FILE_BYTES: usize = 5 * 1024 * 1024;

/// One file entry in a `workspace.list_files` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFile {
    /// Path relative to the workspace root, forward-slash separated.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
}

/// MCP workspace tool adapter: canonicalizes the root once and delegates
/// every file operation to [`FilesService`].
pub struct WorkspaceMcp {
    files: FilesService,
    root: PathBuf,
}

impl WorkspaceMcp {
    /// Creates a workspace handle, canonicalizing and validating the root exists.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root
            .into()
            .canonicalize()
            .context("workspace root does not exist")?;
        Ok(Self {
            files: FilesService::new(&root),
            root,
        })
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
        let prefix = normalize_prefix(relative);
        let mut files = Vec::new();
        for entry in self.files.list(relative)? {
            if entry.is_dir {
                continue;
            }
            let path = match &prefix {
                Some(prefix) => format!("{prefix}/{}", entry.name),
                None => entry.name,
            };
            files.push(WorkspaceFile {
                path,
                size: entry.size.unwrap_or(0),
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    /// Reads a workspace file as UTF-8, bounded to 2 MiB of response text.
    ///
    /// A missing file surfaces as the protocol-level error `workspace file
    /// not found` (detected by inspecting the canonical error's root cause
    /// for `io::ErrorKind::NotFound`, never by string matching), while a
    /// containment violation or unreadable file propagates the canonical
    /// service error unchanged.
    pub fn read_file(&self, relative: &str) -> Result<String> {
        if relative.trim().is_empty() {
            bail!("workspace path must not be empty");
        }
        let content = match self.files.read(relative) {
            Ok(content) => content,
            Err(error) if root_cause_is_not_found(&error) => {
                bail!("workspace file not found");
            }
            Err(error) => return Err(error),
        };
        if content.len() > MAX_READ_RESPONSE_BYTES {
            bail!("workspace file exceeds 2 MiB limit");
        }
        Ok(content)
    }

    /// Writes (creates or overwrites) a workspace file, bounded to 5 MiB.
    ///
    /// Delegates to the canonical atomic write (temp file + fsync + rename)
    /// so an MCP write and a Control API write to the same path commit
    /// through the exact same code path. The canonical service runs the
    /// FS-001 coordination boundary (acquire per-resource set, re-validate
    /// the path under the lock) before staging and renaming.
    pub fn write_file(&self, relative: &str, content: &str) -> Result<()> {
        if relative.trim().is_empty() {
            bail!("workspace path must not be empty");
        }
        if content.len() > MAX_WRITE_FILE_BYTES {
            bail!("content exceeds {MAX_WRITE_FILE_BYTES} bytes");
        }
        self.files.write_atomic(relative, content)
    }

    /// Deletes a workspace file, returning whether it existed. Containment
    /// and validation come from the canonical service; a merely missing
    /// file is a no-op reported as `false`.
    pub fn delete_file(&self, relative: &str) -> Result<bool> {
        if relative.trim().is_empty() {
            bail!("workspace path must not be empty");
        }
        self.files.delete_if_exists(relative)
    }
}

/// Maps a directory argument to the path prefix the wire format expects:
/// `""` and `"."` mean the root (no prefix), anything else keeps its
/// forward-slash form with any trailing slash trimmed.
fn normalize_prefix(relative: &str) -> Option<String> {
    let trimmed = relative.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// Whether any error in the chain is an I/O "not found" — the canonical
/// service wraps the underlying `io::Error` with context, so the kind has
/// to be inspected rather than the message text.
fn root_cause_is_not_found(error: &anyhow::Error) -> bool {
    use std::io::ErrorKind;
    for cause in error.chain() {
        if let Some(io_error) = cause.downcast_ref::<std::io::Error>() {
            if matches!(io_error.kind(), ErrorKind::NotFound) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_with_file(name: &str, contents: &str) -> (WorkspaceMcp, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(name), contents).unwrap();
        let mcp = WorkspaceMcp::new(dir.path()).unwrap();
        (mcp, dir)
    }

    #[test]
    fn write_file_round_trips_through_read_file() {
        let (mcp, dir) = workspace_with_file("existing.txt", "old");
        mcp.write_file("existing.txt", "new contents").unwrap();
        assert_eq!(mcp.read_file("existing.txt").unwrap(), "new contents");
        assert!(dir.path().join("existing.txt").is_file());
    }

    #[test]
    fn write_file_creates_missing_parent_directories() {
        let (mcp, dir) = workspace_with_file("root.txt", "x");
        mcp.write_file("docs/notes.txt", "deep").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("docs/notes.txt")).unwrap(),
            "deep"
        );
    }

    #[test]
    fn write_file_rejects_oversized_content() {
        let (mcp, _dir) = workspace_with_file("root.txt", "x");
        let oversized = "y".repeat(MAX_WRITE_FILE_BYTES + 1);
        assert!(mcp.write_file("big.bin", &oversized).is_err());
        // but exactly at the limit is fine
        let at_limit = "y".repeat(MAX_WRITE_FILE_BYTES);
        assert!(mcp.write_file("at-limit.bin", &at_limit).is_ok());
    }

    #[test]
    fn write_and_read_reject_traversal_and_absolute_paths() {
        let (mcp, dir) = workspace_with_file("inside.txt", "data");
        std::fs::write(dir.path().join("sibling.txt"), "outside").unwrap();

        assert!(mcp.write_file("../escape.txt", "x").is_err());
        assert!(mcp.write_file("a/../../escape.txt", "x").is_err());
        assert!(mcp.write_file("/etc/passwd", "x").is_err());

        assert!(mcp.read_file("../sibling.txt").is_err());
        assert!(mcp.read_file("/etc/passwd").is_err());

        // and the escapes never landed
        assert!(!dir.path().join("../escape.txt").exists());
    }

    #[test]
    fn write_file_cannot_escape_root_through_a_symlinked_directory() {
        let outside = tempfile::tempdir().unwrap();
        let (mcp, dir) = workspace_with_file("root.txt", "x");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(outside.path(), dir.path().join("link")).unwrap();

        assert!(mcp.write_file("link/escape.txt", "x").is_err());
        assert!(!outside.path().join("escape.txt").exists());
    }

    #[test]
    fn read_and_delete_cannot_follow_symlinks_out_of_the_root() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        let (mcp, dir) = workspace_with_file("root.txt", "x");
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            dir.path().join("secret-link.txt"),
        )
        .unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(
            outside.path().join("secret.txt"),
            dir.path().join("secret-link.txt"),
        )
        .unwrap();

        assert!(mcp.read_file("secret-link.txt").is_err());
        assert!(mcp.delete_file("secret-link.txt").is_err());
        // the target is untouched
        assert!(outside.path().join("secret.txt").is_file());
    }

    #[test]
    fn delete_file_reports_existence_and_removes_the_file() {
        let (mcp, dir) = workspace_with_file("doomed.txt", "bye");
        assert!(mcp.delete_file("doomed.txt").unwrap());
        assert!(!dir.path().join("doomed.txt").exists());
        assert!(!mcp.delete_file("doomed.txt").unwrap());
        assert!(!mcp.delete_file("never-existed.txt").unwrap());
    }

    #[test]
    fn delete_file_rejects_traversal_and_absolute_paths() {
        let (mcp, _dir) = workspace_with_file("root.txt", "x");
        assert!(mcp.delete_file("../x.txt").is_err());
        assert!(mcp.delete_file("/etc/passwd").is_err());
        assert!(mcp.delete_file("a/../../x.txt").is_err());
    }

    #[test]
    fn empty_and_whitespace_paths_are_rejected() {
        let (mcp, _dir) = workspace_with_file("root.txt", "x");
        assert!(mcp.read_file("").is_err());
        assert!(mcp.write_file("", "x").is_err());
        assert!(mcp.delete_file("").is_err());
        assert!(mcp.read_file("   ").is_err());
    }

    #[test]
    fn read_file_of_a_directory_is_an_error() {
        let (mcp, dir) = workspace_with_file("root.txt", "x");
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        assert!(mcp.read_file("subdir").is_err());
    }

    #[test]
    fn read_file_missing_reports_workspace_file_not_found() {
        let (mcp, _dir) = workspace_with_file("root.txt", "x");
        let error = mcp.read_file("no-such-file.txt").unwrap_err().to_string();
        assert!(
            error.contains("workspace file not found"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn list_files_reports_paths_relative_to_the_root_files_only_sorted() {
        let (mcp, dir) = workspace_with_file("b.txt", "two");
        std::fs::write(dir.path().join("a.txt"), "one").unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        std::fs::write(dir.path().join("nested/c.txt"), "three").unwrap();

        let root_files = mcp.list_files(".").unwrap();
        assert_eq!(root_files.len(), 2);
        assert_eq!(root_files[0].path, "a.txt");
        assert_eq!(root_files[0].size, 3);
        assert_eq!(root_files[1].path, "b.txt");

        let nested = mcp.list_files("nested").unwrap();
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].path, "nested/c.txt");
        assert_eq!(nested[0].size, 5);
    }

    #[test]
    fn list_files_of_a_missing_directory_is_an_error() {
        let (mcp, _dir) = workspace_with_file("root.txt", "x");
        assert!(mcp.list_files("missing-dir").is_err());
    }

    #[test]
    fn list_files_of_a_file_is_an_error() {
        let (mcp, _dir) = workspace_with_file("plain.txt", "x");
        assert!(mcp.list_files("plain.txt").is_err());
    }

    #[test]
    fn context_reads_instruction_files_in_a_fixed_order() {
        let (mcp, dir) = workspace_with_file("README.md", "readme body");
        std::fs::write(dir.path().join("AGENTS.md"), "agents body").unwrap();
        let context = mcp.context().unwrap();
        let agents = context.find("agents body").unwrap();
        let readme = context.find("readme body").unwrap();
        assert!(agents < readme);
    }

    #[test]
    fn context_without_instruction_files_is_empty() {
        let (mcp, _dir) = workspace_with_file("other.txt", "x");
        assert!(mcp.context().unwrap().trim().is_empty());
    }

    #[test]
    fn oversized_read_is_refused_even_though_the_service_allows_it() {
        let (mcp, dir) = workspace_with_file("root.txt", "x");
        // 3 MiB: allowed by the canonical service (8 MiB cap) but above the
        // MCP protocol response bound (2 MiB).
        std::fs::write(dir.path().join("big.txt"), vec![b'a'; 3 * 1024 * 1024]).unwrap();
        let error = mcp.read_file("big.txt").unwrap_err().to_string();
        assert!(error.contains("2 MiB"), "unexpected error: {error}");
    }
}
