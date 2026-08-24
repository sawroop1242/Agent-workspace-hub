use anyhow::Result;
use clap::{Parser, Subcommand};

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
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Status) => {
            println!("Agent Workspace Hub — Rust core");
            println!("status: bootstrap complete");
        }
        None => {
            println!("Agent Workspace Hub — Rust");
            println!("Run `awh status` for status.");
        }
    }

    Ok(())
}
