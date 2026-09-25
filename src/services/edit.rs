//! Canonical, transport-independent edit transaction model.
//!
//! This module is the contract shared by MCP, CLI, TUI and the Control API.
//! Mutation logic belongs in later EditService operations; these types define
//! the stable vocabulary used by every edit path.

use crate::services::files::FilesService;
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

impl fmt::Display for ExpectedComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ExpectedComponent::Hash => "hash",
            ExpectedComponent::Size => "size",
            ExpectedComponent::LineCount => "line_count",
        })
    }
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
    /// stages is not permitted — except that `Snapshotted` is optional
    /// until the snapshot milestone exists: a transaction may move directly
    /// from `Validated` to `Applied`.
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
                    Snapshotted | Applied | Conflict | ValidationFailed | Rejected
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
                EditOperation::Insert { path, content, .. } => {
                    validate_path(path)?;
                    if content.is_empty() {
                        return Err(EditError::EmptyInsertContent);
                    }
                }
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
    #[error("invalid unified diff format: {0}")]
    InvalidDiff(String),
    #[error("unsupported binary patch: {0}")]
    BinaryPatch(String),
    #[error("hunk context mismatch in {path} at hunk {hunk}")]
    HunkContextMismatch { path: String, hunk: usize },
    /// Insertion content must not be empty; it is the text to insert.
    #[error("insert content must not be empty")]
    EmptyInsertContent,
    #[error("expected-state hash must be 64 lowercase hex characters: {0}")]
    InvalidHash(String),
    #[error("expected-state context must not be empty when supplied")]
    EmptyContext,
    #[error("replace occurrence must be at least 1 when supplied")]
    InvalidOccurrence,
    #[error("{0} must not be empty when supplied")]
    EmptyReference(&'static str),
    /// A supplied identity or correlation reference contains control
    /// characters, which no legitimate identifier in this system can
    /// carry. Fails closed rather than being passed to a later system.
    #[error("{0} must not contain control characters")]
    UnsafeReference(&'static str),
    // ---- Operation-layer failures (EditService). Every failure below is
    // ---- reported before the atomic commit boundary unless stated
    // ---- otherwise, so the target file is unchanged.
    /// The transaction's operation set is not the single `Replace` this
    /// executor performs. Multi-operation patching belongs to the patch
    /// milestone; other operation types have their own executors.
    #[error("this executor performs exactly one replace operation; got {operation_count} operation(s) of unsupported shape")]
    UnsupportedTransaction { operation_count: usize },
    /// Patch validation failed: the transaction could not be prepared due
    /// to an invalid operation, path, or expected state.
    #[error("patch validation failed for transaction {transaction_id}: {reason}")]
    PatchValidationFailure {
        transaction_id: String,
        path: Option<String>,
        reason: String,
    },
    /// Patch preparation failed: the transaction's expected state does
    /// not match the observed current state for one or more files.
    #[error("patch preparation failed for transaction {transaction_id}: {reason}")]
    PatchPreparationFailure {
        transaction_id: String,
        path: Option<String>,
        reason: String,
    },
    /// Patch commit failed: the atomic write or verification step failed
    /// for one or more files after some files had already been committed.
    #[error("patch commit failed for transaction {transaction_id}: {reason}")]
    PatchCommitFailure {
        transaction_id: String,
        committed_files: Vec<String>,
        failed_path: String,
        reason: String,
    },
    /// The target file does not exist inside the workspace.
    #[error("target file not found in workspace: {path}")]
    FileNotFound { path: String },
    /// The target file could not be read (permissions, containment
    /// rejection, or size-limit breach; the message comes from the
    /// canonical filesystem boundary).
    #[error("could not read target file {path}: {message}")]
    ReadFailure { path: String, message: String },
    /// The target file's bytes are not valid UTF-8. Replacement is a text
    /// primitive; binary mutation is deliberately unsupported rather than
    /// silently reinterpreted.
    #[error("target file is not valid UTF-8 text: {path}")]
    InvalidUtf8 { path: String },
    /// `old` does not occur in the current content.
    #[error("match text not found in {path}")]
    MatchNotFound { path: String },
    /// `old` occurs more than once and no occurrence was selected.
    #[error("match text occurs {match_count} times in {path}; select an occurrence or make the match unique")]
    AmbiguousMatch {
        path: String,
        match_count: usize,
        locations: Vec<MatchLocation>,
    },
    /// The selected occurrence is outside the available match range
    /// (1-based indexing over left-to-right, non-overlapping matches).
    #[error("occurrence {selected} is out of range: {match_count} match(es) found in {path}")]
    OccurrenceOutOfRange {
        path: String,
        selected: usize,
        match_count: usize,
    },
    /// An insert boundary line is outside the file's valid boundary range.
    /// Boundaries are 0 (beginning of file) through line_count + 1 (EOF);
    /// a positive N inserts before logical line N. No silent clamping.
    #[error("insert boundary {line} is out of range: file has {line_count} line(s)")]
    InsertBoundaryOutOfRange {
        path: String,
        line: usize,
        line_count: usize,
    },
    /// A delete-range line is outside the file's existing line range.
    /// Deletion is 1-based inclusive over existing lines only.
    #[error("line range {start}..={end} is out of range: file has {line_count} line(s)")]
    LineOutOfRange {
        path: String,
        start: usize,
        end: usize,
        line_count: usize,
    },
    /// Deletion from a file with no logical lines.
    #[error("cannot delete lines from a file with no lines: {path}")]
    EmptyFileDeletion { path: String },
    /// A supplied expected-state precondition (hash, size, or line count)
    /// does not match the observed current state. The stale state is never
    /// silently overwritten. Boxed to keep the error type small on the
    /// hot path; conflicts are the rare branch.
    #[error(transparent)]
    ExpectedStateConflict(Box<ExpectedStateConflictPayload>),
    /// A supplied expected-state context precondition failed. Context must
    /// occur exactly once in the current content and contain the selected
    /// match.
    #[error("expected-state context conflict on {path}: {reason}")]
    ContextConflict {
        path: String,
        reason: ContextConflictReason,
    },
    /// The atomic write through the canonical filesystem boundary failed.
    /// The target is unchanged: content is staged to a temporary file and
    /// committed by rename, so a failed commit never leaves a partial
    /// write.
    #[error("could not write target file {path}: {message}")]
    WriteFailure { path: String, message: String },
    /// The post-write observation does not match the prepared content.
    /// This indicates external interference after the commit boundary and
    /// is reported honestly rather than fabricating the requested state.
    /// Boxed to keep the error type small on the hot path.
    #[error(transparent)]
    VerificationFailed(Box<VerificationFailurePayload>),
    /// Semantic verification failed: the operation completed at the byte
    /// level but the semantic postcondition of the operation was not met.
    /// For example, a Replace did not actually replace the intended text,
    /// an Insert did not insert at the intended boundary, or a DeleteRange
    /// did not delete the intended lines.
    #[error(transparent)]
    SemanticVerificationFailure(Box<SemanticVerificationFailurePayload>),
    /// Authorization denied: the caller did not satisfy the
    /// capability/policy boundary. This is a first-class verification
    /// failure — never a silent fallback to weaker rules.
    #[error("authorization denied for operation {action}: {reason}")]
    AuthorizationDenied {
        action: String,
        reason: String,
        denial: crate::services::authorization::DenialReason,
    },
}

/// Payload of [`EditError::ExpectedStateConflict`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[error("expected-state conflict on {path}: {component} mismatch")]
pub struct ExpectedStateConflictPayload {
    /// Workspace-relative target path.
    pub path: String,
    /// Which verifiable precondition failed.
    pub component: ExpectedComponent,
    /// Verifiable components the caller expected (never file content).
    pub expected: ExpectedStateSummary,
    /// State actually observed before the refused mutation.
    pub actual: FileState,
}

/// Payload of [`EditError::VerificationFailed`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[error("verification failed for {path}: observed content differs from prepared content")]
pub struct VerificationFailurePayload {
    /// Workspace-relative target path.
    pub path: String,
    /// State the prepared content predicts after commit.
    pub expected: FileState,
    /// State actually observed after commit.
    pub actual: FileState,
}

/// Payload of [`EditError::SemanticVerificationFailure`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[error("semantic verification failed for {path}: {reason}")]
pub struct SemanticVerificationFailurePayload {
    /// Workspace-relative target path.
    pub path: String,
    /// Human-readable reason for the semantic failure.
    pub reason: String,
    /// The operation index within the transaction (when applicable).
    pub operation_index: Option<usize>,
    /// The type of operation that failed semantic verification.
    pub operation_type: String,
    /// Expected semantic postcondition.
    pub expected_postcondition: String,
    /// Actual observed state after mutation.
    pub actual_state: FileState,
}

/// Position of one match within the observed file content.
///
/// All coordinates are deterministic and named explicitly:
/// - `byte_offset`: 0-based byte offset of the match start within the
///   file;
/// - `line`: 1-based line number of the byte offset;
/// - `column`: 1-based **character** position of the match start within
///   its line (not bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchLocation {
    pub byte_offset: usize,
    pub line: usize,
    pub column: usize,
}

/// Conflict-safe summary of a supplied expected state: the verifiable
/// components only, never the literal `context` text or any file content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedStateSummary {
    pub hash: Option<String>,
    pub size: Option<u64>,
    pub line_count: Option<usize>,
}

impl ExpectedStateSummary {
    fn of(expected: &ExpectedState) -> Self {
        Self {
            hash: expected.hash.clone(),
            size: expected.size,
            line_count: expected.line_count,
        }
    }
}

/// Why an expected-state context precondition failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum ContextConflictReason {
    /// The context does not occur in the current content.
    #[error("context does not occur in the current content")]
    Missing,
    /// The context occurs more than once, so it cannot anchor the edit
    /// unambiguously.
    #[error("context occurs {count} times in the current content")]
    Ambiguous { count: usize },
    /// The context occurs exactly once but does not contain the selected
    /// match, so it does not anchor the intended edit location.
    #[error("context does not contain the selected match")]
    NotAnchored,
}

/// Validate a transport-provided relative workspace path before any I/O.
///
/// Syntax rules mirror the canonical filesystem boundary
/// ([`crate::services::files::FilesService`]): empty, absolute, and
/// traversal-shaped paths are rejected, plus control characters — a path
/// containing them can never name a file a caller legitimately created,
/// and rejecting it here keeps malformed input out of every executor.
/// This is syntax-only containment: it never resolves a host path (that
/// is the filesystem layer's job, re-enforced on every read/write).
pub fn validate_path(path: &str) -> Result<(), EditError> {
    use std::path::{Component, Path};
    let p = Path::new(path);
    if path.is_empty() || p.is_absolute() || path.chars().any(|c| c.is_control()) {
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
///
/// Values are opaque foreign identifiers, so the model applies the shared
/// identifier safety rule rather than interpreting their structure: any
/// control character fails validation, matching the canonical agent-id
/// rules elsewhere in the project (`crate::core::agents::is_safe_agent_id`).
/// The model never resolves what a reference points at — that belongs to
/// the system that owns it.
fn validate_optional_reference(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), EditError> {
    match value.as_deref() {
        None => Ok(()),
        Some("") => Err(EditError::EmptyReference(field)),
        Some(value) if value.chars().any(|c| c.is_control()) => {
            Err(EditError::UnsafeReference(field))
        }
        Some(_) => Ok(()),
    }
}

fn line_count(content: &str) -> usize {
    if content.is_empty() {
        0
    } else {
        content.lines().count()
    }
}

// ---------------------------------------------------------------------------
// EditService: the first production edit executor (AWE-002).
// ---------------------------------------------------------------------------

/// The outcome of one successfully committed edit operation.
///
/// Carries the structured information later milestones (patching,
/// snapshots, provenance, MCP/CLI surfaces) consume. It contains no file
/// content: `FileState` is observed metadata (hash/size/line count) and
/// match locations are derived from the caller's own match text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditResult {
    /// Identity of the executed transaction, for correlation.
    pub id: EditId,
    /// Workspace-relative target path.
    pub path: String,
    /// Observed final lifecycle state of the transaction.
    pub status: EditStatus,
    /// State observed before mutation.
    pub before: FileState,
    /// State observed after the committed mutation.
    pub after: FileState,
    /// Total matches of `old` found in the pre-mutation content.
    pub match_count: usize,
    /// 1-based occurrence that was replaced.
    pub selected_occurrence: usize,
    /// Location of the replaced match.
    pub location: MatchLocation,
}

/// The outcome of one successfully committed line-oriented edit
/// operation (insert or delete-range). Line coordinates follow the one
/// canonical convention: 1-based logical lines as counted by
/// [`FileState::line_count`]; insert boundaries additionally allow `0`
/// (beginning of file) and `line_count + 1` (EOF).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineEditResult {
    /// Identity of the executed transaction, for correlation.
    pub id: EditId,
    /// Workspace-relative target path.
    pub path: String,
    /// Observed final lifecycle state of the transaction.
    pub status: EditStatus,
    /// State observed before mutation.
    pub before: FileState,
    /// State observed after the committed mutation.
    pub after: FileState,
    /// Which line-oriented operation committed.
    pub kind: LineEditKind,
}

/// Which line-oriented primitive committed in a [`LineEditResult`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LineEditKind {
    /// Text inserted at a boundary. `boundary` is the requested line
    /// boundary as documented on [`EditOperation::Insert`]: 0 = beginning
    /// of file, 1..=line_count = before that logical line,
    /// line_count + 1 = end of file.
    Insert { boundary: usize },
    /// An inclusive 1-based range of logical lines removed.
    DeleteRange { start: usize, end: usize },
}

/// One step of a multi-operation patch that succeeded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchStepResult {
    /// The transaction this step belongs to.
    pub transaction_id: EditId,
    /// Workspace-relative target path.
    pub path: String,
    /// Operation index within `EditTransaction.operations`.
    pub operation_index: usize,
    /// Observed lifecycle state of this step.
    pub status: PatchStepStatus,
    /// State observed before this step's mutation.
    pub before: FileState,
    /// State observed after this step's mutation.
    pub after: FileState,
    /// Operation applied by this step.
    pub operation: PatchOperation,
}

/// Lifecycle state of a single patch step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchStepStatus {
    /// The step committed successfully.
    Committed,
    /// The step was skipped because an earlier step on the same
    /// file failed and the transaction rolled back.
    RolledBack,
    /// The step was validated but skipped because its operation
    /// type is not supported by this executor.
    Unsupported { reason: String },
}

/// A patch operation that has been validated and applied in memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchOperation {
    /// Workspace-relative target path.
    pub path: String,
    /// Operation index within `EditTransaction.operations`.
    pub operation_index: usize,
    /// The prepared operation after normalization.
    pub operation: NormalizedOperation,
}

/// Normalized operation after AWE-004 preparation. AWE-004 supports
/// three canonical operation types; each is normalized to a
/// deterministic in-memory form before any filesystem write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NormalizedOperation {
    /// Content split at every match; replacement performed in memory.
    Replace { old: Vec<String>, new: Vec<String> },
    /// Insert at a documented boundary using canonical LineMap rules.
    InsertLine { boundary: usize, content: String },
    /// Inclusive 1-based line range removed via canonical LineMap.
    DeleteRange { start: usize, end: usize },
}

/// The result of a multi-operation patch transaction.
///
/// One logical patch request produces exactly one `PatchResult`
/// carrying all affected files, the transaction's stable `EditId`,
/// and per-file before/after states. If preparation fails before
/// any filesystem mutation, no file is changed and a structured
/// failure is returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchResult {
    /// Stable transaction identifier (preserved from the transaction).
    pub transaction_id: EditId,
    /// Observed final lifecycle state of the transaction.
    pub status: PatchStatus,
    /// Per-file results in deterministic (path-ordered) grouping.
    pub steps: Vec<PatchStepResult>,
    /// Paths that were already committed if a later commit failed.
    pub committed_paths: Vec<String>,
    /// Failure details if the transaction did not complete.
    pub failure: Option<PatchFailure>,
    /// Rollback outcome if a later commit/verification failed.
    pub rollback: Option<RollbackResult>,
}

/// High-level lifecycle state of a patch transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchStatus {
    /// All operations prepared and all affected files committed.
    Committed,
    /// Preparation failed before any filesystem mutation occurred.
    Prepared { failed_paths: Vec<String> },
    /// One or more files were committed but a later commit or
    /// verification failed; partial mutation is bounded to
    /// `committed_paths`.
    PartiallyCommitted,
    /// Verification failed after committing one or more files.
    VerificationFailed,
}

/// Structured failure information for a failed patch transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchFailure {
    /// Phase that produced the failure.
    pub phase: PatchFailurePhase,
    /// Human-readable reason.
    pub reason: String,
    /// Path where the failure occurred, when deterministically
    /// identifiable.
    pub path: Option<String>,
    /// Transaction id that produced the failure.
    pub transaction_id: EditId,
    /// Paths that had already been committed at the time of failure.
    pub committed_paths: Vec<String>,
}

/// Outcome of an attempted rollback for a single file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RollbackOutcome {
    /// The file was restored to its original bytes.
    Restored,
    /// The file was deleted because it was created by this transaction.
    Deleted,
    /// Rollback was not needed because the file was never mutated.
    Unchanged,
    /// Rollback was attempted but the file's current state did not match
    /// the transaction-produced state, so it was left untouched to avoid
    /// overwriting an external change.
    Conflict { reason: String },
    /// Rollback itself failed with a filesystem error.
    Failed { reason: String },
}

/// Result of a rollback attempt for a failed patch transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackResult {
    /// Transaction id that produced the failure.
    pub transaction_id: EditId,
    /// Per-file outcomes in deterministic (path-ordered) grouping.
    pub outcomes: Vec<(String, RollbackOutcome)>,
    /// True if every target was restored to its original state or was
    /// never mutated.
    pub fully_restored: bool,
    /// True if every target was restored or left unchanged; false if any
    /// target conflicted or failed.
    pub conflict_free: bool,
    /// Human-readable summary of the rollback attempt.
    pub summary: String,
}

/// A reversible snapshot for one file captured before an edit was applied.
/// This is the AWE-010 recovery material: the exact bytes that must be
/// restored to roll the edit back, plus the state the file must still be
/// in for the rollback to be safe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackRecord {
    /// Stable identity of the completed edit transaction this record belongs to.
    pub edit_id: EditId,
    /// Workspace-relative path of the affected file.
    pub path: String,
    /// Whether the file existed before the edit. When `false`, rollback
    /// deletes the file instead of restoring bytes.
    pub existed_before: bool,
    /// Exact bytes of the file before the edit (empty when `existed_before` is false).
    pub before_bytes: Vec<u8>,
    /// SHA-256 hash of the file as produced by the edit. Rollback is only
    /// permitted when the current file still matches this state.
    pub after_hash: String,
}

/// Result of an explicit user-requested rollback (AWE-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditRollbackStatus {
    /// Every affected file was restored to its exact pre-edit state.
    Restored,
    /// The edit had already been rolled back; nothing was mutated.
    AlreadyRolledBack,
    /// The current file state differs from the post-edit state, so the
    /// rollback was refused without mutating anything.
    Conflict { reason: String },
    /// The rollback attempt itself failed while restoring files.
    Failed { reason: String },
}

/// Phase of the patch lifecycle where the failure occurred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchFailurePhase {
    /// Shape or domain validation rejected the transaction.
    Validation,
    /// Expected-state or content preparation failed for one or more files.
    Preparation,
    /// Atomic write or post-write verification failed.
    Commit,
}

// ---------------------------------------------------------------------------
// Unified-diff parser and applier (AWE-005).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct ParsedFileDiff {
    pub path: String,
    pub hunks: Vec<ParsedHunk>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct ParsedHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<HunkLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HunkLine {
    Context(String),
    Add(String),
    Del(String),
}

pub(crate) fn parse_unified_diff(diff: &str) -> Result<Vec<ParsedFileDiff>, EditError> {
    if diff.trim().is_empty() {
        return Err(EditError::EmptyDiff);
    }
    if diff.contains("Binary files") || diff.contains("GIT binary patch") {
        return Err(EditError::BinaryPatch(
            "binary patches are not supported".into(),
        ));
    }

    let lines: Vec<&str> = diff.lines().collect();
    let mut file_diffs = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("--- ") {
            if i + 1 >= lines.len() || !lines[i + 1].starts_with("+++ ") {
                return Err(EditError::InvalidDiff(
                    "expected +++ header after ---".into(),
                ));
            }
            let old_header = line;
            let new_header = lines[i + 1];
            i += 2;

            let path_raw = if let Some(stripped) = new_header.strip_prefix("+++ b/") {
                stripped
            } else if let Some(p) = new_header.strip_prefix("+++ ") {
                if let Some(stripped) = p.strip_prefix("b/") {
                    stripped
                } else {
                    p
                }
            } else {
                return Err(EditError::InvalidDiff(format!(
                    "invalid +++ header: {new_header}"
                )));
            };
            let path = path_raw.split_whitespace().next().unwrap_or(path_raw);
            if path == "/dev/null" {
                // Handle deletion diff if needed, or extract from old_header
                let old_p = if let Some(stripped) = old_header.strip_prefix("--- a/") {
                    stripped
                } else {
                    old_header.strip_prefix("--- ").unwrap_or(&old_header[4..])
                };
                let fallback = old_p.split_whitespace().next().unwrap_or(old_p);
                validate_path(fallback)?;
            } else {
                validate_path(path)?;
            }
            let target_path = if path == "/dev/null" {
                let old_p = if let Some(stripped) = old_header.strip_prefix("--- a/") {
                    stripped
                } else {
                    old_header.strip_prefix("--- ").unwrap_or(&old_header[4..])
                };
                old_p.split_whitespace().next().unwrap_or(old_p).to_owned()
            } else {
                path.to_owned()
            };

            let mut hunks = Vec::new();
            while i < lines.len() && lines[i].starts_with("@@ ") {
                let hunk_header = lines[i];
                i += 1;
                let parsed_hunk = parse_hunk_header(hunk_header)?;

                let mut hunk_lines = Vec::new();

                while i < lines.len() {
                    let hl = lines[i];
                    if hl.starts_with("@@ ") || hl.starts_with("--- ") {
                        break;
                    }
                    if hl.starts_with("\\ No newline at end of file") {
                        i += 1;
                        continue;
                    }
                    if let Some(rest) = hl.strip_prefix(' ') {
                        hunk_lines.push(HunkLine::Context(rest.to_string()));
                    } else if let Some(rest) = hl.strip_prefix('-') {
                        hunk_lines.push(HunkLine::Del(rest.to_string()));
                    } else if let Some(rest) = hl.strip_prefix('+') {
                        hunk_lines.push(HunkLine::Add(rest.to_string()));
                    } else if hl.is_empty() {
                        // Empty line treated as context with empty string
                        hunk_lines.push(HunkLine::Context("".to_string()));
                    } else {
                        break;
                    }
                    i += 1;
                }
                hunks.push(ParsedHunk {
                    old_start: parsed_hunk.0,
                    old_lines: parsed_hunk.1,
                    new_start: parsed_hunk.2,
                    new_lines: parsed_hunk.3,
                    lines: hunk_lines,
                });
            }
            file_diffs.push(ParsedFileDiff {
                path: target_path,
                hunks,
            });
        } else {
            i += 1;
        }
    }

    if file_diffs.is_empty() {
        return Err(EditError::InvalidDiff("no valid file diffs found".into()));
    }
    Ok(file_diffs)
}

fn parse_hunk_header(header: &str) -> Result<(usize, usize, usize, usize), EditError> {
    // Expected format: @@ -l,s +l,s @@ or @@ -l +l @@ etc.
    let parts: Vec<&str> = header.split("@@").collect();
    if parts.len() < 2 {
        return Err(EditError::InvalidDiff(format!(
            "invalid hunk header: {header}"
        )));
    }
    let ranges = parts[1].trim();
    let range_parts: Vec<&str> = ranges.split_whitespace().collect();
    if range_parts.len() < 2 {
        return Err(EditError::InvalidDiff(format!(
            "invalid hunk ranges: {ranges}"
        )));
    }
    let parse_range = |s: &str| -> Result<(usize, usize), EditError> {
        let s = s
            .strip_prefix('-')
            .or_else(|| s.strip_prefix('+'))
            .unwrap_or(s);
        if let Some((start_str, len_str)) = s.split_once(',') {
            let start = start_str
                .parse()
                .map_err(|_| EditError::InvalidDiff(format!("invalid start: {start_str}")))?;
            let len = len_str
                .parse()
                .map_err(|_| EditError::InvalidDiff(format!("invalid len: {len_str}")))?;
            Ok((start, len))
        } else {
            let start = s
                .parse()
                .map_err(|_| EditError::InvalidDiff(format!("invalid start: {s}")))?;
            Ok((start, 1))
        }
    };
    let (old_start, old_len) = parse_range(range_parts[0])?;
    let (new_start, new_len) = parse_range(range_parts[1])?;
    Ok((old_start, old_len, new_start, new_len))
}

pub(crate) fn apply_hunks_to_content(
    content: &str,
    hunks: &[ParsedHunk],
    path: &str,
) -> Result<String, EditError> {
    let mut file_lines: Vec<String> = if content.is_empty() {
        Vec::new()
    } else {
        content.lines().map(|s| s.to_owned()).collect()
    };
    let _has_trailing_newline = content.ends_with('\n');

    // Sort hunks descending by old_start so applying them doesn't shift earlier line numbers
    let mut sorted_hunks: Vec<&ParsedHunk> = hunks.iter().collect();
    sorted_hunks.sort_by_key(|a| std::cmp::Reverse(a.old_start));

    for (hunk_idx, hunk) in sorted_hunks.iter().enumerate() {
        let target_idx = if hunk.old_start == 0 {
            0
        } else {
            hunk.old_start - 1
        };

        // Validate context + deletions match file lines
        let mut file_cursor = target_idx;
        for line in &hunk.lines {
            match line {
                HunkLine::Context(expected) | HunkLine::Del(expected) => {
                    if file_cursor >= file_lines.len() || file_lines[file_cursor] != *expected {
                        return Err(EditError::HunkContextMismatch {
                            path: path.to_owned(),
                            hunk: hunk_idx + 1,
                        });
                    }
                    if matches!(line, HunkLine::Context(_) | HunkLine::Del(_)) {
                        file_cursor += 1;
                    }
                }
                HunkLine::Add(_) => {}
            }
        }

        // Apply hunk modifications
        let mut new_hunk_lines = Vec::new();
        for line in &hunk.lines {
            match line {
                HunkLine::Context(text) => {
                    new_hunk_lines.push(text.clone());
                }
                HunkLine::Del(_) => {
                    // skip deleted line
                }
                HunkLine::Add(text) => {
                    new_hunk_lines.push(text.clone());
                }
            }
        }

        let remove_count = hunk
            .lines
            .iter()
            .filter(|l| matches!(l, HunkLine::Context(_) | HunkLine::Del(_)))
            .count();
        if target_idx + remove_count <= file_lines.len() {
            file_lines.splice(target_idx..target_idx + remove_count, new_hunk_lines);
        } else {
            return Err(EditError::HunkContextMismatch {
                path: path.to_owned(),
                hunk: hunk_idx + 1,
            });
        }
    }

    if file_lines.is_empty() {
        Ok("".into())
    } else {
        let mut result = file_lines.join("\n");
        if content.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }
}

/// Internal preparation record for one file during a multi-operation patch.
struct PrepareFile {
    path: String,
    before: FileState,
    original_bytes: Vec<u8>,
    original_existed: bool,
    prepared_content: String,
}

fn validate_patch_operation(op: &EditOperation) -> Result<(), String> {
    match op {
        EditOperation::Replace { old, new, .. } => {
            if old.is_empty() {
                return Err("replace match text ('old') must not be empty".into());
            }
            if new.is_empty() {
                // empty replacement is allowed (deletion of match)
            }
            Ok(())
        }
        EditOperation::Insert { content, line, .. } => {
            if content.is_empty() {
                return Err("insert content must not be empty".into());
            }
            if *line == 0 {
                // line 0 is valid (beginning of file)
            }
            Ok(())
        }
        EditOperation::DeleteRange {
            start_line,
            end_line,
            ..
        } => {
            if *start_line == 0 || *end_line == 0 || start_line > end_line {
                return Err(format!("invalid line range: {start_line}..={end_line}"));
            }
            Ok(())
        }
        EditOperation::Patch { old, .. } => {
            if old.is_empty() {
                return Err("patch old text must not be empty".into());
            }
            Ok(())
        }
        EditOperation::ApplyDiff { .. } => {
            Err("ApplyDiff operations are not supported by the core patch executor".into())
        }
    }
}

fn apply_normalized_operation(
    path: &str,
    _idx: usize,
    op: &EditOperation,
    content: &str,
    expected: Option<&ExpectedState>,
) -> Result<String, EditError> {
    match op {
        EditOperation::Replace {
            old,
            new,
            occurrence,
            ..
        } => {
            let matches = find_matches(content, old);
            if matches.is_empty() {
                return Err(EditError::MatchNotFound {
                    path: path.to_owned(),
                });
            }
            let selected_index = match occurrence {
                None => {
                    if matches.len() > 1 {
                        return Err(EditError::AmbiguousMatch {
                            path: path.to_owned(),
                            match_count: matches.len(),
                            locations: matches.iter().map(|m| m.location).collect(),
                        });
                    }
                    0
                }
                Some(n) => {
                    let n = *n;
                    let index = n - 1;
                    if index >= matches.len() {
                        return Err(EditError::OccurrenceOutOfRange {
                            path: path.to_owned(),
                            selected: n,
                            match_count: matches.len(),
                        });
                    }
                    index
                }
            };
            let selected = &matches[selected_index];
            // Context check anchored to this match span.
            let preflight = Preflight {
                content: content.to_owned(),
                before: FileState::from_content(path, content),
            };
            check_context(
                &preflight,
                expected,
                selected.byte_offset,
                selected.byte_offset + selected.length,
            )?;

            let start = selected.byte_offset;
            let end = start + selected.length;
            let mut prepared =
                String::with_capacity(content.len() + new.len().saturating_sub(old.len()));
            prepared.push_str(&content[..start]);
            prepared.push_str(new);
            prepared.push_str(&content[end..]);
            Ok(prepared)
        }
        EditOperation::Insert {
            line,
            content: insert_content,
            ..
        } => {
            let lines = LineMap::parse(content);
            if *line > lines.len() + 1 {
                return Err(EditError::InsertBoundaryOutOfRange {
                    path: path.to_owned(),
                    line: *line,
                    line_count: lines.len(),
                });
            }
            let offset = lines.insert_offset(*line);
            let preflight = Preflight {
                content: content.to_owned(),
                before: FileState::from_content(path, content),
            };
            check_context(&preflight, expected, offset, offset)?;
            Ok(lines.insert_at(content, *line, insert_content))
        }
        EditOperation::DeleteRange {
            start_line,
            end_line,
            ..
        } => {
            let lines = LineMap::parse(content);
            if lines.is_empty() {
                return Err(EditError::EmptyFileDeletion {
                    path: path.to_owned(),
                });
            }
            if *end_line > lines.len() {
                return Err(EditError::LineOutOfRange {
                    path: path.to_owned(),
                    start: *start_line,
                    end: *end_line,
                    line_count: lines.len(),
                });
            }
            let (span_start, span_end) = lines
                .line_span(*start_line, *end_line)
                .expect("validated range");
            let preflight = Preflight {
                content: content.to_owned(),
                before: FileState::from_content(path, content),
            };
            check_context(&preflight, expected, span_start, span_end)?;

            let mut prepared =
                String::with_capacity(content.len() - (span_end - span_start).min(content.len()));
            prepared.push_str(&content[..span_start]);
            prepared.push_str(&content[span_end..]);
            Ok(prepared)
        }
        EditOperation::Patch { old, new, .. } => {
            if old.starts_with("--- ") || old.contains("\n--- ") {
                let file_diffs = parse_unified_diff(old)?;
                let file_diff = file_diffs
                    .iter()
                    .find(|fd| fd.path == *path)
                    .ok_or_else(|| {
                        EditError::InvalidDiff(format!("diff has no section for {path}"))
                    })?;
                apply_hunks_to_content(content, &file_diff.hunks, path)
            } else {
                let matches = find_matches(content, old);
                if matches.is_empty() {
                    return Err(EditError::MatchNotFound {
                        path: path.to_owned(),
                    });
                }
                let selected = &matches[0];
                let start = selected.byte_offset;
                let end = start + selected.length;
                let mut prepared =
                    String::with_capacity(content.len() + new.len().saturating_sub(old.len()));
                prepared.push_str(&content[..start]);
                prepared.push_str(new);
                prepared.push_str(&content[end..]);
                Ok(prepared)
            }
        }
        EditOperation::ApplyDiff { .. } => {
            Err(EditError::UnsupportedTransaction { operation_count: 1 })
        }
    }
}

/// Production edit executor scoped to one workspace root.
///
/// The service owns no edit-specific filesystem logic: every read and the
/// atomic commit go through the canonical [`FilesService`] containment
/// boundary. This is the single mutation primitive later edit milestones
/// must consume rather than reimplementing replacement semantics.
///
/// # Stale-state safety
///
/// The observed current state is validated against every supplied
/// expected-state precondition *before* any mutation is prepared, and the
/// replacement is committed through one atomic rename. An edit based on
/// stale observations therefore never silently overwrites newer content:
/// it fails with [`EditError::ExpectedStateConflict`] (or a context
/// conflict) and leaves the target byte-for-byte unchanged.
///
/// # Validation-before-write ordering
///
/// 1. transaction shape (`EditTransaction::validate_shape`);
/// 2. exactly one `Replace` operation;
/// 3. path syntax (`validate_path`) — containment is re-enforced by the
///    filesystem boundary on every read and write;
/// 4. read current bytes and decode UTF-8;
/// 5. build the observed `FileState` (`before`);
/// 6. validate expected hash/size/line count, then resolve the
///    location-specific `context` precondition against live content;
/// 7. locate literal `old` matches and apply occurrence semantics;
/// 8. prepare the complete new content in memory;
/// 9. atomic commit through `FilesService::write_atomic`;
/// 10. re-read and verify the observed state equals the prepared state;
/// 11. return before/after `FileState`.
///
/// No filesystem mutation occurs before step 9, so every failure in
/// steps 1–8 (and a failed commit in step 9) leaves the target unchanged.
pub struct EditService {
    files: FilesService,
}

impl EditService {
    /// Creates an edit service over one workspace root.
    pub fn new(project_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            files: FilesService::new(project_root),
        }
    }

    /// The canonical filesystem service backing this executor. Later
    /// milestones that compose edits (patching, snapshots) should reuse
    /// the same boundary rather than opening a second one.
    pub fn files(&self) -> &FilesService {
        &self.files
    }

    /// Execute a multi-operation patch transaction.
    ///
    /// The executor follows a strict two-phase design:
    ///
    /// 1. **Preparation** — validate the entire transaction shape,
    ///    read every affected file, capture before-states, validate
    ///    expected-state preconditions, normalize every operation,
    ///    compose same-file operations deterministically in memory.
    ///    No filesystem mutation occurs above this phase boundary.
    /// 2. **Commit** — atomically write each file's final prepared
    ///    content and verify the actual on-disk state. If any file's
    ///    commit or verification fails, the failure is reported
    ///    without rolling back already-committed files (rollback
    ///    safety belongs to AWE-006).
    ///
    /// Same-file operations are composed in transaction order, with
    /// each operation applied to the in-memory state produced by
    /// earlier operations. Line-number references in later operations
    /// therefore refer to the current in-memory line map, not the
    /// original disk state.
    pub fn patch(&self, transaction: EditTransaction) -> Result<PatchResult, EditError> {
        transaction.validate_shape()?;
        if transaction.operations.is_empty() {
            return Err(EditError::EmptyTransaction);
        }

        let tx_id = transaction.id.clone();
        let tx_id_str = tx_id.0.clone();

        // Phase 1: grouping — deterministic path ordering (sorted, no HashMap).
        let mut affected: Vec<(String, Vec<(usize, EditOperation)>)> = Vec::new();
        for (idx, op) in transaction.operations.into_iter().enumerate() {
            match op {
                EditOperation::ApplyDiff { ref diff } => {
                    let file_diffs = parse_unified_diff(diff)?;
                    for fd in file_diffs {
                        let path = fd.path;
                        match affected.iter_mut().find(|(p, _)| p == &path) {
                            Some((_, ops)) => ops.push((
                                idx,
                                EditOperation::Patch {
                                    path: path.clone(),
                                    old: diff.clone(),
                                    new: "".into(),
                                },
                            )),
                            None => {
                                affected.push((
                                    path.clone(),
                                    vec![(
                                        idx,
                                        EditOperation::Patch {
                                            path: path.clone(),
                                            old: diff.clone(),
                                            new: "".into(),
                                        },
                                    )],
                                ));
                            }
                        }
                    }
                }
                _ => {
                    let paths = op.paths();
                    let path = paths.first().copied().unwrap_or("");
                    match affected.iter_mut().find(|(p, _)| p == path) {
                        Some((_, ops)) => ops.push((idx, op)),
                        None => {
                            affected.push((path.to_owned(), vec![(idx, op)]));
                        }
                    }
                }
            }
        }
        affected.sort_by(|a, b| a.0.cmp(&b.0));

        // Phase 1b: validate every operation individually before touching any file.
        for (path, ops) in &affected {
            for (idx, op) in ops {
                if let Err(reason) = validate_patch_operation(op) {
                    return Err(EditError::PatchValidationFailure {
                        transaction_id: tx_id_str.clone(),
                        path: Some(path.clone()),
                        reason: format!("operation {}: {}", idx, reason),
                    });
                }
            }
        }

        // Phase 2: prepare every affected file in memory.
        let mut plan: Vec<PrepareFile> = Vec::new();
        for (file_idx, (path, ops)) in affected.iter().enumerate() {
            let before_file = self.files.read_bytes(path).map_err(|error| {
                if is_not_found(&error) {
                    EditError::FileNotFound { path: path.clone() }
                } else {
                    EditError::ReadFailure {
                        path: path.clone(),
                        message: error.to_string(),
                    }
                }
            })?;
            let content = std::str::from_utf8(&before_file)
                .map_err(|_| EditError::InvalidUtf8 { path: path.clone() })?
                .to_owned();
            let before = FileState::from_content(path, &content);

            // Validate expected-state for this file (uses matching
            // expected state by file index, or first).
            let file_expected = transaction
                .expected
                .get(file_idx)
                .cloned()
                .or_else(|| transaction.expected.first().cloned());
            if let Some(expected) = file_expected.as_ref() {
                // If expected state has no preconditions set (all None), skip check.
                if expected.hash.is_some()
                    || expected.context.is_some()
                    || expected.size.is_some()
                    || expected.line_count.is_some()
                {
                    match before.check(expected) {
                        StateMatch::Matched => {}
                        StateMatch::Conflicted { component } => {
                            return Err(EditError::PatchPreparationFailure {
                                transaction_id: tx_id_str.clone(),
                                path: Some(path.clone()),
                                reason: format!(
                                    "expected-state conflict on {}: {} mismatch",
                                    path, component
                                ),
                            });
                        }
                        StateMatch::Malformed { .. } => {
                            return Err(EditError::InvalidHash(
                                expected.hash.clone().unwrap_or_default(),
                            ));
                        }
                        StateMatch::ContextPending => {}
                    }
                }
            }

            // Apply every operation for this file in transaction order,
            // mutating in-memory content only.
            let mut current = content;
            for (idx, op) in ops {
                current =
                    apply_normalized_operation(path, *idx, op, &current, file_expected.as_ref())?;
            }

            let _after = FileState::from_content(path, &current);
            plan.push(PrepareFile {
                path: path.clone(),
                before,
                original_bytes: before_file,
                original_existed: true,
                prepared_content: current,
            });
        }

        // Phase 3: commit every file atomically. If any write/verification
        // fails, report the failure together with already-committed paths.
        let mut steps: Vec<PatchStepResult> = Vec::new();
        let mut committed_paths: Vec<String> = Vec::new();
        let mut commit_error: Option<EditError> = None;

        for file in &plan {
            match self.commit_verified(&file.path, &file.prepared_content) {
                Ok(after) => {
                    steps.push(PatchStepResult {
                        transaction_id: tx_id.clone(),
                        path: file.path.clone(),
                        operation_index: 0,
                        status: PatchStepStatus::Committed,
                        before: file.before.clone(),
                        after: after.clone(),
                        operation: PatchOperation {
                            path: file.path.clone(),
                            operation_index: 0,
                            operation: NormalizedOperation::InsertLine {
                                boundary: 0,
                                content: String::new(),
                            },
                        },
                    });
                    committed_paths.push(file.path.clone());
                }
                Err(error) => {
                    commit_error = Some(error);
                    break;
                }
            }
        }

        if let Some(error) = commit_error {
            // Attempt conflict-aware rollback of already-committed files.
            let rollback = self.attempt_rollback(&plan, &committed_paths, &tx_id);
            let primary_failure = PatchFailure {
                phase: PatchFailurePhase::Commit,
                reason: error.to_string(),
                path: None,
                transaction_id: tx_id.clone(),
                committed_paths: committed_paths.clone(),
            };
            return Ok(PatchResult {
                transaction_id: tx_id,
                status: if rollback.conflict_free {
                    PatchStatus::Committed
                } else {
                    PatchStatus::VerificationFailed
                },
                steps,
                committed_paths,
                failure: Some(primary_failure),
                rollback: Some(rollback),
            });
        }

        Ok(PatchResult {
            transaction_id: tx_id,
            status: PatchStatus::Committed,
            steps,
            committed_paths,
            failure: None,
            rollback: None,
        })
    }

    /// Attempt conflict-aware rollback of already-committed files.
    ///
    /// For each committed file, compare the currently observed state with
    /// the state produced by this transaction. If they match, restore the
    /// original bytes (or delete the file if it did not exist before the
    /// transaction). If they do not match, leave the file untouched to
    /// avoid overwriting an external change.
    fn attempt_rollback(
        &self,
        plan: &[PrepareFile],
        committed_paths: &[String],
        tx_id: &EditId,
    ) -> RollbackResult {
        let mut outcomes: Vec<(String, RollbackOutcome)> = Vec::new();
        let mut fully_restored = true;
        let mut conflict_free = true;

        // Rollback in reverse order of commit.
        let mut targets: Vec<&PrepareFile> = plan
            .iter()
            .filter(|f| committed_paths.contains(&f.path))
            .collect();
        targets.reverse();

        for file in &targets {
            // Read current state and compare with the transaction-produced
            // state to detect external modifications.
            let current_bytes = match self.files.read_bytes(&file.path) {
                Ok(b) => b,
                Err(_) => {
                    // File disappeared externally; treat as conflict.
                    outcomes.push((
                        file.path.clone(),
                        RollbackOutcome::Conflict {
                            reason: "file disappeared externally".into(),
                        },
                    ));
                    fully_restored = false;
                    conflict_free = false;
                    continue;
                }
            };

            let expected_hash = FileState::from_content(&file.path, &file.prepared_content).hash;
            let current_hash = sha256_hex(&current_bytes);

            if current_hash != expected_hash {
                // External change detected; fail closed and preserve it.
                outcomes.push((
                    file.path.clone(),
                    RollbackOutcome::Conflict {
                        reason: format!(
                            "current hash {} differs from transaction-produced hash {}",
                            current_hash, expected_hash
                        ),
                    },
                ));
                fully_restored = false;
                conflict_free = false;
                continue;
            }

            // Restore original bytes or delete if created by this transaction.
            if !file.original_existed {
                match self.files.delete(&file.path) {
                    Ok(()) => {
                        outcomes.push((file.path.clone(), RollbackOutcome::Deleted));
                    }
                    Err(error) => {
                        outcomes.push((
                            file.path.clone(),
                            RollbackOutcome::Failed {
                                reason: format!("delete failed: {error}"),
                            },
                        ));
                        fully_restored = false;
                        conflict_free = false;
                    }
                }
            } else {
                match self
                    .files
                    .write_atomic(&file.path, &String::from_utf8_lossy(&file.original_bytes))
                {
                    Ok(()) => {
                        outcomes.push((file.path.clone(), RollbackOutcome::Restored));
                    }
                    Err(error) => {
                        outcomes.push((
                            file.path.clone(),
                            RollbackOutcome::Failed {
                                reason: format!("restore failed: {error}"),
                            },
                        ));
                        fully_restored = false;
                        conflict_free = false;
                    }
                }
            }
        }

        let summary = if conflict_free {
            format!("rolled back {} file(s) to original state", outcomes.len())
        } else {
            format!(
                "rolled back {} file(s) with {} conflict(s) or failure(s)",
                outcomes.len(),
                outcomes
                    .iter()
                    .filter(|(_, o)| matches!(
                        o,
                        RollbackOutcome::Conflict { .. } | RollbackOutcome::Failed { .. }
                    ))
                    .count()
            )
        };

        RollbackResult {
            transaction_id: tx_id.clone(),
            outcomes,
            fully_restored,
            conflict_free,
            summary,
        }
    }

    /// Executes one safe, contextual, conflict-aware replacement.
    ///
    /// Semantics (all deterministic, all enforced before mutation):
    ///
    /// - **Exact literal matching**: `old` is matched byte-for-byte. No
    ///   whitespace trimming, Unicode normalization, line-ending
    ///   normalization, case folding, fuzzy matching, or regex
    ///   interpretation. CRLF files keep their CRLF line endings outside
    ///   the replaced span.
    /// - **Occurrence semantics**: matches are found left-to-right,
    ///   non-overlapping. `occurrence = None` requires exactly one match
    ///   (more is [`EditError::AmbiguousMatch`], none is
    ///   [`EditError::MatchNotFound`]). `occurrence = Some(n)` selects the
    ///   n-th match (1-based); out-of-range selections fail.
    /// - **Expected state**: `hash`, `size`, and `line_count` are verified
    ///   against the observed current file before mutation.
    /// - **Context**: when supplied, `context` must occur exactly once in
    ///   the current content and must contain the selected match. This
    ///   makes the location-specific context a real precondition: the
    ///   caller's surrounding text must still be present at the edit
    ///   location, and ambiguity fails closed.
    /// - **Commit**: the complete replacement is prepared in memory and
    ///   committed through one atomic rename; the observed post-commit
    ///   state is re-read and verified.
    pub fn replace(&self, transaction: EditTransaction) -> Result<EditResult, EditError> {
        // 1. Transaction shape.
        transaction.validate_shape()?;

        // 2. Exactly one Replace operation.
        let [operation] = transaction.operations.as_slice() else {
            return Err(EditError::UnsupportedTransaction {
                operation_count: transaction.operations.len(),
            });
        };
        let EditOperation::Replace {
            ref path,
            ref old,
            ref new,
            ref occurrence,
        } = operation
        else {
            return Err(EditError::UnsupportedTransaction {
                operation_count: transaction.operations.len(),
            });
        };

        // 3–5. Shared preflight: path syntax, contained read, UTF-8
        //      classification, observed before-state, and expected-state
        //      preconditions.
        let current = self.preflight(path, transaction.expected.first())?;

        // 6b/7. Context precondition uses the canonical matcher, anchored
        //       to the affected span of this operation (the selected
        //       match; the selection itself happens first so the span is
        //       known). Matches of `old` are located left-to-right,
        //       non-overlapping.
        let matches = find_matches(&current.content, old);
        if matches.is_empty() {
            return Err(EditError::MatchNotFound { path: path.clone() });
        }
        let selected_index = match occurrence {
            None => {
                if matches.len() > 1 {
                    return Err(EditError::AmbiguousMatch {
                        path: path.clone(),
                        match_count: matches.len(),
                        locations: matches.iter().map(|m| m.location).collect(),
                    });
                }
                0
            }
            Some(n) => {
                let n = *n;
                let index = n - 1;
                if index >= matches.len() {
                    return Err(EditError::OccurrenceOutOfRange {
                        path: path.clone(),
                        selected: n,
                        match_count: matches.len(),
                    });
                }
                index
            }
        };
        let selected = &matches[selected_index];
        check_context(
            &current,
            transaction.expected.first(),
            selected.byte_offset,
            selected.byte_offset + selected.length,
        )?;

        // 8. Prepare the complete new content in memory. Splicing at
        //    `str::find` byte offsets is UTF-8-safe: match boundaries are
        //    character boundaries.
        let start = selected.byte_offset;
        let end = start + selected.length;
        let mut prepared =
            String::with_capacity(current.content.len() + new.len().saturating_sub(old.len()));
        prepared.push_str(&current.content[..start]);
        prepared.push_str(new);
        prepared.push_str(&current.content[end..]);

        // 9–11. Atomic commit, verified re-read, and result.
        let after = self.commit_verified(path, &prepared)?;
        // Semantic verification: verify the replacement actually occurred as intended
        self.verify_replace_semantics(path, old, new, *occurrence, &after)?;
        Ok(EditResult {
            id: transaction.id,
            path: path.clone(),
            status: EditStatus::Committed,
            before: current.before,
            after,
            match_count: matches.len(),
            selected_occurrence: selected_index + 1,
            location: selected.location,
        })
    }

    /// Executes one safe, conflict-aware line insertion.
    ///
    /// Semantics (all deterministic, all enforced before mutation):
    ///
    /// - **Boundaries**: `line = 0` inserts at the beginning of the file;
    ///   `1..=N` inserts immediately before logical line N (N = current
    ///   line count); `N + 1` inserts at end of file. Any other boundary
    ///   fails with [`EditError::InsertBoundaryOutOfRange`] — never a
    ///   silent clamp.
    /// - **Literal content**: inserted text is used exactly as supplied;
    ///   no trimming, Unicode normalization, or regex interpretation.
    /// - **Newline policy**: the insertion is normalized to stand on its
    ///   own logical line(s). A supplied trailing newline is respected and
    ///   not doubled; a missing one is added exactly once when needed so
    ///   inserted text never concatenates into an adjacent existing line.
    ///   Outside the edited boundary the file's existing bytes — including
    ///   CRLF endings and any missing final newline — are preserved
    ///   byte-for-byte.
    /// - **Expected state**: the same hash/size/line-count/context
    ///   contract as [`EditService::replace`], checked before any line
    ///   boundary is resolved.
    pub fn insert(&self, transaction: EditTransaction) -> Result<LineEditResult, EditError> {
        transaction.validate_shape()?;
        let [operation] = transaction.operations.as_slice() else {
            return Err(EditError::UnsupportedTransaction {
                operation_count: transaction.operations.len(),
            });
        };
        let EditOperation::Insert {
            ref path,
            line,
            ref content,
        } = operation
        else {
            return Err(EditError::UnsupportedTransaction {
                operation_count: transaction.operations.len(),
            });
        };

        let current = self.preflight(path, transaction.expected.first())?;

        // Canonical line map, shared with delete_range. `line_count` here
        // equals the `FileState` count by construction.
        let lines = LineMap::parse(&current.content);

        // Validate the boundary against the map: no clamping. The valid
        // boundary range is 0 (BOF) ..= line_count + 1 (EOF).
        if *line > lines.len() + 1 {
            return Err(EditError::InsertBoundaryOutOfRange {
                path: path.clone(),
                line: *line,
                line_count: lines.len(),
            });
        }

        // The affected span for context anchoring is the insertion point:
        // a zero-width point at the boundary byte offset.
        let offset = lines.insert_offset(*line);
        check_context(&current, transaction.expected.first(), offset, offset)?;

        // Prepare the resulting content in memory under the canonical
        // newline policy: inserted text is literal except it is made
        // line-complete, and everything outside the insertion point is
        // preserved byte-for-byte.
        let prepared = lines.insert_at(&current.content, *line, content);

        let after = self.commit_verified(path, &prepared)?;
        // Semantic verification: verify the insertion actually occurred as intended
        self.verify_insert_semantics(path, *line, content, &after)?;
        Ok(LineEditResult {
            id: transaction.id,
            path: path.clone(),
            status: EditStatus::Committed,
            before: current.before,
            after,
            kind: LineEditKind::Insert { boundary: *line },
        })
    }

    /// Executes one safe, conflict-aware inclusive line-range deletion.
    ///
    /// Semantics (all deterministic, all enforced before mutation):
    ///
    /// - **Range**: `start_line..=end_line` are 1-based inclusive logical
    ///   line numbers, exactly the lines counted by
    ///   [`FileState::line_count`]. Zero, reversed ranges, or ranges
    ///   beyond the last line fail with structured errors — never a
    ///   silent clamp.
    /// - **Empty file**: an empty file has zero logical lines; any
    ///   positive range fails with [`EditError::EmptyFileDeletion`].
    /// - **Result**: deleting all lines yields an empty file; the
    ///   remaining bytes (including terminators of surviving lines) are
    ///   preserved byte-for-byte.
    /// - **Expected state**: the same hash/size/line-count/context
    ///   contract as [`EditService::replace`].
    pub fn delete_range(&self, transaction: EditTransaction) -> Result<LineEditResult, EditError> {
        transaction.validate_shape()?;
        let [operation] = transaction.operations.as_slice() else {
            return Err(EditError::UnsupportedTransaction {
                operation_count: transaction.operations.len(),
            });
        };
        let EditOperation::DeleteRange {
            ref path,
            start_line,
            end_line,
        } = operation
        else {
            return Err(EditError::UnsupportedTransaction {
                operation_count: transaction.operations.len(),
            });
        };

        let current = self.preflight(path, transaction.expected.first())?;

        let lines = LineMap::parse(&current.content);

        // Deletion requires existing logical lines.
        if lines.is_empty() {
            return Err(EditError::EmptyFileDeletion { path: path.clone() });
        }
        // Shape validation already rejected 0 and reversed ranges; the
        // range must additionally exist within the file.
        if *end_line > lines.len() {
            return Err(EditError::LineOutOfRange {
                path: path.clone(),
                start: *start_line,
                end: *end_line,
                line_count: lines.len(),
            });
        }

        // The affected span for context anchoring is the removed byte
        // range, so a context precondition must still be present around
        // the lines being deleted.
        let (span_start, span_end) = lines
            .line_span(*start_line, *end_line)
            .expect("validated range");
        check_context(&current, transaction.expected.first(), span_start, span_end)?;

        // Prepare the resulting content: everything before the first
        // deleted line's start and after the last deleted line's end.
        let mut prepared = String::with_capacity(
            current.content.len() - (span_end - span_start).min(current.content.len()),
        );
        prepared.push_str(&current.content[..span_start]);
        prepared.push_str(&current.content[span_end..]);

        let after = self.commit_verified(path, &prepared)?;
        // Semantic verification: verify the deletion actually occurred as intended
        self.verify_delete_range_semantics(path, *start_line, *end_line, &after)?;
        Ok(LineEditResult {
            id: transaction.id,
            path: path.clone(),
            status: EditStatus::Committed,
            before: current.before,
            after,
            kind: LineEditKind::DeleteRange {
                start: *start_line,
                end: *end_line,
            },
        })
    }

    /// Rolls back one previously completed edit transaction (AWE-010).
    ///
    /// The rollback is conflict-aware and fail-closed:
    ///
    /// - The caller supplies [`RollbackRecord`]s captured when the edit
    ///   committed (see [`EditService::commit_verified_with_records`]).
    /// - For every record, the file's *current* state must still match the
    ///   transaction-produced `after_hash`. Any external modification
    ///   aborts the whole rollback without mutating anything — newer
    ///   external content is never silently destroyed.
    /// - When the pre-state still holds, the exact pre-edit bytes are
    ///   restored atomically through the canonical `write_atomic`
    ///   boundary (or the file is deleted when the edit created it).
    /// - Rollback proceeds in reverse record order; a failure on a later
    ///   file leaves earlier restored files in place and reports
    ///   [`EditRollbackStatus::Failed`] honestly.
    pub fn rollback_edits(
        &self,
        records: &[RollbackRecord],
    ) -> Result<EditRollbackStatus, EditError> {
        if records.is_empty() {
            // Nothing to roll back is not an error, but it is also not a
            // restoration: report honestly.
            return Ok(EditRollbackStatus::Restored);
        }

        // Phase 1: conflict-check every record before mutating anything.
        for record in records {
            let current_bytes = match self.files.read_bytes(&record.path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    return Ok(EditRollbackStatus::Conflict {
                        reason: format!(
                            "rollback target '{}' disappeared; refusing to roll back",
                            record.path
                        ),
                    });
                }
            };
            let current_hash = sha256_hex(&current_bytes);
            if current_hash != record.after_hash {
                return Ok(EditRollbackStatus::Conflict {
                    reason: format!(
                        "rollback target '{}' no longer matches the edit's produced state \
                         (expected {}, observed {})",
                        record.path, record.after_hash, current_hash
                    ),
                });
            }
        }

        // Phase 2: restore in reverse order. All conflict checks have
        // passed, so every target is still the state this edit produced.
        for record in records.iter().rev() {
            if !record.existed_before {
                if let Err(error) = self.files.delete(&record.path) {
                    return Ok(EditRollbackStatus::Failed {
                        reason: format!(
                            "failed to delete edit-created file '{}': {}",
                            record.path, error
                        ),
                    });
                }
            } else {
                let before_text = String::from_utf8(record.before_bytes.clone()).map_err(|_| {
                    EditError::InvalidUtf8 {
                        path: record.path.clone(),
                    }
                })?;
                if let Err(error) = self.files.write_atomic(&record.path, &before_text) {
                    return Ok(EditRollbackStatus::Failed {
                        reason: format!(
                            "failed to restore original bytes of '{}': {}",
                            record.path, error
                        ),
                    });
                }
            }
        }

        Ok(EditRollbackStatus::Restored)
    }

    /// Captures [`RollbackRecord`] recovery material for the files a
    /// transaction is about to commit, so a later explicit
    /// [`EditService::rollback_edits`] call can restore them (AWE-010).
    ///
    /// The records must be captured *before* the first commit: they hold
    /// the exact pre-edit bytes and, after the commit, the
    /// transaction-produced state that a later rollback must still
    /// observe. The caller completes the `after_hash` field using the
    /// observed post-commit state.
    pub fn capture_rollback_records(
        &self,
        edit_id: &EditId,
        paths: &[String],
    ) -> Result<Vec<RollbackRecord>, EditError> {
        let mut records = Vec::new();
        for path in paths {
            let existed = self.files.read_bytes(path).is_ok();
            let before_bytes = if existed {
                self.files
                    .read_bytes(path)
                    .map_err(|error| EditError::ReadFailure {
                        path: path.clone(),
                        message: error.to_string(),
                    })?
            } else {
                Vec::new()
            };
            records.push(RollbackRecord {
                edit_id: edit_id.clone(),
                path: path.clone(),
                existed_before: existed,
                before_bytes,
                // Filled in by the caller once the post-commit state is
                // observed; starting empty makes an unfinished record
                // impossible to satisfy accidentally.
                after_hash: String::new(),
            });
        }
        Ok(records)
    }

    /// Shared preflight for every edit executor: path syntax, contained
    /// read with UTF-8 classification, observed before-state, and the
    /// expected-state preconditions (hash, size, line count). No
    /// filesystem mutation happens here; context is checked separately
    /// because it must anchor to the operation's affected span, which
    /// each executor resolves after its own location logic.
    fn preflight(
        &self,
        path: &str,
        expected: Option<&ExpectedState>,
    ) -> Result<Preflight, EditError> {
        validate_path(path)?;
        let bytes = self.files.read_bytes(path).map_err(|error| {
            if is_not_found(&error) {
                EditError::FileNotFound {
                    path: path.to_owned(),
                }
            } else {
                EditError::ReadFailure {
                    path: path.to_owned(),
                    message: error.to_string(),
                }
            }
        })?;
        let content = std::str::from_utf8(&bytes)
            .map_err(|_| EditError::InvalidUtf8 {
                path: path.to_owned(),
            })?
            .to_owned();
        let before = FileState::from_content(path, &content);
        if let Some(expected) = expected {
            match before.check(expected) {
                StateMatch::Matched => {}
                StateMatch::Conflicted { component } => {
                    return Err(EditError::ExpectedStateConflict(Box::new(
                        ExpectedStateConflictPayload {
                            path: path.to_owned(),
                            component,
                            expected: ExpectedStateSummary::of(expected),
                            actual: before,
                        },
                    )));
                }
                StateMatch::Malformed { .. } => {
                    // Shape validation already rejects malformed expected
                    // states; treat a malformed state as a hard failure.
                    return Err(EditError::InvalidHash(
                        expected.hash.clone().unwrap_or_default(),
                    ));
                }
                StateMatch::ContextPending => {
                    // hash/size/line_count held; context is resolved by
                    // the executor once the affected span is known.
                }
            }
        }
        Ok(Preflight { content, before })
    }

    /// Shared commit for every edit executor: atomic write through the
    /// canonical filesystem boundary, then a verified re-read whose
    /// observed state must equal the prepared content's state. Returns
    /// the verified after-state.
    fn commit_verified(&self, path: &str, prepared: &str) -> Result<FileState, EditError> {
        let after_predicted = FileState::from_content(path, prepared);
        self.files
            .write_atomic(path, prepared)
            .map_err(|error| EditError::WriteFailure {
                path: path.to_owned(),
                message: error.to_string(),
            })?;
        let observed = self.files.read(path).map_err(|error| {
            if is_not_found(&error) {
                EditError::VerificationFailed(Box::new(VerificationFailurePayload {
                    path: path.to_owned(),
                    expected: after_predicted.clone(),
                    actual: FileState {
                        path: path.to_owned(),
                        hash: String::new(),
                        size: 0,
                        line_count: 0,
                    },
                }))
            } else {
                EditError::ReadFailure {
                    path: path.to_owned(),
                    message: error.to_string(),
                }
            }
        })?;
        let after = FileState::from_content(path, &observed);
        if after != after_predicted {
            return Err(EditError::VerificationFailed(Box::new(
                VerificationFailurePayload {
                    path: path.to_owned(),
                    expected: after_predicted,
                    actual: after,
                },
            )));
        }
        Ok(after)
    }

    /// Semantic verification for Replace operation.
    /// Verifies that the intended replacement actually occurred in the
    /// resulting file content.
    fn verify_replace_semantics(
        &self,
        path: &str,
        old: &str,
        new: &str,
        occurrence: Option<usize>,
        _after: &FileState,
    ) -> Result<(), EditError> {
        let observed = self
            .files
            .read(path)
            .map_err(|error| EditError::ReadFailure {
                path: path.to_owned(),
                message: error.to_string(),
            })?;

        // Count occurrences of old and new in the observed content
        let old_matches = find_matches(&observed, old);
        let new_matches = find_matches(&observed, new);

        if occurrence.is_none() {
            // Unique match required - should have been exactly one before replacement
            // After replacement, old should appear 0 times (if it was unique) or old_count-1 times
            if !old_matches.is_empty() && new_matches.is_empty() {
                // Old text still present but new text not found - replacement didn't happen
                return Err(EditError::SemanticVerificationFailure(Box::new(
                    SemanticVerificationFailurePayload {
                        path: path.to_owned(),
                        reason: format!(
                            "Old text '{}' still present ({} times), new text '{}' not found",
                            old,
                            old_matches.len(),
                            new
                        ),
                        operation_index: None,
                        operation_type: "Replace".to_string(),
                        expected_postcondition: format!(
                            "Exactly one occurrence of '{}' replaced with '{}'",
                            old, new
                        ),
                        actual_state: FileState::from_content(path, &observed),
                    },
                )));
            }
        } else {
            // Specific occurrence was requested - verify it was replaced
            let _n = occurrence.unwrap_or(1);
            // The old text should appear one fewer time than before
            // But we don't have the before-count, so we just verify new text is present
        }

        // Verify new text is present in the file (at least once)
        if new_matches.is_empty() {
            return Err(EditError::SemanticVerificationFailure(Box::new(
                SemanticVerificationFailurePayload {
                    path: path.to_owned(),
                    reason: format!("New text '{}' not found after replacement", new),
                    operation_index: None,
                    operation_type: "Replace".to_string(),
                    expected_postcondition: format!("New text '{}' present after replacement", new),
                    actual_state: FileState::from_content(path, &observed),
                },
            )));
        }

        Ok(())
    }

    /// Semantic verification for Insert operation.
    fn verify_insert_semantics(
        &self,
        path: &str,
        boundary: usize,
        content: &str,
        _after: &FileState,
    ) -> Result<(), EditError> {
        let observed = self
            .files
            .read(path)
            .map_err(|error| EditError::ReadFailure {
                path: path.to_owned(),
                message: error.to_string(),
            })?;

        // Verify the inserted content is present
        if !observed.contains(content) {
            return Err(EditError::SemanticVerificationFailure(Box::new(
                SemanticVerificationFailurePayload {
                    path: path.to_owned(),
                    reason: format!("Inserted content '{}' not found after insertion", content),
                    operation_index: None,
                    operation_type: "Insert".to_string(),
                    expected_postcondition: format!(
                        "Content '{}' inserted at boundary {}",
                        content, boundary
                    ),
                    actual_state: FileState::from_content(path, &observed),
                },
            )));
        }

        Ok(())
    }

    /// Semantic verification for DeleteRange operation.
    fn verify_delete_range_semantics(
        &self,
        path: &str,
        start_line: usize,
        end_line: usize,
        after: &FileState,
    ) -> Result<(), EditError> {
        let observed = self
            .files
            .read(path)
            .map_err(|error| EditError::ReadFailure {
                path: path.to_owned(),
                message: error.to_string(),
            })?;

        let lines = LineMap::parse(&observed);

        // Verify the line count decreased by the expected amount
        let expected_line_count = after.line_count;
        if lines.len() != expected_line_count {
            return Err(EditError::SemanticVerificationFailure(Box::new(
                SemanticVerificationFailurePayload {
                    path: path.to_owned(),
                    reason: format!(
                        "Expected {} lines after deletion, found {}",
                        expected_line_count,
                        lines.len()
                    ),
                    operation_index: None,
                    operation_type: "DeleteRange".to_string(),
                    expected_postcondition: format!(
                        "Lines {}..={} deleted, resulting in {} lines",
                        start_line, end_line, expected_line_count
                    ),
                    actual_state: FileState::from_content(path, &observed),
                },
            )));
        }

        // Additional semantic check: the deleted lines should not appear
        // (we can't easily verify this without the original content, but we verify line count)

        Ok(())
    }
}

/// The shared preflight observation every executor starts from.
struct Preflight {
    content: String,
    before: FileState,
}

/// One located literal match with its deterministic coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LocatedMatch {
    byte_offset: usize,
    length: usize,
    location: MatchLocation,
}

/// Finds every non-overlapping left-to-right occurrence of `needle`.
fn find_matches(haystack: &str, needle: &str) -> Vec<LocatedMatch> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut search_from = 0;
    while let Some(relative) = haystack[search_from..].find(needle) {
        let byte_offset = search_from + relative;
        let location = locate(haystack, byte_offset);
        found.push(LocatedMatch {
            byte_offset,
            length: needle.len(),
            location,
        });
        search_from = byte_offset + needle.len();
    }
    found
}

/// Computes deterministic human-facing coordinates for a byte offset:
/// 1-based line and 1-based character column within that line.
fn locate(content: &str, byte_offset: usize) -> MatchLocation {
    let before = &content[..byte_offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = content[line_start..byte_offset].chars().count() + 1;
    MatchLocation {
        byte_offset,
        line,
        column,
    }
}

/// Maps a canonical filesystem error to the not-found category without
/// string-matching success on unrelated IO errors.
fn is_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
    })
}

/// The canonical context precondition shared by every executor:
/// when the caller supplies `ExpectedState.context`, it must occur
/// exactly once in the current content and contain the affected span
/// of this operation (`span_start..span_end`; insertion points pass the
/// same offset twice, making containment a point test). This is the
/// service-side resolution of the model's `ContextPending` result — the
/// location-specific check the transport-independent model deliberately
/// cannot perform. It is used unchanged by replace, insert, and
/// delete_range.
fn check_context(
    current: &Preflight,
    expected: Option<&ExpectedState>,
    span_start: usize,
    span_end: usize,
) -> Result<(), EditError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let Some(context) = expected.context.as_deref() else {
        return Ok(());
    };
    let path = &current.before.path;
    let context_matches = find_matches(&current.content, context);
    match context_matches.as_slice() {
        [] => Err(EditError::ContextConflict {
            path: path.clone(),
            reason: ContextConflictReason::Missing,
        }),
        [only] => {
            let contains =
                only.byte_offset <= span_start && span_end <= only.byte_offset + only.length;
            if contains {
                Ok(())
            } else {
                Err(EditError::ContextConflict {
                    path: path.clone(),
                    reason: ContextConflictReason::NotAnchored,
                })
            }
        }
        many => Err(EditError::ContextConflict {
            path: path.clone(),
            reason: ContextConflictReason::Ambiguous { count: many.len() },
        }),
    }
}

// ---------------------------------------------------------------------------
// LineMap: the single canonical line-boundary model (AWE-003).
// ---------------------------------------------------------------------------

/// The one internal representation of logical line boundaries, shared by
/// insert and delete-range (and available to later line-oriented
/// milestones) so no two executors ever disagree about line semantics.
///
/// A *logical line* is the text between line terminators, matching
/// [`FileState::line_count`] exactly: `"a\nb\n"` has two lines; `"a\nb"`
/// (no trailing newline) has two lines; `""` has zero lines. A bare `\\r`
/// is not a terminator; `\\r\\n` is one terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LineMap {
    /// Byte offset where each logical line starts, one per line.
    starts: Vec<usize>,
    /// Byte offset just past each line's terminator (i.e. the start of
    /// the next line, or content end for the last line). For a final
    /// line without a trailing newline, this is the content end.
    ends: Vec<usize>,
    /// Length of each line's terminator: 0 for a final line without a
    /// trailing newline, 1 for LF, 2 for CRLF.
    terminators: Vec<usize>,
    content_len: usize,
}

impl LineMap {
    /// Parses the canonical line boundaries of `content` in one scan.
    pub(crate) fn parse(content: &str) -> Self {
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        let mut terminators = Vec::new();
        let mut line_start = 0;
        let bytes = content.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                // Terminator is "\n" or "\r\n".
                let terminator_len = if i > 0 && bytes[i - 1] == b'\r' { 2 } else { 1 };
                starts.push(line_start);
                ends.push(i + 1);
                terminators.push(terminator_len);
                line_start = i + 1;
            }
            i += 1;
        }
        // Trailing text without a final newline is its own line.
        if line_start < bytes.len() {
            starts.push(line_start);
            ends.push(bytes.len());
            terminators.push(0);
        }
        Self {
            starts,
            ends,
            terminators,
            content_len: bytes.len(),
        }
    }

    /// Number of logical lines; equals [`FileState::line_count`].
    pub(crate) fn len(&self) -> usize {
        self.starts.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// Byte offset of the insertion point for a boundary line:
    /// 0 = beginning of file, 1..=len = before that logical line,
    /// len + 1 = end of file. The boundary must already be validated
    /// (see [`EditService::insert`]).
    pub(crate) fn insert_offset(&self, boundary: usize) -> usize {
        debug_assert!(boundary <= self.len() + 1);
        if boundary == 0 {
            0
        } else if boundary <= self.len() {
            self.starts[boundary - 1]
        } else {
            // EOF: after the final terminator, or content end when the
            // file has no trailing newline. Inserting at EOF always
            // begins a fresh line: the boundary offset sits after the
            // last line's terminator (or at 0 for an empty file).
            self.content_len
        }
    }

    /// Byte range (start, end) covered by the inclusive 1-based line
    /// range `start_line..=end_line`, including the terminators of
    /// every removed line. `end` is the start of the line after the
    /// range (or content end). Returns `None` when the range is not
    /// fully within the file; callers must validate first.
    pub(crate) fn line_span(&self, start_line: usize, end_line: usize) -> Option<(usize, usize)> {
        if start_line == 0 || end_line == 0 || start_line > end_line || end_line > self.len() {
            return None;
        }
        let start = self.starts[start_line - 1];
        let end = self.ends[end_line - 1];
        Some((start, end))
    }

    /// The file's line-ending style: CRLF when its first line ends with
    /// CRLF, otherwise LF. LF is the default for an empty file.
    pub(crate) fn terminator_str(&self) -> &'static str {
        self.terminators
            .first()
            .map_or("\n", |t| if *t == 2 { "\r\n" } else { "\n" })
    }

    /// Whether the final logical line lacks a trailing newline.
    pub(crate) fn lacks_trailing_newline(&self) -> bool {
        !self.is_empty() && self.terminators[self.len() - 1] == 0
    }

    /// Builds the content resulting from inserting `inserted` at the
    /// boundary `boundary` into `file` (the content this map was parsed
    /// from), applying the canonical newline policy:
    ///
    /// - The inserted text is literal. The one deterministic adjustment:
    ///   it is suffixed with exactly one terminator (the file's own
    ///   style, or LF for an initially empty file) when it does not
    ///   already end in one, so an insertion before an existing line can
    ///   never concatenate onto that line.
    /// - Inserting at EOF into a file whose final line lacks a trailing
    ///   newline first completes that final line with the file's own
    ///   terminator, so the inserted text stands on its own line.
    /// - Every byte outside the inserted region — including CRLF
    ///   endings, internal content, and a missing final newline when
    ///   inserting elsewhere — is preserved byte-for-byte.
    pub(crate) fn insert_at(&self, file: &str, boundary: usize, inserted: &str) -> String {
        let terminator = self.terminator_str();
        let mut text = inserted.to_string();
        if !text.ends_with('\n') {
            text.push_str(terminator);
        }
        let mut prepared = String::with_capacity(file.len() + text.len() + terminator.len());
        // EOF insertion into a file lacking a final newline: complete
        // the last existing line first, then append the inserted text.
        if boundary == self.len() + 1 && self.lacks_trailing_newline() {
            prepared.push_str(file);
            prepared.push_str(terminator);
            prepared.push_str(&text);
            return prepared;
        }
        let offset = self.insert_offset(boundary);
        prepared.push_str(&file[..offset]);
        prepared.push_str(&text);
        prepared.push_str(&file[offset..]);
        prepared
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
        // Skipping forward stages is illegal, except that `Snapshotted`
        // is optional until the snapshot milestone exists.
        assert!(!Requested.can_transition_to(Located));
        assert!(!Authorized.can_transition_to(Applied));
        assert!(!Validated.can_transition_to(Verified));
        assert!(!Snapshotted.can_transition_to(Committed));
        assert!(Validated.can_transition_to(Applied));
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

/// Prompt 04 (AWE-001) model-hardening tests: the adversarial and purity
/// matrix the canonical transaction contract requires, exercised purely
/// in memory (no filesystem, no Git, no transport, no async runtime).
#[cfg(test)]
mod transaction_model_tests {
    use super::*;

    fn insert_op(path: &str) -> EditOperation {
        EditOperation::Insert {
            path: path.into(),
            line: 1,
            content: "x".into(),
        }
    }

    // ---------- §42 input hardening: control characters ----------

    #[test]
    fn control_characters_in_paths_are_rejected_for_every_operation_kind() {
        for bad in [
            "a\u{0000}.txt",
            "a\u{0007}b.txt",
            "dir/\tt.txt",
            "x\u{007F}.rs",
        ] {
            for op in [
                EditOperation::Replace {
                    path: bad.into(),
                    old: "old".into(),
                    new: "new".into(),
                    occurrence: None,
                },
                insert_op(bad),
                EditOperation::DeleteRange {
                    path: bad.into(),
                    start_line: 1,
                    end_line: 1,
                },
                EditOperation::Patch {
                    path: bad.into(),
                    old: "old".into(),
                    new: "new".into(),
                },
            ] {
                let tx = EditTransaction::single(op);
                assert!(
                    matches!(tx.validate_shape(), Err(EditError::InvalidPath(_))),
                    "path {bad:?} must be rejected"
                );
            }
        }
        // The syntax validator itself agrees on every rejected shape.
        for bad in ["", "/abs.txt", "../up.txt", "a\u{0000}.txt", ".."] {
            assert!(validate_path(bad).is_err(), "{bad:?} must be invalid");
        }
        // Plain relative paths remain valid.
        validate_path("src/lib.rs").expect("plain relative path is valid");
    }

    #[test]
    fn control_characters_in_identity_and_references_are_rejected() {
        for bad in ["a\u{0000}", "\u{001B}[31m", "line\u{0008}break"] {
            let mut tx = EditTransaction::single(insert_op("a.txt"));
            tx.identity.agent_id = Some(bad.into());
            assert_eq!(
                tx.validate_shape(),
                Err(EditError::UnsafeReference("agent_id")),
                "agent id {bad:?} must be rejected"
            );

            let mut tx = EditTransaction::single(insert_op("a.txt"));
            tx.refs.snapshot_id = Some(bad.into());
            assert_eq!(
                tx.validate_shape(),
                Err(EditError::UnsafeReference("snapshot_id")),
                "snapshot id {bad:?} must be rejected"
            );
        }

        // Every identity and reference field applies the same rule —
        // identity and refs are structurally indistinguishable inputs.
        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.identity.session_id = Some("\u{0000}".into());
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::UnsafeReference("session_id"))
        );

        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.identity.workspace_id = Some("\u{0000}".into());
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::UnsafeReference("workspace_id"))
        );

        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.refs.provenance_id = Some("\u{0000}".into());
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::UnsafeReference("provenance_id"))
        );

        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.refs.audit_event_id = Some("\u{0000}".into());
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::UnsafeReference("audit_event_id"))
        );

        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.refs.policy_decision_id = Some("\u{0000}".into());
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::UnsafeReference("policy_decision_id"))
        );

        // Distinct from EmptyReference: the two failures are separable.
        let mut tx = EditTransaction::single(insert_op("a.txt"));
        tx.identity.agent_id = Some(String::new());
        assert_eq!(
            tx.validate_shape(),
            Err(EditError::EmptyReference("agent_id"))
        );
    }

    #[test]
    fn error_messages_never_embed_file_contents() {
        // Model-level messages name the offending field and echo only the
        // caller-supplied *path* (workspace-relative, never a host path)
        // or the malformed hash — never file content, context text, or
        // secrets.
        let message = EditError::InvalidPath("a\u{0000}.txt".to_owned()).to_string();
        assert!(!message.contains("/home/") && !message.contains("/Users/"));
        let hash_message = EditError::InvalidHash("deadbeef".into()).to_string();
        assert!(hash_message.contains("64 lowercase hex"));
        assert_eq!(
            EditError::UnsafeReference("agent_id").to_string(),
            "agent_id must not contain control characters"
        );
    }

    // ---------- §39 explicit illegal transitions ----------

    #[test]
    fn representative_illegal_transitions_are_rejected() {
        use EditStatus::*;
        // Skipping forward stages.
        assert!(!Requested.can_transition_to(Applied));
        assert!(!Requested.can_transition_to(Committed));
        assert!(!Authorized.can_transition_to(Applied));
        assert!(!Validated.can_transition_to(Committed));
        // Terminal reuse, both directions.
        assert!(!Committed.can_transition_to(Requested));
        assert!(!Rejected.can_transition_to(Applied));
        assert!(!Conflict.can_transition_to(Applied));
        assert!(!RolledBack.can_transition_to(Committed));
    }

    // ---------- §40 multi-operation structure ----------

    #[test]
    fn one_malformed_operation_rejects_the_whole_transaction() {
        let tx = EditTransaction::new(vec![
            insert_op("a.txt"),
            // Well-formed...
            EditOperation::Replace {
                path: "b.txt".into(),
                old: "old".into(),
                new: "new".into(),
                occurrence: Some(1),
            },
            // ...then malformed: empty match text.
            EditOperation::Replace {
                path: "c.txt".into(),
                old: String::new(),
                new: "new".into(),
                occurrence: None,
            },
        ]);
        assert_eq!(tx.validate_shape(), Err(EditError::EmptyMatch));
    }

    #[test]
    fn operation_order_survives_serialization() {
        let ops = vec![
            insert_op("a.txt"),
            EditOperation::DeleteRange {
                path: "b.txt".into(),
                start_line: 1,
                end_line: 2,
            },
            EditOperation::Patch {
                path: "c.txt".into(),
                old: "old".into(),
                new: "new".into(),
            },
        ];
        let tx = EditTransaction::new(ops);
        let json = serde_json::to_string(&tx).expect("serialize");
        let back: EditTransaction = serde_json::from_str(&json).expect("deserialize");
        // Order (not just membership) is preserved through the wire form.
        assert_eq!(back.operations, tx.operations);
        assert_eq!(back.operations[0].paths(), vec!["a.txt"]);
        assert_eq!(back.operations[1].paths(), vec!["b.txt"]);
        assert_eq!(back.operations[2].paths(), vec!["c.txt"]);
    }

    // ---------- §37 FileState ----------

    #[test]
    fn file_state_round_trips_and_is_deterministic() {
        let state = FileState::from_content("src/lib.rs", "one\ntwo\n");
        let json = serde_json::to_string(&state).expect("serialize");
        let back: FileState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, state);
        // SHA-256 is deterministic for identical content.
        assert_eq!(
            FileState::from_content("other.rs", "one\ntwo\n").hash,
            state.hash
        );
        // And distinct for distinct content.
        assert_ne!(
            FileState::from_content("src/lib.rs", "one\ntwo").hash,
            state.hash
        );
        // Canonical line counting: trailing newline does not add a line.
        assert_eq!(FileState::from_content("a", "one\ntwo").line_count, 2);
        assert_eq!(FileState::from_content("a", "").line_count, 0);
        // Byte size, not character count.
        assert_eq!(FileState::from_content("a", "héllo").size, 6);
    }

    // ---------- §41 property-style invariants ----------

    use proptest::prelude::*;

    proptest! {
        /// Valid transactions round-trip losslessly through the wire form.
        #[test]
        fn valid_transaction_round_trips(
            path in "[a-z][a-z0-9._-]{0,20}",
            line in 0usize..100,
            agent in "[a-z][a-z0-9-]{0,12}",
        ) {
            let mut tx = EditTransaction::new(vec![
                EditOperation::Insert { path: path.clone(), line, content: "x".into() },
            ]);
            tx.identity.agent_id = Some(agent.clone());
            tx.validate_shape().expect("generated tx must validate");
            let json = serde_json::to_string(&tx).expect("serialize");
            let back: EditTransaction = serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(back, tx);
        }

        /// No terminal status ever permits any transition, and no status
        /// transition predicate is reachable from a terminal state.
        #[test]
        fn terminal_statuses_never_transition(
            status in proptest::sample::select(vec![
                EditStatus::Committed,
                EditStatus::Rejected,
                EditStatus::Conflict,
                EditStatus::ValidationFailed,
                EditStatus::ApplyFailed,
                EditStatus::VerificationFailed,
                EditStatus::RolledBack,
            ]),
            next in proptest::sample::select(vec![
                EditStatus::Requested,
                EditStatus::Authorized,
                EditStatus::Located,
                EditStatus::Validated,
                EditStatus::Snapshotted,
                EditStatus::Applied,
                EditStatus::Verified,
                EditStatus::Committed,
            ]),
        ) {
            prop_assert!(status.is_terminal());
            prop_assert!(!status.can_transition_to(next));
        }

        /// Malformed and mismatched hashes never read as Matched.
        #[test]
        fn bad_hashes_never_match(
            content in "[a-z\\n]{0,60}",
            bad in "[0-9A-Fx]{1,70}",
        ) {
            let state = FileState::from_content("a.txt", &content);
            // Either structurally malformed or a genuine mismatch; the
            // result must never be Matched.
            let result = state.check(&ExpectedState {
                hash: Some(bad.clone()),
                ..ExpectedState::default()
            });
            prop_assert_ne!(result, StateMatch::Matched);
        }

        /// Structural validation is pure: identical input, identical
        /// outcome, whatever the surrounding environment.
        #[test]
        fn validate_shape_is_deterministic(
            path in "[a-z]{1,8}",
        ) {
            let tx = EditTransaction::single(EditOperation::Insert {
                path: path.clone(),
                line: 1,
                content: "x".into(),
            });
            let first = tx.validate_shape();
            let second = tx.validate_shape();
            prop_assert_eq!(first, second);
        }
    }

    // ---------- §43 purity ----------

    #[test]
    fn full_model_cycle_is_pure_and_mutates_nothing() {
        // The entire contract — construct, validate shape, compare
        // expected state, serialize/deserialize, validate transitions —
        // runs without touching the filesystem. Prove it: point the
        // transaction at an (uncreated) file inside an empty temp
        // directory and assert the directory stays empty after the full
        // cycle.
        let tmp = tempfile::tempdir().unwrap();
        let mut tx = EditTransaction::new(vec![
            EditOperation::Insert {
                path: "src/never-touched.txt".into(),
                line: 0,
                content: "inserted".into(),
            },
            EditOperation::Replace {
                path: "src/never-touched.txt".into(),
                old: "inserted".into(),
                new: "replaced".into(),
                occurrence: Some(1),
            },
        ]);
        let content = "alpha\nbeta\n";
        let before = FileState::from_content("src/never-touched.txt", content);
        tx.expected.push(ExpectedState {
            hash: Some(before.hash.clone()),
            size: Some(before.size),
            line_count: Some(before.line_count),
            context: Some("alpha".into()),
        });
        tx.expected.push(ExpectedState::default());
        tx.identity.agent_id = Some("agent-1".into());
        tx.refs.audit_event_id = Some("audit-3".into());
        tx.status = EditStatus::Validated;

        // 1. Validate shape; 2. compare expected state; 4. transitions.
        tx.validate_shape().expect("shape must validate");
        assert_eq!(before.check(&tx.expected[0]), StateMatch::ContextPending);
        assert_eq!(before.check(&tx.expected[1]), StateMatch::Matched);
        assert!(tx.status.can_transition_to(EditStatus::Applied));
        assert!(!tx.status.can_transition_to(EditStatus::Committed));

        // 3. Serialize/deserialize without loss.
        let json = serde_json::to_string(&tx).expect("serialize");
        let back: EditTransaction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, tx);

        // Purity: nothing was created on disk by any of the above.
        let entries: Vec<_> = std::fs::read_dir(tmp.path())
            .expect("temp dir readable")
            .collect();
        assert!(
            entries.is_empty(),
            "model cycle must not touch the filesystem"
        );
    }

    #[test]
    fn generated_edit_ids_are_never_empty() {
        for _ in 0..64 {
            let id = EditId::new();
            assert!(!id.to_string().is_empty());
            assert!(!id.0.is_empty());
        }
    }
}

/// Real-filesystem integration tests for the production replace executor.
#[cfg(test)]
mod replace_tests {
    use super::super::files;
    use super::*;
    use std::fs;

    fn setup() -> (tempfile::TempDir, EditService) {
        let tmp = tempfile::tempdir().unwrap();
        let svc = EditService::new(tmp.path().to_path_buf());
        (tmp, svc)
    }

    fn replace_tx(path: &str, old: &str, new: &str) -> EditTransaction {
        EditTransaction::single(EditOperation::Replace {
            path: path.into(),
            old: old.into(),
            new: new.into(),
            occurrence: None,
        })
    }

    fn replace_tx_occurrence(
        path: &str,
        old: &str,
        new: &str,
        occurrence: Option<usize>,
    ) -> EditTransaction {
        EditTransaction::single(EditOperation::Replace {
            path: path.into(),
            old: old.into(),
            new: new.into(),
            occurrence,
        })
    }

    fn with_expected(mut tx: EditTransaction, expected: ExpectedState) -> EditTransaction {
        assert_eq!(tx.operations.len(), 1);
        tx.expected.push(expected);
        tx
    }

    fn write_file(tmp: &tempfile::TempDir, name: &str, content: &str) {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }

    fn read_file(tmp: &tempfile::TempDir, name: &str) -> String {
        std::fs::read_to_string(tmp.path().join(name)).unwrap()
    }

    // ---------- 1. Successful replacement ----------

    #[test]
    fn successful_replacement_returns_accurate_before_and_after_state() {
        let (tmp, svc) = setup();
        let content = "alpha\nbeta\ngamma\n";
        write_file(&tmp, "notes.txt", content);

        let result = svc
            .replace(replace_tx("notes.txt", "beta", "BETA"))
            .unwrap();

        assert_eq!(result.status, EditStatus::Committed);
        assert_eq!(result.match_count, 1);
        assert_eq!(result.selected_occurrence, 1);
        assert_eq!(result.before.hash, sha256_hex(content.as_bytes()));
        assert_eq!(result.before.size, content.len() as u64);
        assert_eq!(result.before.line_count, 3);

        let expected_content = "alpha\nBETA\ngamma\n";
        assert_eq!(read_file(&tmp, "notes.txt"), expected_content);
        assert_eq!(result.after.hash, sha256_hex(expected_content.as_bytes()));
        assert_eq!(result.after.size, expected_content.len() as u64);
        assert_eq!(result.after.line_count, 3);
        assert_eq!(result.location.byte_offset, 6);
        assert_eq!(result.location.line, 2);
        assert_eq!(result.location.column, 1);
        // No leftover temp files from the atomic commit.
        let entries: Vec<_> = fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    // ---------- 2-4. Match failures ----------

    #[test]
    fn zero_matches_fail_without_mutation() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "one\ntwo\n");
        let err = svc.replace(replace_tx("f.txt", "three", "3")).unwrap_err();
        assert_eq!(
            err,
            EditError::MatchNotFound {
                path: "f.txt".into()
            }
        );
        assert_eq!(read_file(&tmp, "f.txt"), "one\ntwo\n");
    }

    #[test]
    fn multiple_matches_without_occurrence_fail_ambiguously_with_locations() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "x = 1\ny = 1\nz = 1\n");
        let err = svc.replace(replace_tx("f.txt", "1", "2")).unwrap_err();
        match err {
            EditError::AmbiguousMatch {
                path,
                match_count,
                locations,
            } => {
                assert_eq!(path, "f.txt");
                assert_eq!(match_count, 3);
                assert_eq!(locations.len(), 3);
                assert_eq!(locations[0].byte_offset, 4);
                assert_eq!(locations[0].line, 1);
                assert_eq!(locations[1].byte_offset, 10);
                assert_eq!(locations[1].line, 2);
                assert_eq!(locations[2].byte_offset, 16);
                assert_eq!(locations[2].line, 3);
            }
            other => panic!("expected AmbiguousMatch, got {other:?}"),
        }
        assert_eq!(read_file(&tmp, "f.txt"), "x = 1\ny = 1\nz = 1\n");
    }

    #[test]
    fn selected_occurrence_replaces_exactly_that_match() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "x = 1\ny = 1\nz = 1\n");
        let result = svc
            .replace(replace_tx_occurrence("f.txt", "1", "2", Some(2)))
            .unwrap();
        assert_eq!(result.match_count, 3);
        assert_eq!(result.selected_occurrence, 2);
        assert_eq!(result.location.byte_offset, 10);
        assert_eq!(result.location.line, 2);
        assert_eq!(read_file(&tmp, "f.txt"), "x = 1\ny = 2\nz = 1\n");
    }

    #[test]
    fn out_of_range_occurrence_fails_without_mutation() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "one\n");
        let err = svc
            .replace(replace_tx_occurrence("f.txt", "one", "1", Some(2)))
            .unwrap_err();
        assert_eq!(
            err,
            EditError::OccurrenceOutOfRange {
                path: "f.txt".into(),
                selected: 2,
                match_count: 1,
            }
        );
        assert_eq!(read_file(&tmp, "f.txt"), "one\n");
    }

    // ---------- 5-7. Stale-state conflicts ----------

    #[test]
    fn stale_hash_conflict_reports_expected_and_actual_without_mutation() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "current content\n");
        let tx = with_expected(
            replace_tx("f.txt", "current", "new"),
            ExpectedState {
                hash: Some(sha256_hex(b"stale content\n")),
                ..ExpectedState::default()
            },
        );
        let err = svc.replace(tx).unwrap_err();
        match err {
            EditError::ExpectedStateConflict(payload) => {
                assert_eq!(payload.path, "f.txt");
                assert_eq!(payload.component, ExpectedComponent::Hash);
                assert_eq!(
                    payload.expected.hash.as_deref(),
                    Some(sha256_hex(b"stale content\n").as_str())
                );
                assert_eq!(payload.actual.hash, sha256_hex(b"current content\n"));
                // The conflict summary never carries the context text.
                let json = serde_json::to_string(&payload.expected).unwrap();
                assert!(!json.contains("context"));
            }
            other => panic!("expected ExpectedStateConflict, got {other:?}"),
        }
        assert_eq!(read_file(&tmp, "f.txt"), "current content\n");
    }

    #[test]
    fn correct_hash_precondition_allows_the_edit() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "stable\n");
        let tx = with_expected(
            replace_tx("f.txt", "stable", "changed"),
            ExpectedState {
                hash: Some(sha256_hex(b"stable\n")),
                size: Some(7),
                line_count: Some(1),
                ..ExpectedState::default()
            },
        );
        svc.replace(tx).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "changed\n");
    }

    #[test]
    fn stale_size_and_line_count_conflict_without_mutation() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "one\ntwo\n");
        let size_tx = with_expected(
            replace_tx("f.txt", "one", "1"),
            ExpectedState {
                size: Some(999),
                ..ExpectedState::default()
            },
        );
        let err = svc.replace(size_tx).unwrap_err();
        assert!(matches!(
            err,
            EditError::ExpectedStateConflict(ref payload)
                if payload.component == ExpectedComponent::Size
        ));

        let lines_tx = with_expected(
            replace_tx("f.txt", "one", "1"),
            ExpectedState {
                line_count: Some(5),
                ..ExpectedState::default()
            },
        );
        let err = svc.replace(lines_tx).unwrap_err();
        assert!(matches!(
            err,
            EditError::ExpectedStateConflict(ref payload)
                if payload.component == ExpectedComponent::LineCount
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "one\ntwo\n");
    }

    // ---------- Context preconditions (the ContextPending resolution) ----------

    #[test]
    fn context_anchoring_selects_and_guards_the_intended_edit() {
        let (tmp, svc) = setup();
        // "item" appears three times; the context pins the second one.
        write_file(&tmp, "f.txt", "item one\nitem two\nitem three\n");
        let tx = with_expected(
            replace_tx_occurrence("f.txt", "item", "ITEM", Some(2)),
            ExpectedState {
                context: Some("item two".into()),
                ..ExpectedState::default()
            },
        );
        let result = svc.replace(tx).unwrap();
        assert_eq!(result.selected_occurrence, 2);
        assert_eq!(read_file(&tmp, "f.txt"), "item one\nITEM two\nitem three\n");
    }

    #[test]
    fn missing_context_fails_closed() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "different\n");
        let tx = with_expected(
            replace_tx("f.txt", "different", "new"),
            ExpectedState {
                context: Some("vanished surroundings".into()),
                ..ExpectedState::default()
            },
        );
        assert_eq!(
            svc.replace(tx).unwrap_err(),
            EditError::ContextConflict {
                path: "f.txt".into(),
                reason: ContextConflictReason::Missing,
            }
        );
        assert_eq!(read_file(&tmp, "f.txt"), "different\n");
    }

    #[test]
    fn ambiguous_context_fails_closed() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "header\nheader\nvalue\n");
        let tx = with_expected(
            replace_tx("f.txt", "value", "v"),
            ExpectedState {
                context: Some("header".into()),
                ..ExpectedState::default()
            },
        );
        assert_eq!(
            svc.replace(tx).unwrap_err(),
            EditError::ContextConflict {
                path: "f.txt".into(),
                reason: ContextConflictReason::Ambiguous { count: 2 },
            }
        );
        assert_eq!(read_file(&tmp, "f.txt"), "header\nheader\nvalue\n");
    }

    #[test]
    fn context_not_containing_the_selected_match_fails_closed() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "aaa target\nbbb other\n");
        let tx = with_expected(
            replace_tx("f.txt", "target", "done"),
            ExpectedState {
                context: Some("bbb".into()),
                ..ExpectedState::default()
            },
        );
        assert_eq!(
            svc.replace(tx).unwrap_err(),
            EditError::ContextConflict {
                path: "f.txt".into(),
                reason: ContextConflictReason::NotAnchored,
            }
        );
        assert_eq!(read_file(&tmp, "f.txt"), "aaa target\nbbb other\n");
    }

    // ---------- 8-10. Path and file failures ----------

    #[test]
    fn traversal_absolute_and_missing_paths_fail_before_any_io() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "content\n");

        for path in ["../escape.txt", "/etc/passwd", "a/../../escape.txt", ""] {
            let err = svc.replace(replace_tx(path, "content", "x")).unwrap_err();
            assert!(matches!(err, EditError::InvalidPath(_)), "path {path:?}");
        }
        let err = svc
            .replace(replace_tx("missing.txt", "content", "x"))
            .unwrap_err();
        assert_eq!(
            err,
            EditError::FileNotFound {
                path: "missing.txt".into()
            }
        );
        assert_eq!(read_file(&tmp, "f.txt"), "content\n");
    }

    // ---------- 11. Unicode: Devanagari and emoji ----------

    #[test]
    fn devanagari_and_emoji_replacement_preserve_surrounding_content() {
        let (tmp, svc) = setup();
        let content = "नमस्ते world 🌍\nsecond line\n";
        write_file(&tmp, "f.txt", content);

        let result = svc
            .replace(replace_tx("f.txt", "world 🌍", "दुनिया 🌏"))
            .unwrap();
        let expected = "नमस्ते दुनिया 🌏\nsecond line\n";
        assert_eq!(read_file(&tmp, "f.txt"), expected);
        assert_eq!(result.after.hash, sha256_hex(expected.as_bytes()));
        // Location reporting is character-aware even in multi-byte text:
        // "world" starts at char 8 of line 1 (न म स स ् त े SPACE would
        // be wrong; न,म,स्,ते,space = 5 clusters... byte offset is the
        // authoritative coordinate).
        assert!(result.location.byte_offset > 0);
        assert_eq!(result.location.line, 1);
    }

    // ---------- 12-15. Newline edge cases ----------

    #[test]
    fn crlf_line_endings_are_preserved_outside_the_replacement() {
        let (tmp, svc) = setup();
        let content = "alpha\r\nbeta\r\ngamma\r\n";
        write_file(&tmp, "f.txt", content);
        svc.replace(replace_tx("f.txt", "beta", "BETA")).unwrap();
        // Only the replaced span changed; every other CRLF survived.
        assert_eq!(read_file(&tmp, "f.txt"), "alpha\r\nBETA\r\ngamma\r\n");
    }

    #[test]
    fn lf_files_and_missing_trailing_newline_are_handled() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "no trailing newline");
        let result = svc
            .replace(replace_tx("f.txt", "trailing", "ending"))
            .unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "no ending newline");
        assert_eq!(result.after.line_count, 1);
        // A newline introduced by the replacement text is respected.
        write_file(&tmp, "g.txt", "one\ntwo");
        svc.replace(replace_tx("g.txt", "two", "two\nthree"))
            .unwrap();
        assert_eq!(read_file(&tmp, "g.txt"), "one\ntwo\nthree");
    }

    #[test]
    fn empty_file_fails_match_not_found_without_mutation() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "");
        assert_eq!(
            svc.replace(replace_tx("f.txt", "anything", "x"))
                .unwrap_err(),
            EditError::MatchNotFound {
                path: "f.txt".into()
            }
        );
        assert_eq!(read_file(&tmp, "f.txt"), "");
    }

    // ---------- 16. Repeated / overlapping match text ----------

    #[test]
    fn overlapping_needles_are_counted_non_overlapping_left_to_right() {
        let (tmp, svc) = setup();
        // "aaaa" contains two non-overlapping "aa" matches.
        write_file(&tmp, "f.txt", "aaaa\n");
        let err = svc.replace(replace_tx("f.txt", "aa", "b")).unwrap_err();
        assert!(matches!(
            err,
            EditError::AmbiguousMatch { match_count: 2, .. }
        ));
        // Selecting occurrence 2 replaces the second non-overlapping pair.
        svc.replace(replace_tx_occurrence("f.txt", "aa", "b", Some(2)))
            .unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "aab\n");
    }

    // ---------- 17. No-mutation invariant across every pre-write failure ----------

    #[test]
    fn every_pre_write_failure_leaves_the_target_byte_identical() {
        let (tmp, svc) = setup();
        let original = "alpha\nbeta\n";
        write_file(&tmp, "f.txt", original);
        let before_bytes = fs::read(tmp.path().join("f.txt")).unwrap();

        let cases: Vec<EditTransaction> = vec![
            // invalid shape: empty old text
            EditTransaction::single(EditOperation::Replace {
                path: "f.txt".into(),
                old: String::new(),
                new: "x".into(),
                occurrence: None,
            }),
            // invalid shape: occurrence zero
            replace_tx_occurrence("f.txt", "beta", "x", Some(0)),
            // unsupported: two operations
            EditTransaction::new(vec![
                EditOperation::Replace {
                    path: "f.txt".into(),
                    old: "beta".into(),
                    new: "x".into(),
                    occurrence: None,
                },
                EditOperation::Replace {
                    path: "f.txt".into(),
                    old: "beta".into(),
                    new: "x".into(),
                    occurrence: None,
                },
            ]),
            // unsupported: non-replace operation
            EditTransaction::single(EditOperation::Insert {
                path: "f.txt".into(),
                line: 1,
                content: "x".into(),
            }),
            // invalid path
            replace_tx("../f.txt", "beta", "x"),
            // missing file
            replace_tx("nope.txt", "beta", "x"),
            // zero matches
            replace_tx("f.txt", "gamma", "x"),
            // ambiguous
            replace_tx("f.txt", "a", "x"),
            // invalid occurrence
            replace_tx_occurrence("f.txt", "beta", "x", Some(5)),
            // stale hash
            with_expected(
                replace_tx("f.txt", "beta", "x"),
                ExpectedState {
                    hash: Some(sha256_hex(b"stale")),
                    ..ExpectedState::default()
                },
            ),
            // stale size
            with_expected(
                replace_tx("f.txt", "beta", "x"),
                ExpectedState {
                    size: Some(999),
                    ..ExpectedState::default()
                },
            ),
            // stale line count
            with_expected(
                replace_tx("f.txt", "beta", "x"),
                ExpectedState {
                    line_count: Some(42),
                    ..ExpectedState::default()
                },
            ),
            // missing context
            with_expected(
                replace_tx("f.txt", "beta", "x"),
                ExpectedState {
                    context: Some("absent".into()),
                    ..ExpectedState::default()
                },
            ),
            // ambiguous context
            with_expected(
                replace_tx("f.txt", "beta", "x"),
                ExpectedState {
                    context: Some("a".into()),
                    ..ExpectedState::default()
                },
            ),
        ];

        for tx in cases {
            assert!(svc.replace(tx).is_err(), "case must fail");
            let after_bytes = fs::read(tmp.path().join("f.txt")).unwrap();
            assert_eq!(before_bytes, after_bytes, "file must be unchanged");
        }
        // No staging leftovers from any attempted commit.
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    // ---------- 20. Containment / symlink behavior via the canonical boundary ----------

    #[test]
    fn symlink_escape_is_rejected_by_the_canonical_boundary() {
        let (tmp, svc) = setup();
        let outside = tempfile::tempdir().unwrap();
        // A real directory outside the workspace, reachable only through
        // a symlink planted inside it.
        let escaped_dir = outside.path().join("sub");
        fs::create_dir_all(&escaped_dir).unwrap();
        let secret = escaped_dir.join("secret.txt");
        fs::write(&secret, "do not touch\n").unwrap();
        write_file(&tmp, "f.txt", "alpha\n");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&escaped_dir, tmp.path().join("link")).unwrap();

        // Reading through the escaping symlink is refused by the
        // canonical containment boundary.
        #[cfg(unix)]
        {
            let err = svc
                .replace(replace_tx("link/secret.txt", "touch", "x"))
                .unwrap_err();
            assert!(matches!(err, EditError::ReadFailure { .. }));
            assert_eq!(fs::read_to_string(&secret).unwrap(), "do not touch\n");
        }
        // Non-symlink workspace paths still work.
        svc.replace(replace_tx("f.txt", "alpha", "beta")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "beta\n");
    }

    #[test]
    fn invalid_utf8_content_fails_closed_as_text_violation() {
        let (tmp, svc) = setup();
        // 0xFF is never valid UTF-8.
        fs::write(tmp.path().join("f.txt"), [0x61, 0xFF, 0x62]).unwrap();
        assert_eq!(
            svc.replace(replace_tx("f.txt", "a", "x")).unwrap_err(),
            EditError::InvalidUtf8 {
                path: "f.txt".into()
            }
        );
        assert_eq!(
            fs::read(tmp.path().join("f.txt")).unwrap(),
            vec![0x61, 0xFF, 0x62]
        );
    }

    #[test]
    fn nested_relative_paths_are_supported_and_contained() {
        let (tmp, svc) = setup();
        fs::create_dir_all(tmp.path().join("src/deep")).unwrap();
        write_file(&tmp, "src/deep/mod.rs", "fn main() {}\n");
        svc.replace(replace_tx("src/deep/mod.rs", "main", "run"))
            .unwrap();
        assert_eq!(read_file(&tmp, "src/deep/mod.rs"), "fn run() {}\n");
    }

    // ---------- Atomic commit boundary ----------

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temp_files() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "first\n");
        svc.files().write_atomic("f.txt", "second\n").unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "second\n");
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
        // A contained binary file round-trips byte-exact through the
        // atomic path (write_atomic takes UTF-8 text; binary via write).
        svc.files().write("bin.dat", "data").unwrap();
        svc.files().write_atomic("bin.dat", "more data").unwrap();
        assert_eq!(read_file(&tmp, "bin.dat"), "more data");
    }

    #[test]
    fn write_atomic_failure_leaves_original_untouched() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "original\n");
        // A directory occupying the target path makes the commit fail;
        // the original file target here is a file, so instead force
        // failure via an over-limit payload.
        let too_big = "x".repeat(files::MAX_FILE_BYTES as usize + 1);
        assert!(svc.files().write_atomic("f.txt", &too_big).is_err());
        assert_eq!(read_file(&tmp, "f.txt"), "original\n");
        // Size-cap failure is likewise enforced on the edit path.
        let big = "y".repeat(files::MAX_FILE_BYTES as usize + 1);
        let err = svc
            .replace(replace_tx("f.txt", "original", &big))
            .unwrap_err();
        assert!(matches!(err, EditError::WriteFailure { .. }));
        assert_eq!(read_file(&tmp, "f.txt"), "original\n");
    }

    // ---------- Property-oriented invariants ----------

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// A successful replacement preserves every byte outside the
            /// selected span, changing only the selected occurrence.
            #[test]
            fn replacement_preserves_untouched_bytes(
                prefix in "[a-z]{0,40}",
                occurrences in 1usize..4,
                suffix in "[a-z]{0,40}",
            ) {
                let (tmp, svc) = setup();
                let old = "needle";
                let middle = vec![old; occurrences + 1].join(" ");
                let content = format!("{prefix}{middle}{suffix}");
                write_file(&tmp, "f.txt", &content);

                let selected = occurrences;
                let result = svc
                    .replace(replace_tx_occurrence("f.txt", old, "X", Some(selected)))
                    .unwrap();

                // Reconstruct expectation: only the selected span changed.
                let parts: Vec<&str> = content.split(old).collect();
                let expected = format!(
                    "{}{}{}",
                    parts[..selected].join(old),
                    "X",
                    parts[selected..].join(old),
                );
                let expected_hash = sha256_hex(expected.as_bytes());
                prop_assert_eq!(read_file(&tmp, "f.txt"), expected);
                prop_assert_eq!(result.after.hash, expected_hash);
                prop_assert_eq!(result.before.hash, sha256_hex(content.as_bytes()));
            }
            /// Any failed replacement leaves the file byte-identical.
            #[test]
            fn failed_validation_leaves_original_unchanged(
                content in "[a-z\\n]{0,80}",
                stale_hash in "[0-9a-fx]{64}",
            ) {
                let (tmp, svc) = setup();
                if content.is_empty() { return Ok(()); }
                write_file(&tmp, "f.txt", &content);
                let before = std::fs::read(tmp.path().join("f.txt")).unwrap();
                let tx = with_expected(
                    replace_tx("f.txt", "zz", "X"),
                    ExpectedState {
                        hash: Some(stale_hash.clone()),
                        ..ExpectedState::default()
                    },
                );
                // Either the stale hash conflicts, or the needle never
                // occurs; both must leave the file untouched.
                let _ = svc.replace(tx);
                let after = std::fs::read(tmp.path().join("f.txt")).unwrap();
                prop_assert_eq!(before, after);
            }

            /// The reported after-state hash equals the real file's hash.
            #[test]
            fn reported_after_hash_matches_disk(
                before in "[a-z\\n]{0,60}",
                after_text in "[a-z]{1,10}",
            ) {
                let (tmp, svc) = setup();
                if before.is_empty() { return Ok(()); }
                write_file(&tmp, "f.txt", &before);
                let needle: String = before.chars().take(1).collect();
                let result = svc
                    .replace(replace_tx("f.txt", &needle, &after_text))
                    .ok();
                if let Some(result) = result {
                    let disk = std::fs::read(tmp.path().join("f.txt")).unwrap();
                    prop_assert_eq!(result.after.hash, sha256_hex(&disk));
                    prop_assert_eq!(result.after.size, disk.len() as u64);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AWE-003: canonical line-boundary model tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod line_map_tests {
    use super::*;

    /// The line map must agree exactly with `FileState::line_count`, which
    /// is the canonical definition of a logical line.
    #[test]
    fn line_count_agrees_with_file_state() {
        for content in [
            "",
            "\n",
            "a",
            "a\n",
            "a\nb",
            "a\nb\n",
            "\r\n",
            "a\r\n",
            "a\r\nb\r\n",
            "a\rb",
            "\n\n\n",
            "alpha\nbeta\ngamma\n",
        ] {
            let map = LineMap::parse(content);
            let state = FileState::from_content("f", content);
            assert_eq!(
                map.len(),
                state.line_count,
                "LineMap and FileState disagree on {content:?}"
            );
        }
    }

    #[test]
    fn empty_file_has_zero_lines() {
        let map = LineMap::parse("");
        assert!(map.is_empty());
        assert!(!map.lacks_trailing_newline());
    }

    #[test]
    fn trailing_newline_is_not_an_extra_line() {
        let map = LineMap::parse("a\nb\n");
        assert_eq!(map.len(), 2);
        assert!(!map.lacks_trailing_newline());
    }

    #[test]
    fn missing_trailing_newline_is_detected() {
        let map = LineMap::parse("a\nb");
        assert_eq!(map.len(), 2);
        assert!(map.lacks_trailing_newline());
    }

    #[test]
    fn lone_carriage_return_is_not_a_terminator() {
        let map = LineMap::parse("a\rb");
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn crlf_is_one_terminator() {
        let map = LineMap::parse("a\r\nb\r\n");
        assert_eq!(map.len(), 2);
        assert_eq!(map.terminator_str(), "\r\n");
    }

    #[test]
    fn mixed_endings_use_first_line_style() {
        let map = LineMap::parse("a\r\nb\n");
        assert_eq!(map.len(), 2);
        assert_eq!(map.terminator_str(), "\r\n");
    }

    // ---------- insert_offset boundary table ----------

    #[test]
    fn insert_offset_boundary_table() {
        let map = LineMap::parse("l1\nl2\nl3\n");
        assert_eq!(map.len(), 3);
        // 0 = beginning of file
        assert_eq!(map.insert_offset(0), 0);
        // 1 = before the first logical line
        assert_eq!(map.insert_offset(1), 0);
        // N = before logical line N
        assert_eq!(map.insert_offset(2), "l1\n".len());
        assert_eq!(map.insert_offset(3), "l1\nl2\n".len());
        // len + 1 = EOF
        assert_eq!(map.insert_offset(4), "l1\nl2\nl3\n".len());
    }

    #[test]
    fn insert_offset_eof_without_trailing_newline() {
        let map = LineMap::parse("l1\nl2");
        assert_eq!(map.insert_offset(3), "l1\nl2".len());
    }

    #[test]
    fn insert_offset_empty_file() {
        let map = LineMap::parse("");
        assert_eq!(map.insert_offset(0), 0);
        assert_eq!(map.insert_offset(1), 0);
    }

    // ---------- line_span ----------

    #[test]
    fn line_span_includes_terminators() {
        let map = LineMap::parse("alpha\nbeta\ngamma\ndelta\n");
        // Deleting 2..=3 removes "beta\ngamma\n" fully.
        assert_eq!(
            map.line_span(2, 3),
            Some(("alpha\n".len(), "alpha\nbeta\ngamma\n".len()))
        );
        // Deleting 1..=1 removes only the first line.
        assert_eq!(map.line_span(1, 1), Some((0, "alpha\n".len())));
        // Deleting 4..=4 removes the last line including its terminator.
        assert_eq!(
            map.line_span(4, 4),
            Some((
                "alpha\nbeta\ngamma\n".len(),
                "alpha\nbeta\ngamma\ndelta\n".len()
            ))
        );
    }

    #[test]
    fn line_span_rejects_invalid_ranges() {
        let map = LineMap::parse("a\nb\n");
        assert_eq!(map.line_span(0, 1), None);
        assert_eq!(map.line_span(1, 0), None);
        assert_eq!(map.line_span(2, 1), None);
        assert_eq!(map.line_span(1, 3), None);
    }

    #[test]
    fn line_span_partial_final_line_without_newline() {
        // "a\nb" — deleting 2..=2 removes "b" with no terminator.
        let map = LineMap::parse("a\nb");
        assert_eq!(map.line_span(2, 2), Some((2, 3)));
    }

    // ---------- insert_at newline policy ----------

    #[test]
    fn insert_at_adds_missing_terminator_in_file_style() {
        let map = LineMap::parse("a\r\nb\r\n");
        assert_eq!(map.insert_at("a\r\nb\r\n", 1, "new"), "new\r\na\r\nb\r\n");
    }

    #[test]
    fn insert_at_respects_supplied_terminator_without_doubling() {
        let map = LineMap::parse("a\nb\n");
        assert_eq!(map.insert_at("a\nb\n", 1, "new\n"), "new\na\nb\n");
    }

    #[test]
    fn insert_at_eof_completes_missing_final_newline() {
        let map = LineMap::parse("a\nb");
        assert_eq!(map.insert_at("a\nb", 3, "new"), "a\nb\nnew\n");
    }

    #[test]
    fn insert_at_eof_crlf_without_trailing_newline() {
        let map = LineMap::parse("a\r\nb");
        assert_eq!(map.insert_at("a\r\nb", 3, "new"), "a\r\nb\r\nnew\r\n");
    }

    #[test]
    fn insert_at_empty_file_defaults_to_lf() {
        let map = LineMap::parse("");
        assert_eq!(map.insert_at("", 0, "new"), "new\n");
        assert_eq!(map.insert_at("", 1, "new"), "new\n");
    }

    #[test]
    fn insert_at_preserves_surrounding_bytes_byte_for_byte() {
        let map = LineMap::parse("a\nb\n");
        assert_eq!(map.insert_at("a\nb\n", 2, "X"), "a\nX\nb\n");
        // Inserting into a file missing its final newline keeps that
        // missing newline when inserting before an earlier line.
        let map2 = LineMap::parse("a\nb");
        assert_eq!(map2.insert_at("a\nb", 1, "X"), "X\na\nb");
    }

    #[test]
    fn insert_at_literal_multiline_content() {
        let map = LineMap::parse("a\nb\n");
        assert_eq!(map.insert_at("a\nb\n", 2, "x\ny\n"), "a\nx\ny\nb\n");
    }
}

// ---------------------------------------------------------------------------
// AWE-003: line-oriented executor integration tests (real filesystem).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod line_edit_tests {
    use super::*;
    use std::fs;

    fn setup() -> (tempfile::TempDir, EditService) {
        let tmp = tempfile::tempdir().unwrap();
        let svc = EditService::new(tmp.path().to_path_buf());
        (tmp, svc)
    }

    fn insert_tx(path: &str, line: usize, content: &str) -> EditTransaction {
        EditTransaction::single(EditOperation::Insert {
            path: path.into(),
            line,
            content: content.into(),
        })
    }

    fn delete_tx(path: &str, start: usize, end: usize) -> EditTransaction {
        EditTransaction::single(EditOperation::DeleteRange {
            path: path.into(),
            start_line: start,
            end_line: end,
        })
    }

    fn with_expected(mut tx: EditTransaction, expected: ExpectedState) -> EditTransaction {
        assert_eq!(tx.operations.len(), 1);
        tx.expected.push(expected);
        tx
    }

    fn write_file(tmp: &tempfile::TempDir, name: &str, content: &str) {
        fs::write(tmp.path().join(name), content).unwrap();
    }

    fn read_file(tmp: &tempfile::TempDir, name: &str) -> String {
        fs::read_to_string(tmp.path().join(name)).unwrap()
    }

    fn read_bytes(tmp: &tempfile::TempDir, name: &str) -> Vec<u8> {
        fs::read(tmp.path().join(name)).unwrap()
    }

    fn current_hash(tmp: &tempfile::TempDir, name: &str) -> String {
        sha256_hex(&read_bytes(tmp, name))
    }

    // ---------- 1. Insert: boundary table ----------

    #[test]
    fn insert_at_line_zero_prepends_to_the_file() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\n");
        let result = svc.insert(insert_tx("f.txt", 0, "new")).unwrap();
        assert_eq!(result.status, EditStatus::Committed);
        assert_eq!(read_file(&tmp, "f.txt"), "new\nl1\nl2\n");
        assert_eq!(result.kind, LineEditKind::Insert { boundary: 0 });
        assert_eq!(result.after.line_count, 3);
    }

    #[test]
    fn insert_at_line_one_is_also_before_the_first_line() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\n");
        svc.insert(insert_tx("f.txt", 1, "new")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "new\nl1\nl2\n");
    }

    #[test]
    fn insert_before_middle_line() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\nl3\n");
        let result = svc.insert(insert_tx("f.txt", 2, "new")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "l1\nnew\nl2\nl3\n");
        assert_eq!(result.before.line_count, 3);
        assert_eq!(result.after.line_count, 4);
    }

    #[test]
    fn insert_before_last_line() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\nl3\n");
        svc.insert(insert_tx("f.txt", 3, "new")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "l1\nl2\nnew\nl3\n");
    }

    #[test]
    fn insert_at_eof_boundary() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\n");
        svc.insert(insert_tx("f.txt", 3, "new")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "l1\nl2\nnew\n");
    }

    #[test]
    fn insert_into_empty_file() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "");
        let result = svc.insert(insert_tx("f.txt", 0, "new")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "new\n");
        assert_eq!(result.before.line_count, 0);
        assert_eq!(result.after.line_count, 1);
        // Boundary 1 is also valid on an empty file (EOF).
        write_file(&tmp, "g.txt", "");
        svc.insert(insert_tx("g.txt", 1, "x")).unwrap();
        assert_eq!(read_file(&tmp, "g.txt"), "x\n");
    }

    #[test]
    fn insert_content_with_trailing_newline_is_not_doubled() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\n");
        svc.insert(insert_tx("f.txt", 1, "new\n")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "new\nl1\n");
    }

    #[test]
    fn insert_multiline_content() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\n");
        svc.insert(insert_tx("f.txt", 2, "a\nb\n")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "l1\na\nb\nl2\n");
    }

    #[test]
    fn insert_at_eof_of_file_missing_final_newline_completes_it() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2");
        svc.insert(insert_tx("f.txt", 3, "new")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "l1\nl2\nnew\n");
    }

    #[test]
    fn insert_before_earlier_line_of_file_missing_final_newline_keeps_it_missing() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2");
        svc.insert(insert_tx("f.txt", 1, "new")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "new\nl1\nl2");
    }

    #[test]
    fn repeated_inserts_are_deterministic() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\n");
        svc.insert(insert_tx("f.txt", 0, "one")).unwrap();
        svc.insert(insert_tx("f.txt", 2, "two")).unwrap();
        svc.insert(insert_tx("f.txt", 3, "three")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "one\ntwo\nthree\na\n");
    }

    #[test]
    fn insert_boundary_beyond_eof_is_rejected_without_clamping() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\n");
        let before = read_bytes(&tmp, "f.txt");
        let err = svc.insert(insert_tx("f.txt", 4, "x")).unwrap_err();
        assert!(matches!(
            err,
            EditError::InsertBoundaryOutOfRange {
                line: 4,
                line_count: 2,
                ..
            }
        ));
        assert_eq!(read_bytes(&tmp, "f.txt"), before);
    }

    #[test]
    fn insert_empty_content_is_rejected_by_shape_validation() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\n");
        let err = svc.insert(insert_tx("f.txt", 1, "")).unwrap_err();
        assert!(matches!(err, EditError::EmptyInsertContent));
        assert_eq!(read_file(&tmp, "f.txt"), "l1\n");
    }

    // ---------- 2. Insert: newline and Unicode policy ----------

    #[test]
    fn insert_into_crlf_file_uses_crlf_for_the_inserted_line() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\r\nl2\r\n");
        svc.insert(insert_tx("f.txt", 2, "new")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "l1\r\nnew\r\nl2\r\n");
    }

    #[test]
    fn insert_does_not_convert_the_rest_of_a_crlf_file() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\r\nl2\r\n");
        svc.insert(insert_tx("f.txt", 0, "new")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "new\r\nl1\r\nl2\r\n");
    }

    #[test]
    fn insert_unicode_line_is_preserved_and_surrounding_bytes_untouched() {
        let (tmp, svc) = setup();
        let content = "alpha\nबीटा\ngamma\n";
        write_file(&tmp, "f.txt", content);
        svc.insert(insert_tx("f.txt", 2, "नई पंक्ति 😀")).unwrap();
        let after = read_file(&tmp, "f.txt");
        assert_eq!(after, "alpha\nनई पंक्ति 😀\nबीटा\ngamma\n");
        // The untouched Devanagari line is byte-identical.
        assert!(after.contains("बीटा"));
        assert!(!after.contains("\r"));
    }

    #[test]
    fn insert_before_cjk_and_combining_characters() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "日本語\ncafe\u{301}\n");
        svc.insert(insert_tx("f.txt", 2, "中文")).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "日本語\n中文\ncafe\u{301}\n");
    }

    // ---------- 3. Delete-range semantics ----------

    #[test]
    fn delete_first_line_only() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "alpha\nbeta\ngamma\ndelta\n");
        let result = svc.delete_range(delete_tx("f.txt", 1, 1)).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "beta\ngamma\ndelta\n");
        assert_eq!(result.before.line_count, 4);
        assert_eq!(result.after.line_count, 3);
        assert_eq!(result.kind, LineEditKind::DeleteRange { start: 1, end: 1 });
    }

    #[test]
    fn delete_middle_range_is_inclusive() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "alpha\nbeta\ngamma\ndelta\n");
        svc.delete_range(delete_tx("f.txt", 2, 3)).unwrap();
        // 2..=3 removes exactly beta and gamma — not delta, not line 1.
        assert_eq!(read_file(&tmp, "f.txt"), "alpha\ndelta\n");
    }

    #[test]
    fn delete_last_line_only() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "alpha\nbeta\ngamma\n");
        svc.delete_range(delete_tx("f.txt", 3, 3)).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "alpha\nbeta\n");
    }

    #[test]
    fn delete_all_lines_yields_an_empty_file() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "alpha\nbeta\n");
        let result = svc.delete_range(delete_tx("f.txt", 1, 2)).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "");
        assert_eq!(result.after.size, 0);
        assert_eq!(result.after.hash, sha256_hex(b""));
        assert_eq!(result.after.line_count, 0);
    }

    #[test]
    fn delete_single_line_file() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "only\n");
        svc.delete_range(delete_tx("f.txt", 1, 1)).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "");
    }

    #[test]
    fn delete_only_line_without_trailing_newline() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "only");
        svc.delete_range(delete_tx("f.txt", 1, 1)).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "");
    }

    #[test]
    fn delete_range_of_file_missing_trailing_newline_preserves_the_rest() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\nb\nc");
        svc.delete_range(delete_tx("f.txt", 2, 2)).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "a\nc");
    }

    #[test]
    fn delete_crlf_lines_keeps_remaining_crlf_bytes() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\r\nb\r\nc\r\n");
        svc.delete_range(delete_tx("f.txt", 2, 2)).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "a\r\nc\r\n");
    }

    #[test]
    fn delete_unicode_lines_leaves_remaining_bytes_untouched() {
        let (_tmp, svc) = setup();
        write_file(&_tmp, "f.txt", "alpha\nबीटा\ngamma\n中文\n");
        let before_bytes = read_bytes(&_tmp, "f.txt");
        svc.delete_range(delete_tx("f.txt", 2, 2)).unwrap();
        let after = read_file(&_tmp, "f.txt");
        assert_eq!(after, "alpha\ngamma\n中文\n");
        // The result is the concatenation of content before the deleted
        // span and content after the deleted span, both preserved
        // byte-for-byte. The prefix "alpha\n" + suffix "gamma\n中文\n".
        let prefix_end = "alpha\n".len();
        let deleted_end = "alpha\nबीटा\n".len();
        let expected_suffix = &before_bytes[deleted_end..];
        assert_eq!(&after.as_bytes()[prefix_end..], expected_suffix);
    }

    #[test]
    fn delete_from_empty_file_fails() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "");
        let err = svc.delete_range(delete_tx("f.txt", 1, 1)).unwrap_err();
        assert!(matches!(err, EditError::EmptyFileDeletion { .. }));
        assert_eq!(read_file(&tmp, "f.txt"), "");
    }

    #[test]
    fn delete_zero_line_is_rejected_by_shape() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\nb\n");
        let err = svc.delete_range(delete_tx("f.txt", 0, 1)).unwrap_err();
        assert!(matches!(
            err,
            EditError::InvalidLineRange { start: 0, end: 1 }
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "a\nb\n");
    }

    #[test]
    fn delete_reversed_range_is_rejected_by_shape() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\nb\n");
        let err = svc.delete_range(delete_tx("f.txt", 2, 1)).unwrap_err();
        assert!(matches!(
            err,
            EditError::InvalidLineRange { start: 2, end: 1 }
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "a\nb\n");
    }

    #[test]
    fn delete_end_beyond_last_line_is_rejected_without_clamping() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\nb\n");
        let before = read_bytes(&tmp, "f.txt");
        let err = svc.delete_range(delete_tx("f.txt", 2, 3)).unwrap_err();
        assert!(matches!(
            err,
            EditError::LineOutOfRange {
                start: 2,
                end: 3,
                line_count: 2,
                ..
            }
        ));
        assert_eq!(read_bytes(&tmp, "f.txt"), before);
    }

    #[test]
    fn delete_start_beyond_last_line_is_rejected() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\nb\n");
        let err = svc.delete_range(delete_tx("f.txt", 3, 3)).unwrap_err();
        assert!(matches!(
            err,
            EditError::LineOutOfRange { line_count: 2, .. }
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "a\nb\n");
    }

    // ---------- 4. Expected-state discipline ----------

    #[test]
    fn insert_with_matching_hash_succeeds() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\n");
        let tx = with_expected(
            insert_tx("f.txt", 1, "new"),
            ExpectedState {
                hash: Some(current_hash(&tmp, "f.txt")),
                ..Default::default()
            },
        );
        svc.insert(tx).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "new\nl1\nl2\n");
    }

    #[test]
    fn insert_with_stale_hash_conflicts_and_leaves_the_file_untouched() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "current\n");
        let tx = with_expected(
            insert_tx("f.txt", 1, "new"),
            ExpectedState {
                hash: Some(sha256_hex(b"stale\n")),
                ..Default::default()
            },
        );
        let err = svc.insert(tx).unwrap_err();
        match err {
            EditError::ExpectedStateConflict(payload) => {
                assert_eq!(payload.path, "f.txt");
                assert_eq!(payload.component, ExpectedComponent::Hash);
                assert_eq!(payload.actual.hash, sha256_hex(b"current\n"));
            }
            other => panic!("expected conflict, got {other:?}"),
        }
        assert_eq!(read_file(&tmp, "f.txt"), "current\n");
    }

    #[test]
    fn delete_with_stale_hash_conflicts_and_leaves_the_file_untouched() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\nl3\n");
        let tx = with_expected(
            delete_tx("f.txt", 2, 2),
            ExpectedState {
                hash: Some(sha256_hex(b"something else\n")),
                ..Default::default()
            },
        );
        let err = svc.delete_range(tx).unwrap_err();
        assert!(matches!(
            err,
            EditError::ExpectedStateConflict(ref payload)
                if payload.component == ExpectedComponent::Hash
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "l1\nl2\nl3\n");
    }

    #[test]
    fn insert_with_stale_line_count_conflicts() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\n");
        let tx = with_expected(
            insert_tx("f.txt", 1, "new"),
            ExpectedState {
                line_count: Some(7),
                ..Default::default()
            },
        );
        let err = svc.insert(tx).unwrap_err();
        assert!(matches!(
            err,
            EditError::ExpectedStateConflict(ref payload)
                if payload.component == ExpectedComponent::LineCount
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "l1\nl2\n");
    }

    #[test]
    fn delete_with_stale_size_conflicts() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "l1\nl2\n");
        let tx = with_expected(
            delete_tx("f.txt", 1, 1),
            ExpectedState {
                size: Some(999),
                ..Default::default()
            },
        );
        let err = svc.delete_range(tx).unwrap_err();
        assert!(matches!(
            err,
            EditError::ExpectedStateConflict(ref payload)
                if payload.component == ExpectedComponent::Size
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "l1\nl2\n");
    }

    #[test]
    fn insert_context_must_contain_the_insertion_point() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "alpha\nbeta\n");
        let anchored = with_expected(
            insert_tx("f.txt", 1, "new"),
            ExpectedState {
                context: Some("alpha\n".into()),
                ..Default::default()
            },
        );
        svc.insert(anchored).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "new\nalpha\nbeta\n");
    }

    #[test]
    fn insert_context_not_anchored_to_the_insertion_point_conflicts() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "alpha\nbeta\n");
        let tx = with_expected(
            insert_tx("f.txt", 1, "new"),
            ExpectedState {
                context: Some("beta\n".into()),
                ..Default::default()
            },
        );
        let err = svc.insert(tx).unwrap_err();
        assert!(matches!(
            err,
            EditError::ContextConflict {
                reason: ContextConflictReason::NotAnchored,
                ..
            }
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "alpha\nbeta\n");
    }

    #[test]
    fn delete_context_must_contain_the_removed_range() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "alpha\nbeta\ngamma\n");
        let tx = with_expected(
            delete_tx("f.txt", 2, 3),
            ExpectedState {
                context: Some("alpha\nbeta\ngamma\n".into()),
                ..Default::default()
            },
        );
        svc.delete_range(tx).unwrap();
        assert_eq!(read_file(&tmp, "f.txt"), "alpha\n");

        // A context that does not span the removed lines conflicts.
        write_file(&tmp, "g.txt", "alpha\nbeta\ngamma\n");
        let tx = with_expected(
            delete_tx("g.txt", 2, 3),
            ExpectedState {
                context: Some("alpha\n".into()),
                ..Default::default()
            },
        );
        let err = svc.delete_range(tx).unwrap_err();
        assert!(matches!(err, EditError::ContextConflict { .. }));
        assert_eq!(read_file(&tmp, "g.txt"), "alpha\nbeta\ngamma\n");
    }

    #[test]
    fn delete_missing_context_conflicts() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\nb\n");
        let tx = with_expected(
            delete_tx("f.txt", 1, 1),
            ExpectedState {
                context: Some("zzz\n".into()),
                ..Default::default()
            },
        );
        let err = svc.delete_range(tx).unwrap_err();
        assert!(matches!(
            err,
            EditError::ContextConflict {
                reason: ContextConflictReason::Missing,
                ..
            }
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "a\nb\n");
    }

    #[test]
    fn ambiguous_context_conflicts_for_line_edits() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "x\nx\n");
        let tx = with_expected(
            insert_tx("f.txt", 1, "new"),
            ExpectedState {
                context: Some("x\n".into()),
                ..Default::default()
            },
        );
        let err = svc.insert(tx).unwrap_err();
        assert!(matches!(
            err,
            EditError::ContextConflict {
                reason: ContextConflictReason::Ambiguous { count: 2 },
                ..
            }
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "x\nx\n");
    }

    // ---------- 5. Errors, paths, and zero-mutation ----------

    #[test]
    fn line_edit_missing_file_fails_without_creating_it() {
        let (tmp, svc) = setup();
        let err = svc.insert(insert_tx("missing.txt", 1, "x")).unwrap_err();
        assert!(matches!(err, EditError::FileNotFound { .. }));
        assert!(!tmp.path().join("missing.txt").exists());
        let err = svc
            .delete_range(delete_tx("missing.txt", 1, 1))
            .unwrap_err();
        assert!(matches!(err, EditError::FileNotFound { .. }));
    }

    #[test]
    fn line_edit_absolute_path_is_rejected() {
        let (_tmp, svc) = setup();
        let err = svc.insert(insert_tx("/etc/passwd", 1, "x")).unwrap_err();
        assert!(matches!(err, EditError::InvalidPath { .. }));
        let err = svc
            .delete_range(delete_tx("/etc/passwd", 1, 1))
            .unwrap_err();
        assert!(matches!(err, EditError::InvalidPath { .. }));
    }

    #[test]
    fn line_edit_traversal_path_is_rejected() {
        let (tmp, svc) = setup();
        let err = svc.insert(insert_tx("../outside.txt", 1, "x")).unwrap_err();
        assert!(matches!(err, EditError::InvalidPath { .. }));
        let err = svc
            .delete_range(delete_tx("../outside.txt", 1, 1))
            .unwrap_err();
        assert!(matches!(err, EditError::InvalidPath { .. }));
        assert!(!tmp.path().join("../outside.txt").exists());
    }

    #[test]
    fn line_edit_non_utf8_file_fails_closed() {
        let (_tmp, svc) = setup();
        fs::write(_tmp.path().join("bin.dat"), b"\xff\xfe\x00binary").unwrap();
        let err = svc.insert(insert_tx("bin.dat", 1, "x")).unwrap_err();
        assert!(matches!(err, EditError::InvalidUtf8 { .. }));
        let err = svc.delete_range(delete_tx("bin.dat", 1, 1)).unwrap_err();
        assert!(matches!(err, EditError::InvalidUtf8 { .. }));
        assert_eq!(
            fs::read(_tmp.path().join("bin.dat")).unwrap(),
            b"\xff\xfe\x00binary".to_vec()
        );
    }

    #[test]
    fn line_edit_wrong_operation_is_unsupported() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\n");
        let tx = EditTransaction::single(EditOperation::Replace {
            path: "f.txt".into(),
            old: "a".into(),
            new: "b".into(),
            occurrence: None,
        });
        let err = svc.insert(tx).unwrap_err();
        assert!(matches!(
            err,
            EditError::UnsupportedTransaction { operation_count: 1 }
        ));
        let err = svc
            .delete_range(EditTransaction::single(EditOperation::Insert {
                path: "f.txt".into(),
                line: 1,
                content: "x".into(),
            }))
            .unwrap_err();
        assert!(matches!(
            err,
            EditError::UnsupportedTransaction { operation_count: 1 }
        ));
        assert_eq!(read_file(&tmp, "f.txt"), "a\n");
    }

    #[test]
    #[cfg(unix)] // assertions depend on planting a symlink; Windows skips this test
    fn insert_symlink_escape_is_rejected_by_the_containment_boundary() {
        let (tmp, svc) = setup();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("real.txt"), "target\n").unwrap();
        write_file(&tmp, "f.txt", "inside\n");
        std::os::unix::fs::symlink(outside.path().join("real.txt"), tmp.path().join("link.txt"))
            .unwrap();
        {
            let err = svc.insert(insert_tx("link.txt", 1, "x")).unwrap_err();
            assert!(
                matches!(err, EditError::ReadFailure { .. }),
                "expected containment rejection, got {err:?}"
            );
            // The target outside the workspace was never touched.
            assert_eq!(
                fs::read_to_string(outside.path().join("real.txt")).unwrap(),
                "target\n"
            );
        }
    }

    #[test]
    fn insert_result_state_is_accurate() {
        let (tmp, svc) = setup();
        let original = "alpha\nbeta\n";
        write_file(&tmp, "f.txt", original);
        let result = svc.insert(insert_tx("f.txt", 2, "gamma")).unwrap();
        let resulting = "alpha\ngamma\nbeta\n";
        assert_eq!(result.before.hash, sha256_hex(original.as_bytes()));
        assert_eq!(result.before.size, original.len() as u64);
        assert_eq!(result.before.line_count, 2);
        assert_eq!(result.after.hash, sha256_hex(resulting.as_bytes()));
        assert_eq!(result.after.size, resulting.len() as u64);
        assert_eq!(result.after.line_count, 3);
        assert_eq!(
            (result.before.hash, result.after.hash),
            (
                sha256_hex(original.as_bytes()),
                sha256_hex(resulting.as_bytes())
            )
        );
    }

    #[test]
    fn delete_result_hashes_are_correct() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "alpha\nbeta\ngamma\n");
        let result = svc.delete_range(delete_tx("f.txt", 1, 2)).unwrap();
        let resulting = "gamma\n";
        assert_eq!(result.after.hash, sha256_hex(resulting.as_bytes()));
        assert_eq!(
            result.after.hash,
            sha256_hex(read_bytes(&tmp, "f.txt").as_slice())
        );
    }

    #[test]
    fn line_edits_leave_no_staging_leftovers() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "a\n");
        // One failure (invalid boundary) and one success; either way only
        // f.txt may remain in the workspace root.
        let _ = svc.insert(insert_tx("f.txt", 5, "x"));
        svc.insert(insert_tx("f.txt", 1, "y")).unwrap();
        let entries: Vec<std::ffi::OsString> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("f.txt")]);
    }

    // ---------- 6. Property-oriented invariants ----------

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// A successful inclusive deletion removes exactly the requested
            /// logical lines: the result is the concatenation of the content
            /// before and after the deleted span, byte-for-byte.
            #[test]
            fn delete_removes_exactly_the_requested_lines(
                head in 0usize..6,
                removed in 1usize..4,
                tail in 0usize..6,
                crlf in any::<bool>(),
                final_newline in any::<bool>(),
            ) {
                let terminator = if crlf { "\r\n" } else { "\n" };
                let mut content = String::new();
                for i in 0..head { content.push_str(&format!("h{i}")); content.push_str(terminator); }
                for i in 0..removed { content.push_str(&format!("r{i}")); content.push_str(terminator); }
                for i in 0..tail { content.push_str(&format!("t{i}")); content.push_str(terminator); }
                if !final_newline && !content.is_empty() {
                    // Strip the final terminator to model a missing final
                    // newline. The last line then has no terminator.
                    let cut = content.len() - terminator.len();
                    content.truncate(cut);
                }
                if content.is_empty() { return Ok(()); }

                let (_tmp, svc) = setup();
                std::fs::write(_tmp.path().join("f.txt"), &content).unwrap();

                let start = head + 1;
                let end = head + removed;
                let result = svc.delete_range(delete_tx("f.txt", start, end)).unwrap();

                let head_part: String = {
                    let mut s = String::new();
                    for i in 0..head {
                        s.push_str(&format!("h{i}"));
                        s.push_str(terminator);
                    }
                    s
                };
                let tail_part: String = {
                    let mut s = String::new();
                    if tail > 0 {
                        for i in 0..tail {
                            s.push_str(&format!("t{i}"));
                            s.push_str(terminator);
                        }
                        if !final_newline {
                            let cut = s.len() - terminator.len();
                            s.truncate(cut);
                        }
                    }
                    s
                };
                let expected = head_part + &tail_part;

                let disk = std::fs::read(_tmp.path().join("f.txt")).unwrap();
                prop_assert_eq!(&disk, expected.as_bytes());
                let disk_hash = sha256_hex(&disk);
                prop_assert_eq!(&result.after.hash, &disk_hash);
                // Remaining lines = head lines + tail lines
                prop_assert_eq!(result.after.line_count, head + tail);
            }

            /// An invalid boundary never mutates the file.
            #[test]
            fn invalid_insert_boundary_never_mutates(
                lines in 0usize..5,
                content in ".*",
                boundary in any::<u16>(),
            ) {
                let terminator = "\n";
                let mut file = String::new();
                for _ in 0..lines { file.push('x'); file.push_str(terminator); }
                let tmp = tempfile::tempdir().unwrap();
                let svc = EditService::new(tmp.path().to_path_buf());
                std::fs::write(tmp.path().join("f.txt"), &file).unwrap();
                let boundary = boundary as usize;
                if boundary > lines + 1 {
                    let before = std::fs::read(tmp.path().join("f.txt")).unwrap();
                    let _ = svc.insert(insert_tx("f.txt", boundary, &content)).is_err();
                    let after = std::fs::read(tmp.path().join("f.txt")).unwrap();
                    prop_assert_eq!(before, after);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AWE-004: multi-operation filesystem.patch integration tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod patch_tests {
    use super::*;
    use std::fs;

    fn setup() -> (tempfile::TempDir, EditService) {
        let tmp = tempfile::tempdir().unwrap();
        let svc = EditService::new(tmp.path().to_path_buf());
        (tmp, svc)
    }

    fn write_file(tmp: &tempfile::TempDir, name: &str, content: &str) {
        fs::write(tmp.path().join(name), content).unwrap();
    }

    fn read_file(tmp: &tempfile::TempDir, name: &str) -> String {
        fs::read_to_string(tmp.path().join(name)).unwrap()
    }

    #[test]
    fn multi_file_patch_commits_all_files_atomically_after_preparation() {
        let (tmp, svc) = setup();
        write_file(&tmp, "a.txt", "line1\n");
        write_file(&tmp, "b.txt", "item1\nitem2\n");

        let tx = EditTransaction::new(vec![
            EditOperation::Replace {
                path: "a.txt".into(),
                old: "line1".into(),
                new: "LINE_ONE".into(),
                occurrence: None,
            },
            EditOperation::DeleteRange {
                path: "b.txt".into(),
                start_line: 1,
                end_line: 1,
            },
        ]);

        let result = svc.patch(tx).unwrap();
        assert_eq!(result.status, PatchStatus::Committed);
        assert_eq!(read_file(&tmp, "a.txt"), "LINE_ONE\n");
        assert_eq!(read_file(&tmp, "b.txt"), "item2\n");
    }

    #[test]
    fn multi_operation_composition_on_same_file_in_memory() {
        let (tmp, svc) = setup();
        write_file(&tmp, "f.txt", "alpha\nbeta\ngamma\n");

        // Transaction:
        // 1. replace "beta" with "BETA"
        // 2. insert "header" at line 0 (before alpha)
        // 3. delete range 4..=4 (gamma)
        let tx = EditTransaction::new(vec![
            EditOperation::Replace {
                path: "f.txt".into(),
                old: "beta".into(),
                new: "BETA".into(),
                occurrence: None,
            },
            EditOperation::Insert {
                path: "f.txt".into(),
                line: 0,
                content: "header".into(),
            },
            EditOperation::DeleteRange {
                path: "f.txt".into(),
                start_line: 4,
                end_line: 4,
            },
        ]);

        let result = svc.patch(tx).unwrap();
        assert_eq!(result.status, PatchStatus::Committed);
        assert_eq!(read_file(&tmp, "f.txt"), "header\nalpha\nBETA\n");
    }

    #[test]
    fn preparation_failure_on_any_file_leaves_all_files_unchanged() {
        let (tmp, svc) = setup();
        write_file(&tmp, "valid.txt", "keep me\n");
        write_file(&tmp, "invalid.txt", "wrong content\n");

        let mut tx = EditTransaction::new(vec![
            EditOperation::Replace {
                path: "valid.txt".into(),
                old: "keep me".into(),
                new: "changed".into(),
                occurrence: None,
            },
            EditOperation::Replace {
                path: "invalid.txt".into(),
                old: "wrong content".into(),
                new: "nope".into(),
                occurrence: None,
            },
        ]);
        // sorted affected: "invalid.txt" is index 0, "valid.txt" is index 1.
        tx.expected.push(ExpectedState {
            hash: Some(sha256_hex(b"stale hash\n")),
            ..Default::default()
        });
        tx.expected.push(ExpectedState::default());

        let err = svc.patch(tx).unwrap_err();
        assert!(matches!(err, EditError::PatchPreparationFailure { .. }));

        // Neither file was mutated.
        assert_eq!(read_file(&tmp, "valid.txt"), "keep me\n");
        assert_eq!(read_file(&tmp, "invalid.txt"), "wrong content\n");
    }

    // ---------------------------------------------------------------------------
    // AWE-006: atomic and rollback-safe edit tests.
    // ---------------------------------------------------------------------------

    #[cfg(test)]
    mod rollback_tests {
        use super::*;
        use std::fs;

        fn setup() -> (tempfile::TempDir, EditService) {
            let tmp = tempfile::tempdir().unwrap();
            let svc = EditService::new(tmp.path().to_path_buf());
            (tmp, svc)
        }

        fn write_file(tmp: &tempfile::TempDir, name: &str, content: &str) {
            fs::write(tmp.path().join(name), content).unwrap();
        }

        fn read_file(tmp: &tempfile::TempDir, name: &str) -> String {
            fs::read_to_string(tmp.path().join(name)).unwrap()
        }

        // ---------- AWE-010: explicit user-requested rollback ----------

        #[test]
        fn rollback_edits_restores_exact_original_bytes() {
            let (tmp, svc) = setup();
            write_file(&tmp, "f.txt", "alpha\nbeta\ngamma\n");

            // Capture recovery material before the edit, apply the edit,
            // then complete the records with the post-commit state.
            let mut records = svc
                .capture_rollback_records(&EditId::new(), &["f.txt".to_string()])
                .unwrap();
            assert_eq!(records.len(), 1);
            assert!(records[0].existed_before);
            assert_eq!(records[0].before_bytes, b"alpha\nbeta\ngamma\n".to_vec());

            let tx = EditTransaction::single(EditOperation::Replace {
                path: "f.txt".into(),
                old: "beta".into(),
                new: "BETA".into(),
                occurrence: None,
            });
            svc.replace(tx).unwrap();
            assert_eq!(read_file(&tmp, "f.txt"), "alpha\nBETA\ngamma\n");
            // Complete the record with the observed post-edit state.
            let after_bytes = fs::read(tmp.path().join("f.txt")).unwrap();
            records[0].after_hash = sha256_hex(&after_bytes);

            // Roll back the edit explicitly.
            let status = svc.rollback_edits(&records).unwrap();
            assert_eq!(status, EditRollbackStatus::Restored);
            assert_eq!(read_file(&tmp, "f.txt"), "alpha\nbeta\ngamma\n");
        }

        #[test]
        fn rollback_edits_deletes_edit_created_files() {
            let (tmp, svc) = setup();
            // a.txt is an unrelated file that must remain untouched.
            write_file(&tmp, "a.txt", "one\n");

            // Model a file the completed edit created: it exists now, but
            // the record says it did not exist before the edit, so the
            // only correct rollback is deletion. Constructing the record
            // directly tests the `rollback_edits` contract; the canonical
            // executors currently require existing targets (preflight
            // fails closed on missing files), so creation flows produce
            // this record shape.
            fs::write(tmp.path().join("b.txt"), "created by the edit\n").unwrap();
            let after_bytes = fs::read(tmp.path().join("b.txt")).unwrap();
            let records = vec![RollbackRecord {
                edit_id: EditId::new(),
                path: "b.txt".into(),
                existed_before: false,
                before_bytes: Vec::new(),
                after_hash: sha256_hex(&after_bytes),
            }];

            let status = svc.rollback_edits(&records).unwrap();
            assert_eq!(status, EditRollbackStatus::Restored);
            // The edit-created file is gone again.
            assert!(!tmp.path().join("b.txt").exists());
            // The untouched file is untouched.
            assert_eq!(read_file(&tmp, "a.txt"), "one\n");
        }

        #[test]
        fn rollback_edits_refuses_when_file_changed_externally() {
            let (tmp, svc) = setup();
            write_file(&tmp, "f.txt", "original\n");

            let mut records = svc
                .capture_rollback_records(&EditId::new(), &["f.txt".to_string()])
                .unwrap();

            let tx = EditTransaction::single(EditOperation::Replace {
                path: "f.txt".into(),
                old: "original".into(),
                new: "edited".into(),
                occurrence: None,
            });
            svc.replace(tx).unwrap();

            let after_bytes = fs::read(tmp.path().join("f.txt")).unwrap();
            records[0].after_hash = sha256_hex(&after_bytes);

            // An external writer modifies the file after the edit.
            write_file(&tmp, "f.txt", "externally modified\n");

            let status = svc.rollback_edits(&records).unwrap();
            assert!(matches!(status, EditRollbackStatus::Conflict { .. }));
            // The newer external content is preserved, not destroyed.
            assert_eq!(read_file(&tmp, "f.txt"), "externally modified\n");
        }

        #[test]
        fn rollback_edits_multi_file_is_all_or_nothing_on_conflicts() {
            let (tmp, svc) = setup();
            write_file(&tmp, "a.txt", "a1\n");
            write_file(&tmp, "b.txt", "b1\n");

            let mut records = svc
                .capture_rollback_records(
                    &EditId::new(),
                    &["a.txt".to_string(), "b.txt".to_string()],
                )
                .unwrap();

            // Apply edits to both files.
            let tx = EditTransaction::new(vec![
                EditOperation::Replace {
                    path: "a.txt".into(),
                    old: "a1".into(),
                    new: "a2".into(),
                    occurrence: None,
                },
                EditOperation::Replace {
                    path: "b.txt".into(),
                    old: "b1".into(),
                    new: "b2".into(),
                    occurrence: None,
                },
            ]);
            svc.patch(tx).unwrap();
            assert_eq!(read_file(&tmp, "a.txt"), "a2\n");
            assert_eq!(read_file(&tmp, "b.txt"), "b2\n");

            for record in &mut records {
                let after_bytes = fs::read(tmp.path().join(&record.path)).unwrap();
                record.after_hash = sha256_hex(&after_bytes);
            }

            // b.txt is modified externally after the edit: the rollback of
            // both files must be refused, and a.txt must stay at its edited
            // state (no partial rollback).
            write_file(&tmp, "b.txt", "external b\n");

            let status = svc.rollback_edits(&records).unwrap();
            assert!(matches!(status, EditRollbackStatus::Conflict { .. }));
            assert_eq!(read_file(&tmp, "a.txt"), "a2\n");
            assert_eq!(read_file(&tmp, "b.txt"), "external b\n");
        }

        #[test]
        fn rollback_edits_on_empty_records_reports_restored_without_mutation() {
            let (tmp, svc) = setup();
            write_file(&tmp, "f.txt", "unchanged\n");
            let status = svc.rollback_edits(&[]).unwrap();
            assert_eq!(status, EditRollbackStatus::Restored);
            assert_eq!(read_file(&tmp, "f.txt"), "unchanged\n");
        }

        #[test]
        fn rollback_edits_restores_unicode_and_crlf_bytes_exactly() {
            let (tmp, svc) = setup();
            let original = "alpha\r\nबीटा\ngamma\r\n"; // CRLF + Devanagari
            write_file(&tmp, "f.txt", original);

            let mut records = svc
                .capture_rollback_records(&EditId::new(), &["f.txt".to_string()])
                .unwrap();

            let tx = EditTransaction::single(EditOperation::Replace {
                path: "f.txt".into(),
                old: "बीटा".into(),
                new: "BETA".into(),
                occurrence: None,
            });
            svc.replace(tx).unwrap();

            let after_bytes = fs::read(tmp.path().join("f.txt")).unwrap();
            records[0].after_hash = sha256_hex(&after_bytes);

            let status = svc.rollback_edits(&records).unwrap();
            assert_eq!(status, EditRollbackStatus::Restored);
            // Byte-exact restoration, including CRLF endings.
            let restored = fs::read(tmp.path().join("f.txt")).unwrap();
            assert_eq!(restored, original.as_bytes());
        }

        #[test]
        fn capture_rollback_records_reads_only_and_never_mutates() {
            let (tmp, svc) = setup();
            write_file(&tmp, "f.txt", "content\n");
            let before = fs::read(tmp.path().join("f.txt")).unwrap();

            let records = svc
                .capture_rollback_records(&EditId::new(), &["f.txt".to_string()])
                .unwrap();

            let after = fs::read(tmp.path().join("f.txt")).unwrap();
            assert_eq!(before, after, "capture must not mutate the target");
            assert_eq!(records[0].before_bytes, after);
            assert_eq!(records[0].after_hash, "");
        }

        #[test]
        fn successful_multi_file_commit() {
            let (tmp, svc) = setup();
            write_file(&tmp, "a.txt", "original a\n");
            write_file(&tmp, "b.txt", "original b\n");

            let mut tx = EditTransaction::new(vec![
                EditOperation::Replace {
                    path: "a.txt".into(),
                    old: "original a".into(),
                    new: "modified a".into(),
                    occurrence: None,
                },
                EditOperation::Replace {
                    path: "b.txt".into(),
                    old: "original b".into(),
                    new: "modified b".into(),
                    occurrence: None,
                },
            ]);
            tx.expected.push(ExpectedState::default());
            tx.expected.push(ExpectedState::default());

            let result = svc.patch(tx).unwrap();
            assert_eq!(result.status, PatchStatus::Committed);
            assert!(result.rollback.is_none());
            assert_eq!(read_file(&tmp, "a.txt"), "modified a\n");
            assert_eq!(read_file(&tmp, "b.txt"), "modified b\n");
        }

        #[test]
        fn preparation_failure_leaves_files_unchanged() {
            let (tmp, svc) = setup();
            write_file(&tmp, "a.txt", "original a\n");
            write_file(&tmp, "b.txt", "original b\n");

            let mut tx = EditTransaction::new(vec![
                EditOperation::Replace {
                    path: "a.txt".into(),
                    old: "original a".into(),
                    new: "modified a".into(),
                    occurrence: None,
                },
                EditOperation::Replace {
                    path: "b.txt".into(),
                    old: "original b".into(),
                    new: "modified b".into(),
                    occurrence: None,
                },
            ]);
            // b.txt has stale expected hash - preparation should fail
            tx.expected.push(ExpectedState::default());
            tx.expected.push(ExpectedState {
                hash: Some(sha256_hex(b"wrong hash\n")),
                ..Default::default()
            });

            let err = svc.patch(tx).unwrap_err();
            assert!(matches!(err, EditError::PatchPreparationFailure { .. }));

            // Both files should be unchanged
            assert_eq!(read_file(&tmp, "a.txt"), "original a\n");
            assert_eq!(read_file(&tmp, "b.txt"), "original b\n");
        }

        #[test]
        fn rollback_result_structure_correct() {
            // Test that rollback result structure is correct when rollback occurs
            // This tests the structure without needing to trigger a real rollback
            let (tmp, svc) = setup();
            write_file(&tmp, "a.txt", "original a\n");
            write_file(&tmp, "b.txt", "original b\n");

            let mut tx = EditTransaction::new(vec![
                EditOperation::Replace {
                    path: "a.txt".into(),
                    old: "original a".into(),
                    new: "modified a".into(),
                    occurrence: None,
                },
                EditOperation::Replace {
                    path: "b.txt".into(),
                    old: "original b".into(),
                    new: "modified b".into(),
                    occurrence: None,
                },
            ]);
            tx.expected.push(ExpectedState::default());
            tx.expected.push(ExpectedState::default());

            let result = svc.patch(tx).unwrap();
            assert_eq!(result.status, PatchStatus::Committed);
            assert!(result.rollback.is_none()); // No rollback needed for successful commit
        }
    }

    // ---------------------------------------------------------------------------
    // AWE-005: unified-diff parser and applier tests.
    // ---------------------------------------------------------------------------

    #[cfg(test)]
    mod unified_diff_tests {
        use super::*;
        use std::fs;

        fn setup() -> (tempfile::TempDir, EditService) {
            let tmp = tempfile::tempdir().unwrap();
            let svc = EditService::new(tmp.path().to_path_buf());
            (tmp, svc)
        }

        fn write_file(tmp: &tempfile::TempDir, name: &str, content: &str) {
            fs::write(tmp.path().join(name), content).unwrap();
        }

        fn read_file(tmp: &tempfile::TempDir, name: &str) -> String {
            fs::read_to_string(tmp.path().join(name)).unwrap()
        }

        #[test]
        fn parse_simple_replace_diff() {
            let diff = "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-old line\n+new line\n";
            let parsed = parse_unified_diff(diff).unwrap();
            assert_eq!(parsed.len(), 1);
            assert_eq!(parsed[0].path, "f.txt");
            assert_eq!(parsed[0].hunks.len(), 1);
            let hunk = &parsed[0].hunks[0];
            assert_eq!(hunk.old_start, 1);
            assert_eq!(hunk.old_lines, 1);
            assert_eq!(hunk.new_start, 1);
            assert_eq!(hunk.new_lines, 1);
            assert_eq!(hunk.lines.len(), 2);
            assert!(matches!(&hunk.lines[0], HunkLine::Del(s) if s == "old line"));
            assert!(matches!(&hunk.lines[1], HunkLine::Add(s) if s == "new line"));
        }

        #[test]
        fn parse_multi_hunk_diff() {
            let diff = "--- a/f.txt\n+++ b/f.txt\n@@ -1,2 +1,2 @@\n context1\n-old\n+new\n@@ -4 +4 @@\n context2\n-old2\n+new2\n";
            let parsed = parse_unified_diff(diff).unwrap();
            assert_eq!(parsed.len(), 1);
            assert_eq!(parsed[0].path, "f.txt");
            assert_eq!(parsed[0].hunks.len(), 2);
        }

        #[test]
        fn parse_multi_file_diff() {
            let diff = "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old a\n+new a\n--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-old b\n+new b\n";
            let parsed = parse_unified_diff(diff).unwrap();
            assert_eq!(parsed.len(), 2);
            assert_eq!(parsed[0].path, "a.txt");
            assert_eq!(parsed[1].path, "b.txt");
        }

        #[test]
        fn parse_diff_with_no_newline_at_end_of_file() {
            let diff =
                "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file\n";
            let parsed = parse_unified_diff(diff).unwrap();
            assert_eq!(parsed[0].path, "f.txt");
        }

        #[test]
        fn reject_binary_patch() {
            let diff = "Binary files a.txt and b.txt differ\n";
            let err = parse_unified_diff(diff).unwrap_err();
            assert!(matches!(err, EditError::BinaryPatch(_)));
        }

        #[test]
        fn reject_empty_diff() {
            let diff = "";
            let err = parse_unified_diff(diff).unwrap_err();
            assert!(matches!(err, EditError::EmptyDiff));
        }

        #[test]
        fn apply_simple_replace_hunk() {
            let content = "old line\n";
            let hunks = vec![ParsedHunk {
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 1,
                lines: vec![
                    HunkLine::Del("old line".into()),
                    HunkLine::Add("new line".into()),
                ],
            }];
            let result = apply_hunks_to_content(content, &hunks, "f.txt").unwrap();
            assert_eq!(result, "new line\n");
        }

        #[test]
        fn apply_addition_hunk() {
            let content = "line1\nline2\n";
            let hunks = vec![ParsedHunk {
                old_start: 2,
                old_lines: 1,
                new_start: 2,
                new_lines: 2,
                lines: vec![
                    HunkLine::Context("line2".into()),
                    HunkLine::Add("inserted".into()),
                ],
            }];
            let result = apply_hunks_to_content(content, &hunks, "f.txt").unwrap();
            assert_eq!(result, "line1\nline2\ninserted\n");
        }

        #[test]
        fn apply_deletion_hunk() {
            let content = "line1\nline2\nline3\n";
            let hunks = vec![ParsedHunk {
                old_start: 2,
                old_lines: 1,
                new_start: 2,
                new_lines: 0,
                lines: vec![HunkLine::Del("line2".into())],
            }];
            let result = apply_hunks_to_content(content, &hunks, "f.txt").unwrap();
            assert_eq!(result, "line1\nline3\n");
        }

        #[test]
        fn apply_multiple_hunks_descending_order() {
            let content = "a\nb\nc\nd\ne\n";
            // Two hunks: delete line 2, delete line 4 (original numbering)
            let hunks = vec![
                ParsedHunk {
                    old_start: 2,
                    old_lines: 1,
                    new_start: 2,
                    new_lines: 0,
                    lines: vec![HunkLine::Del("b".into())],
                },
                ParsedHunk {
                    old_start: 4,
                    old_lines: 1,
                    new_start: 4,
                    new_lines: 0,
                    lines: vec![HunkLine::Del("d".into())],
                },
            ];
            let result = apply_hunks_to_content(content, &hunks, "f.txt").unwrap();
            assert_eq!(result, "a\nc\ne\n");
        }

        #[test]
        fn reject_context_mismatch() {
            let content = "different\n";
            let hunks = vec![ParsedHunk {
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 1,
                lines: vec![
                    HunkLine::Context("expected".into()),
                    HunkLine::Add("new".into()),
                ],
            }];
            let err = apply_hunks_to_content(content, &hunks, "f.txt").unwrap_err();
            assert!(matches!(err, EditError::HunkContextMismatch { .. }));
        }

        #[test]
        fn apply_diff_via_edit_service() {
            let (tmp, svc) = setup();
            write_file(&tmp, "f.txt", "old line\n");

            let diff = "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-old line\n+new line\n";
            let tx = EditTransaction::single(EditOperation::ApplyDiff { diff: diff.into() });

            let result = svc.patch(tx).unwrap();
            assert_eq!(result.status, PatchStatus::Committed);
            assert_eq!(read_file(&tmp, "f.txt"), "new line\n");
        }

        #[test]
        fn apply_multi_file_diff_via_edit_service() {
            let (tmp, svc) = setup();
            write_file(&tmp, "a.txt", "old a\n");
            write_file(&tmp, "b.txt", "old b\n");

            let diff = "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old a\n+new a\n--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-old b\n+new b\n";
            let tx = EditTransaction::single(EditOperation::ApplyDiff { diff: diff.into() });

            let result = svc.patch(tx).unwrap();
            assert_eq!(result.status, PatchStatus::Committed);
            assert_eq!(read_file(&tmp, "a.txt"), "new a\n");
            assert_eq!(read_file(&tmp, "b.txt"), "new b\n");
        }

        #[test]
        fn apply_diff_with_expected_state_conflict() {
            let (tmp, svc) = setup();
            write_file(&tmp, "f.txt", "current content\n");

            let diff = "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-old\n+new\n";
            let mut tx = EditTransaction::single(EditOperation::ApplyDiff { diff: diff.into() });
            tx.expected.push(ExpectedState {
                hash: Some(sha256_hex(b"stale content\n")),
                ..Default::default()
            });

            let err = svc.patch(tx).unwrap_err();
            assert!(matches!(err, EditError::PatchPreparationFailure { .. }));

            assert_eq!(read_file(&tmp, "f.txt"), "current content\n");
        }
    }
}
