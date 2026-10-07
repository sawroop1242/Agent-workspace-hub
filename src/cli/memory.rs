//! `awh memory` (MEM-001): thin CLI adapter over the canonical
//! [`crate::core::memory::MemoryStore`] — the same authority MCP, the
//! Control API, the TUI, and the Context Engine use. The CLI owns
//! argument parsing, bounded output, and exit codes only; every mutation
//! flows through the store's validated, lock-guarded, atomically
//! published path and emits a correlated audit event.

use crate::core::memory::{MemoryScope, MemoryStore};
use crate::mcp::dispatcher::parse_scope;
use crate::services::audit;
use anyhow::{bail, Context as _, Result};
use clap::Subcommand;

/// Default and maximum bounds for list/search output. The store bounds
/// what is stored; these bound what the CLI prints.
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;

#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    /// List memory entries, optionally filtered by scope.
    List {
        /// Scope filter: Session, Project, Global.
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,
    },
    /// Show one memory entry by id.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Search memory by content or tag substring.
    Search {
        #[arg(long)]
        query: String,
        /// Scope filter: Session, Project, Global.
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Append a memory entry at an explicit scope.
    Add {
        /// Entry content; read from stdin when omitted.
        #[arg(long)]
        content: Option<String>,
        /// Scope: Session, Project, Global (default Project).
        #[arg(long, default_value = "Project")]
        scope: String,
        /// Categorization tag (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Update an existing memory entry; only provided fields change.
    Update {
        #[arg(long)]
        id: String,
        /// New content; omit for a scope/tags-only partial update.
        #[arg(
            long,
            required_unless_present_any = ["scope", "tags"]
        )]
        content: Option<String>,
        /// New scope: Session, Project, Global.
        #[arg(long)]
        scope: Option<String>,
        /// Replacement tags (repeatable; replaces wholesale when present).
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Delete a memory entry by id.
    Delete {
        #[arg(long)]
        id: String,
    },
}

fn store(root: &std::path::Path) -> Result<MemoryStore> {
    MemoryStore::for_project(root).context("failed to open the memory store")
}

fn read_content(explicit: Option<String>) -> Result<String> {
    match explicit {
        Some(content) => Ok(content),
        None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read content from stdin")?;
            Ok(buf)
        }
    }
}

fn bound_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_LIMIT)
}

fn print_entries(entries: Vec<crate::core::memory::MemoryEntry>, limit: usize) {
    let shown = entries.len().min(bound_limit(limit));
    for entry in entries.iter().take(shown) {
        println!(
            "{}\t{}\t{}\t{}",
            entry.id,
            scope_name(&entry.scope),
            entry.tags.join(","),
            entry.content.replace(['\n', '\r'], " ")
        );
    }
    if entries.len() > shown {
        eprintln!("(showing {shown} of {} — raise --limit)", entries.len());
    }
}

fn scope_name(scope: &MemoryScope) -> &'static str {
    match scope {
        MemoryScope::Session => "Session",
        MemoryScope::Project => "Project",
        MemoryScope::Global => "Global",
    }
}

/// Mutations audit into the canonical ring; ids and scope names only —
/// never raw memory content (it may hold sensitive user data).
fn audit_mutation(action: &str, id: &str, detail: &str) {
    audit::global().record("allow", action, id, detail);
}

pub fn handle_memory_cli(root: &std::path::Path, command: MemoryCommand) -> Result<()> {
    match command {
        MemoryCommand::List { scope, limit } => {
            let store = store(root)?;
            let entries = match scope {
                Some(scope) => {
                    let scope = parse_scope(Some(&scope))?;
                    store
                        .list_all()?
                        .into_iter()
                        .filter(|e| e.scope == scope)
                        .collect()
                }
                None => store.list_all()?,
            };
            print_entries(entries, limit);
        }
        MemoryCommand::Get { id } => {
            let store = store(root)?;
            match store.get(&id)? {
                Some(entry) => println!("{}", serde_json::to_string_pretty(&entry)?),
                None => bail!("memory entry not found: {id}"),
            }
        }
        MemoryCommand::Search {
            query,
            scope,
            limit,
        } => {
            let store = store(root)?;
            let scope = scope.as_deref().map(|s| parse_scope(Some(s))).transpose()?;
            let hits = store.search(&query, scope)?;
            print_entries(hits, limit);
        }
        MemoryCommand::Add {
            content,
            scope,
            tags,
        } => {
            let store = store(root)?;
            let content = read_content(content)?;
            let scope = parse_scope(Some(&scope))?;
            let entry = store.append_scoped(&content, scope.clone(), tags)?;
            audit_mutation(
                "cli_memory_add",
                &entry.id,
                &format!(
                    "scope {} ({} bytes)",
                    scope_name(&entry.scope),
                    content.len()
                ),
            );
            println!(
                "added memory entry {} (scope {})",
                entry.id,
                scope_name(&entry.scope)
            );
        }
        MemoryCommand::Update {
            id,
            content,
            scope,
            tags,
        } => {
            let store = store(root)?;
            let content = match content {
                Some(content) => Some(read_content(Some(content))?),
                None => None,
            };
            let scope = scope.as_deref().map(|s| parse_scope(Some(s))).transpose()?;
            let updated =
                store.update_partial(&id, content, scope, (!tags.is_empty()).then_some(tags))?;
            audit_mutation(
                "cli_memory_update",
                &id,
                &format!("scope {}", scope_name(&updated.scope)),
            );
            println!(
                "updated memory entry {} (scope {})",
                updated.id,
                scope_name(&updated.scope)
            );
        }
        MemoryCommand::Delete { id } => {
            let store = store(root)?;
            if !store.delete(&id)? {
                bail!("memory entry not found: {id}");
            }
            audit_mutation("cli_memory_delete", &id, "deleted");
            println!("deleted memory entry {id}");
        }
    }
    Ok(())
}
