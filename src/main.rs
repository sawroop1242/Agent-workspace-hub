mod core;
mod models;
mod skills;

use anyhow::Result;
use clap::{Parser, Subcommand};
use skills::{GlobalSkillRegistry, ProjectSkillReferences, RegistryClient};

#[derive(Debug, Parser)]
#[command(name = "awh", version, about = "Agent Workspace Hub")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Status,
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    Registry {
        #[command(subcommand)]
        command: RegistryCommand,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommand {
    Create { name: String, #[arg(short, long)] description: String },
    List,
    Read { name: String },
    Add { name: String },
    Remove { name: String },
    Project,
    Search { query: String, #[arg(long)] registry: String },
}

#[derive(Debug, Subcommand)]
enum RegistryCommand {
    Search { query: String, url: String },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let registry = GlobalSkillRegistry::default()?;
    let project = ProjectSkillReferences::new(std::env::current_dir()?);

    match cli.command {
        Some(Command::Status) => {
            println!("Agent Workspace Hub — Rust core");
            println!("status: bootstrap complete");
        }
        Some(Command::Skill { command }) => match command {
            SkillCommand::Create { name, description } => {
                let skill = registry.create(&name, &description)?;
                println!("created global skill: {}", skill.name);
                println!("path: {}", skill.path.display());
            }
            SkillCommand::List => {
                for skill in registry.list()? {
                    println!("{} — {}", skill.name, skill.description);
                }
            }
            SkillCommand::Read { name } => match registry.get(&name)? {
                Some(skill) => {
                    println!("name: {}", skill.name);
                    println!("description: {}", skill.description);
                    println!("version: {}", skill.version.as_deref().unwrap_or("unknown"));
                    println!("path: {}", skill.path.display());
                }
                None => println!("skill not found: {name}"),
            },
            SkillCommand::Add { name } => {
                project.add(&name, &registry)?;
                println!("added skill reference: {name}");
                println!("references: {}", project.path().display());
            }
            SkillCommand::Remove { name } => {
                if project.remove(&name)? {
                    println!("removed skill reference: {name}");
                } else {
                    println!("skill reference not found: {name}");
                }
            }
            SkillCommand::Project => {
                for skill in project.resolve(&registry)? {
                    println!("{} — {}", skill.name, skill.description);
                }
            }
            SkillCommand::Search { query, registry: url } => {
                let rt = tokio::runtime::Runtime::new()?;
                for skill in rt.block_on(RegistryClient::new(url).search(&query))? {
                    println!("{} v{} — {}", skill.name, skill.version, skill.description);
                }
            }
        },
        Some(Command::Registry { command }) => match command {
            RegistryCommand::Search { query, url } => {
                let rt = tokio::runtime::Runtime::new()?;
                for skill in rt.block_on(RegistryClient::new(url).search(&query))? {
                    println!("{} v{} — {}", skill.name, skill.version, skill.description);
                }
            }
        },
        None => {
            println!("Agent Workspace Hub — Rust");
            println!("Run `awh status` for status.");
        }
    }

    Ok(())
}
