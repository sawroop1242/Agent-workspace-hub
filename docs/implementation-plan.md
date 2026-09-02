# AWH Implementation Plan (Master Specification)

Derived from `AWH_Master_Architecture_Implementation_Specification.txt`
after a full repository audit (Phase 0). Baseline before work:
`cargo check`, `cargo test` (222 tests), `cargo fmt --check`,
`cargo clippy -D warnings` all pass.

## Audit summary

Already present and preserved as-is:

- CLI: `awh status|mcp|skill|registry` (`src/main.rs`) — command
  semantics unchanged.
- MCP: 24 tools, stdio + HTTP/SSE transports, bearer auth, TLS,
  permissions, trust store, sandbox, circuit breaker, audit, limits
  (`src/mcp/*`). stdio stdout already protocol-only (stderr writer).
- Context engine: planning, offload, token budgets (`src/context/*`).
- Core stores: projects/files/memory/tasks with traversal-safe paths
  (`src/core/*`, `src/models/*`).
- Skills: global/project registries, installer, trust (`src/skills/*`).

Gaps addressed by this plan: shared service layer for Git/terminal,
Ratatui TUI + `WorkspaceBackend`, versioned Control API, remote TUI,
tunnel abstraction.

## Architecture

    CLI ── TUI ── API ── MCP  →  services  →  core  →  disk/git/processes

- `src/services/` — application service layer owning all business logic.
  Interfaces (CLI/TUI/API/MCP) call services; services call core stores.
- `src/services/git.rs` — structured `git` invocation (argument vectors,
  no shell strings), timeouts, high-risk operation confirmation flags.
- `src/services/terminal.rs` — bounded command execution: timeouts,
  output caps, session registry, audit hooks.
- `src/services/projects.rs`, `files.rs` — wrap/extend core stores with
  workspace-root enforcement, size limits, listing/search.
- `src/tui/` — Ratatui app; `WorkspaceBackend` trait with `LocalBackend`
  (services) and `RemoteBackend` (HTTPS `/api/v1`). All I/O async via
  channels so the UI never blocks.
- `src/api/` — `/api/v1/*` Control API (axum, already a dependency):
  status/projects/files/git/terminal/context/memory/skills/logs/audit;
  bearer auth (reuses `mcp::auth`), scoped authorization, in-process
  rate limiting, structured JSON errors.
- `src/tunnel/` — `TunnelProvider` trait + `NgrokProvider` (spawns local
  `ngrok` binary via structured args); start/stop/status/public URL.
  Providers are pluggable; nothing else in AWH depends on ngrok details.

## New dependencies (Section 40 discipline)

- `ratatui` + `crossterm` — pure Rust, MIT, no system requirements,
  Termux-compatible; required for the mandated TUI. Nothing else added:
  Git uses the `git` CLI with argument vectors; rate limiting is a small
  in-process middleware; tunneling shells out to the `ngrok` binary.

## Phases

- Phase 1 — services: Git, terminal, projects/files extensions. CLI
  untouched; MCP gains `git.*`/`terminal.run` tools backed by services.
- Phase 2 — TUI foundation: `WorkspaceBackend`, LocalBackend, event loop,
  navigation (Dashboard/Projects/Files/Editor/Git/Terminal/MCP/Context/
  Memory/Skills/Logs/Settings/Remote/Help), key hints, small-terminal
  handling, `awh tui` command.
- Phase 3 — Projects/File Manager/Editor screens (dirty state, safe
  large-file refusal, destructive confirmation).
- Phase 4 — Git + Terminal screens (bounded output, kill, confirmations).
- Phase 5 — MCP stdout regression test formalized as integration test.
- Phase 6 — Control API `/api/v1` + auth + rate limits + health.
- Phase 7 — RemoteBackend + Remote Connection screen with connection
  states (Connecting/Connected/Reconnecting/AuthFailed/Unavailable/
  Incompatible/Disconnected) and timeouts.
- Phase 8 — Context/Memory/Skills screens over existing engines.
- Phase 9 — `TunnelProvider` + ngrok + `awh tunnel` CLI.
- Phase 10 — hardening pass, security tests, docs.

Validation at every phase: `cargo fmt --check`, `cargo check`,
`cargo test`, `cargo clippy -D warnings`.
