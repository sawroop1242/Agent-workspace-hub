mod core;
mod models;
mod skills;

use anyhow::Result;
use clap::{Parser, Subcommand};
use skills::{GlobalSkillRegistry, ProjectSkillReferences};

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
}

#[derive(Debug, Subcommand)]
enum SkillCommand {
    /// Create a skill in the global registry.
    Create { name: String, #[arg(short, long)] description: String },
    /// List globally installed skills.
    List,
    /// Read a globally installed skill.
    Read { name: String },
    /// Add a global skill reference to the current project.
    Add { name: String },
    /// Remove a skill reference from the current project.
    Remove { name: String },
    /// List skills referenced by the current project.
    Project,
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
        },
        None => {
            println!("Agent Workspace Hub — Rust");
            println!("Run `awh status` for status.");
        }
    }

    Ok(())
}
