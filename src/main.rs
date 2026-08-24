mod core;
mod models;
mod skills;

use anyhow::Result;
use clap::{Parser, Subcommand};
use skills::SkillStore;

#[derive(Debug, Parser)]
#[command(name = "awh", version, about = "Agent Workspace Hub")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show the Rust Agent Workspace Hub status.
    Status,
    /// Manage project skills.
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommand {
    /// Create a new local skill.
    Create { name: String, #[arg(short, long)] description: String },
    /// List installed local skills.
    List,
    /// Read skill metadata.
    Read { name: String },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let root = std::env::current_dir()?;
    let store = SkillStore::new(root);

    match cli.command {
        Some(Command::Status) => {
            println!("Agent Workspace Hub — Rust core");
            println!("status: bootstrap complete");
        }
        Some(Command::Skill { command }) => match command {
            SkillCommand::Create { name, description } => {
                let skill = store.create(&name, &description)?;
                println!("created skill: {}", skill.name);
                println!("path: {}", skill.path.display());
            }
            SkillCommand::List => {
                for skill in store.list()? {
                    println!("{} — {}", skill.name, skill.description);
                }
            }
            SkillCommand::Read { name } => match store.get(&name)? {
                Some(skill) => {
                    println!("name: {}", skill.name);
                    println!("description: {}", skill.description);
                    println!("version: {}", skill.version.as_deref().unwrap_or("unknown"));
                    println!("path: {}", skill.path.display());
                }
                None => println!("skill not found: {name}"),
            },
        },
        None => {
            println!("Agent Workspace Hub — Rust");
            println!("Run `awh status` for status.");
        }
    }

    Ok(())
}
