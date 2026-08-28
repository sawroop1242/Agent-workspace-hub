# Agent-workspace-hub

## Documentation

- [Security policy and threat model](docs/SECURITY.md)
- [Installation and upgrade guide](docs/INSTALL.md)
- [Project status and implementation guide](docs/PROJECT_STATUS.md)
- [Community MCP registry](docs/community-mcp-registry.md)

## One-line install

Install Agent Workspace Hub with one command (downloads a prebuilt Rust binary
for your OS/architecture):

```bash
curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash
```

Build directly from Git source instead of the latest release binary:

```bash
curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash -s -- --source source
```

Install a specific release tag, or to a custom directory:

```bash
curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash -s -- --version v0.1.0 --prefix "$HOME/.bin"
```

The installer requires `curl`; source installs additionally require `cargo`.
Prebuilt binaries target Linux (x86_64, aarch64), macOS (x86_64, aarch64), and
Windows (x86_64), falling back to a `cargo build` when no matching asset exists.



## Agent handoff workflow

Agent Workspace Hub is an MCP server that preserves enough project state for a
different AI agent to continue work without a new bootstrap prompt. Each project
keeps durable state in `.agent/context.md`, `.agent/memory.json`, `.agent/tasks/`,
enabled skills, and configured connectors.

Recommended new-agent startup:

1. Call `workspace.context` to receive the persisted project context.
2. Call `memory.search` (or `memory.get` for a known id) when recent decisions and
   notes are needed.
3. Call `tasks.list` to see active tasks and their status.
4. Read relevant skills via `skills.read` before applying project-specific procedures.
5. Use `connector.invoke` only when a configured connector is required by the active task.

This loop makes context transfer explicit: the outgoing agent records decisions and
progress, and the incoming agent starts from the persisted state instead of asking
the user to repeat the project idea.

| File / directory                            | Purpose                                              |
| ------------------------------------------- | ---------------------------------------------------- |
| `Cargo.toml` / `Cargo.lock`                 | Rust package config, dependencies, binary target `awh` |
| `src/main.rs`                               | CLI entry point                                      |
| `src/lib.rs`                                | Library root                                         |
| `src/core/*.rs`                             | Workspace, project, context, memory, files, tasks     |
| `src/models/*.rs`                           | Data models (memory, project, task)                  |
| `src/mcp/*.rs`                              | MCP server, providers, trust, permissions, sandbox    |
| `src/mcp/audit.rs`                          | Structured security audit logging                    |
| `src/mcp/circuit_breaker.rs`                | Fail-fast breaker for misbehaving MCP servers         |
| `src/mcp/config.rs`                         | Resource limits + config precedence rules            |
| `src/mcp/sandbox.rs`                        | Per-platform process sandboxing (Linux/macOS/Windows) |
| `src/mcp/schema.rs`                         | JSON Schema argument-validation gate                 |
| `src/skills/*.rs`                           | Skill registry, installer, package, remote fetching  |
| `tests/*.rs`                                | Integration + security test suites                   |
| `examples/bench.rs`                         | Micro-benchmark for the schema-validation gate       |
| `docs/*.md`                                 | Security policy, install guide, project status       |
| `scripts/install.sh`                        | One-line Rust-binary installer                       |
| `.github/workflows/*.yml`                   | CI (fmt/test/clippy) and release pipeline            |
