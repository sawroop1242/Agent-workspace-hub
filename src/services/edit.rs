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
    // ---- Operation-layer failures (EditService). Every failure below is
    // ---- reported before the atomic commit boundary unless stated
    // ---- otherwise, so the target file is unchanged.
    /// The transaction's operation set is not the single `Replace` this
    /// executor performs. Multi-operation patching belongs to the patch
    /// milestone; other operation types have their own executors.
    #[error("this executor performs exactly one replace operation; got {operation_count} operation(s) of unsupported shape")]
    UnsupportedTransaction { operation_count: usize },
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
            path,
            old,
            new,
            occurrence,
        } = operation
        else {
            return Err(EditError::UnsupportedTransaction {
                operation_count: transaction.operations.len(),
            });
        };

        // 3. Path syntax. Containment is enforced again by the filesystem
        //    boundary below on both the read and the atomic write.
        validate_path(path)?;

        // 4. Read current bytes and decode UTF-8. Binary content fails
        //    closed here rather than being reinterpreted as text.
        let bytes = self.files.read_bytes(path).map_err(|error| {
            if is_not_found(&error) {
                EditError::FileNotFound { path: path.clone() }
            } else {
                EditError::ReadFailure {
                    path: path.clone(),
                    message: error.to_string(),
                }
            }
        })?;
        let content = std::str::from_utf8(&bytes)
            .map_err(|_| EditError::InvalidUtf8 { path: path.clone() })?;

        // 5. Observed pre-mutation state.
        let before = FileState::from_content(path, content);

        // 6. Expected-state preconditions, in the model's fixed order:
        //    hash, then size, then line count. Context is resolved below
        //    after match selection (it must contain the selected match).
        let expected = transaction.expected.first();
        if let Some(expected) = expected {
            match before.check(expected) {
                StateMatch::Matched => {}
                StateMatch::Conflicted { component } => {
                    return Err(EditError::ExpectedStateConflict(Box::new(
                        ExpectedStateConflictPayload {
                            path: path.clone(),
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
                    // hash/size/line_count held; resolve context below.
                }
            }
        }

        // 7. Locate literal matches (left-to-right, non-overlapping).
        let matches = find_matches(content, old);
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

        // 6b. Context precondition: exactly one occurrence of the context
        //     in the current content, containing the selected match. This
        //     is the service-side resolution of the model's
        //     `ContextPending` result — the location-specific check the
        //     transport-independent model deliberately cannot perform.
        if let Some(expected) = expected {
            if let Some(context) = expected.context.as_deref() {
                let context_matches = find_matches(content, context);
                match context_matches.as_slice() {
                    [] => {
                        return Err(EditError::ContextConflict {
                            path: path.clone(),
                            reason: ContextConflictReason::Missing,
                        });
                    }
                    [only] => {
                        let contains = only.byte_offset <= selected.byte_offset
                            && selected.byte_offset + selected.length
                                <= only.byte_offset + only.length;
                        if !contains {
                            return Err(EditError::ContextConflict {
                                path: path.clone(),
                                reason: ContextConflictReason::NotAnchored,
                            });
                        }
                    }
                    _ => {
                        return Err(EditError::ContextConflict {
                            path: path.clone(),
                            reason: ContextConflictReason::Ambiguous {
                                count: context_matches.len(),
                            },
                        });
                    }
                }
            }
        }

        // 8. Prepare the complete new content in memory. Splicing at
        //    `str::find` byte offsets is UTF-8-safe: match boundaries are
        //    character boundaries.
        let start = selected.byte_offset;
        let end = start + selected.length;
        let mut prepared =
            String::with_capacity(content.len() + new.len().saturating_sub(old.len()));
        prepared.push_str(&content[..start]);
        prepared.push_str(new);
        prepared.push_str(&content[end..]);

        // 9. Atomic commit through the canonical filesystem boundary.
        let after_predicted = FileState::from_content(path, &prepared);
        self.files
            .write_atomic(path, &prepared)
            .map_err(|error| EditError::WriteFailure {
                path: path.clone(),
                message: error.to_string(),
            })?;

        // 10. Verified post-commit observation. The re-read uses the same
        //     containment boundary; a mismatch means external interference
        //     and is reported honestly.
        let observed = self.files.read(path).map_err(|error| {
            if is_not_found(&error) {
                EditError::VerificationFailed(Box::new(VerificationFailurePayload {
                    path: path.clone(),
                    expected: after_predicted.clone(),
                    actual: FileState {
                        path: path.clone(),
                        hash: String::new(),
                        size: 0,
                        line_count: 0,
                    },
                }))
            } else {
                EditError::ReadFailure {
                    path: path.clone(),
                    message: error.to_string(),
                }
            }
        })?;
        let after = FileState::from_content(path, &observed);
        if after != after_predicted {
            return Err(EditError::VerificationFailed(Box::new(
                VerificationFailurePayload {
                    path: path.clone(),
                    expected: after_predicted,
                    actual: after,
                },
            )));
        }

        // 11. Committed result.
        Ok(EditResult {
            id: transaction.id,
            path: path.clone(),
            status: EditStatus::Committed,
            before,
            after,
            match_count: matches.len(),
            selected_occurrence: selected_index + 1,
            location: selected.location,
        })
    }
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
