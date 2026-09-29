//! GIT-001 §19: the `awh worktree` CLI surface — a thin adapter over the
//! canonical [`WorktreeStore`] lifecycle. The CLI owns argument parsing
//! and exit codes only; identity binding, path containment, Git
//! invocation (structured argv, never a shell), and audit all stay in
//! the service.
//!
//! Exit codes follow the same stable categories as the `fs` family:
//!
//! ```text
//! 0  success
//! 1  general service/git failure
//! 2  usage/parse failure or uninitialized/non-Git workspace
//! 4  ownership/conflict failure (possession of an id is never authority)
//! 5  recovery-class failure (missing, dirty-preserving, corrupt metadata)
//! ```

use crate::services::init::load_workspace_manifest;
use crate::services::worktree::{WorktreeError, WorktreeStore};
use clap::Subcommand;
use serde_json::json;
use std::path::Path;

/// The `awh worktree` subcommand tree.
#[derive(Debug, Subcommand)]
pub enum WorktreeCommand {
    /// Create an isolated Git worktree bound to one agent session.
    Create {
        /// Agent that owns the session (validated shape).
        agent_id: String,
        /// The runtime session the worktree is bound to.
        session_id: String,
        /// Branch to create (defaults to awh/<agent>/<session>).
        #[arg(long)]
        branch: Option<String>,
        /// Start point for the new branch (defaults to HEAD).
        #[arg(long)]
        from: Option<String>,
        /// Emit a bounded machine-readable JSON result.
        #[arg(long)]
        json: bool,
    },
    /// List managed worktrees, newest first.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Inspect one managed worktree and reconcile it against Git.
    Inspect {
        worktree_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Remove one managed worktree (refuses to discard uncommitted
    /// changes; verifies session ownership).
    Remove {
        worktree_id: String,
        /// The session that owns the worktree (ownership check).
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        json: bool,
    },
}

enum WorktreeCliError {
    Workspace(String),
    Domain(WorktreeError),
}

impl WorktreeCliError {
    fn exit_code(&self) -> i32 {
        match self {
            WorktreeCliError::Workspace(_) => 2,
            WorktreeCliError::Domain(error) => match error {
                // Input/usage-class failures.
                WorktreeError::Invalid(_) | WorktreeError::NotARepository => 2,
                // Possession of an id is never authority; collision /
                // ownership refusals are conflict-class.
                WorktreeError::Ownership(_)
                | WorktreeError::BranchCheckedOut(_)
                | WorktreeError::AlreadyExists(_) => 4,
                // Recovery-class: the target is gone, user work is
                // preserved, or metadata failed closed.
                WorktreeError::NotFound(_)
                | WorktreeError::Dirty(_)
                | WorktreeError::Corrupt(_) => 5,
                WorktreeError::Store(_) => 1,
            },
        }
    }

    fn print(&self) {
        match self {
            WorktreeCliError::Workspace(message) => {
                eprintln!("awh worktree: workspace error: {message}")
            }
            WorktreeCliError::Domain(error) => eprintln!("awh worktree: {error}"),
        }
    }
}

/// Runs one `awh worktree` subcommand against `root`, returning the
/// process exit code (0 on success).
pub fn run(command: WorktreeCommand, root: &Path) -> i32 {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let result: Result<i32, WorktreeCliError> = runtime.block_on(run_inner(command, root));
    match result {
        Ok(code) => code,
        Err(error) => {
            error.print();
            error.exit_code()
        }
    }
}

async fn run_inner(command: WorktreeCommand, root: &Path) -> Result<i32, WorktreeCliError> {
    // The workspace identity is the canonical manifest — never cwd or
    // an arbitrary user-supplied value.
    let manifest = load_workspace_manifest(root)
        .map_err(|error| WorktreeCliError::Workspace(format!("{error:#}")))?;
    let store = WorktreeStore::new(root);

    match command {
        WorktreeCommand::Create {
            agent_id,
            session_id,
            branch,
            from,
            json,
        } => {
            let record = store
                .create(
                    manifest.workspace_id.as_str(),
                    &agent_id,
                    &session_id,
                    branch.as_deref(),
                    from.as_deref(),
                )
                .await
                .map_err(WorktreeCliError::Domain)?;
            if json {
                println!(
                    "{}",
                    json!({
                        "command": "worktree create",
                        "status": "created",
                        "worktree_id": record.worktree_id,
                        "branch": record.branch,
                        "path": record.path,
                    })
                );
            } else {
                println!(
                    "worktree {} created (branch {}, path {})",
                    record.worktree_id, record.branch, record.path
                );
            }
            Ok(0)
        }
        WorktreeCommand::List { json } => {
            let records = store.list().map_err(WorktreeCliError::Domain)?;
            if json {
                let items: Vec<_> = records
                    .iter()
                    .map(|record| {
                        json!({
                            "worktree_id": record.worktree_id,
                            "agent_id": record.agent_id,
                            "session_id": record.session_id,
                            "branch": record.branch,
                            "state": record.state.as_str(),
                            "path": record.path,
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    json!({ "command": "worktree list", "worktrees": items })
                );
            } else {
                for record in records {
                    println!(
                        "{}  {}  {}  {}",
                        record.created_at,
                        record.state.as_str(),
                        record.worktree_id,
                        record.branch
                    );
                }
            }
            Ok(0)
        }
        WorktreeCommand::Inspect { worktree_id, json } => {
            let record = store
                .inspect(&worktree_id)
                .await
                .map_err(WorktreeCliError::Domain)?;
            if json {
                println!(
                    "{}",
                    json!({
                        "command": "worktree inspect",
                        "worktree_id": record.worktree_id,
                        "state": record.state.as_str(),
                        "branch": record.branch,
                        "path": record.path,
                    })
                );
            } else {
                println!(
                    "worktree {}: {} (branch {})",
                    record.worktree_id,
                    record.state.as_str(),
                    record.branch
                );
            }
            Ok(0)
        }
        WorktreeCommand::Remove {
            worktree_id,
            session_id,
            json,
        } => {
            let record = store
                .remove(&worktree_id, &session_id)
                .await
                .map_err(WorktreeCliError::Domain)?;
            if json {
                println!(
                    "{}",
                    json!({
                        "command": "worktree remove",
                        "status": "removed",
                        "worktree_id": record.worktree_id,
                    })
                );
            } else {
                println!("worktree {} removed", record.worktree_id);
            }
            Ok(0)
        }
    }
}
