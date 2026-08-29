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
        #[command(subcommand)]
        command: McpCommand,
    },
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
enum McpCommand {
    Serve,
    List,
    Add {
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "stdio")]
        transport: String,
        #[arg(long)]
        command: Option<String>,
        #[arg(long = "arg")]
        args: Vec<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long = "env")]
        env: Vec<String>,
    },
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Search {
        query: String,
        #[arg(long, default_value = DEFAULT_MCP_REGISTRY)]
        registry: String,
    },
    Install {
        id: String,
        #[arg(long, default_value = DEFAULT_MCP_REGISTRY)]
        registry: String,
        #[arg(long)]
        add: bool,
    },
    Update {
        id: String,
        #[arg(long, default_value = DEFAULT_MCP_REGISTRY)]
        registry: String,
    },
    Uninstall {
        id: String,
    },
    Trust {
        id: String,
        #[arg(long, default_value = "local")]
        version: String,
    },
    Block {
        id: String,
        #[arg(long, default_value = "local")]
        version: String,
    },
    Revoke {
        id: String,
    },
    Status {
        id: String,
    },
    Permissions {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommand {
    Create {
        name: String,
        #[arg(short, long)]
        description: String,
    },
    List,
    Read {
        name: String,
    },
    Add {
        name: String,
    },
    Remove {
        name: String,
    },
    Project,
    Search {
        query: String,
        #[arg(long)]
        registry: String,
    },
    Install {
        name: String,
        #[arg(long)]
        registry: String,
        #[arg(long)]
        add: bool,
    },
    Uninstall {
        name: String,
    },
}
#[derive(Debug, Subcommand)]
enum RegistryCommand {
    Add { url: String },
    List,
    Remove { url: String },
    Search { query: String, url: String },
}

fn main() -> Result<()> {
    // JSON-RPC responses are written to stdout; keep all diagnostics on stderr
    // so they never corrupt the protocol stream consumed by MCP clients.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    if let Some(Command::Mcp {
        command: McpCommand::Serve,
    }) = cli.command
    {
        let server = StdioMcpServer::new(std::env::current_dir()?)?;
        use std::io::{self, BufRead, Write};
        for line in io::stdin().lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            println!("{}", server.handle_response(&line));
            io::stdout().flush()?;
        }
        return Ok(());
    }
    let global = GlobalSkillRegistry::discover()?;
    let project = ProjectSkillReferences::new(std::env::current_dir()?);
    let home = dirs::home_dir().context("could not determine home directory")?;
    let registry_store = RegistryStore::new(home.join(".agent-workspace-hub"));
    match cli.command {
        Some(Command::Status) => println!("Agent Workspace Hub — Rust\nstatus: bootstrap complete"),
        Some(Command::Mcp { command }) => handle_mcp_cli(command)?,
        Some(Command::Skill { command }) => match command {
            SkillCommand::Create { name, description } => {
                let s = global.create(&name, &description)?;
                println!(
                    "created global skill: {}\npath: {}",
                    s.name,
                    s.path.display()
                );
            }
            SkillCommand::List => {
                for s in global.list()? {
                    println!("{} — {}", s.name, s.description)
                }
            }
            SkillCommand::Read { name } => match global.get(&name)? {
                Some(s) => println!(
                    "name: {}\ndescription: {}\nversion: {}\npath: {}",
                    s.name,
                    s.description,
                    s.version.as_deref().unwrap_or("unknown"),
                    s.path.display()
                ),
                None => println!("skill not found: {name}"),
            },
            SkillCommand::Add { name } => {
                project.add(&name, &global)?;
                println!("added project skill reference: {name}")
            }
            SkillCommand::Remove { name } => {
                if project.remove(&name)? {
                    println!("removed project skill reference: {name}")
                } else {
                    println!("skill reference not found: {name}")
                }
            }
            SkillCommand::Project => {
                for s in project.resolve(&global)? {
                    println!("{} — {}", s.name, s.description)
                }
            }
            SkillCommand::Search {
                query,
                registry: url,
            } => search_registry(&url, &query)?,
            SkillCommand::Install {
                name,
                registry: url,
                add,
            } => {
                let rt = tokio::runtime::Runtime::new()?;
                let client = RegistryClient::new(url.clone());
                let cache = home
                    .join(".agent-workspace-hub")
                    .join("cache")
                    .join("skills");
                rt.block_on(
                    SkillInstaller::new(cache).install_from_registry(&client, &name, &global),
                )?;
                println!("installed global skill: {name}");
                if add {
                    project.add(&name, &global)?;
                    println!("added project reference: {name}")
                }
            }
            SkillCommand::Uninstall { name } => {
                let path = global.skills_dir().join(&name);
                if path.exists() {
                    std::fs::remove_dir_all(path)?;
                    println!("uninstalled global skill: {name}")
                } else {
                    println!("skill not installed: {name}")
                }
            }
        },
        Some(Command::Registry { command }) => match command {
            RegistryCommand::Add { url } => {
                if registry_store.add(&url)? {
                    println!("registry added: {url}")
                } else {
                    println!("registry already exists: {url}")
                }
            }
            RegistryCommand::List => {
                for url in registry_store.load()?.registries {
                    println!("{url}")
                }
            }
            RegistryCommand::Remove { url } => {
                if registry_store.remove(&url)? {
                    println!("registry removed: {url}")
                } else {
                    println!("registry not found: {url}")
                }
            }
            RegistryCommand::Search { query, url } => search_registry(&url, &query)?,
        },
        None => println!("Agent Workspace Hub — Rust\nRun `awh --help` for commands."),
    }
    Ok(())
}

fn trust_dir() -> Result<std::path::PathBuf> {
    Ok(dirs::home_dir()
        .context("could not determine home directory")?
        .join(".agent-workspace-hub"))
}

fn handle_mcp_cli(command: McpCommand) -> Result<()> {
    let registry = CustomMcpRegistry::new(std::env::current_dir()?)?;
    match command {
        McpCommand::List => {
            for s in registry.list()? {
                println!(
                    "{} — {} [{:?}] {}",
                    s.id,
                    s.name,
                    s.transport,
                    if s.enabled { "enabled" } else { "disabled" }
                )
            }
        }
        McpCommand::Add {
            id,
            name,
            transport,
            command,
            args,
            url,
            env,
        } => {
            let transport = match transport.to_ascii_lowercase().as_str() {
                "stdio" => McpTransport::Stdio,
                "streamablehttp" | "streamable-http" | "http" => McpTransport::StreamableHttp,
                other => anyhow::bail!("unsupported MCP transport: {other}"),
            };
            let mut environment = std::collections::HashMap::new();
            for item in env {
                let mut parts = item.splitn(2, '=');
                let k = parts.next().unwrap_or("");
                let v = parts.next().unwrap_or("");
                if k.is_empty() {
                    anyhow::bail!("--env must be KEY=VALUE")
                }
                environment.insert(k.to_string(), v.to_string());
            }
            let s = registry.add(CustomMcpServerConfig {
                id,
                name,
                transport,
                command,
                args,
                url,
                env: environment,
                permissions: McpPermissions::default(),
                enabled: true,
            })?;
            println!("added MCP: {}", s.id);
        }
        McpCommand::Remove { id } => {
            if registry.remove(&id)? {
                println!("removed MCP: {id}")
            } else {
                println!("MCP not found: {id}")
            }
        }
        McpCommand::Enable { id } => {
            registry.set_enabled(&id, true)?;
            println!("enabled MCP: {id}")
        }
        McpCommand::Disable { id } => {
            registry.set_enabled(&id, false)?;
            println!("disabled MCP: {id}")
        }
        McpCommand::Search {
            query,
            registry: url,
        } => {
            let rt = tokio::runtime::Runtime::new()?;
            for m in rt.block_on(CommunityMcpRegistryClient::new(url).search(&query))? {
                println!(
                    "{} v{} — {}",
                    m.id,
                    if m.version.is_empty() {
                        "unknown"
                    } else {
                        &m.version
                    },
                    m.description
                );
            }
        }
        McpCommand::Install {
            id,
            registry: url,
            add,
        } => {
            let rt = tokio::runtime::Runtime::new()?;
            let global = GlobalMcpRegistry::new()?;
            let project = ProjectMcpReferences::new(std::env::current_dir()?)?;
            let entry = rt.block_on(CommunityMcpRegistryClient::new(url).install(&global, &id))?;
            println!(
                "installed global MCP: {} v{}",
                entry.config.id, entry.version
            );
            if add {
                project.add(&id)?;
                println!("added project MCP reference: {id}")
            }
        }
        McpCommand::Update { id, registry: url } => {
            let rt = tokio::runtime::Runtime::new()?;
            let global = GlobalMcpRegistry::new()?;
            let entry = rt.block_on(CommunityMcpRegistryClient::new(url).update(&global, &id))?;
            println!("updated MCP: {} v{}", entry.config.id, entry.version);
        }
        McpCommand::Uninstall { id } => {
            let global = GlobalMcpRegistry::new()?;
            if global.remove(&id)? {
                println!("uninstalled global MCP: {id}")
            } else {
                println!("global MCP not found: {id}")
            }
        }
        McpCommand::Trust { id, version } => {
            let config = registry.get(&id)?.context("MCP not found")?;
            let dir = trust_dir()?;
            let mut store = PersistentTrustStore::new(&dir)?;
            store.approve(
                id.clone(),
                TrustLevel::Reviewed,
                config.permissions,
                version,
            )?;
            store.save(&dir)?;
            println!("trusted MCP: {id}");
        }
        McpCommand::Block { id, version } => {
            let dir = trust_dir()?;
            let mut store = PersistentTrustStore::new(&dir)?;
            store.approve(
                id.clone(),
                TrustLevel::Blocked,
                McpPermissions::default(),
                version,
            )?;
            store.save(&dir)?;
            println!("blocked MCP: {id}");
        }
        McpCommand::Revoke { id } => {
            let dir = trust_dir()?;
            let mut store = PersistentTrustStore::new(&dir)?;
            if !store.revoke(&id) {
                anyhow::bail!("no trust record found for {id}")
            }
            store.save(&dir)?;
            println!("revoked MCP trust: {id}");
        }
        McpCommand::Status { id } => {
            let dir = trust_dir()?;
            let store = PersistentTrustStore::new(&dir)?;
            let config = registry.get(&id)?;
            match store.approvals.iter().find(|a| a.id == id) {
                Some(a) => {
                    println!(
                        "MCP: {id}\ntrust: {:?}\napproved version: {}",
                        a.level,
                        if a.approved_version.is_empty() {
                            "any"
                        } else {
                            &a.approved_version
                        }
                    );
                }
                None => println!("MCP: {id}\ntrust: unknown"),
            }
            if let Some(c) = config {
                println!("enabled: {}\ntransport: {:?}", c.enabled, c.transport);
            }
        }
        McpCommand::Permissions { id } => {
            let config = registry.get(&id)?.context("MCP not found")?;
            let p = config.permissions;
            println!(
                "MCP: {id}\nnetwork: {}\nprocess: {}\nfilesystem: {}\nenvironment: {}\nsecrets: {}",
                p.network,
                p.process,
                if p.filesystem.is_empty() {
                    "none".into()
                } else {
                    p.filesystem.join(", ")
                },
                if p.environment.is_empty() {
                    "none".into()
                } else {
                    p.environment.join(", ")
                },
                if p.secrets.is_empty() {
                    "none".into()
                } else {
                    p.secrets.join(", ")
                }
            );
        }
        McpCommand::Serve => unreachable!(),
    }
    Ok(())
}
fn search_registry(url: &str, query: &str) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    for s in rt.block_on(RegistryClient::new(url).search(query))? {
        println!("{} v{} — {}", s.name, s.version, s.description)
    }
    Ok(())
}
