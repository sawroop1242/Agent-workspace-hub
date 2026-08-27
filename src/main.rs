use agent_workspace_hub::mcp::{
    CommunityMcpRegistryClient, CustomMcpRegistry, CustomMcpServerConfig, GlobalMcpRegistry,
    McpPermissions, McpTransport, PersistentTrustStore, ProjectMcpReferences, StdioMcpServer,
    TrustLevel,
};
use agent_workspace_hub::skills::{
    GlobalSkillRegistry, ProjectSkillReferences, RegistryClient, RegistryStore, SkillInstaller,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

const DEFAULT_MCP_REGISTRY: &str = "https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/rust/registry/mcps/index.json";

#[derive(Debug, Parser)]
#[command(name = "awh", version, about = "Agent Workspace Hub")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Status,
    #[command(name = "mcp")]
    Mcp {
