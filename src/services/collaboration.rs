//! Canonical collaboration service (COL-001).
//!
//! The single collaboration authority every interface plane must call:
//! it validates ownership mutations against the canonical runtime,
//! task, worktree, and workspace contracts, delegates state changes to
//! the [`CollaborationStore`] (the only collaboration persistence owner),
//! and emits correlated audit events through the existing persistent
//! [`AuditLog`]. The service owns relationship state only — never agent
//! identity, task lifecycle, worktree mechanics, authorization, or
//! audit storage.
//!
//! Security posture at the mutation boundary (contract §5): the owner
//! agent must exist and be enabled; a supplied session must resolve
//! through the runtime (which re-validates ownership, workspace
//! binding, and usability); the referenced task/worktree must exist in
//! this workspace. An agent that is merely *stopped* (not disabled) may
//! still hold or receive ownership — that mismatch surfaces as
//! evidence through [`CollaborationService::conflicts`] instead of
//! silently blocking the transfer, keeping assignment, execution, and
//! collaboration lifecycles distinct.
//!
//! Refusals are audited as denies (`record_deny`), successful
//! transitions as correlated `collab` events — the same redacted
//! persistent audit authority every other domain uses.

use crate::core::collaboration::{
    CollabRecord, CollabResult, CollaborationStore, ResourceKind, DEFAULT_LIST_LIMIT,
    MAX_LIST_LIMIT,
};
use crate::core::tasks::TaskStore;
use crate::models::agent::Agent;
use crate::services::agent_runtime::AgentRuntimeService;
use crate::services::audit::{self, AuditCorrelation, AuditEntry, AuditLog};
use crate::services::worktree::WorktreeStore;
use anyhow::{bail, Context as _, Result};
use std::path::PathBuf;

/// Audit kind for collaboration transition events (allows only —
/// refusals audit as `deny`).
pub const COLLAB_AUDIT_KIND: &str = "collab";

/// One evidence-based conflict finding. Evidence is an observable fact
/// plus the ids that prove it — never a semantic conclusion.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConflictFinding {
    /// Resource the finding is about.
    pub resource_kind: ResourceKind,
    /// Canonical resource id.
    pub resource_id: String,
    /// The record's current owner (for `owner_*` evidence).
    pub owner_agent: Option<String>,
    /// What was observed, as a stable code.
    pub evidence: &'static str,
    /// Human-readable restatement of the evidence (ids only).
    pub detail: String,
}

/// The canonical collaboration service for one workspace root.
pub struct CollaborationService {
    root: PathBuf,
}

impl CollaborationService {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn store(&self) -> CollaborationStore {
        CollaborationStore::new(&self.root)
    }

    fn runtime(&self) -> AgentRuntimeService {
        AgentRuntimeService::new(&self.root)
    }

    /// Audits a refusal as a deny (never a transition event).
    fn audit_deny(&self, action: &str, reason: &str, subject: &str) {
        audit::record_deny(action, reason, subject);
    }

    fn correlation(
        &self,
        agent_id: Option<&str>,
        session_id: Option<&str>,
        reason: &str,
    ) -> AuditCorrelation {
        let workspace_id = self
            .runtime()
            .workspace_id()
            .ok()
            .flatten()
            .map(|id| id.as_str().to_owned());
        AuditCorrelation {
            workspace_id,
            agent_id: agent_id.map(str::to_owned),
            session_id: session_id.map(str::to_owned),
            edit_id: None,
            snapshot_id: None,
            reason: Some(reason.to_owned()),
        }
    }

    /// Lists agent profiles eligible to hold collaboration ownership
    /// (the `collaboration agents` surface). Enabled state is reported,
    /// not enforced here.
    pub fn agents(&self) -> Result<Vec<Agent>> {
        self.runtime().profiles()
    }

    /// Current ownership records, bounded by `limit` (default
    /// [`DEFAULT_LIST_LIMIT`], cap [`MAX_LIST_LIMIT`]), newest change
    /// first. `kind` optionally filters by resource kind.
    pub fn status(
        &self,
        kind: Option<ResourceKind>,
        limit: Option<usize>,
    ) -> Result<Vec<CollabRecord>> {
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).min(MAX_LIST_LIMIT);
        let records = self.store().list(kind)?;
        Ok(records.into_iter().take(limit).collect())
    }

    /// One record, when the resource has ever been assigned.
    pub fn get(&self, kind: ResourceKind, resource_id: &str) -> Result<Option<CollabRecord>> {
        self.store().get(kind, resource_id)
    }

    /// Validates the *owner* side of a mutation: the agent must exist
    /// and be enabled; when a session is supplied it must resolve
    /// through the runtime (which re-checks ownership, workspace
    /// binding, and usability on every call).
    fn validate_owner(&self, agent_id: &str, session_id: Option<&str>) -> Result<()> {
        let runtime = self.runtime();
        match runtime.profile(agent_id)? {
            Some(profile) => {
                if !profile.enabled {
                    bail!("agent {agent_id} is disabled");
                }
            }
            None => bail!("agent not found: {agent_id}"),
        }
        if let Some(session_id) = session_id {
            runtime
                .resolve_session(agent_id, session_id)
                .with_context(|| format!("cannot bind session {session_id} to agent {agent_id}"))?;
        }
        Ok(())
    }

    /// Validates the *resource* side: the task/worktree must exist in
    /// this workspace. Possession of an id is never enough.
    fn validate_resource(&self, kind: ResourceKind, resource_id: &str) -> Result<()> {
        match kind {
            ResourceKind::Task => {
                let store = TaskStore::new(&self.root).context("failed to open the task store")?;
                match store.get(resource_id)? {
                    Some(_) => Ok(()),
                    None => bail!("task not found: {resource_id}"),
                }
            }
            ResourceKind::Worktree => {
                let store = WorktreeStore::new(&self.root);
                let exists = store
                    .list()?
                    .iter()
                    .any(|w| w.worktree_id == resource_id && w.removed_at.is_none());
                if exists {
                    Ok(())
                } else {
                    bail!("worktree not found: {resource_id}")
                }
            }
        }
    }

    fn map<T>(outcome: CollabResult<T>) -> Result<T> {
        match outcome {
            Ok(value) => Ok(value),
            Err(err) => bail!("{}", err.message()),
        }
    }

    /// Assigns a resource to an agent. Audits the refusal (validation
    /// or store denial) or the successful transition.
    pub fn assign(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        agent_id: &str,
        session_id: Option<&str>,
        note: Option<&str>,
        expected_revision: Option<u64>,
    ) -> Result<CollabRecord> {
        let subject = format!("{}:{resource_id}", kind.as_str());
        if let Err(err) = self.validate_owner(agent_id, session_id) {
            self.audit_deny("collab_assign", &err.to_string(), &subject);
            return Err(err);
        }
        if let Err(err) = self.validate_resource(kind, resource_id) {
            self.audit_deny("collab_assign", &err.to_string(), &subject);
            return Err(err);
        }
        match self.store().assign(
            kind,
            resource_id,
            agent_id,
            session_id,
            note,
            expected_revision,
        ) {
            Ok(outcome) => match Self::map(outcome) {
                Ok(record) => {
                    audit::global().record_correlated(
                        COLLAB_AUDIT_KIND,
                        "assign",
                        &subject,
                        &format!(
                            "owner={agent_id} revision={} state={}",
                            record.revision,
                            record.state.as_str()
                        ),
                        &self.correlation(Some(agent_id), session_id, "assigned"),
                    );
                    Ok(record)
                }
                Err(err) => {
                    self.audit_assign_denial(&subject, &err);
                    Err(err)
                }
            },
            Err(err) => Err(err.context("failed to assign")),
        }
    }

    /// Audits a store-level domain denial from `assign`.
    fn audit_assign_denial(&self, subject: &str, err: &anyhow::Error) {
        self.audit_deny("collab_assign", &err.to_string(), subject);
    }

    /// Audits a store-level domain denial from `accept_handoff`.
    fn audit_accept_denial(&self, subject: &str, err: &anyhow::Error) {
        self.audit_deny("collab_accept", &err.to_string(), subject);
    }

    /// Marks a held resource as actively worked on. Owner-only.
    pub fn activate(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        agent_id: &str,
        expected_revision: u64,
    ) -> Result<CollabRecord> {
        self.owned_transition(
            "collab_activate",
            "activate",
            kind,
            resource_id,
            agent_id,
            |store| store.activate(kind, resource_id, agent_id, expected_revision),
        )
    }

    /// Requests a handoff from the current owner to `to_agent`. The
    /// target must itself be a valid owner (exists + enabled).
    pub fn request_handoff(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        from_agent: &str,
        to_agent: &str,
        note: Option<&str>,
        expected_revision: u64,
    ) -> Result<CollabRecord> {
        if let Err(err) = self.validate_owner(to_agent, None) {
            self.audit_deny(
                "collab_handoff",
                &err.to_string(),
                &format!("{}:{resource_id}", kind.as_str()),
            );
            return Err(err);
        }
        self.owned_transition(
            "collab_handoff",
            "handoff_request",
            kind,
            resource_id,
            from_agent,
            |store| {
                store.request_handoff(
                    kind,
                    resource_id,
                    from_agent,
                    to_agent,
                    note,
                    expected_revision,
                )
            },
        )
    }

    /// Accepts a pending handoff as the target agent. The accepting
    /// agent must be a valid owner; the record must hold the pending
    /// request naming it.
    pub fn accept_handoff(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        accepting_agent: &str,
        expected_revision: u64,
    ) -> Result<CollabRecord> {
        let subject = format!("{}:{resource_id}", kind.as_str());
        if let Err(err) = self.validate_owner(accepting_agent, None) {
            self.audit_deny("collab_accept", &err.to_string(), &subject);
            return Err(err);
        }
        match self
            .store()
            .accept_handoff(kind, resource_id, accepting_agent, expected_revision)
        {
            Ok(outcome) => match Self::map(outcome) {
                Ok(record) => {
                    audit::global().record_correlated(
                        COLLAB_AUDIT_KIND,
                        "handoff_accept",
                        &subject,
                        &format!(
                            "owner={accepting_agent} revision={} state={}",
                            record.revision,
                            record.state.as_str()
                        ),
                        &self.correlation(Some(accepting_agent), None, "handoff_accepted"),
                    );
                    Ok(record)
                }
                Err(err) => {
                    self.audit_accept_denial(&subject, &err);
                    Err(err)
                }
            },
            Err(err) => Err(err.context("failed to accept the handoff")),
        }
    }

    /// Releases a held resource. Owner-only.
    pub fn release(
        &self,
        kind: ResourceKind,
        resource_id: &str,
        agent_id: &str,
        expected_revision: u64,
    ) -> Result<CollabRecord> {
        self.owned_transition(
            "collab_release",
            "release",
            kind,
            resource_id,
            agent_id,
            |store| store.release(kind, resource_id, agent_id, expected_revision),
        )
    }

    /// Shared owner-gated transition: audited deny on refusal, audited
    /// correlated `collab` event on success.
    fn owned_transition(
        &self,
        deny_action: &'static str,
        allow_action: &'static str,
        kind: ResourceKind,
        resource_id: &str,
        agent_id: &str,
        apply: impl FnOnce(&CollaborationStore) -> Result<CollabResult<CollabRecord>>,
    ) -> Result<CollabRecord> {
        let subject = format!("{}:{resource_id}", kind.as_str());
        if let Err(err) = self.validate_owner(agent_id, None) {
            self.audit_deny(deny_action, &err.to_string(), &subject);
            return Err(err);
        }
        match apply(&self.store()) {
            Ok(outcome) => match Self::map(outcome) {
                Ok(record) => {
                    audit::global().record_correlated(
                        COLLAB_AUDIT_KIND,
                        allow_action,
                        &subject,
                        &format!(
                            "owner={agent_id} revision={} state={}",
                            record.revision,
                            record.state.as_str()
                        ),
                        &self.correlation(Some(agent_id), None, allow_action),
                    );
                    Ok(record)
                }
                Err(err) => {
                    self.audit_deny(deny_action, &err.to_string(), &subject);
                    Err(err)
                }
            },
            Err(err) => Err(err.context("collaboration transition failed")),
        }
    }

    /// Scans held records for observable ownership problems. Every
    /// finding states a fact plus the ids that prove it; nothing is
    /// resolved automatically.
    pub fn conflicts(&self) -> Result<Vec<ConflictFinding>> {
        let runtime = self.runtime();
        let tasks = TaskStore::new(&self.root).context("failed to open the task store")?;
        let worktrees = WorktreeStore::new(&self.root);
        let records = self.store().list(None)?;
        let mut findings = Vec::new();
        for record in records.iter().filter(|r| r.state.is_held()) {
            // Owner profile evidence.
            match runtime.profile(&record.owner_agent) {
                Ok(None) => findings.push(ConflictFinding {
                    resource_kind: record.resource_kind,
                    resource_id: record.resource_id.clone(),
                    owner_agent: Some(record.owner_agent.clone()),
                    evidence: "owner_agent_unknown",
                    detail: format!("owner agent {} does not exist", record.owner_agent),
                }),
                Ok(Some(profile)) => {
                    if !profile.enabled {
                        findings.push(ConflictFinding {
                            resource_kind: record.resource_kind,
                            resource_id: record.resource_id.clone(),
                            owner_agent: Some(record.owner_agent.clone()),
                            evidence: "owner_agent_disabled",
                            detail: format!("owner agent {} is disabled", record.owner_agent),
                        });
                    } else if profile.status != crate::models::agent::AgentStatus::Active {
                        findings.push(ConflictFinding {
                            resource_kind: record.resource_kind,
                            resource_id: record.resource_id.clone(),
                            owner_agent: Some(record.owner_agent.clone()),
                            evidence: "owner_agent_stopped",
                            detail: format!(
                                "owner agent {} is not active while holding ownership",
                                record.owner_agent
                            ),
                        });
                    }
                }
                Err(err) => findings.push(ConflictFinding {
                    resource_kind: record.resource_kind,
                    resource_id: record.resource_id.clone(),
                    owner_agent: Some(record.owner_agent.clone()),
                    evidence: "owner_check_failed",
                    detail: format!("owner check failed: {err}"),
                }),
            }
            // Bound-session evidence.
            if let Some(session_id) = &record.owner_session {
                match runtime.session(session_id) {
                    Ok(Some(session)) if !session.status.is_usable() => {
                        findings.push(ConflictFinding {
                            resource_kind: record.resource_kind,
                            resource_id: record.resource_id.clone(),
                            owner_agent: Some(record.owner_agent.clone()),
                            evidence: "owner_session_not_usable",
                            detail: format!(
                                "bound session {session_id} is {} and cannot serve the owner",
                                session.status.label()
                            ),
                        })
                    }
                    Ok(None) => findings.push(ConflictFinding {
                        resource_kind: record.resource_kind,
                        resource_id: record.resource_id.clone(),
                        owner_agent: Some(record.owner_agent.clone()),
                        evidence: "owner_session_unknown",
                        detail: format!("bound session {session_id} does not exist"),
                    }),
                    Ok(Some(_)) => {}
                    Err(err) => findings.push(ConflictFinding {
                        resource_kind: record.resource_kind,
                        resource_id: record.resource_id.clone(),
                        owner_agent: Some(record.owner_agent.clone()),
                        evidence: "owner_check_failed",
                        detail: format!("session check failed: {err}"),
                    }),
                }
            }
            // Resource evidence.
            match record.resource_kind {
                ResourceKind::Task => match tasks.get(&record.resource_id) {
                    Ok(None) => findings.push(ConflictFinding {
                        resource_kind: record.resource_kind,
                        resource_id: record.resource_id.clone(),
                        owner_agent: Some(record.owner_agent.clone()),
                        evidence: "task_missing",
                        detail: format!("task {} no longer exists", record.resource_id),
                    }),
                    Ok(Some(task)) => {
                        if task.status.is_terminal() {
                            findings.push(ConflictFinding {
                                resource_kind: record.resource_kind,
                                resource_id: record.resource_id.clone(),
                                owner_agent: Some(record.owner_agent.clone()),
                                evidence: "task_terminal",
                                detail: format!(
                                    "task {} is {} while still held",
                                    record.resource_id,
                                    task.status.as_str()
                                ),
                            });
                        }
                    }
                    Err(err) => findings.push(ConflictFinding {
                        resource_kind: record.resource_kind,
                        resource_id: record.resource_id.clone(),
                        owner_agent: Some(record.owner_agent.clone()),
                        evidence: "resource_check_failed",
                        detail: format!("task check failed: {err}"),
                    }),
                },
                ResourceKind::Worktree => {
                    let missing = match worktrees.list() {
                        Ok(list) => !list
                            .iter()
                            .any(|w| w.worktree_id == record.resource_id && w.removed_at.is_none()),
                        Err(_) => true,
                    };
                    if missing {
                        findings.push(ConflictFinding {
                            resource_kind: record.resource_kind,
                            resource_id: record.resource_id.clone(),
                            owner_agent: Some(record.owner_agent.clone()),
                            evidence: "worktree_missing",
                            detail: format!("worktree {} no longer exists", record.resource_id),
                        });
                    }
                }
            }
        }
        Ok(findings)
    }

    /// Bounded query over durable collaboration transition events
    /// (audit kind [`COLLAB_AUDIT_KIND`]). `action` optionally filters
    /// by exact action name (e.g. `assign`, `handoff_accept`).
    pub fn events(&self, limit: Option<usize>, action: Option<&str>) -> Result<Vec<AuditEntry>> {
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).min(MAX_LIST_LIMIT);
        let log = AuditLog::open(&self.root).context("failed to open the audit log")?;
        let mut events: Vec<AuditEntry> = log
            .recent(limit.max(MAX_LIST_LIMIT))
            .into_iter()
            .filter(|entry| entry.kind == COLLAB_AUDIT_KIND)
            .filter(|entry| action.is_none_or(|action| entry.action == action))
            .take(limit)
            .collect();
        events.reverse(); // oldest → newest reads better for history
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::collaboration::{CollabError, OwnershipState};

    fn workspace() -> (tempfile::TempDir, CollaborationService) {
        let dir = tempfile::tempdir().unwrap();
        // A minimal initialized workspace: manifest + agent store so
        // runtime identity checks resolve.
        crate::services::init::initialize_workspace(dir.path()).unwrap();
        let service = CollaborationService::new(dir.path());
        (dir, service)
    }

    fn seed_agent(service: &CollaborationService, id: &str) -> crate::models::agent::Agent {
        service
            .runtime()
            .register_profile(id, "Test Agent", "worker")
            .unwrap()
    }

    fn seed_task(dir: &std::path::Path, id: &str) -> crate::core::tasks::Task {
        crate::core::tasks::TaskStore::new(dir)
            .unwrap()
            .create(
                id.to_owned(),
                "title".to_owned(),
                "description".to_owned(),
                crate::core::tasks::TaskPriority::Normal,
                vec![],
            )
            .unwrap()
    }

    #[test]
    fn assign_validates_agent_and_resource() {
        let (dir, service) = workspace();
        let task = seed_task(dir.path(), "task-1");
        let agent = seed_agent(&service, "agent-a");

        let err = service
            .assign(
                ResourceKind::Task,
                &task.id,
                "agent-missing",
                None,
                None,
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("agent not found"), "{err}");

        let err = service
            .assign(
                ResourceKind::Task,
                "task-missing",
                &agent.id,
                None,
                None,
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("task not found"), "{err}");

        let record = service
            .assign(ResourceKind::Task, &task.id, &agent.id, None, None, None)
            .unwrap();
        assert_eq!(record.owner_agent, agent.id);
        assert_eq!(record.state, OwnershipState::Assigned);
    }

    #[test]
    fn disabled_agent_cannot_receive_ownership() {
        let (dir, service) = workspace();
        let task = seed_task(dir.path(), "task-1");
        let agent = seed_agent(&service, "agent-a");
        service
            .runtime()
            .set_profile_enabled(&agent.id, false)
            .unwrap();
        let err = service
            .assign(ResourceKind::Task, &task.id, &agent.id, None, None, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("disabled"), "{err}");
    }

    #[test]
    fn foreign_session_is_rejected() {
        let (dir, service) = workspace();
        let task = seed_task(dir.path(), "task-1");
        let agent_a = seed_agent(&service, "agent-a");
        let agent_b = seed_agent(&service, "agent-b");
        // A session opened by agent B cannot be bound to agent A.
        service.runtime().start_agent(&agent_b.id).unwrap();
        let session = service.runtime().open_session(&agent_b.id).unwrap();
        let err = service
            .assign(
                ResourceKind::Task,
                &task.id,
                &agent_a.id,
                Some(&session.session_id),
                None,
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("does not belong") || err.contains("cannot bind session"),
            "{err}"
        );
    }

    #[test]
    fn stopped_owner_surfaces_as_conflict_evidence() {
        let (dir, service) = workspace();
        let task = seed_task(dir.path(), "task-1");
        let agent = seed_agent(&service, "agent-a");
        service
            .assign(ResourceKind::Task, &task.id, &agent.id, None, None, None)
            .unwrap();
        service.runtime().stop_agent(&agent.id).unwrap();
        let findings = service.conflicts().unwrap();
        assert!(
            findings.iter().any(|f| f.evidence == "owner_agent_stopped"),
            "{findings:?}"
        );
    }

    #[test]
    fn terminal_task_surfaces_as_conflict_evidence() {
        let (dir, service) = workspace();
        let task = seed_task(dir.path(), "task-1");
        let agent = seed_agent(&service, "agent-a");
        service
            .assign(ResourceKind::Task, &task.id, &agent.id, None, None, None)
            .unwrap();
        let store = crate::core::tasks::TaskStore::new(dir.path()).unwrap();
        store
            .update(
                &task.id,
                Some(crate::core::tasks::TaskStatus::Done),
                None,
                None,
            )
            .unwrap();
        let findings = service.conflicts().unwrap();
        assert!(
            findings.iter().any(|f| f.evidence == "task_terminal"),
            "{findings:?}"
        );
    }

    #[test]
    fn full_handoff_flow_through_the_service() {
        let (dir, service) = workspace();
        let task = seed_task(dir.path(), "task-1");
        let agent_a = seed_agent(&service, "agent-a");
        let agent_b = seed_agent(&service, "agent-b");
        let r0 = service
            .assign(ResourceKind::Task, &task.id, &agent_a.id, None, None, None)
            .unwrap();
        let r1 = service
            .activate(ResourceKind::Task, &task.id, &agent_a.id, r0.revision)
            .unwrap();
        let r2 = service
            .request_handoff(
                ResourceKind::Task,
                &task.id,
                &agent_a.id,
                &agent_b.id,
                None,
                r1.revision,
            )
            .unwrap();
        assert_eq!(r2.state, OwnershipState::HandoffRequested);
        let r3 = service
            .accept_handoff(ResourceKind::Task, &task.id, &agent_b.id, r2.revision)
            .unwrap();
        assert_eq!(r3.owner_agent, agent_b.id);
        assert_eq!(r3.state, OwnershipState::HandedOff);
    }

    #[test]
    fn store_errors_surface_as_stable_categories() {
        let (dir, service) = workspace();
        let task = seed_task(dir.path(), "task-1");
        let agent_a = seed_agent(&service, "agent-a");
        let agent_b = seed_agent(&service, "agent-b");
        service
            .assign(ResourceKind::Task, &task.id, &agent_a.id, None, None, None)
            .unwrap();
        let err = service
            .assign(ResourceKind::Task, &task.id, &agent_b.id, None, None, None)
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            CollabError::AlreadyOwned.message(),
            "stable category text across planes"
        );
        let err = service
            .activate(ResourceKind::Task, &task.id, &agent_b.id, 1)
            .unwrap_err()
            .to_string();
        assert_eq!(err, CollabError::NotOwner.message());
    }

    #[test]
    fn events_query_returns_only_collab_events() {
        let (dir, service) = workspace();
        let task = seed_task(dir.path(), "task-1");
        let agent = seed_agent(&service, "agent-a");
        service
            .assign(ResourceKind::Task, &task.id, &agent.id, None, None, None)
            .unwrap();
        // The durable-file reader (`events`) depends on the process-wide
        // `init_global` root, which is OnceLock-owned by whichever test
        // runs first — the durable round-trip is pinned by the real
        // e2e CLI test instead. Here the audit write itself is pinned
        // through the always-visible global ring.
        let collab: Vec<AuditEntry> = audit::global()
            .recent(100)
            .into_iter()
            .filter(|e| e.kind == COLLAB_AUDIT_KIND)
            .collect();
        assert!(
            collab.iter().any(|e| e.action == "assign"
                && e.subject.contains(&task.id)
                && e.detail.contains(&agent.id)),
            "assign must be audited with resource + owner"
        );
        let handoff_events: Vec<AuditEntry> = collab
            .iter()
            .filter(|e| e.action == "handoff_accept")
            .cloned()
            .collect();
        assert!(handoff_events.is_empty(), "no handoff happened here");
    }
}
