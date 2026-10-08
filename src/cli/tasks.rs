//! `awh task` (TSK-001): thin CLI adapter over the canonical
//! [`crate::core::tasks::TaskStore`] — the same authority the MCP task
//! tools use. The CLI owns argument parsing, bounded output, and exit
//! codes only: state-machine validation, assignee existence checks,
//! locking, and atomic publication all live in the store, and every
//! mutation emits a correlated audit event (ids, status, and assignee
//! only — never task text, which may be sensitive).

use crate::core::identity::TaskId;
use crate::core::tasks::{TaskPriority, TaskStatus, TaskStore};
use crate::services::audit;
use anyhow::{bail, Context as _, Result};
use clap::Subcommand;

/// Default and maximum bounds for list output.
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// List tasks, optionally filtered by status.
    List {
        /// Status filter: Todo, InProgress, Blocked, Done, Cancelled.
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,
    },
    /// Show one task by id.
    Show {
        #[arg(long)]
        id: String,
    },
    /// Create a task.
    Create {
        /// Explicit task id; a canonical `task-…` id is minted when omitted.
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        title: String,
        /// Longer description; read from stdin when omitted.
        #[arg(long)]
        description: Option<String>,
        /// Priority: Low, Normal, High, Critical (default Normal).
        #[arg(long, default_value = "Normal")]
        priority: String,
        /// Categorization tag (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Update a task's status, priority, and/or assignee.
    Update {
        #[arg(long)]
        id: String,
        /// New status: Todo, InProgress, Blocked, Done, Cancelled.
        #[arg(long)]
        status: Option<String>,
        /// New priority: Low, Normal, High, Critical.
        #[arg(long)]
        priority: Option<String>,
        /// Assign to an existing agent profile, `sess-…`, or `wt-…` id.
        #[arg(
            long,
            conflicts_with = "unassign",
            required_unless_present_any = ["status", "priority", "unassign"]
        )]
        assignee: Option<String>,
        /// Clear the assignee.
        #[arg(long)]
        unassign: bool,
    },
    /// Cancel a task (terminal; a cancelled task is never reopened).
    Cancel {
        #[arg(long)]
        id: String,
    },
    /// Assign a task to an existing agent profile, `sess-…`, or `wt-…` id.
    Assign {
        #[arg(long)]
        id: String,
        #[arg(long)]
        to: String,
    },
    /// Delete a task by id.
    Delete {
        #[arg(long)]
        id: String,
    },
}

fn store(root: &std::path::Path) -> Result<TaskStore> {
    TaskStore::new(root).context("failed to open the task store")
}

fn parse_status_cli(value: &str) -> Result<TaskStatus> {
    // Same names as the MCP wire contract.
    match value {
        "Todo" => Ok(TaskStatus::Todo),
        "InProgress" => Ok(TaskStatus::InProgress),
        "Blocked" => Ok(TaskStatus::Blocked),
        "Done" => Ok(TaskStatus::Done),
        "Cancelled" => Ok(TaskStatus::Cancelled),
        other => bail!(
            "invalid status '{other}': expected Todo, InProgress, Blocked, Done, or Cancelled"
        ),
    }
}

fn parse_priority_cli(value: &str) -> Result<TaskPriority> {
    match value {
        "Low" => Ok(TaskPriority::Low),
        "Normal" => Ok(TaskPriority::Normal),
        "High" => Ok(TaskPriority::High),
        "Critical" => Ok(TaskPriority::Critical),
        other => bail!("invalid priority '{other}': expected Low, Normal, High, or Critical"),
    }
}

fn print_tasks(tasks: Vec<crate::core::tasks::Task>, limit: usize) {
    let shown = tasks.len().min(limit.clamp(1, MAX_LIMIT));
    for task in tasks.iter().take(shown) {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            task.id,
            task.status.as_str(),
            priority_name(&task.priority),
            task.assignee.as_deref().unwrap_or("-"),
            task.title.replace(['\n', '\r'], " ")
        );
    }
    if tasks.len() > shown {
        eprintln!("(showing {shown} of {} — raise --limit)", tasks.len());
    }
}

fn priority_name(priority: &TaskPriority) -> &'static str {
    match priority {
        TaskPriority::Low => "Low",
        TaskPriority::Normal => "Normal",
        TaskPriority::High => "High",
        TaskPriority::Critical => "Critical",
    }
}

fn audit_mutation(action: &str, id: &str, detail: &str) {
    audit::global().record("allow", action, id, detail);
}

pub fn handle_task_cli(root: &std::path::Path, command: TaskCommand) -> Result<()> {
    match command {
        TaskCommand::List { status, limit } => {
            let store = store(root)?;
            let status = status.as_deref().map(parse_status_cli).transpose()?;
            print_tasks(store.list(status)?, limit);
        }
        TaskCommand::Show { id } => {
            let store = store(root)?;
            match store.get(&id)? {
                Some(task) => println!("{}", serde_json::to_string_pretty(&task)?),
                None => bail!("task not found: {id}"),
            }
        }
        TaskCommand::Create {
            id,
            title,
            description,
            priority,
            tags,
        } => {
            let store = store(root)?;
            let description = match description {
                Some(description) => description,
                None => {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin()
                        .read_to_string(&mut buf)
                        .context("failed to read description from stdin")?;
                    buf
                }
            };
            let priority = parse_priority_cli(&priority)?;
            let id = id.unwrap_or_else(|| TaskId::new().as_str().to_owned());
            let task = store.create(id.clone(), title, description, priority, tags)?;
            audit_mutation(
                "cli_task_create",
                &task.id,
                &format!("status {}", task.status.as_str()),
            );
            println!("created task {} (status {})", task.id, task.status.as_str());
        }
        TaskCommand::Update {
            id,
            status,
            priority,
            assignee,
            unassign,
        } => {
            let store = store(root)?;
            let status = status.as_deref().map(parse_status_cli).transpose()?;
            let priority = priority.as_deref().map(parse_priority_cli).transpose()?;
            let assignee = if unassign {
                Some(None)
            } else {
                assignee.map(Some)
            };
            let updated = store
                .update(&id, status.clone(), priority, assignee.clone())?
                .with_context(|| format!("task not found: {id}"))?;
            audit_mutation(
                "cli_task_update",
                &id,
                &format!(
                    "status {}{}",
                    updated.status.as_str(),
                    updated
                        .assignee
                        .as_deref()
                        .map(|a| format!(", assignee {a}"))
                        .unwrap_or_default()
                ),
            );
            println!(
                "updated task {} (status {})",
                updated.id,
                updated.status.as_str()
            );
        }
        TaskCommand::Cancel { id } => {
            let store = store(root)?;
            let task = store.cancel(&id)?;
            audit_mutation("cli_task_cancel", &id, "status Cancelled");
            println!("cancelled task {} (terminal)", task.id);
        }
        TaskCommand::Assign { id, to } => {
            let store = store(root)?;
            let updated = store
                .update(&id, None, None, Some(Some(to.clone())))?
                .with_context(|| format!("task not found: {id}"))?;
            audit_mutation(
                "cli_task_assign",
                &id,
                &format!("assignee {}", updated.assignee.as_deref().unwrap_or("-")),
            );
            println!(
                "assigned task {} to {}",
                updated.id,
                updated.assignee.as_deref().unwrap_or("-")
            );
        }
        TaskCommand::Delete { id } => {
            let store = store(root)?;
            if !store.delete(&id)? {
                bail!("task not found: {id}");
            }
            audit_mutation("cli_task_delete", &id, "deleted");
            println!("deleted task {id}");
        }
    }
    Ok(())
}
