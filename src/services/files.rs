//! Files service: workspace-safe file operations shared by all interfaces.
//!
//! All paths are validated against the project root, including symlink
//! escape prevention: resolved paths (after following symlinks) must stay
//! inside the authorized project directory. Reads and writes enforce size
//! limits, and listings are bounded.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Maximum bytes for a single read/write.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum entries returned by a directory listing or search.
pub const MAX_LIST_ENTRIES: usize = 1000;

/// Files application service scoped to one project root.
pub struct FilesService {
    root: PathBuf,
}

impl FilesService {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            root: project_root.into(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolves `relative` under the root with full traversal and symlink
    /// escape checks, returning the canonicalized absolute path. An empty
    /// `relative` denotes the project root itself.
    fn resolve_checked(&self, relative: &str) -> Result<PathBuf> {
        let path = Path::new(relative);
        if path.is_absolute() {
            bail!("absolute paths are not allowed");
        }
        for component in path.components() {
            if matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            ) {
                bail!("path traversal is not allowed");
            }
        }
        if relative.is_empty() {
            return Ok(self.root.clone());
        }
        let joined = self.root.join(path);
        // Symlink escape: canonicalize the deepest existing ancestor and
        // require the resolved target to remain inside the project root.
        let root_canonical = self
            .root
            .canonicalize()
            .context("project root must exist")?;
        let target = joined.canonicalize().unwrap_or_else(|_| joined.clone());
        let mut ancestor = target.as_path();
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or_else(|| anyhow::anyhow!("path has no existing ancestor"))?;
        }
        let resolved = ancestor.canonicalize()?;
        if !resolved.starts_with(&root_canonical) {
            bail!("resolved path escapes the project root");
        }
        Ok(joined)
    }

    /// Reads a UTF-8 text file under the root, enforcing the size cap.
    pub fn read(&self, relative: &str) -> Result<String> {
        let path = self.resolve_checked(relative)?;
        let meta = fs::metadata(&path).with_context(|| format!("stat {}", path.display()))?;
        if meta.len() > MAX_FILE_BYTES {
            bail!("file exceeds the {} byte limit", MAX_FILE_BYTES);
        }
        fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
    }

    /// Writes a UTF-8 text file under the root, enforcing the size cap.
    pub fn write(&self, relative: &str, content: &str) -> Result<()> {
        let path = self.resolve_checked(relative)?;
        if content.len() as u64 > MAX_FILE_BYTES {
            bail!("content exceeds the {} byte limit", MAX_FILE_BYTES);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent of {}", path.display()))?;
        }
        fs::write(&path, content).with_context(|| format!("write {}", path.display()))
    }

    /// Deletes a file or (empty or not) directory under the root. The
    /// caller must have confirmed the destructive action.
    pub fn delete(&self, relative: &str) -> Result<()> {
        let path = self.resolve_checked(relative)?;
        if path == self.root {
            bail!("refusing to delete the project root");
        }
        if path.is_dir() {
            fs::remove_dir_all(&path).with_context(|| format!("delete dir {}", path.display()))
        } else {
            fs::remove_file(&path).with_context(|| format!("delete {}", path.display()))
        }
    }

    /// Renames/moves within the root. `to` must stay inside the root.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        let src = self.resolve_checked(from)?;
        let dst = self.resolve_checked(to)?;
        if !src.starts_with(&self.root) || !dst.starts_with(&self.root) {
            bail!("rename source or destination escapes the root");
        }
        if dst.exists() {
            bail!("destination already exists: {to}");
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&src, &dst).with_context(|| format!("rename {from} -> {to}"))
    }

    /// Creates a directory (with parents) under the root.
    pub fn create_dir(&self, relative: &str) -> Result<()> {
        let path = self.resolve_checked(relative)?;
        if path == self.root {
            bail!("refusing to create the project root");
        }
        fs::create_dir_all(&path).with_context(|| format!("mkdir {}", path.display()))
    }

    /// Lists directory entries under `relative`, bounded to
    /// [`MAX_LIST_ENTRIES`], each with kind and size.
    pub fn list(&self, relative: &str) -> Result<Vec<ListEntry>> {
        let dir = self.resolve_checked(relative)?;
        if !dir.is_dir() {
            bail!("not a directory: {relative}");
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&dir).with_context(|| format!("list {}", dir.display()))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let meta = entry.metadata()?;
            entries.push(ListEntry {
                name,
                is_dir: meta.is_dir(),
                size: if meta.is_dir() {
                    None
                } else {
                    Some(meta.len())
                },
            });
            if entries.len() >= MAX_LIST_ENTRIES {
                bail!("directory listing exceeded {} entries", MAX_LIST_ENTRIES);
            }
        }
        Ok(entries)
    }

    /// Case-insensitive substring search across project text files.
    /// Returns at most `limit` matches.
    pub fn search(&self, needle: &str, limit: usize) -> Result<Vec<SearchHit>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut hits = Vec::new();
        let mut queue: Vec<PathBuf> = vec![self.root.clone()];
        while let Some(dir) = queue.pop() {
            if hits.len() >= limit {
                break;
            }
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.filter_map(|e| e.ok()) {
                if hits.len() >= limit {
                    break;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == ".git" || name == ".agent" {
                    continue;
                }
                let path = entry.path();
                let Ok(meta) = fs::metadata(&path) else {
                    continue;
                };
                if meta.is_dir() {
                    queue.push(path);
                } else if meta.is_file() && meta.len() <= MAX_FILE_BYTES {
                    self.search_file(&path, &needle.to_lowercase(), limit, &mut hits);
                }
            }
        }
        Ok(hits)
    }

    /// Appends matches from one file, respecting the overall limit.
    fn search_file(
        &self,
        path: &Path,
        needle_lower: &str,
        limit: usize,
        hits: &mut Vec<SearchHit>,
    ) {
        let Ok(text) = fs::read_to_string(path) else {
            return; // binary or unreadable; skip
        };
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        for (idx, line) in text.lines().enumerate() {
            if hits.len() >= limit {
                return;
            }
            if line.to_lowercase().contains(needle_lower) {
                hits.push(SearchHit {
                    path: rel.clone(),
                    line_number: idx + 1,
                    line: line.chars().take(200).collect(),
                });
            }
        }
    }

    /// Describes a path for UIs and agents without reading its bytes:
    /// existence, kind, and size. The Editor uses this to refuse
    /// oversized files and to detect binaries before loading them.
    pub fn meta(&self, relative: &str) -> Result<FileMeta> {
        let path = self.resolve_checked(relative)?;
        let meta = fs::metadata(&path).with_context(|| format!("stat {}", path.display()))?;
        let kind = if meta.is_dir() {
            PathKind::Directory
        } else if is_probably_binary(&path)? {
            PathKind::BinaryFile
        } else {
            PathKind::TextFile
        };
        Ok(FileMeta {
            kind,
            size: meta.len(),
        })
    }
}

/// Kind of a path as seen by file-oriented UIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PathKind {
    Directory,
    TextFile,
    BinaryFile,
}

/// Metadata snapshot for one path.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct FileMeta {
    pub kind: PathKind,
    pub size: u64,
}

/// Reads the first 8 KiB and declares the file binary when it contains
/// a NUL byte or invalid UTF-8. Binary detection must stay cheap: it
/// runs on every file the Editor or listings probe.
fn is_probably_binary(path: &Path) -> Result<bool> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut head = [0u8; 8192];
    let mut filled = 0;
    while filled < head.len() {
        let n = file.read(&mut head[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    let slice = &head[..filled];
    Ok(std::str::from_utf8(slice).is_err() || slice.contains(&0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, FilesService) {
        let tmp = tempfile::tempdir().unwrap();
        let svc = FilesService::new(tmp.path().to_path_buf());
        (tmp, svc)
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        let (_tmp, svc) = setup();
        assert!(svc.read("../escape.txt").is_err());
        assert!(svc.read("/etc/passwd").is_err());
        assert!(svc.read("a/../../escape").is_err());
        assert!(svc.read("").is_err());
    }

    #[test]
    fn write_read_roundtrip() {
        let (_tmp, svc) = setup();
        svc.write("hello.txt", "hi").unwrap();
        assert_eq!(svc.read("hello.txt").unwrap(), "hi");
    }

    #[test]
    fn write_creates_parents() {
        let (_tmp, svc) = setup();
        svc.write("a/b/c.txt", "deep").unwrap();
        assert_eq!(svc.read("a/b/c.txt").unwrap(), "deep");
    }

    #[test]
    fn delete_and_rename_stay_inside_root() {
        let (_tmp, svc) = setup();
        svc.write("f.txt", "x").unwrap();
        svc.rename("f.txt", "g.txt").unwrap();
        assert!(svc.read("f.txt").is_err());
        assert_eq!(svc.read("g.txt").unwrap(), "x");
        svc.delete("g.txt").unwrap();
        assert!(svc.read("g.txt").is_err());
    }

    #[test]
    fn rename_rejects_existing_destination_and_escape() {
        let (_tmp, svc) = setup();
        svc.write("a.txt", "1").unwrap();
        svc.write("b.txt", "2").unwrap();
        assert!(svc.rename("a.txt", "b.txt").is_err());
        assert!(svc.rename("a.txt", "../out").is_err());
        assert!(svc.rename("../in", "a.txt").is_err());
    }

    #[test]
    fn symlink_escape_is_rejected() {
        let outer = tempfile::tempdir().unwrap();
        let inner = tempfile::tempdir().unwrap();
        let secret = outer.path().join("secret.txt");
        fs::write(&secret, "top secret").unwrap();

        let svc = FilesService::new(inner.path().to_path_buf());
        // Symlink pointing outside the project root.
        let link = inner.path().join("leak.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        #[cfg(windows)]
        {
            let _ = &link; // symlink creation needs privileges on Windows
        }

        #[cfg(unix)]
        {
            assert!(svc.read("leak.txt").is_err());
            assert!(svc.write("leak.txt", "poison").is_err());
        }
    }

    #[test]
    fn refuses_to_delete_root() {
        let (_tmp, svc) = setup();
        assert!(svc.delete("").is_err());
    }

    #[test]
    fn list_returns_entries_with_metadata() {
        let (_tmp, svc) = setup();
        svc.write("x.txt", "12345").unwrap();
        svc.create_dir("sub").unwrap();
        let entries = svc.list("").unwrap();
        assert_eq!(entries.len(), 2);
        let sub = entries.iter().find(|e| e.name == "sub").unwrap();
        assert!(sub.is_dir);
        let x = entries.iter().find(|e| e.name == "x.txt").unwrap();
        assert_eq!(x.size, Some(5));
    }

    #[test]
    fn search_finds_matches_with_line_numbers() {
        let (_tmp, svc) = setup();
        svc.write("one.txt", "alpha\nTarget Line\nbeta").unwrap();
        svc.write("two.txt", "gamma\ntarget lowercase").unwrap();
        let hits = svc.search("target", 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits
            .iter()
            .any(|h| h.path == "one.txt" && h.line_number == 2));
    }

    #[test]
    fn search_respects_limit() {
        let (_tmp, svc) = setup();
        svc.write("a.txt", "match\nmatch").unwrap();
        let hits = svc.search("match", 1).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn read_rejects_oversized_file() {
        let tmp = tempfile::tempdir().unwrap();
        let big = tmp.path().join("big.txt");
        let mut content = String::with_capacity(MAX_FILE_BYTES as usize + 1);
        while content.len() <= MAX_FILE_BYTES as usize {
            content.push('x');
        }
        fs::write(&big, content).unwrap();
        let svc = FilesService::new(tmp.path().to_path_buf());
        assert!(svc.read("big.txt").is_err());
    }

    #[test]
    fn meta_classifies_text_binary_and_directory() {
        let (tmp, svc) = setup();
        fs::write(tmp.path().join("text.md"), "hello").unwrap();
        fs::write(tmp.path().join("blob.bin"), b"ok\x00binary").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();

        assert_eq!(svc.meta("text.md").unwrap().kind, PathKind::TextFile);
        assert_eq!(svc.meta("blob.bin").unwrap().kind, PathKind::BinaryFile);
        assert_eq!(svc.meta("sub").unwrap().kind, PathKind::Directory);
        assert_eq!(svc.meta("text.md").unwrap().size, 5);
        assert!(svc.meta("missing.txt").is_err());
    }

    #[test]
    fn meta_reports_oversized_text_as_too_large() {
        let (tmp, svc) = setup();
        let big = tmp.path().join("big.txt");
        let mut content = String::with_capacity(MAX_FILE_BYTES as usize + 1);
        while content.len() <= MAX_FILE_BYTES as usize {
            content.push('x');
        }
        fs::write(&big, content).unwrap();
        let meta = svc.meta("big.txt").unwrap();
        assert_eq!(meta.kind, PathKind::TextFile);
        assert!(meta.size > MAX_FILE_BYTES);
    }

    #[test]
    fn rename_moves_and_refuses_existing_destination() {
        let (tmp, svc) = setup();
        fs::write(tmp.path().join("a.txt"), "1").unwrap();
        fs::write(tmp.path().join("b.txt"), "2").unwrap();
        svc.rename("a.txt", "renamed.txt").unwrap();
        assert!(svc.read("renamed.txt").is_ok());
        assert!(svc.read("a.txt").is_err());
        assert!(svc.rename("b.txt", "renamed.txt").is_err());
        assert!(svc.rename("renamed.txt", "../out.txt").is_err());
    }

    #[test]
    fn create_dir_roundtrip() {
        let (tmp, svc) = setup();
        svc.create_dir("deep/nested/dir").unwrap();
        assert!(tmp.path().join("deep/nested/dir").is_dir());
        assert!(svc.create_dir("").is_err());
    }
}

/// One directory entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
}

/// One matched line from a search.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub line_number: usize,
    pub line: String,
}
