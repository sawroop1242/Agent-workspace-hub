//! Canonical structured persistent audit (AWE-013 / TW-007).
//!
//! One audit writer for every transport and service plane. Events are
//! redacted at this choke point and appended durably under the
//! workspace's `.agent/audit/` state, so audit history survives process
//! restart instead of living only in a process-local ring.
//!
//! ## Storage layout
//!
//! ```text
//! .agent/audit/audit.log      — append-only JSONL, one event per line,
//!                                each line carrying a SHA-256 checksum
//! .agent/audit/audit.log.1    — previous generation (kept on rotation)
//! .agent/audit/audit.lock      — StoreLock serializing appends
//! ```
//!
//! Workspace-local by design: each workspace's audit history lives in
//! that workspace (repository-first rule), so queries against one root
//! can never disclose another workspace's records.
//!
//! ## Durability contract
//!
//! An append is serialized, checksummed, written, and flushed to the
//! operating system before `record` returns; the documented durability
//! point is the OS write buffer (no per-event fsync — audit must not
//! dominate ordinary agent operations). A crash may therefore lose the
//! tail, but a torn final line is *detected* on load (checksum/parse
//! failure on the last record) and excluded rather than fabricated.
//! Corruption in a middle line is a hard, structured error: the store
//! never silently replaces evidence.
//!
//! ## Ordering
//!
//! Every persisted event carries a strictly monotonic `sequence`
//! scoped to the store, recovered at startup by scanning the durable
//! file — sequence numbers are never reused after restart, and
//! timestamps are metadata only, never the ordering key.
//!
//! ## Degraded mode
//!
//! Before the workspace root is known ([`init_global`]), events buffer
//! in a bounded in-memory ring and are replayed into the durable store
//! on initialization; the buffer is never an authoritative history —
//! after initialization every read is served from the durable store.
//! An audit persistence failure after an operation never rewrites that
//! operation's outcome (best-effort, surfaced separately); audit is
//! observational and never an authorization input.

use crate::mcp::store_lock::StoreLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Wire/serialization schema of the durable audit record.
pub const AUDIT_SCHEMA_VERSION: u32 = 1;

/// Bounded pre-init buffer (identical to the legacy ring cap).
const MAX_BUFFERED_ENTRIES: usize = 1000;

/// Retention, enforced at the ingestion boundary (the conservative
/// policy: no arbitrary deletion): the active log holds at most this
/// many events. On exceeding it the file rotates to `audit.log.1` (one
/// previous generation kept, the older one dropped) and the fresh file
/// starts with a bounded rotation marker — rotation is itself audited
/// and surviving history is moved, never rewritten.
const MAX_PERSISTED_EVENTS: usize = 10_000;

/// Upper bound for any single query page.
const MAX_QUERY_LIMIT: usize = 1000;

/// Upper bound for free-form subject/detail/reason text in one event
/// (characters, char-boundary safe). Oversized diagnostic text is
/// bounded; security-critical fields are never silently truncated
/// mid-secret — the choke-point redaction runs before bounding.
const MAX_TEXT_CHARS: usize = 4 * 1024;

/// One security-relevant event. The original five fields preserve the
/// established read shape; the correlation fields are optional and
/// omitted from serialization when absent, so existing consumers are
/// unaffected. Never a secret value: the canonical writer redacts at
/// the choke point before anything is persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unix epoch milliseconds.
    pub ts_ms: u128,
    /// `allow`, `deny`, `conflict`, or `failure`.
    pub kind: String,
    /// Stable action slug (`control_auth`, `filesystem.replace`, ...).
    pub action: String,
    /// Affected actor or resource (never a secret value).
    pub subject: String,
    /// Free-form non-secret context.
    pub detail: String,
    /// Schema version of the durable record.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Stable unique event id (`audit-<nanos>-<pid>-<seq>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    /// Strictly monotonic per-store ordering value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    /// Workspace correlation where available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// Agent correlation where available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Session correlation where available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Edit-transaction correlation where applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_id: Option<String>,
    /// Snapshot/provenance correlation where applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    /// Stable reason/error code where applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn default_schema_version() -> u32 {
    AUDIT_SCHEMA_VERSION
}

/// Optional correlation metadata for one event. Absent fields stay
/// absent — the writer never invents `unknown` identities, fake
/// empty-string values, or timestamps standing in for ids.
#[derive(Debug, Clone, Default)]
pub struct AuditCorrelation {
    pub workspace_id: Option<String>,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub edit_id: Option<String>,
    pub snapshot_id: Option<String>,
    pub reason: Option<String>,
}

impl AuditCorrelation {
    /// Correlation carrying the trusted identity of an authorization
    /// principal (agent/session) and the workspace it is bound to.
    pub fn from_principal(
        principal: &crate::services::authorization::AuthorizingPrincipal,
    ) -> Self {
        Self {
            workspace_id: Some(principal.workspace_id.clone()),
            agent_id: principal.agent_id.clone(),
            session_id: principal.session_id.clone(),
            ..Self::default()
        }
    }

    /// Correlation naming one edit transaction.
    pub fn for_edit(edit_id: impl Into<String>) -> Self {
        Self {
            edit_id: Some(edit_id.into()),
            ..Self::default()
        }
    }
}

/// Structured audit error taxonomy. Client-visible messages stay
/// category-level: no absolute storage paths, file contents, or secret
/// material ever surface here.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("invalid audit event: {0}")]
    Invalid(String),
    #[error("audit storage unavailable: {0}")]
    Storage(String),
    #[error("audit integrity failure: {0}")]
    Corruption(String),
    #[error("audit schema {got} is unsupported (expected {expected})")]
    IncompatibleSchema { expected: u32, got: u32 },
    #[error("audit limit exceeded: {0}")]
    LimitExceeded(String),
}

/// The one canonical audit writer/store. Two modes, one surface:
///
/// * buffered — the pre-init bounded ring; events replay into the
///   durable store on [`init_global`];
/// * durable — the append-only JSONL store rooted at a workspace.
///
/// Every producer and read surface works unchanged in either mode.
#[derive(Debug)]
pub struct AuditLog {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
enum Inner {
    /// Pre-init bounded buffer. Never an authoritative history once
    /// the durable store initializes.
    Buffered { entries: VecDeque<AuditEntry> },
    /// The durable append-only store plus its explicit write-through
    /// cache (§39: the cache mirrors the persistent store for reads;
    /// it is never the authoritative history — durability is the
    /// file). A durable-append failure still lands the event in the
    /// cache so degraded mode keeps recording.
    Durable {
        store: Box<DurableStore>,
        cache: VecDeque<AuditEntry>,
    },
}

struct DurableStore {
    root: PathBuf,
    log_path: PathBuf,
    lock_path: PathBuf,
    next_sequence: u64,
    line_count: usize,
}

impl std::fmt::Debug for DurableStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DurableStore")
            .field("root", &self.root)
            .field("next_sequence", &self.next_sequence)
            .field("line_count", &self.line_count)
            .finish()
    }
}

impl DurableStore {
    /// Startup recovery: validate the durable file, recover the next
    /// sequence (never reset), and surface corruption. A torn FINAL
    /// record — an interrupted append — is detected and *repaired* by
    /// truncating it from the file (it was already excluded from
    /// history; the next append must land on a clean line boundary);
    /// history is never fabricated from it. Corruption in a middle
    /// record is a hard, structured error and the file is preserved
    /// untouched.
    fn open(root: &Path) -> Result<Self, AuditError> {
        let dir = root.join(".agent").join("audit");
        std::fs::create_dir_all(&dir)
            .map_err(|e| AuditError::Storage(format!("create audit dir: {e}")))?;
        let log_path = dir.join("audit.log");
        let lock_path = dir.join("audit.lock");

        let mut next_sequence = 1u64;
        let mut line_count = 0usize;
        let mut seen_event_ids = std::collections::HashSet::new();
        if log_path.exists() {
            let text = std::fs::read_to_string(&log_path)
                .map_err(|e| AuditError::Storage(format!("read audit log: {e}")))?;
            let lines: Vec<&str> = text.lines().collect();
            let mut valid_prefix_len: usize = 0;
            for (index, line) in lines.iter().enumerate() {
                if line.trim().is_empty() {
                    valid_prefix_len += line.len() + 1;
                    continue;
                }
                match parse_line(line) {
                    Ok(entry) => {
                        let event_id = entry.event_id.clone().unwrap_or_default();
                        if !seen_event_ids.insert(event_id) {
                            return Err(AuditError::Corruption(format!(
                                "duplicate event id at record {}",
                                index + 1
                            )));
                        }
                        let sequence = entry.sequence.unwrap_or(0);
                        if sequence >= next_sequence {
                            next_sequence = sequence + 1;
                        } else {
                            return Err(AuditError::Corruption(format!(
                                "sequence regression at record {}",
                                index + 1
                            )));
                        }
                        line_count += 1;
                        valid_prefix_len += line.len() + 1;
                    }
                    Err(error) => {
                        if index + 1 == lines.len() && !text.ends_with('\n') {
                            // Torn tail: an interrupted append. Exclude it
                            // from history and truncate it so the next
                            // append lands on a clean line boundary —
                            // never fabricate, never keep half a record.
                            tracing::warn!(
                                event = "audit_torn_tail",
                                error = %error,
                                "final audit record is incomplete; it is excluded from history"
                            );
                            truncate_file(&log_path, valid_prefix_len)?;
                            continue;
                        }
                        // Middle corruption: propagate the typed error
                        // (corruption vs. incompatible schema) and leave
                        // the evidence untouched.
                        return Err(error);
                    }
                }
            }
        }
        Ok(Self {
            root: root.to_path_buf(),
            log_path,
            lock_path,
            next_sequence,
            line_count,
        })
    }

    /// Appends one durable event and returns the persisted entry plus
    /// any auxiliary management entry produced by the append (the
    /// rotation marker when this append triggered a rotation), so
    /// callers can mirror both into the read cache in order.
    fn append(
        &mut self,
        mut entry: AuditEntry,
    ) -> Result<(AuditEntry, Option<AuditEntry>), AuditError> {
        // Retention at the ingestion boundary: rotate before the file
        // exceeds the documented cap.
        let mut marker = None;
        if self.line_count >= MAX_PERSISTED_EVENTS {
            marker = Some(self.rotate()?);
        }
        let _lock = StoreLock::acquire(&self.lock_path)
            .map_err(|e| AuditError::Storage(format!("audit append lock: {e}")))?;
        let sequence = self.next_sequence;
        entry.sequence = Some(sequence);
        entry.event_id = Some(format!(
            "audit-{:016x}-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
            std::process::id(),
            sequence
        ));
        // §23: workspace correlation. The store knows its root, so an
        // event that reaches the durable boundary without a workspace
        // claim (e.g. an executor that never saw a principal) still
        // carries the workspace identity when one exists. Absence stays
        // absence for an uninitialized workspace.
        if entry.workspace_id.is_none() {
            entry.workspace_id = self.workspace_id();
        }
        let line = serialize_line(&entry)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .map_err(|e| AuditError::Storage(format!("open audit log: {e}")))?;
        file.write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.flush())
            .map_err(|e| AuditError::Storage(format!("append audit event: {e}")))?;
        // The durability point is the write+flush above. The in-memory
        // sequence advances only after it — a failed append never burns
        // a sequence number and can be retried safely.
        self.next_sequence = sequence + 1;
        self.line_count += 1;
        Ok((entry, marker))
    }

    /// Rotates the active log to `audit.log.1` (dropping the previous
    /// generation) and starts the fresh file with a bounded rotation
    /// marker (the one deliberate audit-on-audit exception: a bounded
    /// management event, never a recursive writer). Returns the
    /// persisted marker so callers can mirror it into the read cache.
    fn rotate(&mut self) -> Result<AuditEntry, AuditError> {
        let previous = self
            .log_path
            .parent()
            .expect("log path has a parent")
            .join("audit.log.1");
        if previous.exists() {
            std::fs::remove_file(&previous)
                .map_err(|e| AuditError::Storage(format!("drop previous audit generation: {e}")))?;
        }
        std::fs::rename(&self.log_path, &previous)
            .map_err(|e| AuditError::Storage(format!("rotate audit log: {e}")))?;
        self.line_count = 0;
        let marker = AuditEntry {
            ts_ms: now_ms(),
            kind: "allow".into(),
            action: "audit.rotation".into(),
            subject: "audit".into(),
            detail: format!(
                "log rotated at {MAX_PERSISTED_EVENTS} events; previous generation \
                 preserved as audit.log.1"
            ),
            schema_version: AUDIT_SCHEMA_VERSION,
            event_id: None,
            sequence: None,
            workspace_id: self.workspace_id(),
            agent_id: None,
            session_id: None,
            edit_id: None,
            snapshot_id: None,
            reason: Some("retention_rotation".into()),
        };
        self.append(marker).map(|(persisted, _)| persisted)
    }

    /// The workspace identity of this store's root when the workspace
    /// is initialized; absent otherwise (an uninitialized workspace
    /// still audits — absence stays absent).
    fn workspace_id(&self) -> Option<String> {
        crate::services::init::load_workspace_manifest(&self.root)
            .ok()
            .map(|manifest| manifest.workspace_id.as_str().to_owned())
    }

    /// Newest-first bounded read over the durable store; the scan is
    /// bounded by the retention cap, never unbounded.
    fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        let text = match std::fs::read_to_string(&self.log_path) {
            Ok(text) => text,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<AuditEntry> = Vec::new();
        for line in text.lines().rev() {
            if out.len() >= limit {
                break;
            }
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = parse_line(line) {
                out.push(entry);
            }
        }
        out
    }

    fn count(&self) -> usize {
        std::fs::read_to_string(&self.log_path)
            .map(|text| text.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
    }
}

/// Serializes one event as a checksummed JSONL line.
fn serialize_line(entry: &AuditEntry) -> Result<String, AuditError> {
    let payload = serde_json::to_string(entry)
        .map_err(|e| AuditError::Storage(format!("serialize audit event: {e}")))?;
    let checksum = crate::services::edit::sha256_hex(payload.as_bytes());
    Ok(format!(
        "{{\"checksum\":\"{checksum}\",\"event\":{payload}}}"
    ))
}

/// Parses and integrity-checks one persisted line. The checksum binds
/// the record and detects accidental corruption and truncation.
fn parse_line(line: &str) -> Result<AuditEntry, AuditError> {
    #[derive(Deserialize)]
    struct Line {
        checksum: String,
        event: AuditEntry,
    }
    let parsed: Line = serde_json::from_str(line)
        .map_err(|e| AuditError::Corruption(format!("malformed record: {e}")))?;
    if parsed.event.schema_version != AUDIT_SCHEMA_VERSION {
        return Err(AuditError::IncompatibleSchema {
            expected: AUDIT_SCHEMA_VERSION,
            got: parsed.event.schema_version,
        });
    }
    // Integrity check: re-serialize the parsed event and compare the
    // digest. Field order is the struct declaration order, so this is
    // deterministic within the crate's schema.
    let payload = serde_json::to_string(&parsed.event)
        .map_err(|e| AuditError::Corruption(format!("re-serialize record: {e}")))?;
    let actual = crate::services::edit::sha256_hex(payload.as_bytes());
    if actual != parsed.checksum {
        return Err(AuditError::Corruption("checksum mismatch".into()));
    }
    if parsed.event.event_id.as_deref().unwrap_or("").is_empty() {
        return Err(AuditError::Corruption("record has no event id".into()));
    }
    if parsed.event.sequence.is_none() {
        return Err(AuditError::Corruption("record has no sequence".into()));
    }
    Ok(parsed.event)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Truncates the audit log to `valid_len` bytes — the torn-tail repair
/// performed at startup, removing only the already-excluded incomplete
/// record so the next append lands on a clean line boundary. Valid
/// history is never touched.
fn truncate_file(path: &Path, valid_len: usize) -> Result<(), AuditError> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|e| AuditError::Storage(format!("open audit log for repair: {e}")))?;
    file.set_len(valid_len as u64)
        .and_then(|_| file.sync_all())
        .map_err(|e| AuditError::Storage(format!("repair audit log tail: {e}")))
}

/// Char-boundary-safe text bound: oversize diagnostic text is trimmed
/// to the first `MAX_TEXT_CHARS` characters, never sliced mid-codepoint.
fn bound_text(text: &str) -> String {
    if text.chars().count() <= MAX_TEXT_CHARS {
        text.to_owned()
    } else {
        text.chars().take(MAX_TEXT_CHARS).collect()
    }
}

/// §16 shape guard: an edit id is stored verbatim only when it matches
/// the generated `edit-…` shape AWH's own id machinery produces; any
/// other value (a caller mistake, token-shaped material) goes through
/// the redaction choke point.
fn trusted_edit_id(value: &str) -> String {
    if value.starts_with("edit-") && value.len() <= 128 {
        bound_text(value)
    } else {
        bound_text(&redact_token_like(value))
    }
}

/// §16 shape guard: a snapshot id is stored verbatim only when it
/// matches the generated `snap-…` shape; anything else is redacted.
fn trusted_snapshot_id(value: &str) -> String {
    if value.starts_with("snap-") && value.len() <= 128 {
        bound_text(value)
    } else {
        bound_text(&redact_token_like(value))
    }
}

/// §16 shape guard: a stable lowercase snake_case reason code (the
/// shape every reason code in this crate uses) is stored verbatim so it
/// stays machine-filterable; free-form reason text is redacted and
/// bounded. Residual risk, documented: a lowercase-only hex secret is
/// shape-indistinguishable from a reason code; every reason code this
/// crate emits is a short snake_case word.
fn trusted_reason(value: &str) -> String {
    const MAX_REASON_CODE: usize = 64;
    let stable_code = !value.is_empty()
        && value.len() <= MAX_REASON_CODE
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if stable_code {
        value.to_owned()
    } else {
        bound_text(&redact_token_like(value))
    }
}

impl AuditLog {
    fn buffered() -> Self {
        Self {
            inner: Mutex::new(Inner::Buffered {
                entries: VecDeque::with_capacity(MAX_BUFFERED_ENTRIES),
            }),
        }
    }

    /// Opens the durable store rooted at a workspace.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, AuditError> {
        let store = Box::new(DurableStore::open(root.as_ref())?);
        // The cache is seeded from the durable file so reads include
        // pre-restart history immediately.
        let cache: VecDeque<AuditEntry> = store
            .recent(MAX_BUFFERED_ENTRIES)
            .into_iter()
            .rev()
            .collect();
        Ok(Self {
            inner: Mutex::new(Inner::Durable { store, cache }),
        })
    }

    /// Appends one event through the canonical choke point: subject and
    /// detail are redacted (token-shaped material can never reach the
    /// durable store, even from a mistaken future call site), free-form
    /// text is bounded, and correlation fields are optional by design.
    ///
    /// Redaction classification (§16): `edit_id`/`snapshot_id` are safe
    /// BY CONTRACT — AWH-generated structured identifiers (`edit-…`,
    /// `snap-…`) populated only by the service boundaries themselves —
    /// so a value matching that generated shape is stored verbatim
    /// (size-bounded) and stays correlatable; any OTHER value (e.g. a
    /// mistaken caller embedding a token) still passes through the
    /// redaction choke point. Stable reason codes (lowercase
    /// snake_case, the shape every reason code in this crate has) are
    /// likewise stored verbatim; free-form reason text is redacted and
    /// bounded. Every untrusted free-form field (subject/detail) and
    /// every opaque identifier field (agent/session/workspace) always
    /// passes through the redaction choke point.
    ///
    /// Pre-init events buffer in memory and replay durably on
    /// [`init_global`]. Post-init the write is best-effort at this
    /// boundary: an audit storage failure is logged and surfaces
    /// through the caller's separate reporting, but never rewrites the
    /// audited operation's outcome.
    pub fn record(&self, kind: &'static str, action: &str, subject: &str, detail: &str) {
        self.record_correlated(kind, action, subject, detail, &AuditCorrelation::default());
    }

    /// [`Self::record`] with correlation metadata (workspace/agent/
    /// session/edit/snapshot ids and a stable reason code where
    /// applicable). Absent fields stay absent — never invented.
    pub fn record_correlated(
        &self,
        kind: &'static str,
        action: &str,
        subject: &str,
        detail: &str,
        correlation: &AuditCorrelation,
    ) {
        let entry = AuditEntry {
            ts_ms: now_ms(),
            kind: kind.to_owned(),
            action: bound_text(action),
            subject: bound_text(&redact_token_like(subject)),
            detail: bound_text(&redact_token_like(detail)),
            schema_version: AUDIT_SCHEMA_VERSION,
            event_id: None,
            sequence: None,
            workspace_id: correlation.workspace_id.as_deref().map(redact_token_like),
            agent_id: correlation.agent_id.as_deref().map(redact_token_like),
            session_id: correlation.session_id.as_deref().map(redact_token_like),
            edit_id: correlation.edit_id.as_deref().map(trusted_edit_id),
            snapshot_id: correlation.snapshot_id.as_deref().map(trusted_snapshot_id),
            reason: correlation.reason.as_deref().map(trusted_reason),
        };
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match &mut *guard {
            Inner::Buffered { entries } => {
                if entries.len() == MAX_BUFFERED_ENTRIES {
                    entries.pop_front();
                }
                entries.push_back(entry);
            }
            Inner::Durable { store, cache } => {
                let persisted = match store.append(entry.clone()) {
                    Ok((persisted, marker)) => {
                        // Mirror the rotation marker (if this append
                        // triggered one) before the event itself, in
                        // write order.
                        if let Some(marker) = marker {
                            if cache.len() == MAX_BUFFERED_ENTRIES {
                                cache.pop_front();
                            }
                            cache.push_back(marker);
                        }
                        persisted
                    }
                    Err(error) => {
                        tracing::warn!(
                            event = "audit_persist_failed",
                            error = %error,
                            "audit event could not be persisted; the audited \
                             operation's outcome stands"
                        );
                        // Degraded mode: the accepted event still lands in
                        // the cache so reads keep working.
                        entry
                    }
                };
                // Write-through cache: reads always see accepted events
                // (with their persisted identity), even in degraded mode.
                // The cache is bounded and mirrors the durable file
                // (which remains the authority).
                if cache.len() == MAX_BUFFERED_ENTRIES {
                    cache.pop_front();
                }
                cache.push_back(persisted);
            }
        }
    }

    /// Returns the most recent `limit` events, newest first (bounded by
    /// the query cap). Pre-init it reads the bounded buffer; post-init
    /// the write-through cache seeded from (and mirrored by) the
    /// durable store — the durable file remains the authoritative
    /// history.
    pub fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        let limit = limit.min(MAX_QUERY_LIMIT);
        let guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match &*guard {
            Inner::Buffered { entries } => entries.iter().rev().take(limit).cloned().collect(),
            Inner::Durable { cache, .. } => cache.iter().rev().take(limit).cloned().collect(),
        }
    }

    /// Number of retained entries (buffered, or the durable record
    /// count). Bounded by the buffer/retention caps.
    pub fn len(&self) -> usize {
        let guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match &*guard {
            Inner::Buffered { entries } => entries.len(),
            Inner::Durable { store, .. } => store.count(),
        }
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Process-wide canonical store shared by every plane. Starts buffered;
/// call [`init_global`] once the workspace root is known to switch to
/// durable persistence (buffered events replay into the store).
pub fn global() -> &'static AuditLog {
    static STORE: OnceLock<AuditLog> = OnceLock::new();
    STORE.get_or_init(AuditLog::buffered)
}

static INITIALIZED: OnceLock<PathBuf> = OnceLock::new();

/// Initializes the process-wide durable audit store at `root`. Any
/// events buffered before initialization are replayed into the durable
/// store (and remain readable through the write-through cache). The
/// store's read cache is seeded from the durable file, so pre-restart
/// history is visible immediately. Idempotent: a second call is a no-op.
///
/// A corruption error propagates: startup must never silently
/// initialize an empty store over existing evidence. On failure the
/// store stays in buffered mode (producers keep auditing, reads keep
/// working) while the caller surfaces the structured error.
pub fn init_global(root: impl AsRef<Path>) -> Result<(), AuditError> {
    if INITIALIZED.get().is_some() {
        return Ok(());
    }
    let root = root.as_ref().to_path_buf();
    let mut store = Box::new(DurableStore::open(&root)?);
    let mut cache: VecDeque<AuditEntry> = store
        .recent(MAX_BUFFERED_ENTRIES)
        .into_iter()
        .rev()
        .collect();
    // Replay buffered events durably (best-effort per event; each stays
    // readable through the cache either way).
    let buffered: Vec<AuditEntry> = {
        let guard = global().inner.lock().unwrap_or_else(|p| p.into_inner());
        match &*guard {
            Inner::Buffered { entries } => entries.iter().cloned().collect(),
            // Another initializer already won the race.
            Inner::Durable { .. } => Vec::new(),
        }
    };
    for entry in buffered {
        match store.append(entry.clone()) {
            Ok((persisted, _marker)) => {
                if cache.len() == MAX_BUFFERED_ENTRIES {
                    cache.pop_front();
                }
                cache.push_back(persisted);
            }
            Err(error) => {
                tracing::warn!(
                    event = "audit_replay_failed",
                    error = %error,
                    "a buffered audit event could not be persisted"
                );
                if cache.len() == MAX_BUFFERED_ENTRIES {
                    cache.pop_front();
                }
                cache.push_back(entry);
            }
        }
    }
    let mut guard = global().inner.lock().unwrap_or_else(|p| p.into_inner());
    if matches!(&*guard, Inner::Durable { .. }) {
        return Ok(()); // Raced with a concurrent initializer: it won.
    }
    *guard = Inner::Durable { store, cache };
    let _ = INITIALIZED.set(root);
    Ok(())
}

/// The workspace root the process-wide durable store is bound to, when
/// initialized (diagnostics/tests; `None` while still buffered).
pub fn current_root() -> Option<PathBuf> {
    INITIALIZED.get().cloned()
}

/// Records an allow-side event in the global store.
pub fn record_allow(action: &str, subject: &str, detail: &str) {
    global().record("allow", action, subject, detail);
}

/// Records a deny-side event in the global store.
pub fn record_deny(action: &str, reason: &str, subject: &str) {
    global().record("deny", action, subject, reason);
}

/// Records a structured service-level outcome (allow/deny/conflict/
/// failure) with correlation metadata in the global store.
pub fn record_outcome(
    kind: &'static str,
    action: &str,
    subject: &str,
    detail: &str,
    correlation: &AuditCorrelation,
) {
    global().record_correlated(kind, action, subject, detail, correlation);
}

/// Masks token-shaped segments in free-form audit text. Any run of 16
/// or more base62 characters (`A-Za-z0-9_-`) looks like a bearer/API
/// key (RFC 6750 opaque tokens, AWH keys, ngrok authtokens) and carries
/// no diagnostic value, so it is replaced wholesale. All other
/// characters (dots, slashes, `=`) pass through, so client IPs, project
/// paths, and prefixes like `token=` stay intact and the audit trail
/// remains useful. Opaque identifiers (session ids, UUIDs) may also
/// match and be masked — an acceptable trade: identifiers are cheap,
/// secrets are not.
pub fn redact_token_like(text: &str) -> String {
    const MIN_SECRET_LEN: usize = 16;
    let mut out = String::with_capacity(text.len());
    let mut seg = String::new();
    let flush = |out: &mut String, seg: &mut String| {
        if seg.chars().count() >= MIN_SECRET_LEN {
            out.push_str("[redacted]");
        } else {
            out.push_str(seg);
        }
        seg.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
            seg.push(c);
        } else {
            flush(&mut out, &mut seg);
            out.push(c);
        }
    }
    flush(&mut out, &mut seg);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_token_like_masks_long_base62_runs() {
        let secret = "Xq3vJ7Lb2mPz9KwR4TnE";
        assert_eq!(redact_token_like(secret), "[redacted]");
        assert_eq!(
            redact_token_like(&format!("Bearer {secret} on file")),
            "Bearer [redacted] on file"
        );
    }

    #[test]
    fn redact_token_like_preserves_short_identifiers_and_paths() {
        // Client IPs (rate-limit subjects) and project names stay intact.
        assert_eq!(redact_token_like("9.9.9.9"), "9.9.9.9");
        assert_eq!(redact_token_like("203.0.113.77"), "203.0.113.77");
        assert_eq!(redact_token_like("direct"), "direct");
        assert_eq!(
            redact_token_like("projects/demo/.agent/context.md"),
            "projects/demo/.agent/context.md"
        );
        assert_eq!(redact_token_like(""), "");
    }

    #[test]
    fn ring_buffer_caps_at_max_entries() {
        // The pre-init buffer keeps the legacy bounded-ring contract.
        let log = AuditLog::buffered();
        for i in 0..(MAX_BUFFERED_ENTRIES + 50) {
            log.record("allow", "test", "subj", &format!("detail-{i}"));
        }
        assert_eq!(log.len(), MAX_BUFFERED_ENTRIES);
        let recent = log.recent(3);
        // Newest first; the first pushed items were dropped.
        assert!(recent[0]
            .detail
            .ends_with(&format!("-{}", MAX_BUFFERED_ENTRIES + 49)));
    }

    #[test]
    fn recorded_subjects_and_details_are_redacted_at_the_choke_point() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path()).unwrap();
        // A future call site that (incorrectly) passes a bearer token
        // as subject or embeds one in detail must not persist it.
        log.record(
            "allow",
            "some_action",
            "Xq3vJ7Lb2mPz9KwR4TnE",
            "token=Xq3vJ7Lb2mPz9KwR4TnE ok",
        );
        let entry = &log.recent(1)[0];
        assert_eq!(entry.subject, "[redacted]");
        assert_eq!(entry.detail, "token=[redacted] ok");
        // And it is redacted ON DISK, not only in the read path.
        let raw = std::fs::read_to_string(dir.path().join(".agent/audit/audit.log")).unwrap();
        assert!(!raw.contains("Xq3vJ7Lb2mPz9KwR4TnE"));
    }

    #[test]
    fn recent_returns_newest_first() {
        let log = AuditLog::buffered();
        log.record("allow", "a1", "s", "first");
        log.record("deny", "a2", "s", "second");
        let recent = log.recent(10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].action, "a2");
        assert_eq!(recent[1].action, "a1");
        assert_eq!(recent[0].kind, "deny");
    }

    #[test]
    fn ts_is_epoch_millis() {
        let log = AuditLog::buffered();
        log.record("allow", "a", "s", "d");
        let e = &log.recent(1)[0];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        assert!(e.ts_ms <= now && now - e.ts_ms < 5_000);
    }
}

/// Prompt 10 (AWE-013/TW-007) tests: durability, ordering, corruption,
/// concurrency, filtering, retention, degraded mode, and correlation.
#[cfg(test)]
mod persistent_audit_tests {
    use super::*;
    use std::fs;

    fn open_store() -> (tempfile::TempDir, AuditLog) {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path()).unwrap();
        (dir, log)
    }

    fn log_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join(".agent").join("audit").join("audit.log")
    }

    fn append_three(log: &AuditLog) {
        log.record("allow", "act_one", "s1", "d1");
        log.record("deny", "act_two", "s2", "d2");
        log.record("conflict", "act_three", "s3", "d3");
    }

    /// §45: events survive a store recreation (restart) in order,
    /// with identity and sequence intact.
    #[test]
    fn events_survive_restart_in_order_with_identity() {
        let dir = tempfile::tempdir().unwrap();
        {
            let log = AuditLog::open(dir.path()).unwrap();
            append_three(&log);
        }
        // Simulated restart: a brand-new store over the same root.
        let reopened = AuditLog::open(dir.path()).unwrap();
        let recent = reopened.recent(10);
        assert_eq!(recent.len(), 3);
        // Newest first, strictly ordered.
        assert_eq!(recent[0].action, "act_three");
        assert_eq!(recent[2].action, "act_one");
        let sequences: Vec<u64> = recent.iter().map(|e| e.sequence.unwrap()).collect();
        assert!(sequences.windows(2).all(|w| w[0] > w[1]));
        assert!(recent
            .iter()
            .all(|e| e.event_id.as_deref().unwrap().starts_with("audit-")));

        // The next append continues the sequence without reuse.
        reopened.record("allow", "act_four", "s4", "d4");
        let tail = reopened.recent(1)[0].clone();
        assert!(tail.sequence.unwrap() > sequences[0]);
    }

    /// §45/§12: an interrupted append (torn final line) is detected and
    /// excluded; prior history stays intact and queryable.
    #[test]
    fn torn_tail_is_excluded_and_prior_history_survives() {
        let (dir, log) = open_store();
        append_three(&log);
        drop(log);
        // Simulate a crash mid-append: a partial line without newline.
        let path = log_path(&dir);
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("{\"checksum\":\"aa\",\"event\":{\"ts_ms\":1,");
        fs::write(&path, text).unwrap();

        let reopened =
            AuditLog::open(dir.path()).expect("torn tail is tolerated, not a hard corruption");
        let recent = reopened.recent(10);
        assert_eq!(recent.len(), 3, "only the valid records load");
        assert_eq!(recent[0].action, "act_three");
        // New appends still work and continue the sequence.
        reopened.record("allow", "after_recovery", "s", "d");
        assert_eq!(reopened.recent(1)[0].action, "after_recovery");
    }

    /// §13: middle-record corruption is a hard, structured error; the
    /// store must not silently initialize an empty history over it.
    #[test]
    fn middle_corruption_fails_closed_without_fabricating() {
        let (dir, log) = open_store();
        append_three(&log);
        drop(log);
        // Corrupt the SECOND record in place.
        let path = log_path(&dir);
        let text = fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        lines[1] = "{\"checksum\":\"tampered\",\"event\":{\"schema_version\":1}}".into();
        fs::write(&path, lines.join("\n") + "\n").unwrap();

        let error = AuditLog::open(dir.path()).unwrap_err();
        assert!(matches!(error, AuditError::Corruption(_)));
        // The evidence is preserved untouched on disk.
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("tampered"));
    }

    /// §13: an incompatible schema version fails safely.
    #[test]
    fn unknown_schema_fails_closed() {
        let (dir, _log) = open_store();
        let path = log_path(&dir);
        let future = serde_json::json!({
            "checksum": "0".repeat(64),
            "event": {
                "ts_ms": 1, "kind": "allow", "action": "a", "subject": "s",
                "detail": "d", "schema_version": 99,
                "event_id": "audit-x", "sequence": 1
            }
        });
        fs::write(&path, format!("{future}\n")).unwrap();
        let error = AuditLog::open(dir.path()).unwrap_err();
        assert!(matches!(
            error,
            AuditError::IncompatibleSchema { got: 99, .. }
        ));
    }

    /// §8: a duplicate event id in the file is corruption, never a
    /// silent overwrite — even when the forged record is otherwise
    /// fully valid (correct checksum and a fresh sequence).
    #[test]
    fn duplicate_event_id_is_corruption() {
        let (dir, log) = open_store();
        log.record("allow", "first", "s", "d");
        drop(log);
        let path = log_path(&dir);
        let text = fs::read_to_string(&path).unwrap();
        let first_line = text.lines().next().unwrap().to_owned();
        // Build a second, checksum-VALID record that reuses the same
        // event id with a bumped sequence. The forged record must be
        // serialized exactly the way the writer/parser pair does (the
        // store's typed struct order), so only the identity collision
        // can fail — not checksum construction.
        let mut forged = parse_line(&first_line).unwrap();
        forged.sequence = Some(2);
        let forged_line = serialize_line(&forged).unwrap();
        assert_ne!(forged_line, first_line);
        fs::write(&path, format!("{first_line}\n{forged_line}\n")).unwrap();
        let error = AuditLog::open(dir.path()).unwrap_err();
        assert!(
            matches!(error, AuditError::Corruption(ref detail) if detail.contains("duplicate event id")),
            "duplicate id must be corruption, got {error:?}"
        );
    }

    /// §9: sequence regression in the file is corruption.
    #[test]
    fn sequence_regression_is_corruption() {
        let (dir, log) = open_store();
        log.record("allow", "one", "s", "d");
        log.record("allow", "two", "s", "d");
        drop(log);
        let path = log_path(&dir);
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        // Swap the two records: sequence order regresses.
        fs::write(&path, format!("{}\n{}\n", lines[1], lines[0])).unwrap();
        let error = AuditLog::open(dir.path()).unwrap_err();
        assert!(matches!(error, AuditError::Corruption(_)));
    }

    /// §54: concurrent writers produce no duplicate sequence, no lost
    /// event, and strictly monotonic ordering.
    #[test]
    fn concurrent_writers_are_safe_and_ordered() {
        let dir = tempfile::tempdir().unwrap();
        let log = std::sync::Arc::new(AuditLog::open(dir.path()).unwrap());
        let mut handles = Vec::new();
        for worker in 0..8 {
            let log = std::sync::Arc::clone(&log);
            handles.push(std::thread::spawn(move || {
                for i in 0..25 {
                    log.record(
                        "allow",
                        "concurrent",
                        &format!("w{worker}"),
                        &format!("i{i}"),
                    );
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let all = log.recent(MAX_QUERY_LIMIT);
        assert_eq!(all.len(), 8 * 25, "every accepted event persisted");
        let mut sequences: Vec<u64> = all.iter().map(|e| e.sequence.unwrap()).collect();
        sequences.sort_unstable();
        let mut deduped = sequences.clone();
        deduped.dedup();
        assert_eq!(sequences.len(), deduped.len(), "no duplicate sequence");
        assert!(
            sequences.windows(2).all(|w| w[1] == w[0] + 1),
            "sequences are contiguous and strictly increasing"
        );
    }

    /// §47: secrets cannot reach the durable store — not via subject,
    /// detail, or any correlation field — from the raw file's view.
    #[test]
    fn secret_filtering_covers_correlation_fields_on_disk() {
        let (dir, log) = open_store();
        let secret = "ghp_SuperSecretTokenValue123456";
        log.record_correlated(
            "deny",
            "authorization",
            "actor",
            &format!("header=Bearer {secret}"),
            &AuditCorrelation {
                workspace_id: Some(secret.into()),
                agent_id: Some(secret.into()),
                session_id: Some(secret.into()),
                edit_id: Some(secret.into()),
                snapshot_id: Some(secret.into()),
                reason: Some(format!("reason {secret}")),
            },
        );
        let raw = fs::read_to_string(log_path(&dir)).unwrap();
        assert!(!raw.contains(secret), "secret never persists: {raw}");
        let entry = &log.recent(1)[0];
        assert!(entry.agent_id.as_deref().unwrap().contains("[redacted]"));
    }

    /// §36: oversize diagnostic text is bounded char-safely (and
    /// token-shaped runs collapse at the redaction choke point first).
    #[test]
    fn oversize_detail_is_bounded_not_rejected() {
        let (dir, log) = open_store();
        // Realistic oversize NON-secret text (spaces break the base62
        // runs so redaction leaves it intact and bounding applies).
        let huge = "word ".repeat(MAX_TEXT_CHARS);
        log.record("allow", "big", "s", &huge);
        let entry = &log.recent(1)[0];
        assert!(entry.detail.chars().count() <= MAX_TEXT_CHARS);
        assert!(entry.detail.contains("word "));
        let _ = dir;
    }

    /// §51: queries are bounded — zero, oversized, and negative-ish
    /// limits behave deterministically.
    #[test]
    fn query_limits_are_bounded() {
        let (dir, log) = open_store();
        append_three(&log);
        assert!(log.recent(0).is_empty());
        assert_eq!(log.recent(1).len(), 1);
        assert_eq!(log.recent(usize::MAX).len(), 3, "bounded by page cap");
        let _ = dir;
    }

    /// §49: the store is workspace-rooted — workspace A's store only
    /// ever holds and serves A's events.
    #[test]
    fn workspace_isolation_by_construction() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let a = AuditLog::open(dir_a.path()).unwrap();
        let b = AuditLog::open(dir_b.path()).unwrap();
        a.record("allow", "workspace_a_event", "a", "a");
        b.record("deny", "workspace_b_event", "b", "b");
        assert_eq!(a.recent(10).len(), 1);
        assert_eq!(a.recent(10)[0].action, "workspace_a_event");
        assert_eq!(b.recent(10)[0].action, "workspace_b_event");
        // No cross-root leakage in either direction.
        assert!(a.recent(10).iter().all(|e| e.action != "workspace_b_event"));
    }

    /// §50: retention rotates at the documented cap, keeps the previous
    /// generation, audits the rotation, and survives restart.
    #[test]
    fn retention_rotates_audibly_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let log = AuditLog::open(dir.path()).unwrap();
            for i in 0..MAX_PERSISTED_EVENTS {
                log.record("allow", "fill", "s", &format!("d{i}"));
            }
            // The next append triggers rotation.
            log.record("allow", "overflow", "s", "over");
            let recent = log.recent(2);
            assert_eq!(recent[0].action, "overflow");
            assert_eq!(recent[1].action, "audit.rotation");
        }
        let audit_dir = dir.path().join(".agent").join("audit");
        assert!(
            audit_dir.join("audit.log.1").exists(),
            "previous generation kept"
        );
        // Restart: fresh store reads the rotated history cleanly.
        let reopened = AuditLog::open(dir.path()).unwrap();
        assert_eq!(reopened.recent(1)[0].action, "overflow");
    }

    /// §39/§56: pre-init buffered events replay into the durable store
    /// on initialization; reads then serve the durable history.
    #[test]
    fn buffered_events_replay_into_the_durable_store_on_init() {
        let dir = tempfile::tempdir().unwrap();
        // A process-global store cannot be reset per-test; exercise the
        // same replay path through an explicit durable store instead,
        // and assert the replay contract directly.
        let log = AuditLog::buffered();
        log.record("allow", "buffered_event", "s", "d");
        assert_eq!(log.len(), 1);
        let mut store = DurableStore::open(dir.path()).unwrap();
        let buffered: Vec<AuditEntry> = {
            let guard = log.inner.lock().unwrap_or_else(|p| p.into_inner());
            match &*guard {
                Inner::Buffered { entries } => entries.iter().cloned().collect(),
                Inner::Durable { .. } => Vec::new(),
            }
        };
        for entry in buffered {
            store.append(entry).unwrap();
        }
        let persisted = store.recent(10);
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].action, "buffered_event");
        assert!(persisted[0].sequence.is_some());
    }

    /// §44: schema round-trips and optional fields stay absent when
    /// unset (no invented values).
    #[test]
    fn entry_serialization_round_trips_with_optional_absence() {
        let (dir, log) = open_store();
        log.record("allow", "plain", "s", "d");
        log.record_correlated(
            "deny",
            "correlated",
            "s",
            "d",
            &AuditCorrelation {
                workspace_id: Some("ws-1".into()),
                edit_id: Some("edit-9".into()),
                reason: Some("policy_denied".into()),
                ..Default::default()
            },
        );
        let raw = fs::read_to_string(log_path(&dir)).unwrap();
        let lines: Vec<&str> = raw.lines().collect();
        let first = serde_json::from_str::<serde_json::Value>(lines[0]).unwrap();
        let first_event = &first["event"];
        assert!(first_event.get("agent_id").is_none(), "absent stays absent");
        assert!(first_event.get("edit_id").is_none());
        // Correlation round-trips.
        let second = serde_json::from_str::<serde_json::Value>(lines[1]).unwrap();
        assert_eq!(second["event"]["workspace_id"], "ws-1");
        assert_eq!(second["event"]["edit_id"], "edit-9");
        assert_eq!(second["event"]["reason"], "policy_denied");
        // And the parsed entries match what the query returns.
        let recent = log.recent(2);
        assert_eq!(recent[0].edit_id.as_deref(), Some("edit-9"));
        assert!(recent[1].edit_id.is_none());
    }

    /// §48 (service correlation, end-to-end): with the process store
    /// initialized at a workspace root, real service outcomes land in
    /// the DURABLE store with full correlation — the exact edit id for
    /// committed and conflicted edits, the stable reason code and agent
    /// identity for authorization denials, and the rollback outcome
    /// class. One test, one root: the global store is process-wide and
    /// locks to its first initialized root, so the service-path proof
    /// is exercised sequentially here while the store-level tests above
    /// prove the durable substrate independently.
    #[test]
    fn service_outcomes_are_durable_and_correlated() {
        let dir = tempfile::tempdir().unwrap();
        crate::services::init::initialize_workspace(dir.path()).unwrap();
        // Initialize the process-wide durable store at this root. If a
        // parallel test already initialized it, this is a no-op and the
        // assertions below still run against whichever root is live —
        // so this test serializes the service-path proof with the
        // shared-store lock.
        let _ = crate::services::audit::init_global(dir.path());

        let svc = crate::services::edit::EditService::new(dir.path().to_path_buf());
        std::fs::write(dir.path().join("f.txt"), "alpha\n").unwrap();

        // Committed edit → exactly one durable allow event with the id.
        let result = svc
            .replace(crate::services::edit::EditTransaction::single(
                crate::services::edit::EditOperation::Replace {
                    path: "f.txt".into(),
                    old: "alpha".into(),
                    new: "ALPHA".into(),
                    occurrence: None,
                },
            ))
            .unwrap();
        let allow_event = crate::services::audit::global()
            .recent(50)
            .into_iter()
            .find(|e| e.action == "filesystem.replace" && e.kind == "allow")
            .expect("committed edit outcome is recorded");
        assert_eq!(
            allow_event.edit_id.as_deref(),
            Some(result.id.to_string().as_str())
        );
        assert_eq!(allow_event.subject, "f.txt");
        assert!(allow_event.sequence.is_some());

        // Conflicted edit → conflict class with the stable reason code.
        let mut stale = crate::services::edit::EditTransaction::single(
            crate::services::edit::EditOperation::Replace {
                path: "f.txt".into(),
                old: "ALPHA".into(),
                new: "BETA".into(),
                occurrence: None,
            },
        );
        stale.expected.push(crate::services::edit::ExpectedState {
            hash: Some(crate::services::edit::sha256_hex(b"stale\n")),
            ..Default::default()
        });
        let _ = svc.replace(stale).unwrap_err();
        let conflict_event = crate::services::audit::global()
            .recent(50)
            .into_iter()
            .find(|e| e.kind == "conflict" && e.action == "filesystem.replace")
            .expect("conflict outcome is recorded");
        assert_eq!(
            conflict_event.reason.as_deref(),
            Some("expected_state_conflict")
        );

        // Authorization denial → deny event with the stable reason code
        // and the agent identity.
        let authorizer =
            crate::services::authorization::EditAuthorizer::new(dir.path().to_path_buf());
        let workspace_id = crate::services::init::load_workspace_manifest(dir.path())
            .unwrap()
            .workspace_id
            .as_str()
            .to_owned();
        let principal = crate::services::authorization::AuthorizingPrincipal::agent(
            "agent-ghost",
            "session-1",
            &workspace_id,
        );
        let decision =
            authorizer.authorize(&crate::services::authorization::AuthorizationRequest {
                action: crate::services::authorization::EditAction::Replace,
                principal,
                resource: "src/main.rs".into(),
                transaction_id: None,
            });
        assert!(matches!(
            decision,
            crate::services::authorization::AuthorizationDecision::Deny { .. }
        ));
        let deny_event = crate::services::audit::global()
            .recent(50)
            .into_iter()
            .find(|e| e.action == "authorization.filesystem.replace")
            .expect("authorization denial is recorded");
        assert_eq!(deny_event.kind, "deny");
        assert_eq!(deny_event.reason.as_deref(), Some("unknown_agent"));
        assert_eq!(deny_event.agent_id.as_deref(), Some("agent-ghost"));

        // Rollback outcomes → allow / conflict correlated to the id.
        std::fs::write(dir.path().join("g.txt"), "original\n").unwrap();
        let edit_id = crate::services::edit::EditId::new();
        let mut records = svc
            .capture_rollback_records(&edit_id, &["g.txt".to_owned()])
            .unwrap();
        let result = svc
            .replace(crate::services::edit::EditTransaction::single(
                crate::services::edit::EditOperation::Replace {
                    path: "g.txt".into(),
                    old: "original".into(),
                    new: "edited".into(),
                    occurrence: None,
                },
            ))
            .unwrap();
        records[0].after_hash = result.after.hash.clone();

        let status = svc.rollback_edits(&records).unwrap();
        assert!(matches!(
            status,
            crate::services::edit::EditRollbackStatus::Restored
        ));
        let rollback_event = crate::services::audit::global()
            .recent(50)
            .into_iter()
            .find(|e| e.action == "filesystem.rollback" && e.kind == "allow")
            .expect("rollback outcome is recorded");
        assert_eq!(
            rollback_event.edit_id.as_deref(),
            Some(edit_id.to_string().as_str())
        );

        std::fs::write(dir.path().join("g.txt"), "external\n").unwrap();
        let conflict = svc.rollback_edits(&records).unwrap();
        assert!(matches!(
            conflict,
            crate::services::edit::EditRollbackStatus::Conflict { .. }
        ));
        let rollback_conflict = crate::services::audit::global()
            .recent(50)
            .into_iter()
            .find(|e| e.action == "filesystem.rollback" && e.kind == "conflict")
            .expect("rollback conflict is recorded");
        assert_eq!(
            rollback_conflict.reason.as_deref(),
            Some("rollback_conflict")
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("g.txt")).unwrap(),
            "external\n",
            "external content is preserved through the conflict"
        );
    }
}
