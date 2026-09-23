//! Agent runtime identity service (TW-002 / AGENT-001).
//!
//! The single shared application service that owns the agent runtime
//! identity boundary:
//!
//! ```text
//! AgentProfile (persisted `models::Agent`)  ──register──▶ AgentRegistry
//! AgentRegistry ──start──▶ active profile
//! active profile + workspace manifest ──open_session──▶ AgentSession
//! AgentSession ──resolve──▶ trusted caller identity (never authority)
//! ```
//!
//! Every surface — CLI, MCP, TUI, Control API — must call these operations
//! rather than implementing their own lifecycle logic (TW-002 §19, §20).
//!
//! Boundary rules enforced here (TW-002 §4–§11):
//! * A session is the AWH-native runtime identity. It is NOT the MCP
//!   protocol session (transport lifecycle) and the two are never conflated.
//! * Identity resolution answers "which caller is this?" only. It never
//!   grants capabilities, never consults or modifies policy, and never
//!   substitutes for authorization — callers that need enforcement go
//!   through the existing capability/policy subsystems.
//! * Sessions bind one agent to exactly one workspace. Cross-workspace or
//!   cross-agent substitution is rejected, never silently redirected.
//! * Disabled/unknown/inactive agents and stopped/failed sessions fail
//!   closed. Nothing is ever silently reactivated.
//! * Session records persist for correlation/audit, but resolution
//!   re-validates everything on every call — persisted state is never
//!   trusted merely because it exists (TW-002 §14).

use crate::core::agents::AgentStore;
use crate::core::identity::{
    resolve_session_relation, AgentId, IdentityRelation, SessionId, SessionIdentity, WorkspaceId,
};
use crate::core::sessions::SessionStore;
use crate::models::{Agent, AgentSessionRecord, AgentStatus, SessionStatus};
use crate::services::init::load_workspace_manifest;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;

/// The trusted resolution outcome: who the caller actually is. Identity
/// only — consuming this as permission anywhere is a contract violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCaller {
    pub session_id: String,
    pub agent_id: String,
    pub workspace_id: String,
    pub status: SessionStatus,
}

/// The agent runtime identity service for one workspace root.
pub struct AgentRuntimeService {
    root: PathBuf,
}

impl AgentRuntimeService {
    /// Creates the service bound to `root` (the workspace root containing
    /// `.agent/`).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn agents(&self) -> AgentStore {
        AgentStore::new(&self.root)
    }

    fn sessions(&self) -> SessionStore {
        SessionStore::new(&self.root)
    }

    // ------------------------------------------------------------------
    // AgentProfile / AgentRegistry (TW-002 §4, §5)
    // ------------------------------------------------------------------

    /// Registers a fresh agent profile. Duplicate ids are rejected — a
    /// registry never overwrites an existing identity. Input (id, name,
    /// role) is validated and bounded; the profile carries no capabilities.
    pub fn register_profile(&self, id: &str, name: &str, role: &str) -> Result<Agent> {
        let id = validate_agent_id(id)?;
        validate_text_field("agent name", name, 128)?;
        validate_text_field("agent role", role, 64)?;
        let agent = Agent {
            id,
            name: name.into(),
            role: role.into(),
            status: AgentStatus::Created,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.agents().register(&agent)?;
        Ok(agent)
    }

    /// Returns the profile with the given id, or `None`.
    pub fn profile(&self, id: &str) -> Result<Option<Agent>> {
        self.agents().get(id)
    }

    /// Lists all profiles sorted by id.
    pub fn profiles(&self) -> Result<Vec<Agent>> {
        self.agents().list()
    }

    /// Activates a profile (`Created|Stopped|Paused|Failed → Active`).
    /// Unknown agents fail; disabled profiles fail closed (TW-002 §5: an
    /// inactive or disabled agent must not silently become active).
    pub fn start_agent(&self, id: &str) -> Result<Agent> {
        let agent = self.require_profile(id)?;
        if !agent.enabled {
            bail!("agent {id} is disabled; enable it first");
        }
        let mut agent = agent;
        agent.status = AgentStatus::Active;
        self.agents()
            .create(&agent)
            .with_context(|| format!("failed to persist start of agent {id}"))?;
        Ok(agent)
    }

    /// Deactivates a profile (`Active → Stopped`; deliberate halt).
    /// Idempotent for already-stopped profiles.
    pub fn stop_agent(&self, id: &str) -> Result<Agent> {
        let mut agent = self.require_profile(id)?;
        if agent.status != AgentStatus::Stopped {
            agent.status = AgentStatus::Stopped;
            self.agents()
                .create(&agent)
                .with_context(|| format!("failed to persist stop of agent {id}"))?;
        }
        Ok(agent)
    }

    /// `restart = stop + start` through the same enforced operations; the
    /// resulting profile is always `Active`.
    pub fn restart_agent(&self, id: &str) -> Result<Agent> {
        self.stop_agent(id)?;
        self.start_agent(id)
    }

    /// Marks a profile failed (terminal agent-lifecycle state reached
    /// through an error path). `Failed → Active` still requires an explicit
    /// operator `start`, never an implicit one.
    pub fn fail_agent(&self, id: &str) -> Result<Agent> {
        let mut agent = self.require_profile(id)?;
        agent.status = AgentStatus::Failed;
        self.agents()
            .create(&agent)
            .with_context(|| format!("failed to persist failure of agent {id}"))?;
        Ok(agent)
    }

    /// Toggles the profile's `enabled` switch. Disabling never unloads or
    /// mutates session records; it only makes resolution fail closed.
    pub fn set_profile_enabled(&self, id: &str, enabled: bool) -> Result<Agent> {
        let mut agent = self.require_profile(id)?;
        agent.enabled = enabled;
        self.agents()
            .create(&agent)
            .with_context(|| format!("failed to persist enabled flag of agent {id}"))?;
        Ok(agent)
    }

    fn require_profile(&self, id: &str) -> Result<Agent> {
        self.agents()
            .get(id)?
            .with_context(|| format!("agent not found: {id}"))
    }

    // ------------------------------------------------------------------
    // AgentSession lifecycle (TW-002 §7–§10)
    // ------------------------------------------------------------------

    /// Opens a new runtime session for `agent_id`, binding it to the
    /// workspace recorded in the workspace manifest.
    ///
    /// Fails closed when: the workspace is uninitialized, the manifest is
    /// unreadable/corrupt, the agent is unknown, or the agent is not
    /// active (a `created`/`stopped`/`failed`/`paused` agent must open no
    /// session — TW-002 §8).
    pub fn open_session(&self, agent_id: &str) -> Result<AgentSessionRecord> {
        let manifest = load_workspace_manifest(&self.root).context(
            "cannot open a session: workspace is not initialized (run `awh init` first)",
        )?;
        let workspace_id = manifest.workspace_id;

        let Some(agent) = self.agents().get(agent_id)? else {
            bail!("agent not found: {agent_id}");
        };
        if !agent.enabled {
            bail!("agent {agent_id} is disabled; cannot open a session");
        }
        if agent.status != AgentStatus::Active {
            bail!(
                "agent {agent_id} is {}; start it before opening a session",
                agent.status.label()
            );
        }

        let identity = SessionIdentity::new(
            AgentId::new_checked(agent_id.to_owned())?,
            workspace_id.clone(),
        );
        let now = chrono::Utc::now().to_rfc3339();
        let record = AgentSessionRecord {
            session_id: identity.session_id().as_str().to_owned(),
            agent_id: agent_id.to_owned(),
            workspace_id: workspace_id.as_str().to_owned(),
            status: SessionStatus::Active,
            created_at: now.clone(),
            last_activity_at: now,
        };
        self.sessions().create(&record)?;
        Ok(record)
    }

    /// Resolves a presented (`agent_id`, `session_id`) pair to the trusted
    /// caller identity, re-validating every binding from persisted state:
    ///
    /// ```text
    /// session exists
    ///   → session.agent_id == claimed agent_id   (no substitution)
    ///   → session is usable (active|paused)       (terminal = fail closed)
    ///   → session.workspace_id == manifest       (workspace binding)
    ///   → agent exists + enabled + active        (profile re-check)
    /// ```
    ///
    /// This is deterministic lookup + validation only: no capability is
    /// granted, no policy is consulted (TW-002 §11 — identity resolution
    /// is not authorization).
    pub fn resolve_session(&self, agent_id: &str, session_id: &str) -> Result<ResolvedCaller> {
        let manifest = load_workspace_manifest(&self.root).context(
            "cannot resolve a session: workspace is not initialized (run `awh init` first)",
        )?;

        let Some(session) = self.sessions().get(session_id)? else {
            bail!("session not found: {session_id}");
        };
        if session.agent_id != agent_id {
            // Substitution attack: the session belongs to another agent.
            // Reject without revealing which agent it does belong to.
            bail!("session {session_id} does not belong to agent {agent_id}");
        }
        if !session.status.is_usable() {
            bail!(
                "session {session_id} is {} and cannot be used (open a new session)",
                session.status.label()
            );
        }
        if session.workspace_id != manifest.workspace_id.as_str() {
            bail!("session {session_id} is bound to a different workspace");
        }

        // Full relation re-check through the existing identity layer.
        let identity = SessionIdentity::new_checked(
            SessionId::new_checked(session_id)?,
            AgentId::new_checked(agent_id.to_owned())?,
            manifest.workspace_id.clone(),
        )?;
        match resolve_session_relation(&self.root, &identity)? {
            IdentityRelation::AgentActiveInWorkspace => Ok(ResolvedCaller {
                session_id: session.session_id,
                agent_id: session.agent_id,
                workspace_id: session.workspace_id,
                status: session.status,
            }),
            IdentityRelation::AgentDisabled => {
                bail!("agent {agent_id} is disabled; session {session_id} cannot be used")
            }
            IdentityRelation::AgentInactive => {
                bail!("agent {agent_id} is not active; session {session_id} cannot be used")
            }
            IdentityRelation::UnknownAgent => bail!("agent not found: {agent_id}"),
            IdentityRelation::WrongWorkspace => {
                bail!("session {session_id} is bound to a different workspace")
            }
        }
    }

    /// Applies a lifecycle transition to a session through the validated
    /// table (see [`SessionStore::transition`]). The agent's own claim on
    /// the session is enforced first (no cross-agent stops, TW-002 §16):
    /// ownership, session usability, and workspace binding are validated
    /// through the same checks as [`resolve_session`](Self::resolve_session).
    ///
    /// An exception is made for *terminal* transitions (`Stopped`,
    /// `Failed`): a stopped or disabled agent cannot *use* its sessions,
    /// but it must still be able to *retire* them. Gating teardown on an
    /// active profile would make terminal cleanup unreachable — the
    /// records could never be stopped through the runtime after
    /// `agent stop` / `agent disable` — so these transitions validate
    /// ownership and binding only, not profile activity.
    pub fn transition_session(
        &self,
        agent_id: &str,
        session_id: &str,
        next: SessionStatus,
    ) -> Result<AgentSessionRecord> {
        if next.is_usable() {
            // A transition that *extends* use (resume) or a live-state
            // change (pause) requires the full resolution contract.
            let _ = self.resolve_session(agent_id, session_id)?;
        } else {
            // Terminal transition: ownership + binding still enforced,
            // profile activity deliberately not.
            let manifest = load_workspace_manifest(&self.root).context(
                "cannot transition a session: workspace is not initialized (run `awh init` first)",
            )?;
            let Some(session) = self.sessions().get(session_id)? else {
                bail!("session not found: {session_id}");
            };
            if session.agent_id != agent_id {
                bail!("session {session_id} does not belong to agent {agent_id}");
            }
            if session.workspace_id != manifest.workspace_id.as_str() {
                bail!("session {session_id} is bound to a different workspace");
            }
        }
        self.sessions()
            .transition(session_id, next)?
            .with_context(|| format!("session not found: {session_id}"))
    }

    /// Lists sessions, optionally for one agent.
    pub fn sessions_for(&self, agent_id: Option<&str>) -> Result<Vec<AgentSessionRecord>> {
        self.sessions().list(agent_id)
    }

    /// Returns one session by id (targeted lookup; no full-store scan).
    /// Unsafe ids fail closed as `None`; a corrupt record is a loud error
    /// naming the file.
    pub fn session(&self, session_id: &str) -> Result<Option<AgentSessionRecord>> {
        self.sessions().get(session_id)
    }

    /// The workspace identity this service binds sessions to, if the
    /// workspace is initialized. A *missing* manifest is `None`; a
    /// corrupt or unsupported manifest is an error, never silently
    /// classified as "uninitialized".
    pub fn workspace_id(&self) -> Result<Option<WorkspaceId>> {
        let path = self.root.join(".agent").join("workspace.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(load_workspace_manifest(&self.root)?.workspace_id))
    }
}

/// Validates an agent id for registration: reuses the store's filename
/// safety rules and additionally bounds length.
fn validate_agent_id(id: &str) -> Result<String> {
    if !crate::core::agents::is_safe_agent_id(id) {
        bail!(
            "invalid agent id: {id:?} (must be 1-{} chars, no path separators or traversal)",
            crate::core::agents::MAX_AGENT_ID_LEN
        );
    }
    Ok(id.to_owned())
}

/// Validates a free-text profile field: non-empty and bounded.
fn validate_text_field(what: &str, value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() {
        bail!("invalid {what}: must not be empty");
    }
    if value.len() > max {
        bail!("invalid {what}: must be at most {max} bytes");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_root() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        crate::services::init::initialize_workspace(&root).unwrap();
        (temp, root)
    }

    fn started_service(root: &std::path::Path, id: &str) -> AgentRuntimeService {
        let service = AgentRuntimeService::new(root);
        service
            .register_profile(id, "Test Agent", "worker")
            .unwrap();
        service.start_agent(id).unwrap();
        service
    }

    #[test]
    fn register_rejects_duplicates_and_invalid_input() {
        let (_t, root) = init_root();
        let service = AgentRuntimeService::new(&root);
        service
            .register_profile("writer", "Writer", "writer")
            .unwrap();
        let err = service
            .register_profile("writer", "Impostor", "writer")
            .unwrap_err();
        assert!(err.to_string().contains("already registered"), "{err}");
        // Original untouched.
        assert_eq!(service.profile("writer").unwrap().unwrap().name, "Writer");

        for bad_id in ["../x", "a/b", "", "a\\b"] {
            assert!(service.register_profile(bad_id, "n", "r").is_err());
        }
        assert!(service.register_profile("ok", "", "r").is_err());
        assert!(service.register_profile("ok", "n", "").is_err());
        let long = "x".repeat(129);
        assert!(service.register_profile("ok", &long, "r").is_err());
    }

    #[test]
    fn agent_lifecycle_start_stop_restart() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        assert_eq!(
            service.profile("writer").unwrap().unwrap().status,
            AgentStatus::Active
        );
        service.stop_agent("writer").unwrap();
        assert_eq!(
            service.profile("writer").unwrap().unwrap().status,
            AgentStatus::Stopped
        );
        service.restart_agent("writer").unwrap();
        assert_eq!(
            service.profile("writer").unwrap().unwrap().status,
            AgentStatus::Active
        );
        assert!(service.start_agent("ghost").is_err());
    }

    #[test]
    fn disabled_agent_cannot_start_or_open_sessions() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        service.set_profile_enabled("writer", false).unwrap();

        let err = service.start_agent("writer").unwrap_err();
        assert!(err.to_string().contains("disabled"), "{err}");
        let err = service.open_session("writer").unwrap_err();
        assert!(err.to_string().contains("disabled"), "{err}");
        // Re-enabling restores the ability to open sessions.
        service.set_profile_enabled("writer", true).unwrap();
        service.open_session("writer").unwrap();
    }

    #[test]
    fn open_session_requires_initialized_workspace_and_active_agent() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        // Uninitialized workspace: fail closed.
        let service = AgentRuntimeService::new(&root);
        let err = service.open_session("writer").unwrap_err();
        assert!(err.to_string().contains("not initialized"), "{err}");

        crate::services::init::initialize_workspace(&root).unwrap();
        // Unknown agent.
        let err = service.open_session("ghost").unwrap_err();
        assert!(err.to_string().contains("agent not found"), "{err}");

        // Known but not started (status: created).
        service
            .register_profile("writer", "Writer", "writer")
            .unwrap();
        let err = service.open_session("writer").unwrap_err();
        assert!(err.to_string().contains("start it before"), "{err}");

        // Started: session opens and binds to the workspace manifest id.
        service.start_agent("writer").unwrap();
        let manifest = crate::services::init::load_workspace_manifest(&root).unwrap();
        let session = service.open_session("writer").unwrap();
        assert_eq!(session.workspace_id, manifest.workspace_id.as_str());
        assert_eq!(session.status, SessionStatus::Active);
    }

    #[test]
    fn resolve_session_rejects_substitution_and_rebinds() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        service
            .register_profile("reviewer", "Reviewer", "reviewer")
            .unwrap();
        service.start_agent("reviewer").unwrap();
        let writer_session = service.open_session("writer").unwrap();

        // Reviewer claiming writer's session: rejected.
        let err = service
            .resolve_session("reviewer", &writer_session.session_id)
            .unwrap_err();
        assert!(err.to_string().contains("does not belong"), "{err}");

        // Unknown session id: rejected.
        assert!(service.resolve_session("writer", "sess-ghost").is_err());

        // Legitimate claim resolves.
        let caller = service
            .resolve_session("writer", &writer_session.session_id)
            .unwrap();
        assert_eq!(caller.agent_id, "writer");
        assert_eq!(caller.workspace_id, writer_session.workspace_id);
    }

    #[test]
    fn stopped_and_failed_sessions_never_resolve() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        let session = service.open_session("writer").unwrap();

        service
            .transition_session("writer", &session.session_id, SessionStatus::Stopped)
            .unwrap();
        let err = service
            .resolve_session("writer", &session.session_id)
            .unwrap_err();
        assert!(err.to_string().contains("cannot be used"), "{err}");
        // The service gates transitions on ownership resolution, so a
        // stopped session cannot even be presented for reactivation (the
        // store-level transition table independently rejects it too).
        let err = service
            .transition_session("writer", &session.session_id, SessionStatus::Active)
            .unwrap_err();
        assert!(err.to_string().contains("cannot be used"), "{err}");

        // Failed is terminal as well.
        let session2 = service.open_session("writer").unwrap();
        service
            .transition_session("writer", &session2.session_id, SessionStatus::Failed)
            .unwrap();
        assert!(service
            .resolve_session("writer", &session2.session_id)
            .is_err());
    }

    #[test]
    fn pause_resume_round_trip_via_service() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        let session = service.open_session("writer").unwrap();

        let paused = service
            .transition_session("writer", &session.session_id, SessionStatus::Paused)
            .unwrap();
        assert_eq!(paused.status, SessionStatus::Paused);
        // Paused sessions still resolve (usable states: active|paused).
        assert!(service
            .resolve_session("writer", &session.session_id)
            .is_ok());
        let resumed = service
            .transition_session("writer", &session.session_id, SessionStatus::Active)
            .unwrap();
        assert_eq!(resumed.status, SessionStatus::Active);
    }

    #[test]
    fn session_workspace_binding_survives_agent_state_changes_only_when_valid() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        let session = service.open_session("writer").unwrap();

        // Stopping the agent must invalidate resolution for its sessions.
        service.stop_agent("writer").unwrap();
        let err = service
            .resolve_session("writer", &session.session_id)
            .unwrap_err();
        assert!(err.to_string().contains("not active"), "{err}");

        // Restarting the agent restores resolution — but only through an
        // explicit operator action, never implicitly.
        service.restart_agent("writer").unwrap();
        assert!(service
            .resolve_session("writer", &session.session_id)
            .is_ok());
    }

    #[test]
    fn concurrent_session_creation_isolated_between_agents() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        service
            .register_profile("reviewer", "Reviewer", "reviewer")
            .unwrap();
        service.start_agent("reviewer").unwrap();

        let a = service.open_session("writer").unwrap();
        let b = service.open_session("reviewer").unwrap();

        // Each session resolves for its own agent only, and both bind to
        // the same (single) workspace identity.
        assert_ne!(a.session_id, b.session_id);
        assert_eq!(a.workspace_id, b.workspace_id);
        assert!(service.resolve_session("writer", &a.session_id).is_ok());
        assert!(service.resolve_session("reviewer", &b.session_id).is_ok());
        assert!(service.resolve_session("reviewer", &a.session_id).is_err());
        assert!(service.resolve_session("writer", &b.session_id).is_err());
    }

    #[test]
    fn parallel_session_creation_yields_unique_intact_records() {
        const THREADS: usize = 8;
        const PER_THREAD: usize = 6;

        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        service
            .register_profile("reviewer", "Reviewer", "reviewer")
            .unwrap();
        service.start_agent("reviewer").unwrap();

        let root = std::sync::Arc::new(root);
        let mut handles = Vec::new();
        for t in 0..THREADS {
            let root = root.clone();
            handles.push(std::thread::spawn(move || {
                let agent = if t % 2 == 0 { "writer" } else { "reviewer" };
                let service = AgentRuntimeService::new(root.as_path());
                let mut ids = Vec::new();
                for _ in 0..PER_THREAD {
                    let session = service.open_session(agent).unwrap();
                    ids.push((agent.to_owned(), session.session_id));
                }
                ids
            }));
        }

        // Collect from all threads: ids must be globally unique (no
        // duplicate runtime identity, no accidental agent replacement).
        let mut seen = std::collections::HashSet::new();
        for handle in handles {
            for (agent, id) in handle.join().unwrap() {
                assert!(seen.insert(id.clone()), "duplicate session id: {id}");
                let session = AgentRuntimeService::new(root.as_path())
                    .resolve_session(&agent, &id)
                    .unwrap_or_else(|e| panic!("concurrent record {id} must resolve: {e}"));
                assert_eq!(session.agent_id, agent);
            }
        }
        assert_eq!(seen.len(), THREADS * PER_THREAD);

        // Persistence is intact: every record is loadable, correctly
        // attributed, and workspace-bound (no cross-session leakage).
        let all = AgentRuntimeService::new(root.as_path())
            .sessions_for(None)
            .unwrap();
        assert_eq!(all.len(), THREADS * PER_THREAD);
        for session in &all {
            assert!(matches!(session.status, SessionStatus::Active));
            let manifest = crate::services::init::load_workspace_manifest(root.as_path()).unwrap();
            assert_eq!(session.workspace_id, manifest.workspace_id.as_str());
        }
    }

    #[test]
    fn corrupt_session_record_fails_closed() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        let session = service.open_session("writer").unwrap();
        let second = service.open_session("writer").unwrap();

        let path = root
            .join(".agent")
            .join("sessions")
            .join(format!("{}.json", session.session_id));
        std::fs::write(&path, "{ not json").unwrap();
        let err = service
            .resolve_session("writer", &session.session_id)
            .unwrap_err();
        // Targeted access fails loudly AND names the damaged file so an
        // operator can inspect it (W3: distinguish corrupt from missing).
        let chain = format!("{err:#}");
        assert!(chain.contains("key must be a string"), "{chain}");
        assert!(chain.contains("corrupt session record"), "{chain}");

        // Listing stays usable workspace-wide: the intact record still
        // lists while the damaged one is skipped (W3: one torn file must
        // not brick `agent status` / `session list` / `session show`).
        let listed = service.sessions_for(None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].session_id, second.session_id);
    }

    #[test]
    fn terminal_cleanup_reachable_after_agent_stop_or_disable() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        let session = service.open_session("writer").unwrap();

        // Stopping (or disabling) the agent makes the session unusable...
        service.stop_agent("writer").unwrap();
        let err = service
            .resolve_session("writer", &session.session_id)
            .unwrap_err();
        assert!(err.to_string().contains("not active"), "{err}");

        // ...but teardown remains reachable: a terminal transition still
        // validates ownership + workspace binding without requiring an
        // active profile (S1 review finding: cleanup must not become
        // unreachable). No silent reactivation in the other direction.
        let stopped = service
            .transition_session("writer", &session.session_id, SessionStatus::Stopped)
            .unwrap();
        assert_eq!(stopped.status, SessionStatus::Stopped);
        let err = service
            .transition_session("writer", &session.session_id, SessionStatus::Active)
            .unwrap_err();
        assert!(err.to_string().contains("cannot be used"), "{err}");

        // Ownership is still enforced for terminal transitions: another
        // agent cannot retire a session it does not own.
        service
            .register_profile("reviewer", "Reviewer", "reviewer")
            .unwrap();
        service.start_agent("reviewer").unwrap();
        let err = service
            .transition_session("reviewer", &session.session_id, SessionStatus::Stopped)
            .unwrap_err();
        assert!(err.to_string().contains("does not belong"), "{err}");
    }

    #[test]
    fn disabled_agent_can_still_retire_but_not_resume_sessions() {
        let (_t, root) = init_root();
        let service = started_service(&root, "writer");
        let session = service.open_session("writer").unwrap();

        // Pause while the agent is still enabled (a paused session is
        // usable, so resolution proceeds past the status check).
        service
            .transition_session("writer", &session.session_id, SessionStatus::Paused)
            .unwrap();

        service.set_profile_enabled("writer", false).unwrap();

        // Resume requires full resolution and now fails on the disabled
        // profile — a disabled agent cannot resurrect a session.
        let err = service
            .transition_session("writer", &session.session_id, SessionStatus::Active)
            .unwrap_err();
        assert!(err.to_string().contains("disabled"), "{err}");

        // Terminal teardown stays reachable despite the disabled profile.
        let stopped = service
            .transition_session("writer", &session.session_id, SessionStatus::Stopped)
            .unwrap();
        assert_eq!(stopped.status, SessionStatus::Stopped);
    }

    #[test]
    fn workspace_id_helper_reports_binding() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        let service = AgentRuntimeService::new(&root);
        assert!(service.workspace_id().unwrap().is_none());
        crate::services::init::initialize_workspace(&root).unwrap();
        assert!(service.workspace_id().unwrap().is_some());
    }

    #[test]
    fn workspace_id_distinguishes_missing_from_corrupt_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        let service = AgentRuntimeService::new(&root);

        // Missing manifest: None (genuinely uninitialized).
        assert!(service.workspace_id().unwrap().is_none());

        // Corrupt manifest: a loud error, never silently "uninitialized"
        // (S5 review finding: health checks must not misclassify).
        let agent_dir = root.join(".agent");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(agent_dir.join("workspace.json"), "{ not json").unwrap();
        let err = service.workspace_id().unwrap_err();
        assert!(format!("{err:#}").contains("workspace manifest"), "{err:#}");
    }
}
