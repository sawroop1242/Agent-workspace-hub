use crate::mcp::workspace::WorkspaceMcp;
use anyhow::Result;
use serde::Serialize;

/// Assembled workspace context: the current directory plus any discovered
/// project instructions.
#[derive(Debug, Serialize)]
pub struct WorkspaceContext {
    pub root: String,
    pub instructions: String,
}

/// Loads workspace context (root directory plus `AGENTS.md`/`README.md` instructions).
pub fn load_context(workspace: &WorkspaceMcp) -> Result<WorkspaceContext> {
    Ok(WorkspaceContext {
        root: std::env::current_dir()?.display().to_string(),
        instructions: workspace.context()?,
    })
}
