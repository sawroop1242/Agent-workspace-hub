//! Canonical, transport-independent edit transaction model.
//!
//! This module is the contract shared by MCP, CLI, TUI and the Control API.
//! Mutation logic belongs in later EditService operations; these types define
//! the stable vocabulary used by every edit path.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable identifier for one requested edit transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EditId(pub String);

impl EditId {
    /// Generates a process-local unique identifier without adding another
    /// dependency to the core edit path.
    pub fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(1);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self(format!("edit-{nanos:x}-{sequence:x}"))
    }
}

impl Default for EditId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EditId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The mutation primitives supported by the canonical edit service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditOperation {
    /// Replace one exact string occurrence or an explicitly requested number
    /// of occurrences in a text file.
    Replace {
        path: String,
        old: String,
        new: String,
        #[serde(default)]
        occurrence: Option<usize>,
    },
    /// Insert text at a one-based line boundary. `line` is the line before
    /// which the text is inserted; zero means the beginning of the file.
    Insert {
        path: String,
        line: usize,
        content: String,
    },
    /// Delete an inclusive one-based line range.
    DeleteRange {
        path: String,
        start_line: usize,
        end_line: usize,
    },
    /// Apply a pre-computed replacement to a file after validating its
    /// expected state. The service still owns the actual write.
    Patch {
        path: String,
        old: String,
        new: String,
    },
    /// Apply a unified diff. Parsing/application is implemented by the
    /// unified-diff milestone; this variant keeps the public model stable.
    ApplyDiff {
        diff: String,
    },
}

impl EditOperation {
    pub fn paths(&self) -> Vec<&str> {
        match self {
            Self::Replace { path, .. }
            | Self::Insert { path, .. }
            | Self::DeleteRange { path, .. }
            | Self::Patch { path, .. } => vec![path.as_str()],
            Self::ApplyDiff { .. } => Vec::new(),
        }
    }
}

/// Optional state precondition supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExpectedState {
    /// SHA-256 of the complete file contents before the edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Exact text that must still occur at the intended edit location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Expected file size in bytes before mutation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Expected line count before mutation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_count: Option<usize>,
}

/// Immutable state observed for a file at a transaction boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub line_count: usize,
}

impl FileState {
    pub fn from_content(path: impl Into<String>, content: &str) -> Self {
        Self {
            path: path.into(),
            hash: sha256_hex(content.as_bytes()),
            size: content.len() as u64,
            line_count: line_count(content),
        }
    }

    pub fn matches(&self, expected: &ExpectedState) -> bool {
        expected.hash.as_deref().is_none_or(|v| v == self.hash)
            && expected.size.is_none_or(|v| v == self.size)
            && expected.line_count.is_none_or(|v| v == self.line_count)
    }
}

/// Lifecycle state of an edit transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditStatus {
    Requested,
    Authorized,
    Located,
    Validated,
    Snapshotted,
    Applied,
    Verified,
    Committed,
    Rejected,
    Conflict,
    ValidationFailed,
    ApplyFailed,
    VerificationFailed,
    RolledBack,
}

/// A complete, transport-independent edit request and its observed state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditTransaction {
    pub id: EditId,
    pub operations: Vec<EditOperation>,
    #[serde(default)]
    pub expected: Vec<ExpectedState>,
    #[serde(default)]
    pub before: Vec<FileState>,
    #[serde(default)]
    pub after: Vec<FileState>,
    pub status: EditStatus,
}

impl EditTransaction {
    pub fn new(operations: Vec<EditOperation>) -> Self {
        Self {
            id: EditId::new(),
            operations,
            expected: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
            status: EditStatus::Requested,
        }
    }

    pub fn single(operation: EditOperation) -> Self {
        Self::new(vec![operation])
    }

    pub fn validate_shape(&self) -> Result<(), EditError> {
        if self.operations.is_empty() {
            return Err(EditError::EmptyTransaction);
        }
        if !self.expected.is_empty() && self.expected.len() != self.operations.len() {
            return Err(EditError::ExpectedStateCount {
                operations: self.operations.len(),
                expected: self.expected.len(),
            });
        }
        for operation in &self.operations {
            match operation {
                EditOperation::Replace { path, old, .. } => {
                    validate_path(path)?;
                    if old.is_empty() {
                        return Err(EditError::EmptyMatch);
                    }
                }
                EditOperation::Insert { path, .. } => validate_path(path)?,
                EditOperation::DeleteRange {
                    path,
                    start_line,
                    end_line,
                } => {
                    validate_path(path)?;
                    if *start_line == 0 || *end_line == 0 || start_line > end_line {
                        return Err(EditError::InvalidLineRange {
                            start: *start_line,
                            end: *end_line,
                        });
                    }
                }
                EditOperation::Patch { path, old, .. } => {
                    validate_path(path)?;
                    if old.is_empty() {
                        return Err(EditError::EmptyMatch);
                    }
                }
                EditOperation::ApplyDiff { diff } => {
                    if diff.trim().is_empty() {
                        return Err(EditError::EmptyDiff);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Structured errors for the canonical edit model. Operation-specific
/// filesystem errors are layered on top by EditService implementations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("edit transaction must contain at least one operation")]
    EmptyTransaction,
    #[error("expected state count ({expected}) does not match operation count ({operations})")]
    ExpectedStateCount { operations: usize, expected: usize },
    #[error("edit target path is invalid: {0}")]
    InvalidPath(String),
    #[error("edit match text must not be empty")]
    EmptyMatch,
    #[error("line range is invalid: {start}..={end}")]
    InvalidLineRange { start: usize, end: usize },
    #[error("unified diff must not be empty")]
    EmptyDiff,
}

/// Validate a transport-provided relative workspace path before any I/O.
pub fn validate_path(path: &str) -> Result<(), EditError> {
    use std::path::{Component, Path};
    let p = Path::new(path);
    if path.is_empty() || p.is_absolute() {
        return Err(EditError::InvalidPath(path.to_owned()));
    }
    for component in p.components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            return Err(EditError::InvalidPath(path.to_owned()));
        }
    }
    Ok(())
}

/// Return a stable lower-case SHA-256 digest.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn line_count(content: &str) -> usize {
    if content.is_empty() {
        0
    } else {
        content.lines().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_gets_unique_ids() {
        let a = EditId::new();
        let b = EditId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn file_state_hash_and_metadata_are_stable() {
        let state = FileState::from_content("src/main.rs", "one\ntwo\n");
        assert_eq!(state.path, "src/main.rs");
        assert_eq!(state.size, 8);
        assert_eq!(state.line_count, 2);
        assert_eq!(state.hash.len(), 64);
        assert!(state.matches(&ExpectedState {
            hash: Some(state.hash.clone()),
            size: Some(8),
            line_count: Some(2),
            context: None,
        }));
    }

    #[test]
    fn shape_validation_rejects_empty_transactions() {
        assert_eq!(
            EditTransaction::new(Vec::new()).validate_shape(),
            Err(EditError::EmptyTransaction)
        );
    }

    #[test]
    fn shape_validation_rejects_bad_paths_and_ranges() {
        let bad_path = EditTransaction::single(EditOperation::Insert {
            path: "../escape.rs".into(),
            line: 1,
            content: "x".into(),
        });
        assert!(matches!(bad_path.validate_shape(), Err(EditError::InvalidPath(_))));

        let bad_range = EditTransaction::single(EditOperation::DeleteRange {
            path: "src/lib.rs".into(),
            start_line: 4,
            end_line: 2,
        });
        assert_eq!(
            bad_range.validate_shape(),
            Err(EditError::InvalidLineRange { start: 4, end: 2 })
        );
    }

    #[test]
    fn expected_state_count_must_match_operations() {
        let mut tx = EditTransaction::single(EditOperation::Patch {
            path: "a.txt".into(),
            old: "a".into(),
            new: "b".into(),
        });
        tx.expected.push(ExpectedState::default());
        tx.expected.push(ExpectedState::default());
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::ExpectedStateCount {
                operations: 1,
                expected: 2,
            })
        );
    }

    #[test]
    fn paths_are_exposed_without_transport_coupling() {
        let op = EditOperation::Replace {
            path: "src/lib.rs".into(),
            old: "old".into(),
            new: "new".into(),
            occurrence: None,
        };
        assert_eq!(op.paths(), vec!["src/lib.rs"]);
    }
}
