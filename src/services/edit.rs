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
    ApplyDiff { diff: String },
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
///
/// `FileState` describes what was observed; it is never an instruction to
/// mutate a file. It intentionally does not carry file content: location-
/// specific checks (see [`ExpectedState::context`]) belong to the later
/// EditService, which reads live content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub line_count: usize,
}

/// Which verifiable `ExpectedState` component failed a precondition check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedComponent {
    Hash,
    Size,
    LineCount,
}

/// Deterministic outcome of comparing an [`ExpectedState`] against an
/// observed [`FileState`].
///
/// The domain model can verify `hash`, `size`, and `line_count` directly
/// because [`FileState`] carries them. It cannot verify `context`, which
/// is location-specific: matching it requires reading the file content and
/// resolving the intended edit location — the responsibility of the later
/// EditService, not this transport-independent model. That boundary is
/// explicit: a context precondition never reads as "matched".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StateMatch {
    /// Every supplied verifiable precondition holds and no context
    /// precondition was supplied.
    Matched,
    /// A supplied precondition does not match the observed state. The edit
    /// must not proceed; the stale state must never be silently overwritten.
    Conflicted { component: ExpectedComponent },
    /// Structural preconditions hold, but a `context` precondition was
    /// supplied and must still be resolved against the current content at
    /// the intended edit location by the EditService.
    ContextPending,
    /// The expected state is structurally invalid (for example a malformed
    /// hash). Validation and matching both fail closed on it. The payload
    /// mirrors the matching `EditError` variant without coupling this wire
    /// representation to the error enum.
    Malformed {
        /// Machine-readable reason code; one value per structurally
        /// invalid expected-state component the model can detect.
        reason: MalformedExpectedState,
        /// Human-readable detail, matching the corresponding `EditError`
        /// message. Contains no file contents or secrets.
        detail: String,
    },
}

/// Structurally invalid expected-state components detectable during
/// matching or validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MalformedExpectedState {
    /// `hash` is not 64 lowercase hexadecimal characters.
    InvalidHash,
    /// `context` was supplied empty.
    EmptyContext,
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

    /// Deterministically checks every verifiable precondition of `expected`
    /// against this observed state.
    ///
    /// Evaluation order is fixed: structural validity first (a malformed
    /// hash fails closed before any comparison), then `hash`, `size`, and
    /// `line_count` — the first mismatch wins and is reported. When all
    /// verifiable preconditions hold and `expected.context` is supplied,
    /// the result is [`StateMatch::ContextPending`]: the domain model
    /// deliberately cannot confirm location-specific context, so callers
    /// must not treat it as matched.
    pub fn check(&self, expected: &ExpectedState) -> StateMatch {
        if let Some(hash) = expected.hash.as_deref() {
            if !is_valid_hash(hash) {
                return StateMatch::Malformed {
                    reason: MalformedExpectedState::InvalidHash,
                    detail: EditError::InvalidHash(hash.to_owned()).to_string(),
                };
            }
            if hash != self.hash {
                return StateMatch::Conflicted {
                    component: ExpectedComponent::Hash,
                };
            }
        }
        if expected.size.is_some_and(|v| v != self.size) {
            return StateMatch::Conflicted {
                component: ExpectedComponent::Size,
            };
        }
        if expected.line_count.is_some_and(|v| v != self.line_count) {
            return StateMatch::Conflicted {
                component: ExpectedComponent::LineCount,
            };
        }
        if expected.context.is_some() {
            StateMatch::ContextPending
        } else {
            StateMatch::Matched
        }
    }
}

/// Caller/runtime identity associated with a transaction, where available.
///
/// All fields are optional because a transaction may legitimately be
/// constructed before a complete runtime session exists; absent fields must
/// not be invented. Values are transport-independent opaque identifiers —
/// the edit model never depends on the core agent store or on any
/// interface layer. `agent_id` values follow the same safe-identifier
/// rules as agent ids elsewhere in the project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditIdentity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// Optional correlation references linking a transaction to records owned
/// by later systems (snapshots, provenance, audit, policy).
///
/// These are foreign identifiers produced by those systems — the edit
/// model stores them for correlation only and never owns or interprets
/// their internal structure.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditRefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_decision_id: Option<String>,
}

/// Lifecycle state of an edit transaction.
///
/// Status records the **observed execution state** of the transaction as
/// the execution service performs it — not a requested target state. The
/// model defines which transitions are legal (see [`EditStatus::can_transition_to`]);
/// the execution service owns performing them and recording the result.
/// Every status is deterministic: two transactions in the same status mean
/// the same thing regardless of transport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditStatus {
    /// Transaction received; nothing has been checked or executed.
    #[default]
    Requested,
    /// Capability/policy authorization granted.
    Authorized,
    /// Target resource located within the workspace.
    Located,
    /// Expected-state and structural validation passed.
    Validated,
    /// Pre-apply snapshot recorded; the edit is recoverable.
    Snapshotted,
    /// Operations executed against the target.
    Applied,
    /// Post-edit verification succeeded.
    Verified,
    /// Transaction complete; result is authoritative. Terminal.
    Committed,
    /// Refused before any execution (authorization, location, policy).
    /// Terminal.
    Rejected,
    /// Expected state did not match the observed file; nothing was
    /// mutated. Terminal.
    Conflict,
    /// Structural or expected-state validation failed. Terminal.
    ValidationFailed,
    /// An operation failed during apply; any applied changes are rolled
    /// back by the execution service. Terminal.
    ApplyFailed,
    /// Post-edit verification failed; rollback restores the snapshot.
    /// Terminal.
    VerificationFailed,
    /// Transaction reverted to its pre-edit snapshot. Terminal.
    RolledBack,
}

impl EditStatus {
    /// Returns whether this status is terminal — no further transitions
    /// are legal from it.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Committed
                | Self::Rejected
                | Self::Conflict
                | Self::ValidationFailed
                | Self::ApplyFailed
                | Self::VerificationFailed
                | Self::RolledBack
        )
    }

    /// Pure, side-effect-free legality check for one lifecycle transition.
    ///
    /// The forward path is
    /// `Requested -> Authorized -> Located -> Validated -> Snapshotted ->
    /// Applied -> Verified -> Committed`. Pre-execution refusals may end at
    /// `Rejected`; validation-time failures end at `Conflict` or
    /// `ValidationFailed`; apply/verification failures end at
    /// `ApplyFailed` or `VerificationFailed`; post-snapshot states may be
    /// reverted to `RolledBack`. Terminal states never transition, failure
    /// states never transition into one another, and skipping forward
    /// stages is not permitted.
    pub fn can_transition_to(&self, next: Self) -> bool {
        use EditStatus::*;
        if self.is_terminal() {
            return false;
        }
        matches!(
            (self, next),
            (Requested, Authorized | Rejected)
                | (Authorized, Located | Rejected)
                | (Located, Validated | Rejected)
                | (
                    Validated,
                    Snapshotted | Conflict | ValidationFailed | Rejected
                )
                | (Snapshotted, Applied | RolledBack)
                | (Applied, Verified | ApplyFailed | RolledBack)
                | (Verified, Committed | VerificationFailed | RolledBack)
        )
    }
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
    #[serde(default)]
    pub status: EditStatus,
    #[serde(default)]
    pub identity: EditIdentity,
    #[serde(default)]
    pub refs: EditRefs,
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
            identity: EditIdentity::default(),
            refs: EditRefs::default(),
        }
    }

    pub fn single(operation: EditOperation) -> Self {
        Self::new(vec![operation])
    }

    /// Structural, side-effect-free validation of the whole transaction.
    ///
    /// This checks shape only: non-empty operation list, expected-state
    /// cardinality, per-operation field validity, expected-state field
    /// formats, and non-empty identity/reference values where present.
    /// It performs no filesystem I/O and no mutation; security/path
    /// containment checks are owned by the filesystem service layer.
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
        for expected in &self.expected {
            if let Some(hash) = expected.hash.as_deref() {
                if !is_valid_hash(hash) {
                    return Err(EditError::InvalidHash(hash.to_owned()));
                }
            }
            if expected.context.as_deref() == Some("") {
                return Err(EditError::EmptyContext);
            }
        }
        validate_optional_reference("agent_id", &self.identity.agent_id)?;
        validate_optional_reference("session_id", &self.identity.session_id)?;
        validate_optional_reference("workspace_id", &self.identity.workspace_id)?;
        validate_optional_reference("snapshot_id", &self.refs.snapshot_id)?;
        validate_optional_reference("provenance_id", &self.refs.provenance_id)?;
        validate_optional_reference("audit_event_id", &self.refs.audit_event_id)?;
        validate_optional_reference("policy_decision_id", &self.refs.policy_decision_id)?;
        for operation in &self.operations {
            match operation {
                EditOperation::Replace {
                    path,
                    old,
                    occurrence,
                    ..
                } => {
                    validate_path(path)?;
                    if old.is_empty() {
                        return Err(EditError::EmptyMatch);
                    }
                    if occurrence.is_some_and(|n| n == 0) {
                        return Err(EditError::InvalidOccurrence);
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
    #[error("expected-state hash must be 64 lowercase hex characters: {0}")]
    InvalidHash(String),
    #[error("expected-state context must not be empty when supplied")]
    EmptyContext,
    #[error("replace occurrence must be at least 1 when supplied")]
    InvalidOccurrence,
    #[error("{0} must not be empty when supplied")]
    EmptyReference(&'static str),
}

/// Validate a transport-provided relative workspace path before any I/O.
pub fn validate_path(path: &str) -> Result<(), EditError> {
    use std::path::{Component, Path};
    let p = Path::new(path);
    if path.is_empty() || p.is_absolute() {
        return Err(EditError::InvalidPath(path.to_owned()));
    }
    for component in p.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
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

/// Returns whether `hash` is structurally valid as a SHA-256 digest
/// reference: exactly 64 lowercase hexadecimal characters.
pub fn is_valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A supplied identity or correlation reference must be non-empty; `None`
/// simply means the reference is absent, which is always valid.
fn validate_optional_reference(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), EditError> {
    if value.as_deref() == Some("") {
        return Err(EditError::EmptyReference(field));
    }
    Ok(())
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

    fn insert_op(path: &str) -> EditOperation {
        EditOperation::Insert {
            path: path.into(),
            line: 1,
            content: "x".into(),
        }
    }

    fn replace_op(path: &str, occurrence: Option<usize>) -> EditOperation {
        EditOperation::Replace {
            path: path.into(),
            old: "old".into(),
            new: "new".into(),
            occurrence,
        }
    }

    fn round_trip(tx: &EditTransaction) -> EditTransaction {
        let json = serde_json::to_string(tx).expect("serialize");
        serde_json::from_str(&json).expect("deserialize")
    }

    // ---------- Transaction construction ----------

    #[test]
    fn transaction_gets_unique_ids() {
        let a = EditId::new();
        let b = EditId::new();
        assert_ne!(a, b);
        assert!(!a.to_string().is_empty());
    }

    #[test]
    fn empty_transaction_is_rejected() {
        assert_eq!(
            EditTransaction::new(Vec::new()).validate_shape(),
            Err(EditError::EmptyTransaction)
        );
    }

    #[test]
    fn single_and_multi_operation_transactions_are_accepted() {
        let single = EditTransaction::single(insert_op("a.txt"));
        assert_eq!(single.operations.len(), 1);
        single.validate_shape().expect("single must validate");

        let multi = EditTransaction::new(vec![insert_op("a.txt"), replace_op("b.rs", None)]);
        assert_eq!(multi.operations.len(), 2);
        multi.validate_shape().expect("multi must validate");
    }

    #[test]
    fn new_transactions_start_in_requested_state_with_defaults() {
        let tx = EditTransaction::new(vec![insert_op("a.txt")]);
        assert_eq!(tx.status, EditStatus::Requested);
        assert_eq!(tx.identity, EditIdentity::default());
        assert_eq!(tx.refs, EditRefs::default());
        assert!(tx.expected.is_empty());
        assert!(tx.before.is_empty() && tx.after.is_empty());
    }

    // ---------- Operation validation ----------

    #[test]
    fn every_operation_variant_validates_when_well_formed() {
        let ops = vec![
            replace_op("a.txt", None),
            replace_op("b.txt", Some(2)),
            insert_op("c.txt"),
            EditOperation::DeleteRange {
                path: "d.txt".into(),
                start_line: 1,
                end_line: 3,
            },
            EditOperation::Patch {
                path: "e.txt".into(),
                old: "old".into(),
                new: "new".into(),
            },
            EditOperation::ApplyDiff {
                diff: "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-x\n+y\n".into(),
            },
        ];
        let tx = EditTransaction::new(ops);
        tx.validate_shape().expect("all variants must validate");
        assert_eq!(tx.operations.len(), 6);
    }

    #[test]
    fn invalid_path_is_rejected_for_every_path_carrying_operation() {
        // All of these are invalid on every platform the CI matrix runs:
        // traversal, absolute, and empty paths.
        for op in [
            replace_op("../escape.txt", None),
            insert_op("/abs/path.txt"),
            EditOperation::DeleteRange {
                path: "a/../../b.txt".into(),
                start_line: 1,
                end_line: 1,
            },
            EditOperation::Patch {
                path: "".into(),
                old: "a".into(),
                new: "b".into(),
            },
        ] {
            let tx = EditTransaction::single(op);
            assert!(
                matches!(tx.validate_shape(), Err(EditError::InvalidPath(_))),
                "path must be rejected"
            );
        }
    }

    #[test]
    fn empty_match_text_is_rejected() {
        let tx = EditTransaction::single(EditOperation::Replace {
            path: "a.txt".into(),
            old: String::new(),
            new: "new".into(),
            occurrence: None,
        });
        assert_eq!(tx.validate_shape(), Err(EditError::EmptyMatch));

        let patch = EditTransaction::single(EditOperation::Patch {
            path: "a.txt".into(),
            old: String::new(),
            new: "new".into(),
        });
        assert_eq!(patch.validate_shape(), Err(EditError::EmptyMatch));
    }

    #[test]
    fn zero_occurrence_is_rejected_but_positive_occurrence_is_accepted() {
        let zero = EditTransaction::single(replace_op("a.txt", Some(0)));
        assert_eq!(zero.validate_shape(), Err(EditError::InvalidOccurrence));

        let positive = EditTransaction::single(replace_op("a.txt", Some(1)));
        positive.validate_shape().expect("occurrence 1 is valid");
    }

    #[test]
    fn empty_diff_is_rejected() {
        let tx = EditTransaction::single(EditOperation::ApplyDiff {
            diff: "   \n".into(),
        });
        assert_eq!(tx.validate_shape(), Err(EditError::EmptyDiff));
    }

    #[test]
    fn invalid_line_ranges_are_rejected() {
        for (start, end) in [(0, 2), (2, 0), (4, 2)] {
            let tx = EditTransaction::single(EditOperation::DeleteRange {
                path: "a.txt".into(),
                start_line: start,
                end_line: end,
            });
            assert_eq!(
                tx.validate_shape(),
                Err(EditError::InvalidLineRange { start, end })
            );
        }
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
        let op = replace_op("src/lib.rs", None);
        assert_eq!(op.paths(), vec!["src/lib.rs"]);
    }

    // ---------- Expected-state matching semantics ----------

    #[test]
    fn file_state_hash_and_metadata_are_stable() {
        let state = FileState::from_content("src/main.rs", "one\ntwo\n");
        assert_eq!(state.path, "src/main.rs");
        assert_eq!(state.size, 8);
        assert_eq!(state.line_count, 2);
        assert_eq!(state.hash.len(), 64);
        assert!(is_valid_hash(&state.hash));
    }

    #[test]
    fn hash_size_and_line_count_each_match_or_conflict() {
        let state = FileState::from_content("a.txt", "one\ntwo\n");

        assert_eq!(
            state.check(&ExpectedState {
                hash: Some(state.hash.clone()),
                ..ExpectedState::default()
            }),
            StateMatch::Matched
        );
        assert_eq!(
            state.check(&ExpectedState {
                hash: Some(sha256_hex(b"stale")),
                ..ExpectedState::default()
            }),
            StateMatch::Conflicted {
                component: ExpectedComponent::Hash
            }
        );

        assert_eq!(
            state.check(&ExpectedState {
                size: Some(8),
                ..ExpectedState::default()
            }),
            StateMatch::Matched
        );
        assert_eq!(
            state.check(&ExpectedState {
                size: Some(9),
                ..ExpectedState::default()
            }),
            StateMatch::Conflicted {
                component: ExpectedComponent::Size
            }
        );

        assert_eq!(
            state.check(&ExpectedState {
                line_count: Some(2),
                ..ExpectedState::default()
            }),
            StateMatch::Matched
        );
        assert_eq!(
            state.check(&ExpectedState {
                line_count: Some(3),
                ..ExpectedState::default()
            }),
            StateMatch::Conflicted {
                component: ExpectedComponent::LineCount
            }
        );
    }

    #[test]
    fn conflicting_expectations_report_deterministically_in_fixed_order() {
        let state = FileState::from_content("a.txt", "one\n");
        // Hash is checked before size and line_count, so the first
        // mismatch wins even when several preconditions are stale.
        let expected = ExpectedState {
            hash: Some(sha256_hex(b"stale")),
            size: Some(999),
            line_count: Some(999),
            context: None,
        };
        assert_eq!(
            state.check(&expected),
            StateMatch::Conflicted {
                component: ExpectedComponent::Hash
            }
        );

        let size_first = ExpectedState {
            size: Some(999),
            line_count: Some(999),
            ..ExpectedState::default()
        };
        assert_eq!(
            state.check(&size_first),
            StateMatch::Conflicted {
                component: ExpectedComponent::Size
            }
        );
    }

    #[test]
    fn combined_verifiable_expectations_all_hold_means_matched() {
        let state = FileState::from_content("a.txt", "one\ntwo\n");
        let expected = ExpectedState {
            hash: Some(state.hash.clone()),
            size: Some(8),
            line_count: Some(2),
            context: None,
        };
        assert_eq!(state.check(&expected), StateMatch::Matched);
    }

    #[test]
    fn context_expectation_is_never_reported_as_matched_by_the_model() {
        let state = FileState::from_content("a.txt", "alpha\nbeta\n");
        // Context is location-specific: the domain model cannot resolve it,
        // so it must surface as pending service-side resolution — including
        // when every verifiable component already holds.
        let expected = ExpectedState {
            hash: Some(state.hash.clone()),
            size: Some(state.size),
            line_count: Some(state.line_count),
            context: Some("alpha".into()),
        };
        assert_eq!(state.check(&expected), StateMatch::ContextPending);

        let context_only = ExpectedState {
            context: Some("alpha".into()),
            ..ExpectedState::default()
        };
        assert_eq!(state.check(&context_only), StateMatch::ContextPending);
    }

    #[test]
    fn no_expectation_means_no_precondition() {
        let state = FileState::from_content("a.txt", "anything");
        assert_eq!(state.check(&ExpectedState::default()), StateMatch::Matched);
    }

    #[test]
    fn malformed_expected_hash_fails_closed_as_malformed() {
        let state = FileState::from_content("a.txt", "one\n");
        for bad in ["", "deadbeef", "0123456789ABCDEF"] {
            let expected = ExpectedState {
                hash: Some(bad.into()),
                ..ExpectedState::default()
            };
            assert_eq!(
                state.check(&expected),
                StateMatch::Malformed {
                    reason: MalformedExpectedState::InvalidHash,
                    detail: EditError::InvalidHash(bad.into()).to_string(),
                },
                "hash {bad:?} must be malformed"
            );
        }
        // The same malformed hash is rejected by structural validation.
        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.expected.push(ExpectedState {
            hash: Some("deadbeef".into()),
            ..ExpectedState::default()
        });
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::InvalidHash("deadbeef".into()))
        );
    }

    #[test]
    fn empty_context_expectation_is_rejected_by_validation() {
        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.expected.push(ExpectedState {
            context: Some(String::new()),
            ..ExpectedState::default()
        });
        assert_eq!(tx.validate_shape(), Err(EditError::EmptyContext));
    }

    // ---------- Identity ----------

    #[test]
    fn identity_from_absent_to_complete_is_accepted_and_round_trips() {
        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.validate_shape().expect("absent identity is valid");

        tx.identity.agent_id = Some("agent-1".into());
        tx.validate_shape().expect("agent-only identity is valid");

        tx.identity.session_id = Some("session-9".into());
        tx.identity.workspace_id = Some("ws-main".into());
        tx.validate_shape().expect("complete identity is valid");
        assert_eq!(
            round_trip(&tx).identity,
            EditIdentity {
                agent_id: Some("agent-1".into()),
                session_id: Some("session-9".into()),
                workspace_id: Some("ws-main".into()),
            }
        );
    }

    #[test]
    fn empty_identity_and_reference_values_are_rejected() {
        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.identity.agent_id = Some(String::new());
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::EmptyReference("agent_id"))
        );

        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.refs.policy_decision_id = Some(String::new());
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::EmptyReference("policy_decision_id"))
        );
    }

    // ---------- Correlation references ----------

    #[test]
    fn optional_references_from_absent_to_all_are_accepted_and_round_trip() {
        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.validate_shape().expect("no references is valid");

        tx.refs.snapshot_id = Some("snap-1".into());
        tx.validate_shape().expect("single reference is valid");

        tx.refs = EditRefs {
            snapshot_id: Some("snap-1".into()),
            provenance_id: Some("prov-2".into()),
            audit_event_id: Some("audit-3".into()),
            policy_decision_id: Some("policy-4".into()),
        };
        tx.validate_shape().expect("all references are valid");
        assert_eq!(round_trip(&tx).refs, tx.refs);
    }

    // ---------- Lifecycle ----------

    #[test]
    fn initial_status_is_deterministically_requested() {
        let tx = EditTransaction::single(insert_op("a.txt"));
        assert_eq!(tx.status, EditStatus::Requested);
    }

    #[test]
    fn terminal_statuses_are_known_and_never_transition() {
        for terminal in [
            EditStatus::Committed,
            EditStatus::Rejected,
            EditStatus::Conflict,
            EditStatus::ValidationFailed,
            EditStatus::ApplyFailed,
            EditStatus::VerificationFailed,
            EditStatus::RolledBack,
        ] {
            assert!(terminal.is_terminal());
            for next in [
                EditStatus::Requested,
                EditStatus::Authorized,
                EditStatus::Located,
                EditStatus::Validated,
                EditStatus::Snapshotted,
                EditStatus::Applied,
                EditStatus::Verified,
                EditStatus::Committed,
                EditStatus::Rejected,
                EditStatus::Conflict,
                EditStatus::ValidationFailed,
                EditStatus::ApplyFailed,
                EditStatus::VerificationFailed,
                EditStatus::RolledBack,
            ] {
                assert!(
                    !terminal.can_transition_to(next),
                    "terminal {terminal:?} must not transition to {next:?}"
                );
            }
        }
        for forward in [
            EditStatus::Requested,
            EditStatus::Authorized,
            EditStatus::Located,
            EditStatus::Validated,
            EditStatus::Snapshotted,
            EditStatus::Applied,
            EditStatus::Verified,
        ] {
            assert!(!forward.is_terminal());
        }
    }

    #[test]
    fn forward_lifecycle_path_is_legal_in_order_only() {
        use EditStatus::*;
        let path = [
            Requested,
            Authorized,
            Located,
            Validated,
            Snapshotted,
            Applied,
            Verified,
            Committed,
        ];
        for pair in path.windows(2) {
            assert!(
                pair[0].can_transition_to(pair[1]),
                "{:?} -> {:?} must be legal",
                pair[0],
                pair[1]
            );
        }
        // Skipping forward stages is illegal.
        assert!(!Requested.can_transition_to(Located));
        assert!(!Authorized.can_transition_to(Applied));
        assert!(!Validated.can_transition_to(Verified));
        assert!(!Snapshotted.can_transition_to(Committed));
    }

    #[test]
    fn failure_and_recovery_transitions_are_legal_where_defined() {
        use EditStatus::*;
        assert!(Requested.can_transition_to(Rejected));
        assert!(Authorized.can_transition_to(Rejected));
        assert!(Located.can_transition_to(Rejected));
        assert!(Validated.can_transition_to(Rejected));

        assert!(Validated.can_transition_to(Conflict));
        assert!(Validated.can_transition_to(ValidationFailed));

        assert!(Applied.can_transition_to(ApplyFailed));
        assert!(Verified.can_transition_to(VerificationFailed));

        assert!(Snapshotted.can_transition_to(RolledBack));
        assert!(Applied.can_transition_to(RolledBack));
        assert!(Verified.can_transition_to(RolledBack));

        // Failure states never cross into each other.
        assert!(!Conflict.can_transition_to(ApplyFailed));
        assert!(!ApplyFailed.can_transition_to(RolledBack));
        assert!(!Rejected.can_transition_to(Conflict));
        // Rejected cannot appear after work has begun.
        assert!(!Snapshotted.can_transition_to(Rejected));
        assert!(!Applied.can_transition_to(Rejected));
    }

    #[test]
    fn every_status_serializes_to_snake_case_and_round_trips() {
        let statuses = [
            EditStatus::Requested,
            EditStatus::Authorized,
            EditStatus::Located,
            EditStatus::Validated,
            EditStatus::Snapshotted,
            EditStatus::Applied,
            EditStatus::Verified,
            EditStatus::Committed,
            EditStatus::Rejected,
            EditStatus::Conflict,
            EditStatus::ValidationFailed,
            EditStatus::ApplyFailed,
            EditStatus::VerificationFailed,
            EditStatus::RolledBack,
        ];
        let expected_wire: Vec<&str> = vec![
            "requested",
            "authorized",
            "located",
            "validated",
            "snapshotted",
            "applied",
            "verified",
            "committed",
            "rejected",
            "conflict",
            "validation_failed",
            "apply_failed",
            "verification_failed",
            "rolled_back",
        ];
        for (status, wire) in statuses.iter().zip(expected_wire) {
            assert_eq!(serde_json::to_string(status).unwrap(), format!("{wire:?}"));
            assert_eq!(
                serde_json::from_str::<EditStatus>(&format!("{wire:?}")).unwrap(),
                *status
            );
        }
    }

    // ---------- Serialization round trips ----------

    #[test]
    fn every_operation_variant_round_trips() {
        let ops = vec![
            replace_op("a.txt", None),
            replace_op("b.txt", Some(3)),
            insert_op("c.txt"),
            EditOperation::DeleteRange {
                path: "d.txt".into(),
                start_line: 2,
                end_line: 5,
            },
            EditOperation::Patch {
                path: "e.txt".into(),
                old: "old".into(),
                new: "new".into(),
            },
            EditOperation::ApplyDiff {
                diff: "--- a/f\n+++ b/f\n".into(),
            },
        ];
        for op in ops {
            let json = serde_json::to_string(&op).expect("serialize op");
            let back: EditOperation = serde_json::from_str(&json).expect("deserialize op");
            assert_eq!(back, op);
        }
    }

    #[test]
    fn single_operation_transaction_round_trips() {
        let tx = EditTransaction::single(insert_op("a.txt"));
        assert_eq!(round_trip(&tx), tx);
    }

    #[test]
    fn fully_populated_multi_operation_transaction_round_trips() {
        let mut tx = EditTransaction::new(vec![insert_op("a.txt"), replace_op("b.rs", Some(2))]);
        tx.expected.push(ExpectedState {
            hash: Some(sha256_hex(b"one")),
            size: Some(3),
            line_count: Some(1),
            context: Some("one".into()),
        });
        tx.expected.push(ExpectedState::default());
        tx.before.push(FileState::from_content("a.txt", "old\n"));
        tx.after.push(FileState::from_content("a.txt", "x\n"));
        tx.status = EditStatus::Validated;
        tx.identity = EditIdentity {
            agent_id: Some("agent-1".into()),
            session_id: Some("session-9".into()),
            workspace_id: Some("ws-main".into()),
        };
        tx.refs = EditRefs {
            snapshot_id: Some("snap-1".into()),
            provenance_id: Some("prov-2".into()),
            audit_event_id: Some("audit-3".into()),
            policy_decision_id: Some("policy-4".into()),
        };
        tx.validate_shape().expect("populated tx must validate");
        assert_eq!(round_trip(&tx), tx);
    }

    #[test]
    fn absent_optional_fields_stay_absent_on_the_wire() {
        let tx = EditTransaction::single(insert_op("a.txt"));
        let json = serde_json::to_string(&tx).expect("serialize");
        assert!(!json.contains("agent_id"));
        assert!(!json.contains("snapshot_id"));
        // Deserializing the legacy wire form (no identity/refs keys) still
        // works because every optional member defaults.
        let legacy = format!(
            "{{\"id\":\"{}\",\"operations\":[{{\"type\":\"insert\",\"path\":\"a.txt\",\"line\":1,\"content\":\"x\"}}],\"status\":\"requested\"}}",
            tx.id
        );
        let parsed: EditTransaction = serde_json::from_str(&legacy).expect("legacy wire form");
        assert_eq!(parsed.identity, EditIdentity::default());
        assert_eq!(parsed.refs, EditRefs::default());
    }
}
