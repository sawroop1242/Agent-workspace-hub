use crate::mcp::workspace::WorkspaceMcp;
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct WorkspaceContext {
    pub root: String,
    pub instructions: String,
}

pub fn load_context(workspace: &WorkspaceMcp) -> Result<WorkspaceContext> {
    Ok(WorkspaceContext {
        root: std::env::current_dir()?.display().to_string(),
        instructions: workspace.context()?,
    })
}
