//! `awh context` (CTX-001): thin CLI adapter over the canonical
//! [`crate::context::ContextEngine`]. The CLI owns argument parsing and
//! output formatting only — every operation constructs the engine exactly
//! the way the MCP dispatcher does (same root, same env overrides) and
//! delegates, so CLI and MCP semantics are identical.

use anyhow::{bail, Context as _, Result};
use clap::Subcommand;

use crate::context::{ContextEngine, ContextEngineConfig, ContextItem};
use crate::mcp::dispatcher::{parse_context_scope, parse_context_source};
use crate::services::audit;

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Show context engine status (active items, tokens, budget,
    /// offloads), or one item with --id.
    Show {
        /// Show a single context item instead of engine status.
        #[arg(long)]
        id: Option<String>,
    },
    /// Insert (or replace) a context item in the active window.
    Save {
        /// Item id (filename-safe: [A-Za-z0-9._-], max 256 bytes).
        #[arg(long)]
        id: String,
        /// Item content; read from stdin when omitted.
        #[arg(long)]
        content: Option<String>,
        /// Item source: System, User, Assistant, Tool, Skill, File,
        /// Memory, Workspace, Search, Summary, Other.
        #[arg(long, default_value = "Other")]
        source: String,
        /// Item scope: Session, Project, Global.
        #[arg(long, default_value = "Project")]
        scope: String,
        /// Relevance in [0, 1].
        #[arg(long)]
        relevance: Option<f64>,
        /// Priority in [0, 1].
        #[arg(long)]
        priority: Option<f64>,
    },
    /// Update an existing context item; fails if the id is unknown.
    Update {
        #[arg(long)]
        id: String,
        /// New content; read from stdin when omitted.
        #[arg(long)]
        content: Option<String>,
        #[arg(long, default_value = "Other")]
        source: String,
        #[arg(long, default_value = "Project")]
        scope: String,
        #[arg(long)]
        relevance: Option<f64>,
        #[arg(long)]
        priority: Option<f64>,
    },
    /// Remove one context item (--id) or every ACTIVE item (--all).
    /// Offloaded items are preserved: they are recoverable state.
    Clear {
        #[arg(long, required_unless_present = "all", conflicts_with = "all")]
        id: Option<String>,
        /// Clear every active item (offloads are kept).
        #[arg(long)]
        all: bool,
    },
    /// Search active and offloaded context items for a substring.
    Search {
        #[arg(long)]
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
}

/// Builds the engine the same way the MCP dispatcher does so both
/// interfaces observe the same config and persistence root.
fn engine(root: &std::path::Path) -> Result<ContextEngine> {
    let config = ContextEngineConfig::default().with_env_overrides()?;
    ContextEngine::new(root, config).context("failed to initialize the context engine")
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

fn build_item(
    id: &str,
    content: String,
    source: &str,
    scope: &str,
    relevance: Option<f64>,
    priority: Option<f64>,
) -> Result<ContextItem> {
    let mut item = ContextItem::new(id, parse_context_source(Some(source)), content, 0);
    if let Some(v) = relevance {
        item.relevance = v.clamp(0.0, 1.0) as f32;
    }
    if let Some(v) = priority {
        item.priority = v.clamp(0.0, 1.0) as f32;
    }
    item.scope = parse_context_scope(Some(scope))?;
    Ok(item)
}

fn save_like(root: &std::path::Path, update: bool, args: SaveArgs) -> Result<()> {
    let engine = engine(root)?;
    if update && engine.get_item(&args.id).is_none() {
        bail!(
            "context item {:?} not found — use 'awh context save' to insert a new item",
            args.id
        );
    }
    let content = read_content(args.content)?;
    let item = build_item(
        &args.id,
        content,
        &args.source,
        &args.scope,
        args.relevance,
        args.priority,
    )?;
    let saved = engine.insert(item)?;
    // CTX-001 §5: audit identifiers/outcomes only — content is treated
    // as potentially sensitive and never reaches the audit log.
    audit::global().record(
        "allow",
        if update {
            "cli_context_update"
        } else {
            "cli_context_save"
        },
        &saved.id,
        &format!("scope {:?} ({} tokens)", saved.scope, saved.token_count),
    );
    println!(
        "{} context item {} ({} tokens, scope {:?})",
        if update { "updated" } else { "saved" },
        saved.id,
        saved.token_count,
        saved.scope
    );
    Ok(())
}

/// The shared shape of `save`/`update` arguments.
struct SaveArgs {
    id: String,
    content: Option<String>,
    source: String,
    scope: String,
    relevance: Option<f64>,
    priority: Option<f64>,
}

pub fn handle_context_cli(root: &std::path::Path, command: ContextCommand) -> Result<()> {
    match command {
        ContextCommand::Show { id } => {
            let engine = engine(root)?;
            match id {
                None => {
                    let status = engine.status()?;
                    println!("{}", serde_json::to_string_pretty(&status)?);
                }
                Some(id) => match engine.get_item(&id) {
                    Some(item) => println!("{}", serde_json::to_string_pretty(&item)?),
                    None => bail!("context item {id:?} not found"),
                },
            }
        }
        ContextCommand::Save {
            id,
            content,
            source,
            scope,
            relevance,
            priority,
        } => {
            save_like(
                root,
                false,
                SaveArgs {
                    id,
                    content,
                    source,
                    scope,
                    relevance,
                    priority,
                },
            )?;
        }
        ContextCommand::Update {
            id,
            content,
            source,
            scope,
            relevance,
            priority,
        } => {
            save_like(
                root,
                true,
                SaveArgs {
                    id,
                    content,
                    source,
                    scope,
                    relevance,
                    priority,
                },
            )?;
        }
        ContextCommand::Clear { id, all } => {
            let engine = engine(root)?;
            if all {
                let ids: Vec<String> = engine
                    .list_items()
                    .into_iter()
                    .filter(|i| i.state.is_active())
                    .map(|i| i.id)
                    .collect();
                let count = ids.len();
                for id in ids {
                    engine.remove_item(&id);
                }
                audit::global().record(
                    "allow",
                    "cli_context_clear_all",
                    "workspace",
                    &format!("{count} items"),
                );
                println!("cleared {count} active context item(s); offloaded items are preserved");
            } else {
                let id = id.unwrap_or_default();
                if !engine.remove_item(&id) {
                    bail!("context item {id:?} not found");
                }
                audit::global().record("allow", "cli_context_clear", &id, "cleared");
                println!("cleared context item {id}");
            }
        }
        ContextCommand::Search { query, limit } => {
            let engine = engine(root)?;
            let hits = engine.search(&query, limit)?;
            println!("{}", serde_json::to_string_pretty(&hits)?);
        }
    }
    Ok(())
}
