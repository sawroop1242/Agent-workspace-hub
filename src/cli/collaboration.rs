//! `awh collaboration` (COL-001): thin CLI adapter over the canonical
//! [`CollaborationService`] — the same authority every interface plane
//! must call. The CLI owns argument parsing, bounded output, and exit
//! codes only: owner/resource validation, workspace binding, state-
//! machine enforcement, locking, atomic publication, and audit all live
//! in the service and store. Ownership-changing verbs always take
//! explicit identifiers and echo the observed revision on success.

use crate::core::collaboration::{CollabRecord, ResourceKind, MAX_LIST_LIMIT};
use crate::services::collaboration::{CollaborationService, ConflictFinding};
use anyhow::{bail, Context as _, Result};
use clap::Subcommand;

/// Default list/event bound.
const DEFAULT_LIMIT: usize = 50;

#[derive(Debug, Subcommand)]
pub enum CollaborationCommand {
    /// List agent profiles eligible to hold ownership.
    Agents,
    /// Show current ownership records (bounded, newest change first).
    Status {
        /// Filter by resource kind: task, worktree.
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,
    },
    /// Assign a task or worktree to an agent.
    Assign {
        /// Resource kind: task, worktree.
        #[arg(long)]
        kind: String,
        /// Canonical resource id.
        #[arg(long)]
        id: String,
        /// Agent that will own the resource.
        #[arg(long)]
        agent: String,
        /// Runtime session binding the assignment to a live session.
        #[arg(long)]
        session: Option<String>,
        /// Bounded reason/summary note (ids and reasons, never secrets).
        #[arg(long)]
        note: Option<String>,
        /// Revision of the released record being reactivated (omit for
        /// a fresh assignment).
        #[arg(long)]
        expected_revision: Option<u64>,
    },
    /// Mark a held resource as actively worked on (owner only).
    Activate {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        id: String,
        /// Current owner performing the activation.
        #[arg(long)]
        agent: String,
    },
    /// Transfer ownership to another agent. Default: one-shot
    /// operator-confirmed transfer. `--request` performs only the
    /// request half (the target then accepts via `accept`).
    Handoff {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        id: String,
        /// Current owner.
        #[arg(long)]
        from: String,
        /// Receiving agent.
        #[arg(long)]
        to: String,
        #[arg(long)]
        note: Option<String>,
        /// Only request the handoff; leave it pending for `accept`.
        #[arg(long)]
        request: bool,
    },
    /// Accept a pending handoff as the receiving agent.
    Accept {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        id: String,
        /// Receiving agent performing the acceptance.
        #[arg(long)]
        agent: String,
    },
    /// Release ownership so the resource can be reassigned (owner only).
    Release {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        id: String,
        /// Current owner performing the release.
        #[arg(long)]
        agent: String,
    },
    /// Report ownership problems as observable evidence. Findings are
    /// never resolved automatically.
    Conflicts,
    /// Show durable collaboration transition events (bounded).
    Events {
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,
        /// Exact action filter: assign, activate, handoff_request,
        /// handoff_accept, release.
        #[arg(long)]
        action: Option<String>,
    },
}

fn parse_kind(value: &str) -> Result<ResourceKind> {
    match value {
        "task" => Ok(ResourceKind::Task),
        "worktree" => Ok(ResourceKind::Worktree),
        _ => bail!("unknown resource kind {value:?} (expected `task` or `worktree`)"),
    }
}

/// The record the caller must have observed for a revision-checked
/// mutation; the CLI loads it fresh from the same canonical store.
fn require_record(
    service: &CollaborationService,
    kind: ResourceKind,
    id: &str,
) -> Result<CollabRecord> {
    match service.get(kind, id)? {
        Some(record) => Ok(record),
        None => bail!("no active ownership record for this resource"),
    }
}

fn print_record(record: &CollabRecord) {
    println!(
        "{}:{} owner={} state={} revision={}",
        record.resource_kind.as_str(),
        record.resource_id,
        record.owner_agent,
        record.state.as_str(),
        record.revision
    );
    if let Some(session) = &record.owner_session {
        println!("  session={session}");
    }
    if let Some(pending) = &record.pending_to_agent {
        println!("  pending_to={pending}");
    }
    if let Some(note) = &record.note {
        println!("  note={note}");
    }
}

fn print_finding(finding: &ConflictFinding) {
    println!(
        "{}:{} evidence={} owner={}",
        finding.resource_kind.as_str(),
        finding.resource_id,
        finding.evidence,
        finding.owner_agent.as_deref().unwrap_or("-"),
    );
    println!("  {}", finding.detail);
}

pub fn handle_collaboration_cli(
    root: &std::path::Path,
    command: CollaborationCommand,
) -> Result<()> {
    let service = CollaborationService::new(root);
    match command {
        CollaborationCommand::Agents => {
            let agents = service.agents().context("failed to list agent profiles")?;
            if agents.is_empty() {
                println!("no agents registered");
                return Ok(());
            }
            for agent in agents {
                println!(
                    "{} name={} role={} enabled={}",
                    agent.id, agent.name, agent.role, agent.enabled
                );
            }
        }
        CollaborationCommand::Status { kind, limit } => {
            let kind = kind.as_deref().map(parse_kind).transpose()?;
            let records = service.status(kind, Some(limit.min(MAX_LIST_LIMIT)))?;
            if records.is_empty() {
                println!("no ownership records");
                return Ok(());
            }
            for record in &records {
                print_record(record);
            }
        }
        CollaborationCommand::Assign {
            kind,
            id,
            agent,
            session,
            note,
            expected_revision,
        } => {
            let kind = parse_kind(&kind)?;
            let record = service.assign(
                kind,
                &id,
                &agent,
                session.as_deref(),
                note.as_deref(),
                expected_revision,
            )?;
            print_record(&record);
        }
        CollaborationCommand::Activate { kind, id, agent } => {
            let kind = parse_kind(&kind)?;
            let current = require_record(&service, kind, &id)?;
            let record = service.activate(kind, &id, &agent, current.revision)?;
            print_record(&record);
        }
        CollaborationCommand::Handoff {
            kind,
            id,
            from,
            to,
            note,
            request,
        } => {
            let kind = parse_kind(&kind)?;
            let current = require_record(&service, kind, &id)?;
            let record = service.request_handoff(
                kind,
                &id,
                &from,
                &to,
                note.as_deref(),
                current.revision,
            )?;
            print_record(&record);
            if !request {
                // One-shot operator transfer: accept the pending request
                // in the same invocation (the operator is both sides).
                let record = service.accept_handoff(kind, &id, &to, record.revision)?;
                print_record(&record);
            }
        }
        CollaborationCommand::Accept { kind, id, agent } => {
            let kind = parse_kind(&kind)?;
            let current = require_record(&service, kind, &id)?;
            let record = service.accept_handoff(kind, &id, &agent, current.revision)?;
            print_record(&record);
        }
        CollaborationCommand::Release { kind, id, agent } => {
            let kind = parse_kind(&kind)?;
            let current = require_record(&service, kind, &id)?;
            let record = service.release(kind, &id, &agent, current.revision)?;
            print_record(&record);
        }
        CollaborationCommand::Conflicts => {
            let findings = service.conflicts()?;
            if findings.is_empty() {
                println!("no conflicts detected");
                return Ok(());
            }
            for finding in &findings {
                print_finding(finding);
            }
        }
        CollaborationCommand::Events { limit, action } => {
            let events = service.events(Some(limit.min(MAX_LIST_LIMIT)), action.as_deref())?;
            if events.is_empty() {
                println!("no collaboration events");
                return Ok(());
            }
            for event in &events {
                println!(
                    "{} {} subject={} detail={}",
                    event.ts_ms, event.action, event.subject, event.detail
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_parsing_is_strict() {
        assert!(matches!(parse_kind("task").unwrap(), ResourceKind::Task));
        assert!(matches!(
            parse_kind("worktree").unwrap(),
            ResourceKind::Worktree
        ));
        assert!(parse_kind("bogus").is_err());
        let err = parse_kind("Task").unwrap_err().to_string();
        assert!(err.contains("unknown resource kind"), "{err}");
    }
}
