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
    pub fn for_project(project_path: &Path) -> Self {
        Self { path: project_path.join(".agent/memory.jsonl") }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

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
