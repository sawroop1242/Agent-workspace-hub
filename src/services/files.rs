//! Files service: workspace-safe file operations shared by all interfaces.
//!
//! All paths are validated against the project root, including symlink
//! escape prevention: resolved paths (after following symlinks) must stay
//! inside the authorized project directory. Reads and writes enforce size
//! limits, and listings are bounded.
//!
//! Every mutation (write, atomic write, delete, rename, directory
//! creation) runs under the canonical filesystem coordination boundary
//! (FS-001, [`crate::core::fs_coordination`]): acquire the per-resource
//! coordination set, RE-VALIDATE the path against the root *under the
//! lock* (closing the validate→mutate race for AWH-conformant actors),
//! then mutate, then release. The `*_locked` variants are for callers
//! that already hold the coordination set (the edit service's
//! multi-file commit span) — they revalidate but skip acquisition, so
//! the single-owner rule holds without a reentrant lock.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::core::fs_coordination::{CoordinationSet, FsCoordinator};

/// Upper bound on symlink chain hops in containment validation; a longer
/// chain (or a loop) fails closed instead of recursing without limit.
const SYMLINK_FOLLOW_LIMIT: usize = 8;

/// Maximum bytes for a single read/write.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum entries returned by a directory listing or search.
pub const MAX_LIST_ENTRIES: usize = 1000;

/// Files application service scoped to one project root.
pub struct FilesService {
    root: PathBuf,
    coordination: FsCoordinator,
}

impl FilesService {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let root = project_root.into();
        let coordination = FsCoordinator::new(root.clone());
        Self { root, coordination }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The canonical coordination coordinator backing this service's
    /// mutations. Exposed so composing services (the edit engine) can
    /// acquire their own multi-resource sets against the same boundary
    /// instead of a second lock mechanism.
    pub fn coordination(&self) -> &FsCoordinator {
        &self.coordination
    }

    /// Acquires coordination over the named workspace-relative paths and
    /// returns the held set. Failures are reported as errors (fail
    /// closed) with a stable category the audit layer can filter on.
    fn acquire(&self, relative_paths: &[&str]) -> Result<CoordinationSet> {
        self.coordination.acquire(relative_paths).map_err(|error| {
            anyhow::Error::msg(format!("filesystem coordination unavailable: {error}"))
        })
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
        // FS-001 (§20): a DANGLING final-component symlink escapes the
        // ancestor walk above — `Path::exists()` follows links, so a
        // link to a not-yet-existing outside target reads as "missing"
        // and the walk settles on the workspace root, after which the
        // mutation would follow the link outside. `symlink_metadata`
        // does NOT follow the link: if the final component is a symlink
        // (live or dangling), follow the chain — bounded, loops fail
        // closed — and require every resolution to stay inside the
        // canonical root. A dangling link pointing INSIDE the root
        // stays allowed: writing through it creates the file inside.
        if let Ok(metadata) = fs::symlink_metadata(&joined) {
            if metadata.file_type().is_symlink() {
                let mut link = joined.clone();
                for _ in 0..SYMLINK_FOLLOW_LIMIT {
                    let next = match fs::read_link(&link) {
                        Ok(next) => next,
                        Err(_) => break, // vanished: the ordinary path checks govern
                    };
                    let candidate = if next.is_absolute() {
                        next
                    } else {
                        link.parent()
                            .context("symlink has no parent directory")?
                            .join(next)
                    };
                    let resolved = match candidate.canonicalize() {
                        Ok(resolved) => resolved,
                        Err(_) => {
                            // Dangling link: validate its deepest existing
                            // ancestor, exactly like a plain missing path.
                            let mut ancestor = candidate.as_path();
                            let mut found = false;
                            while let Some(parent) = ancestor.parent() {
                                if ancestor.exists() {
                                    found = true;
                                    break;
                                }
                                ancestor = parent;
                            }
                            if !found {
                                bail!("symlink target has no existing ancestor");
                            }
                            ancestor.canonicalize()?
                        }
                    };
                    if !resolved.starts_with(&root_canonical) {
                        bail!("resolved path escapes the project root");
                    }
                    link = resolved;
                    if fs::symlink_metadata(&link)
                        .map(|meta| !meta.file_type().is_symlink())
                        .unwrap_or(true)
                    {
                        break;
                    }
                }
            }
        }
        Ok(joined)
    }

    /// Reads raw bytes under the root, enforcing the size cap. Containment
    /// is the same canonical `resolve_checked` boundary as every other
    /// operation. Callers own text/binary interpretation of the bytes.
    pub fn read_bytes(&self, relative: &str) -> Result<Vec<u8>> {
        let path = self.resolve_checked(relative)?;
        let meta = fs::metadata(&path).with_context(|| format!("stat {}", path.display()))?;
        if meta.len() > MAX_FILE_BYTES {
            bail!("file exceeds the {} byte limit", MAX_FILE_BYTES);
        }
        fs::read(&path).with_context(|| format!("read {}", path.display()))
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
    /// The write runs under the canonical coordination boundary with the
    /// path re-validated under the lock.
    pub fn write(&self, relative: &str, content: &str) -> Result<()> {
        let _set = self.acquire(&[relative])?;
        self.write_locked(relative, content)
    }

    /// [`Self::write`] for a caller that already holds the coordination
    /// set for `relative`. Revalidates the path but skips acquisition —
    /// the edit engine's multi-file commit span calls this so a nested
    /// acquisition cannot self-deadlock.
    pub fn write_locked(&self, relative: &str, content: &str) -> Result<()> {
        if content.len() as u64 > MAX_FILE_BYTES {
            bail!("content exceeds the {} byte limit", MAX_FILE_BYTES);
        }
        // Re-resolve UNDER the coordination lock: containment, traversal
        // and symlink-escape checks are repeated at the mutation boundary.
        let path = self.resolve_checked(relative)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent of {}", path.display()))?;
        }
        fs::write(&path, content).with_context(|| format!("write {}", path.display()))
    }

    /// Atomically writes a UTF-8 text file under the root, enforcing the
    /// size cap and the canonical containment boundary.
    ///
    /// The complete content is first written to a unique temporary file in
    /// the target's own directory, flushed to disk, and then renamed over
    /// the target. A `rename(2)` within the same filesystem is atomic, so
    /// concurrent readers observe either the previous complete content or
    /// the new complete content — never a truncated or partially written
    /// file. This is the canonical commit boundary for edit operations:
    /// prepare in memory, commit in one step.
    ///
    /// FS-001: the commit runs under the canonical coordination boundary
    /// and the target path is RE-RESOLVED and re-validated under the
    /// lock, immediately before the temporary file is created — the
    /// path-identity that was checked is the path-identity that is
    /// mutated for every AWH-conformant actor.
    ///
    /// On any failure before the rename (containment, size cap, temp-file
    /// creation, write, flush) the target is left untouched and the
    /// temporary file is removed. A failure of the rename itself likewise
    /// leaves the target untouched. The parent directory is fsynced
    /// best-effort so the rename survives a crash on platforms that
    /// support directory fsync.
    ///
    /// Residual window (documented, not claimed closed): a *foreign*
    /// actor that swaps the final component (symlink or rename) between
    /// this revalidation and the rename is not serialized by AWH's
    /// advisory coordination. The swap is detected at verification (the
    /// re-read bytes mismatch) and reported, never silently absorbed.
    /// Stronger descriptor-relative primitives (openat2 with
    /// `RESOLVE_BENEATH|NO_SYMLINKS`) are a Linux-only hardening path and
    /// are documented in `docs/filesystem-coordination.md`.
    pub fn write_atomic(&self, relative: &str, content: &str) -> Result<()> {
        let _set = self.acquire(&[relative])?;
        self.write_atomic_locked(relative, content)
    }

    /// [`Self::write_atomic`] for a caller that already holds the
    /// coordination set for `relative` (the edit engine's commit span).
    /// Revalidates the path under the caller's held lock but performs no
    /// acquisition.
    pub fn write_atomic_locked(&self, relative: &str, content: &str) -> Result<()> {
        if content.len() as u64 > MAX_FILE_BYTES {
            bail!("content exceeds the {} byte limit", MAX_FILE_BYTES);
        }
        let path = self.resolve_checked(relative)?;
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("target has no parent directory"))?;
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent directory of {relative:?}"))?;
        // Same-directory temp file: guarantees the rename stays on one
        // filesystem, where it is atomic. The raw tempfile error names
        // the host temp path; it is replaced with a message that keeps
        // host paths out of caller-visible errors (§20).
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
            anyhow::Error::msg(format!(
                "stage temporary file for {relative:?}: {}",
                sanitize_io_message(&error.to_string())
            ))
        })?;
        std::io::Write::write_all(&mut temp, content.as_bytes())
            .with_context(|| format!("stage {relative:?}"))?;
        std::io::Write::flush(&mut temp).with_context(|| format!("stage {relative:?}"))?;
        temp.as_file()
            .sync_all()
            .with_context(|| format!("sync staged content for {relative:?}"))?;
        temp.persist(&path).map_err(|error| {
            // The temp file is returned on failure and dropped here,
            // removing it; the target is untouched. The error names only
            // the workspace-relative logical path — host and temp paths
            // are deliberately kept out of messages that may reach callers.
            anyhow::Error::new(error.error)
                .context(format!("commit {relative:?} (target left unchanged)"))
        })?;
        // Best-effort directory fsync so the rename itself is durable.
        // Not portable to every platform/filesystem; a failure here never
        // fails the write.
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    /// Deletes a file or (empty or not) directory under the root. The
    /// caller must have confirmed the destructive action.
    ///
    /// FS-001: the deletion runs under the canonical coordination
    /// boundary with the path re-validated under the lock, so an
    /// AWH-conformant concurrent writer cannot interleave with the
    /// delete.
    pub fn delete(&self, relative: &str) -> Result<()> {
        let _set = self.acquire(&[relative])?;
        self.delete_locked(relative)
    }

    /// [`Self::delete`] for a caller that already holds the coordination
    /// set for `relative` (the edit engine's rollback/commit spans).
    pub fn delete_locked(&self, relative: &str) -> Result<()> {
        // Re-resolve UNDER the lock: containment re-checked at the
        // mutation boundary.
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

    /// Deletes a file or directory, reporting whether the target existed.
    ///
    /// The same containment and validation rules as [`Self::delete`] apply;
    /// a target that does not exist is a no-op reported as `false`, while
    /// an unsafe path or an I/O failure is still an error (fail closed).
    ///
    /// FS-001: the deletion runs under the canonical coordination
    /// boundary with the path re-validated under the lock, matching
    /// [`Self::delete`], so an AWH-conformant concurrent writer cannot
    /// interleave with a delete reported as "did not exist" vs "existed
    /// and removed".
    pub fn delete_if_exists(&self, relative: &str) -> Result<bool> {
        let _set = self.acquire(&[relative])?;
        self.delete_if_exists_locked(relative)
    }

    /// [`Self::delete_if_exists`] for a caller that already holds the
    /// coordination set for `relative`.
    pub fn delete_if_exists_locked(&self, relative: &str) -> Result<bool> {
        let path = self.resolve_checked(relative)?;
        if path == self.root {
            bail!("refusing to delete the project root");
        }
        if !path.exists() {
            return Ok(false);
        }
        if path.is_dir() {
            fs::remove_dir_all(&path).with_context(|| format!("delete dir {}", path.display()))?;
        } else {
            fs::remove_file(&path).with_context(|| format!("delete {}", path.display()))?;
        }
        Ok(true)
    }

    /// Renames/moves within the root. `to` must stay inside the root.
    ///
    /// FS-001: both endpoints are coordinated as one set (deterministic
    /// key order; the pair is small) and both are re-validated under the
    /// lock, so a concurrent writer targeting either endpoint cannot
    /// interleave.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        let _set = self.acquire(&[from, to])?;
        self.rename_locked(from, to)
    }

    /// [`Self::rename`] for a caller that already holds the coordination
    /// set covering both endpoints.
    pub fn rename_locked(&self, from: &str, to: &str) -> Result<()> {
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
    ///
    /// FS-001: directory creation is a workspace mutation (it changes
    /// listings and can be raced with a same-name file creation), so it
    /// takes the same per-resource coordination with under-lock
    /// revalidation.
    pub fn create_dir(&self, relative: &str) -> Result<()> {
        let _set = self.acquire(&[relative])?;
        // Re-resolve under the lock: containment re-checked at the
        // mutation boundary.
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

/// Strips host-filesystem path fragments from an OS error message so
/// caller-visible errors never carry host or temp paths (§20): keeps the
/// error kind/classification (`Permission denied (os error 13)`) and
/// drops `at path "..."`-style suffixes that name concrete host paths.
fn sanitize_io_message(message: &str) -> String {
    if let Some(position) = message.find(" at path ") {
        message[..position].to_owned()
    } else {
        message.to_owned()
    }
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
    #[cfg(unix)] // assertions depend on planting a symlink; Windows skips this test
    fn symlink_escape_is_rejected() {
        let outer = tempfile::tempdir().unwrap();
        let inner = tempfile::tempdir().unwrap();
        let secret = outer.path().join("secret.txt");
        fs::write(&secret, "top secret").unwrap();

        let svc = FilesService::new(inner.path().to_path_buf());
        // Symlink pointing outside the project root.
        let link = inner.path().join("leak.txt");
        std::os::unix::fs::symlink(&secret, &link).unwrap();

        assert!(svc.read("leak.txt").is_err());
        assert!(svc.write("leak.txt", "poison").is_err());
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
        // Coordination lock state lives outside the workspace (system
        // temp), so the listing contains exactly the seeded entries.
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
