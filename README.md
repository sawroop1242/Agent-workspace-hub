# Agent-workspace-hub

## Documentation

- [Final architecture and roadmap](docs/PROJECT_ROADMAP.md) — canonical product boundary, architecture, phases, dependencies, CLI contract, build order, and acceptance workflow
- [Final CLI reference](docs/CLI.md) — complete target command tree, phase mapping, dependencies, security ordering, and validation rules
- [Architecture](docs/architecture.md) — existing implementation architecture and request flow
- [Features](docs/FEATURES.md) — final target feature contract
- [Agent Profiles roadmap](docs/ROADMAP_AGENT_PROFILES_POLICY_MCP.md) — TOML configuration, per-agent MCP routes, CLI lifecycle, policy integration and multi-agent sequencing
- [Security policy and threat model](docs/security.md)
- [Detailed threat model](docs/threat-model.md) — 10 threats with mitigations and tests
- [MCP integration](docs/mcp.md) — transports, tools, and interoperability evidence
- [Composio integration guide](docs/composio.md) — add Composio's hosted MCP, connect apps, invoke tools, gotchas
- [Configuration](docs/configuration.md) — every `AWH_*` variable and precedence
- [Development guide](docs/development.md) — conventions, commands, PR process
- [Testing guide](docs/testing.md) — suite map and regression policy
- [Release engineering](docs/release.md) — artifacts, checksums, CI-gated process
- [Completeness audit](docs/completeness-audit.md) — honest per-subsystem status
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

## Agent Profiles & Policy-Routed MCP

AWH uses named external-agent profiles as a target runtime architecture. Profiles are declarative TOML configuration; authorization remains in the canonical PolicyEngine.

Example endpoint model:

```text
/claude/mcp
/claude/sse
/qwen/mcp
/qwen/sse
```

The final CLI controls which configured agent servers are active:

```bash
awh agent list
awh agent show claude
awh agent start claude
awh agent start claude qwen
awh agent start --all
awh agent stop claude
awh agent restart claude
awh agent run claude
awh agent status
```

Starting only Claude means only Claude's configured routes are active; Qwen/OpenCode are not merely denied tools, their agent-specific routes are inactive. URL namespaces identify the profile but are not authorization: requests still pass through the canonical capability/policy engine.

See [docs/FEATURES.md](docs/FEATURES.md), [docs/CLI.md](docs/CLI.md), and [docs/ROADMAP_AGENT_PROFILES_POLICY_MCP.md](docs/ROADMAP_AGENT_PROFILES_POLICY_MCP.md) for the full design and sequencing.

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

## MCP transports

Agent Workspace Hub exposes its MCP tools over two transports:

| Transport | Command | Audience |
|---|---|---|
| stdio (default) | `awh mcp serve` | Local agents on the same host |
| HTTPS + SSE (remote) | `awh mcp serve --transport sse` | Remote agents |

See the existing MCP and security documentation for transport/authentication details. The final agent-profile architecture adds namespaced routes on top of the shared MCP runtime.

## Repository layout

| File / directory | Purpose |
|---|---|
| `Cargo.toml` / `Cargo.lock` | Rust package config and binary target `awh` |
| `src/main.rs` | CLI entry point |
| `src/core/*.rs` | Core workspace/project/context/memory/files/tasks |
| `src/mcp/*.rs` | MCP server, providers, trust, permissions, sandbox |
| `src/skills/*.rs` | Skill registry and package support |
| `tests/*.rs` | Integration and security tests |
| `docs/*.md` | Architecture, roadmap, security, implementation and status documentation |
| `scripts/install.sh` | One-line Rust-binary installer |
| `.github/workflows/*.yml` | CI and release pipelines |
