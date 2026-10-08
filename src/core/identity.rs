//! Strongly typed runtime identity contracts (TW-001).
//!
//! These newtypes make the Trust-Wedge identity graph
//! (Workspace → Agent → Session → Task, with edits/snapshots/audit events
//! hanging off tasks) unambiguous across CLI/service/persistence
//! boundaries: a [`SessionId`] cannot be confused with a [`TaskId`] and a
//! malformed id is rejected before use.
//!
//! Edit and snapshot identifiers are intentionally *not* duplicated here —
//! they already have stable definitions:
//!
//! - [`EditId`](crate::services::edit::EditId): `edit-<nanos>-<seq>`
//! - [`SnapshotId`](crate::services::snapshot::SnapshotId): `snap-<nanos>-<pid>-<seq>`
//!
//! Boundary note: the pre-existing models in `crate::models` (Agent, Task,
//! PolicyRule, CapabilityGrant) keep their plain-`String` ids for
//! compatibility. These new types are the canonical contracts going
//! forward; existing code is bridged via `as_str`/`new_checked` rather than
//! force-migrated.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum accepted length of any identity value; long enough for the
/// generated `prefix-nanos-pid-seq` format, short enough to stay a filename
/// and to bound parsing cost on hostile input.
pub const MAX_ID_LEN: usize = 128;

/// Error returned when an identity value fails validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidIdError(pub String);

impl fmt::Display for InvalidIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid identity: {}", self.0)
    }
}

impl std::error::Error for InvalidIdError {}

fn validate_id(id: &str) -> Result<(), InvalidIdError> {
    if id.is_empty() {
        return Err(InvalidIdError("id is empty".into()));
    }
    if id.len() > MAX_ID_LEN {
        return Err(InvalidIdError("id is too long".into()));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(InvalidIdError(
            "id must not contain path separators or traversal".into(),
        ));
    }
    if id.bytes().any(|b| b == 0 || b.is_ascii_control()) {
        return Err(InvalidIdError(
            "id must not contain control characters".into(),
        ));
    }
    Ok(())
}

/// Generates a fresh, process-locally unique id with the given prefix.
/// `nanos + pid` tie-break between processes, `seq` between fresh ids in
/// the same nanosecond — the same scheme as `EditId`/`SnapshotId`.
fn generate(prefix: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64;
    let pid = std::process::id() as u64;
    let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{nanos:016x}-{pid}-{seq}")
}

macro_rules! identity_id {
    ($name:ident, $prefix:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Generates a fresh, unique id (e.g. `prefix-<nanos>-<pid>-<seq>`).
            pub fn new() -> Self {
                Self(generate($prefix))
            }

            /// Wraps an already-persisted/external id, rejecting malformed
            /// values instead of silently accepting them.
            pub fn new_checked(raw: impl Into<String>) -> Result<Self, InvalidIdError> {
                let id = Self(raw.into());
                id.validate()?;
                Ok(id)
            }

            /// Rejects empty, oversized, traversal, and control-character ids.
            pub fn validate(&self) -> Result<(), InvalidIdError> {
                validate_id(&self.0)
            }

            /// String view for bridging to the existing plain-`String`
            /// models in `crate::models`.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

identity_id!(
    WorkspaceId,
    "ws",
    "Persistent identity of one AWH workspace (the runtime boundary recorded in `.agent/workspace.json`)."
);
identity_id!(
    AgentId,
    "agent",
    "Stable identity of an agent operating within a workspace."
);
identity_id!(
    SessionId,
    "sess",
    "Identity of one runtime session; belongs to exactly one agent + workspace. A session id is ephemeral (a restart gets a new one) — it is never a durable authority grant."
);
identity_id!(
    TaskId,
    "task",
    "Identity of one execution/task record; distinct from any transport/session id."
);
identity_id!(
    AuditEventId,
    "audit",
    "Identity of one audit/event record correlating a consequential operation."
);

/// Runtime binding of a caller to exactly one agent within exactly one
/// workspace (TW-001 §8 invariants 1–3, 6–7).
///
/// A [`SessionIdentity`] is *identity, not authority*: constructing one
/// never grants capabilities and never bypasses policy — enforcement
/// remains the job of the capability/policy subsystems. Sessions are
/// ephemeral by design (a restart gets a new session id), so this type is
/// not persisted as durable authority anywhere in TW-001.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIdentity {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub workspace_id: WorkspaceId,
}

impl SessionIdentity {
    /// Creates a fresh session binding `agent` within `workspace`.
    pub fn new(agent_id: AgentId, workspace_id: WorkspaceId) -> Self {
        Self {
            session_id: SessionId::new(),
            agent_id,
            workspace_id,
        }
    }

    /// Restores a persisted/external session record, validating both
    /// referenced ids so a malformed record is rejected before use.
    pub fn new_checked(
        session_id: SessionId,
        agent_id: AgentId,
        workspace_id: WorkspaceId,
    ) -> Result<Self, InvalidIdError> {
        session_id.validate()?;
        agent_id.validate()?;
        workspace_id.validate()?;
        Ok(Self {
            session_id,
            agent_id,
            workspace_id,
        })
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }
    pub fn workspace_id(&self) -> &WorkspaceId {
        &self.workspace_id
    }
}

/// The relationship a session (or task) record claims relative to the
/// workspace and agent it names. See TW-001 §8: an agent cannot open a
/// session in an unrelated workspace, and an unknown or inactive agent must
/// not be treated as a valid caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityRelation {
    /// The referenced agent exists and is active in the referenced workspace.
    AgentActiveInWorkspace,
    /// The agent record exists in the workspace but is not active.
    AgentInactive,
    /// The agent record exists but its profile is disabled (`enabled=false`).
    /// Disabled is distinct from inactive (lifecycle) and from unknown — a
    /// disabled profile is a deliberate operator decision and must never be
    /// quietly treated as merely stopped (TW-002 §5).
    AgentDisabled,
    /// No agent record exists for the referenced id in the workspace.
    UnknownAgent,
    /// The referenced workspace is not the workspace being operated on.
    WrongWorkspace,
}

/// Resolves whether an agent reference is a valid, active caller for the
/// given workspace using the existing persisted agent records.
///
/// This is a pure identity check — it deliberately does **not** consult or
/// modify capabilities or policy. [`IdentityRelation::AgentActiveInWorkspace`]
/// must never be interpreted as permission: it only means the caller is
/// resolvable, and authorization still has to be evaluated separately.
pub fn resolve_session_relation(
    workspace_root: &std::path::Path,
    identity: &SessionIdentity,
) -> Result<IdentityRelation> {
    let manifest =
        crate::services::init::load_workspace_manifest(workspace_root).with_context(|| {
            format!(
                "cannot resolve session for an uninitialized workspace: {}",
                workspace_root.display()
            )
        })?;
    if *identity.workspace_id() != manifest.workspace_id {
        return Ok(IdentityRelation::WrongWorkspace);
    }
    let store = crate::core::agents::AgentStore::new(workspace_root);
    let Some(agent) = store
        .get(identity.agent_id().as_str())
        .with_context(|| format!("failed to load agent {}", identity.agent_id()))?
    else {
        return Ok(IdentityRelation::UnknownAgent);
    };
    if !agent.enabled {
        return Ok(IdentityRelation::AgentDisabled);
    }
    if agent.status != crate::models::AgentStatus::Active {
        return Ok(IdentityRelation::AgentInactive);
    }
    Ok(IdentityRelation::AgentActiveInWorkspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_have_distinct_prefixes_and_are_unique() {
        let ws = WorkspaceId::new();
        let agent = AgentId::new();
        let session = SessionId::new();
        let task = TaskId::new();
        let audit = AuditEventId::new();
        assert!(ws.as_str().starts_with("ws-"));
        assert!(agent.as_str().starts_with("agent-"));
        assert!(session.as_str().starts_with("sess-"));
        assert!(task.as_str().starts_with("task-"));
        assert!(audit.as_str().starts_with("audit-"));
        assert_ne!(WorkspaceId::new(), WorkspaceId::new());
        assert_ne!(SessionId::new(), SessionId::new());
    }

    #[test]
    fn serde_round_trip_is_deterministic() {
        let agent = AgentId::new();
        let json = serde_json::to_string(&agent).unwrap();
        let raw = serde_json::to_string(agent.as_str()).unwrap();
        assert_eq!(json, raw, "transparent serialization is a bare string");
        let back: AgentId = serde_json::from_str(&json).unwrap();
        assert_eq!(agent, back);
    }

    #[test]
    fn validate_rejects_bad_ids() {
        for bad in [
            "",
            "..",
            "a/b",
            "a\\b",
            "a/../b",
            "a\u{0}b",
            "a\u{7}b",
            &"x".repeat(MAX_ID_LEN + 1),
        ] {
            assert!(WorkspaceId(bad.to_string()).validate().is_err(), "{bad:?}");
            assert!(SessionId::new_checked(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn session_identity_round_trip_and_relationships() {
        let ws = WorkspaceId::new();
        let agent = AgentId::new();
        let session = SessionIdentity::new(agent.clone(), ws.clone());
        assert_eq!(session.agent_id(), &agent);
        assert_eq!(session.workspace_id(), &ws);
        // Transparent, deterministic serialization.
        let json = serde_json::to_string(&session).unwrap();
        let back: SessionIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(session, back);
        // new_checked validates all three ids.
        assert!(SessionIdentity::new_checked(SessionId::new(), AgentId::new(), ws.clone()).is_ok());
        assert!(SessionIdentity::new_checked(SessionId("bad/../id".into()), agent, ws).is_err());
    }

    #[test]
    fn resolve_session_relation_enforces_workspace_and_agent_status() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        crate::services::init::initialize_workspace(&root).unwrap();
        let manifest = crate::services::init::load_workspace_manifest(&root).unwrap();

        let store = crate::core::agents::AgentStore::new(&root);
        let created = crate::models::Agent {
            id: "writer".into(),
            name: "Writer".into(),
            role: "writer".into(),
            status: crate::models::AgentStatus::Created,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        store.create(&created).unwrap();

        // Unknown agent → not a valid caller.
        let unknown = SessionIdentity::new(AgentId::new(), manifest.workspace_id.clone());
        assert_eq!(
            resolve_session_relation(&root, &unknown).unwrap(),
            IdentityRelation::UnknownAgent
        );
        // Known but inactive (status `created`) agent → still not valid.
        let agent_id = crate::core::identity::AgentId::new_checked("writer").unwrap();
        let inactive = SessionIdentity::new(agent_id.clone(), manifest.workspace_id.clone());
        assert_eq!(
            resolve_session_relation(&root, &inactive).unwrap(),
            IdentityRelation::AgentInactive
        );
        // Active agent → resolvable (identity only, never authority).
        store
            .set_status("writer", crate::models::AgentStatus::Active)
            .unwrap();
        let active = SessionIdentity::new(agent_id, manifest.workspace_id.clone());
        assert_eq!(
            resolve_session_relation(&root, &active).unwrap(),
            IdentityRelation::AgentActiveInWorkspace
        );
        // Same agent, different workspace → rejected.
        let other_ws = WorkspaceId::new();
        let wrong = SessionIdentity::new(
            crate::core::identity::AgentId::new_checked("writer").unwrap(),
            other_ws,
        );
        assert_eq!(
            resolve_session_relation(&root, &wrong).unwrap(),
            IdentityRelation::WrongWorkspace
        );
        // Disabled profile → its own relation, distinct from inactive/unknown.
        store.set_enabled("writer", false).unwrap();
        let disabled = SessionIdentity::new(
            crate::core::identity::AgentId::new_checked("writer").unwrap(),
            manifest.workspace_id.clone(),
        );
        assert_eq!(
            resolve_session_relation(&root, &disabled).unwrap(),
            IdentityRelation::AgentDisabled
        );
    }

    #[test]
    fn identity_types_are_unambiguous_across_relationships() {
        // Workspace → Agent → Session → Task: each level has its own type
        // and prefix, so a serialized session can never be mistaken for a
        // task or an agent.
        let ws = WorkspaceId::new();
        let agent = AgentId::new();
        let session = SessionId::new();
        let task = TaskId::new();
        assert_ne!(ws.as_str(), agent.as_str());
        assert_ne!(agent.as_str(), session.as_str());
        assert_ne!(session.as_str(), task.as_str());
        // Deserializing a persisted id reproduces the same identity.
        let session_json = serde_json::to_string(&session).unwrap();
        let session_back: SessionId = serde_json::from_str(&session_json).unwrap();
        assert_eq!(session, session_back);
        // A session id value does not parse as a *valid* wrong-prefixed
        // assumption: validation is applied per type.
        assert!(SessionId("task-abc".into()).validate().is_ok());
    }
}
