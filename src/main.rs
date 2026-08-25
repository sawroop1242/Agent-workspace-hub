mod core;
mod mcp;
mod models;
mod skills;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mcp::StdioMcpServer;
use skills::{GlobalSkillRegistry, ProjectSkillReferences, RegistryClient, RegistryStore, SkillInstaller};

#[derive(Debug, Parser)]
#[command(name = "awh", version, about = "Agent Workspace Hub")]
struct Cli { #[command(subcommand)] command: Option<Command> }

#[derive(Debug, Subcommand)]
enum Command {
    Status,
    Mpc { #[command(subcommand)] command: McpCommand },
    Skill { #[command(subcommand)] command: SkillCommand },
    Registry { #[command(subcommand)] command: RegistryCommand },
}

#[derive(Debug, Subcommand)]
enum McpCommand { Serve }

#[derive(Debug, Subcommand)]
enum SkillCommand {
    Create { name: String, #[arg(short, long)] description: String },
    List,
    Read { name: String },
    Add { name: String },
    Remove { name: String },
    Project,
    Search { query: String, #[arg(long)] registry: String },
    Install { name: String, #[arg(long)] registry: String, #[arg(long)] add: bool },
    Uninstall { name: String },
}

#[derive(Debug, Subcommand)]
enum RegistryCommand { Add { url: String }, List, Remove { url: String }, Search { query: String, url: String } }

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    if let Some(Command::Mpc { command: McpCommand::Serve }) = cli.command {
        let server = StdioMcpServer::new(std::env::current_dir()?)?;
        use std::io::{self, BufRead, Write};
        for line in io::stdin().lock().lines() {
            let line = line?;
            if line.trim().is_empty() { continue; }
            let response = server.handle(&line)?;
            println!("{response}");
            io::stdout().flush()?;
        }
        return Ok(());
    }
    let global = GlobalSkillRegistry::default()?;
    let project = ProjectSkillReferences::new(std::env::current_dir()?);
    let home = dirs::home_dir().context("could not determine home directory")?;
    let registry_store = RegistryStore::new(home.join(".agent-workspace-hub"));

    match cli.command {
        Some(Command::Status) => println!("Agent Workspace Hub — Rust core\nstatus: bootstrap complete"),
        Some(Command::Mpc { .. }) => unreachable!(),
        Some(Command::Skill { command }) => match command {
            SkillCommand::Create { name, description } => { let skill = global.create(&name, &description)?; println!("created global skill: {}\npath: {}", skill.name, skill.path.display()); }
            SkillCommand::List => for skill in global.list()? { println!("{} — {}", skill.name, skill.description); },
            SkillCommand::Read { name } => match global.get(&name)? { Some(skill) => println!("name: {}\ndescription: {}\nversion: {}\npath: {}", skill.name, skill.description, skill.version.as_deref().unwrap_or("unknown"), skill.path.display()), None => println!("skill not found: {name}"), },
            SkillCommand::Add { name } => { project.add(&name, &global)?; println!("added project skill reference: {name}"); }
            SkillCommand::Remove { name } => { if project.remove(&name)? { println!("removed project skill reference: {name}"); } else { println!("skill reference not found: {name}"); } }
            SkillCommand::Project => for skill in project.resolve(&global)? { println!("{} — {}", skill.name, skill.description); },
            SkillCommand::Search { query, registry: url } => search_registry(&url, &query)?,
            SkillCommand::Install { name, registry: url, add } => { let rt = tokio::runtime::Runtime::new()?; let client = RegistryClient::new(url.clone()); let cache = home.join(".agent-workspace-hub").join("cache").join("skills"); rt.block_on(SkillInstaller::new(cache).install_from_registry(&client, &name, &global))?; println!("installed global skill: {name}"); if add { project.add(&name, &global)?; println!("added project reference: {name}"); } }
            SkillCommand::Uninstall { name } => { let path = global.skills_dir().join(&name); if path.exists() { std::fs::remove_dir_all(path)?; println!("uninstalled global skill: {name}"); } else { println!("skill not installed: {name}"); } }
        },
        Some(Command::Registry { command }) => match command { RegistryCommand::Add { url } => { if registry_store.add(&url)? { println!("registry added: {url}"); } else { println!("registry already exists: {url}"); } }, RegistryCommand::List => for url in registry_store.load()?.registries { println!("{url}"); }, RegistryCommand::Remove { url } => { if registry_store.remove(&url)? { println!("registry removed: {url}"); } else { println!("registry not found: {url}"); } }, RegistryCommand::Search { query, url } => search_registry(&url, &query)?, },
        None => println!("Agent Workspace Hub — Rust\nRun `awh --help` for commands."),
    }
    Ok(())
}

fn search_registry(url: &str, query: &str) -> Result<()> { let rt = tokio::runtime::Runtime::new()?; for skill in rt.block_on(RegistryClient::new(url).search(query))? { println!("{} v{} — {}", skill.name, skill.version, skill.description); } Ok(()) }
