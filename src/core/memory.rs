use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::models::MemoryEntry;

/// Append-only JSONL memory compatible with `.agent/memory.jsonl`.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    path: PathBuf,
}

impl MemoryStore {
    /// Creates a memory store rooted at the given project's `.agent/memory.jsonl`.
    pub fn for_project(project_path: &Path) -> Self {
        Self {
            path: project_path.join(".agent/memory.jsonl"),
        }
    }

    /// Returns the backing memory file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one memory entry as a JSONL line, creating the file if needed.
    pub fn append(&self, entry: &MemoryEntry) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let line = serde_json::to_string(entry)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open {}", self.path.display()))?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Reads all memory entries, skipping blank lines and ignoring a missing file.
    pub fn read_all(&self) -> Result<Vec<MemoryEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let mut entries = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            entries.push(serde_json::from_str(&line)?);
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(content: &str) -> MemoryEntry {
        MemoryEntry {
            timestamp: "2026-01-01T00:00:00Z".into(),
            content: content.into(),
        }
    }

    #[test]
    fn for_project_points_at_agent_memory_jsonl() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::for_project(temp.path());
        assert_eq!(store.path(), temp.path().join(".agent/memory.jsonl"));
    }

    #[test]
    fn read_all_on_missing_file_is_empty_not_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::for_project(temp.path());
        assert!(store.read_all().unwrap().is_empty());
    }

    #[test]
    fn append_creates_parent_dirs_and_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::for_project(temp.path());
        store.append(&entry("first")).unwrap();
        store.append(&entry("second")).unwrap();
        assert!(store.path().is_file());
        let entries = store.read_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "first");
        assert_eq!(entries[1].content, "second");
        assert_eq!(entries[0].timestamp, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn append_is_one_jsonl_line_per_entry() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::for_project(temp.path());
        store.append(&entry("first")).unwrap();
        store.append(&entry("second")).unwrap();
        let raw = fs::read_to_string(store.path()).unwrap();
        let lines: Vec<&str> = raw.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            assert!(serde_json::from_str::<MemoryEntry>(line).is_ok());
        }
        // no trailing separator junk: the file ends after the last newline
        assert!(raw.ends_with('\n'));
    }

    #[test]
    fn read_all_skips_blank_lines() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::for_project(temp.path());
        fs::create_dir_all(temp.path().join(".agent")).unwrap();
        let content = format!(
            "{}\n\n   \n{}\n",
            serde_json::to_string(&entry("first")).unwrap(),
            serde_json::to_string(&entry("second")).unwrap()
        );
        fs::write(store.path(), content).unwrap();
        let entries = store.read_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "first");
        assert_eq!(entries[1].content, "second");
    }

    #[test]
    fn read_all_fails_on_corrupt_line() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::for_project(temp.path());
        fs::create_dir_all(temp.path().join(".agent")).unwrap();
        fs::write(store.path(), "not json at all\n").unwrap();
        assert!(store.read_all().is_err());
    }

    #[test]
    fn memory_entry_serializes_with_expected_field_names() {
        let e = entry("hello");
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["timestamp"], "2026-01-01T00:00:00Z");
        assert_eq!(json["content"], "hello");
        let back: MemoryEntry = serde_json::from_value(json).unwrap();
        assert_eq!(back, e);
    }
}
