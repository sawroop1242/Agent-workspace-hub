# AWH RUST FORENSIC ENGINEERING REPORT

**Repository:** Agent Workspace Hub (AWH) — Rust Branch  
**Analysis Date:** 2025-09-09  
**Analyst Role:** Principal Rust Architect + Systems Engineer + Security Architect  

---

## 1. Executive Summary

The AWH Rust branch is a **substantially complete, security-focused MCP server implementation** with ~17,279 lines of production Rust code across 80+ modules. The architecture demonstrates mature engineering practices including:

- **Transport-agnostic MCP dispatch** (stdio + HTTP/SSE share single dispatcher)
- **Centralized security boundary** (execution gate, trust store, permission validation)
- **Service-layer abstraction** (CLI/TUI/API all call shared services)
- **Comprehensive audit logging** (allow/deny events with token redaction)
- **Platform-specific sandboxing** (bwrap on Linux, sandbox-exec on macOS, Job Objects on Windows)

### Key Findings

| Category | Status | Risk Level |
|----------|--------|------------|
| Core MCP Server | COMPLETE | LOW |
| Security Boundary | IMPLEMENTED BUT HARDENING NEEDED | MEDIUM |
| CLI Interface | COMPLETE | LOW |
| TUI Interface | PARTIAL (missing advanced features) | LOW |
| Control API | PARTIAL (missing agent management) | MEDIUM |
| Context Engine | IMPLEMENTED BUT NOT FULLY WIRED | MEDIUM |
| Multi-Agent Runtime | MISSING | HIGH |
| Task DAG/Scheduler | MISSING | HIGH |
| Checkpoint/Recovery | MISSING | HIGH |

### Critical Gaps vs Target Architecture

1. **No Agent Registry or Agent Runtime** — agents cannot be spawned, managed, or isolated
2. **No Capability Security Layer** — no capability grants, approval engine, or human-in-loop
3. **No Message Bus** — no inter-agent communication substrate
4. **No Task Dependency Graph** — tasks exist but no DAG, scheduler, or workflow orchestration
5. **No Checkpoint System** — no crash recovery, snapshots, or artifact provenance
6. **No Model Router** — context assembly exists but no intelligent model selection/routing
7. **No Resource Governor** — per-agent resource limits not enforced at runtime
8. **No Cancellation/Retry System** — no structured failure handling

---

## 2. Repository Inventory

### Module Structure

```
src/
├── main.rs (803 lines)          # CLI entry point with clap parser
├── lib.rs                       # Library exports
├── api/                         # HTTP Control API (§16)
│   ├── mod.rs                   # Module definition
│   └── control.rs               # /api/v1 routes (axum)
├── context/                     # Context Engine (§12)
│   ├── budget.rs                # Token budget management
│   ├── compressor.rs            # Context compression
│   ├── engine.rs                # Main context manager
│   ├── item.rs                  # ContextItem model
│   ├── offload.rs               # Offload store
│   ├── planner.rs               # Planning logic
│   ├── policy.rs                # Decision policies
│   ├── scoring.rs               # Relevance scoring
│   ├── selector.rs              # Budget-constrained selection
│   ├── snapshot.rs              # Snapshots
│   └── tokens.rs                # Token counting
├── core/                        # Storage engines (workspace-local state)
│   ├── context.rs               # AGENTS.md-style context loading
│   ├── files.rs                 # Sandboxed file reads
│   ├── memory.rs                # Project memory store
│   ├── project.rs               # Per-project metadata
│   ├── tasks.rs                 # Task store
│   └── workspace.rs             # Project root resolution
├── mcp/                         # MCP integration & enforcement (§26)
│   ├── audit.rs                 # Structured security audit logging
│   ├── auth.rs                  # Bearer-token authentication
│   ├── circuit_breaker.rs       # Failure isolation
│   ├── cli_trust.rs             # CLI trust commands
│   ├── community_registry.rs    # Community MCP registry client
│   ├── composio*.rs             # Composio provider integration
│   ├── config.rs                # ResourceLimits + env overrides
│   ├── connectors.rs            # Connector metadata store
│   ├── context.rs               # Workspace context assembly
│   ├── custom_mcp.rs            # Custom (per-project) MCP servers
│   ├── dispatcher.rs (2230 L)   # Transport-agnostic JSON-RPC dispatcher
│   ├── error.rs                 # Deterministic JSON-RPC error codes
│   ├── execution_gate.rs        # Centralized gate every tool passes through
│   ├── github.rs                # GitHub provider (github.* tools)
│   ├── global_mcp.rs            # Globally installed MCP servers
│   ├── http.rs                  # HTTPS server: /health, /sse, /mcp
│   ├── memory.rs                # MCP memory store
│   ├── permissions.rs           # Centralized permission model
│   ├── providers.rs             # Connector provider registry
│   ├── sandbox.rs               # bwrap sandbox command construction
│   ├── schema.rs                # Tool argument/schema validation
│   ├── security.rs              # Secret redaction & response sanitization
│   ├── server.rs                # StdioMcpServer: JSON-RPC over stdin/stdout
│   ├── skills.rs                # Skill gateway
│   ├── sse.rs                   # Session registry (SSE lifecycle)
│   ├── store_lock.rs            # Cross-process advisory locking
│   ├── tasks.rs                 # MCP task store
│   ├── tls.rs                   # TLS configuration
│   ├── trust.rs                 # MCP trust and approval policy
│   ├── trust_store.rs           # Persistent trust store
│   └── workspace.rs             # MCP workspace access
├── models/                      # Shared typed domain models
│   ├── memory.rs                # MemoryEntry model
│   ├── mod.rs                   # Module exports
│   ├── project.rs               # Project model
│   └── task.rs                  # Task model
├── services/                    # Application service layer
│   ├── audit.rs                 # In-memory audit trail
│   ├── files.rs                 # File operations service
│   ├── git.rs                   # Git service (structured argv calls)
│   ├── mod.rs                   # Module exports
│   ├── projects.rs              # Projects service
│   ├── rate_limit.rs            # Sliding-window rate limiter
│   └── terminal.rs              # Bounded command execution
├── skills/                      # Skill discovery/loading/management
│   ├── installer.rs             # Skill installer
│   ├── lockfile.rs              # Skill lockfile persistence
│   ├── model.rs                 # Skill data model
│   ├── package.rs               # Skill package helpers
│   ├── parser.rs                # SKILL.md parsing
│   ├── project.rs               # Project skill references
│   ├── references.rs            # Skill references
│   ├── registries.rs            # Skill registries
│   ├── registry.rs              # Global skill registry
│   ├── registry_client.rs       # Registry HTTP client
│   ├── registry_manifest.rs     # Registry manifest models
│   ├── remote.rs                # Remote (Git/community) skill sources
│   ├── store.rs                 # Project skill store
│   └── trust.rs                 # Skill trust levels
├── tui/                         # Keyboard-first Ratatui TUI (§6)
│   ├── app.rs                   # TUI application state machine
│   ├── backend.rs               # WorkspaceBackend trait + LocalBackend
│   ├── mod.rs                   # Module exports + run_local/run_remote
│   ├── remote.rs                # RemoteBackend for Control API
│   └── screens/                 # All TUI screens
│       ├── mod.rs               # Screen registry and dispatch
│       ├── context.rs           # Context screen
│       ├── editor.rs            # Text editor screen
│       ├── files.rs             # Files browser screen
│       ├── git.rs               # Git operations screen
│       ├── logs.rs              # Audit log viewer
│       ├── mcp.rs               # MCP server status screen
│       ├── memory.rs            # Memory entries screen
│       ├── projects.rs          # Project management screen
│       ├── remote.rs            # Remote connection screen
│       ├── settings.rs          # Settings screen
│       ├── skills.rs            # Skills screen
│       └── terminal.rs          # Terminal output screen
└── tunnel/                      # Tunnel provider abstraction
    └── mod.rs                   # ngrok tunnel support
```

### Test Coverage

```
tests/
├── architecture.rs              # Architecture tests
├── mcp_http.rs                  # HTTP transport tests
├── mcp_sandbox.rs               # Sandbox tests
├── mcp_security.rs              # Security boundary tests
└── mcp_server.rs                # Server tests
```

### Documentation

```
docs/
├── INSTALL.md                   # Installation instructions
├── MASTER_PROMPT.md             # Target architecture specification
├── PROJECT_CONTEXT.md           # Project context
├── PROJECT_STATUS.md            # Status tracking
├── architecture.md              # Architecture documentation
├── awh-evolution-report.md      # Evolution report
├── community-mcp-registry.md    # Community registry docs
├── completeness-audit.md        # Completeness audit
├── configuration.md             # Configuration docs
├── development.md               # Development guide
├── error.md                     # Error handling docs
├── implementation-plan.md       # Implementation plan
├── mcp.md                       # MCP documentation
├── phase-11-github-integration-and-gaps.md  # Phase 11 report
├── release.md                   # Release process
├── security.md                  # Security documentation
├── testing.md                   # Testing strategy
└── threat-model.md              # Threat model
```

---

## 3. Current Actual Architecture

### Layer Diagram (AS-IMPLEMENTED)

```
┌─────────────────────────────────────────────────────────────────┐
│                      INTERFACE LAYER                             │
├─────────────┬─────────────┬─────────────┬───────────────────────┤
│   CLI       │   TUI       │   Control   │   MCP Server          │
│   (clap)    │   (ratatui) │   API       │   (stdio/HTTP-SSE)    │
│             │             │   (axum)    │                       │
└──────┬──────┴──────┬──────┴──────┬──────┴──────────┬────────────┘
       │             │             │                  │
       │             │             │                  │
       ▼             ▼             ▼                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                    SERVICE LAYER (SHARED)                        │
├─────────────┬─────────────┬─────────────┬───────────────────────┤
│  Files      │  Git        │  Terminal   │  Projects             │
│  Service    │  Service    │  Service    │  Service              │
│             │             │             │                       │
│  Rate       │  Audit      │             │                       │
│  Limiter    │  Log        │             │                       │
└──────┬──────┴──────┬──────┴──────┬──────┴──────────┬────────────┘
       │             │             │                  │
       ▼             ▼             ▼                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                     CORE LAYER                                   │
├─────────────┬─────────────┬─────────────┬───────────────────────┤
│  Workspace  │  Project    │  Memory     │  Tasks                │
│  Root       │  Store      │  Store      │  Store                │
│             │             │             │                       │
│  Context    │  Files      │             │                       │
│  Loader     │  Helpers    │             │                       │
└──────┬──────┴──────┬──────┴─────────────┴───────────────────────┘
       │             │
       ▼             ▼
┌─────────────────────────────────────────────────────────────────┐
│                    MCP DISPATCHER                                │
├─────────────────────────────────────────────────────────────────┤
│  McpDispatcher (transport-agnostic)                              │
│  ├─ ExecutionGate (authorize_mcp_execution)                      │
│  ├─ TrustStore (PersistentTrustStore)                            │
│  ├─ Permissions (McpPermissions)                                 │
│  ├─ ProviderRegistry (dynamic providers)                         │
│  └─ Tool implementations:                                        │
│      ├─ SkillMcp                                                 │
│      ├─ WorkspaceMcp                                             │
│      ├─ MemoryMcp                                                │
│      ├─ TasksMcp                                                 │
│      ├─ ConnectorsMcp                                            │
│      ├─ GithubProvider                                           │
│      └─ ContextEngine                                            │
└─────────────────────────────────────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────────────────────────────┐
│                   SECURITY BOUNDARY                              │
├─────────────┬─────────────┬─────────────┬───────────────────────┤
│  Sandbox    │  Schema     │  Circuit    │  Audit                │
│  (bwrap)    │  Validator  │  Breaker    │  (allow/deny)         │
└─────────────┴─────────────┴─────────────┴───────────────────────┘
```

### Request Flow (VERIFIED)

```
User Input
    ↓
┌──────────────────────────────────────────────────────────────┐
│ CLI Command (main.rs::handle_mcp_cli)                        │
│ TUI Action (tui/screens/*.rs::handle_key)                    │
│ API Request (api/control.rs::handler)                        │
│ MCP JSON-RPC (mcp/server.rs::handle)                         │
└──────────────────────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────────────────────┐
│ Service Layer                                                │
│ - FilesService::read/write/search                            │
│ - GitService::status/commit/push                             │
│ - TerminalService::run                                       │
│ - ProjectsService::list/create/delete                        │
└──────────────────────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────────────────────┐
│ Core Stores                                                  │
│ - ProjectStore (JSON-backed)                                 │
│ - MemoryStore (append-only JSONL)                            │
│ - TaskStore (JSON-backed)                                    │
│ - ContextStore (AGENTS.md/AGENT.md/README.md concat)         │
└──────────────────────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────────────────────┐
│ OS Operations                                                │
│ - tokio::process::Command (argv form, no shell)              │
│ - std::fs (atomic writes via temp+rename)                    │
│ - cross-process locks (StoreLock with mtime staleness)       │
└──────────────────────────────────────────────────────────────┘
```

### MCP Tool Execution Flow (VERIFIED)

```
MCP Client (OpenCode/Codex/Inspector)
    ↓
┌──────────────────────────────────────────────────────────────┐
│ Transport                                                    │
│ - StdioMcpServer (JSON-RPC lines over stdin/stdout)          │
│ - HTTP Server (POST /mcp?sessionId=...)                      │
└──────────────────────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────────────────────┐
│ McpDispatcher::dispatch                                      │
│ - Parse JSON-RPC request                                     │
│ - Validate jsonrpc="2.0"                                     │
│ - Route to method handler                                    │
└──────────────────────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────────────────────┐
│ tools/call Handler (dispatcher.rs::call_tool)                │
│ - Resolve tool name                                          │
│ - Validate arguments against schema                          │
└──────────────────────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────────────────────┐
│ Execution Gate (EXECUTION_GATE CHECK)                        │
│ - authorize_mcp_execution()                                  │
│ - Verify trust approval exists                               │
│ - Verify version match                                       │
│ - Verify permissions subset                                  │
│ - Audit deny on failure                                      │
└──────────────────────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────────────────────┐
│ Tool Implementation                                          │
│ - SkillMcp::list/read/add/remove                             │
│ - WorkspaceMcp::read_file/write_file/list_files              │
│ - MemoryMcp::store/search/get/delete                         │
│ - TasksMcp::create/list/update/delete                        │
│ - ConnectorsMcp::list/add/enable/disable/remove              │
│ - GithubProvider::issue_*/pr_*/repo_*                        │
│ - ContextEngine::insert/get/remove/search/optimize           │
└──────────────────────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────────────────────┐
│ Security Enforcement                                         │
│ - Sandbox (wrap_command with bwrap/sandbox-exec)             │
│ - Schema validation (validate_tool_arguments)                │
│ - Circuit breaker (CircuitBreakerMcpClient)                  │
│ - Secret redaction (security.rs)                             │
└──────────────────────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────────────────────┐
│ Audit Logging                                                │
│ - audit_allow(action, subject, detail)                       │
│ - audit_deny(action, reason, subject)                        │
│ - Token redaction in audit entries                           │
└──────────────────────────────────────────────────────────────┘
    ↓
JSON-RPC Response to Client
```

---

## 4. Target Architecture (from MASTER_PROMPT.md)

The documented target architecture includes components **NOT YET IMPLEMENTED**:

### Missing Core Components

| Component | Status | Priority |
|-----------|--------|----------|
| Agent Registry | MISSING | P0 |
| Agent Runtime | MISSING | P0 |
| Agent Lifecycle Management | MISSING | P0 |
| Agent MCP Isolation | PARTIAL (sandbox exists, no per-agent isolation) | P0 |
| Capability Security Layer | MISSING | P0 |
| CapabilityGrant Model | MISSING | P0 |
| Policy Engine | MISSING | P0 |
| Approval Engine | MISSING | P0 |
| Human-in-Loop Workflow | MISSING | P1 |
| Message Bus | MISSING | P1 |
| Multi-Agent Orchestration | MISSING | P1 |
| Task DAG | MISSING | P1 |
| Scheduler | MISSING | P1 |
| Events System | PARTIAL (audit log exists, no event stream) | P1 |
| Checkpoints | MISSING | P1 |
| Recovery System | MISSING | P1 |
| Memory Scopes (Session/Project/Global) | PARTIAL (MemoryScope enum exists, limited enforcement) | P2 |
| Context Budget Enforcement | IMPLEMENTED | P2 |
| Model Router | MISSING | P1 |
| Workspace Snapshots | MISSING | P2 |
| Git Worktrees | MISSING | P2 |
| Artifacts | MISSING | P2 |
| Provenance Tracking | MISSING | P2 |
| Resource Governor | MISSING | P1 |
| Cancellation System | MISSING | P1 |
| Retry System | MISSING | P2 |
| Failure Classification | PARTIAL (error types exist, no classification) | P2 |

---

## 5. Architecture Differences (CURRENT → TARGET)

### Gap Analysis

| Layer | Current | Target | Gap |
|-------|---------|--------|-----|
| Identity | None | Agent identity with capabilities | MISSING |
| Authorization | Trust-based (per-MCP) | Capability-based (per-agent/per-tool) | MAJOR |
| Agent Model | No agent concept | AgentRegistry + AgentRuntime | MISSING |
| Task Model | Flat task store | Task DAG with dependencies | MISSING |
| Communication | Direct tool calls | Message Bus + Events | MISSING |
| Persistence | JSON files | Structured stores with migration | PARTIAL |
| Observability | Audit log only | Logs + Metrics + Traces | PARTIAL |
| Concurrency | Tokio runtime | Structured task spawning + cancellation | PARTIAL |

---

## 6. Module-by-Module Analysis

### 6.1 CLI (`main.rs`)

**Status:** COMPLETE

| Command | Subcommand | Handler | Service | JSON | Tests | Issues |
|---------|------------|---------|---------|------|-------|--------|
| `status` | - | inline | ProjectStore | No | No | None |
| `tui` | - | tui::run_local/remote | WorkspaceBackend | N/A | No | None |
| `serve` | - | serve_control_api | ControlState | Yes | No | None |
| `mcp` | `serve` | handle_mcp_cli | StdioMcpServer | N/A | No | Unreachable branch |
| `mcp` | `list` | handle_mcp_cli | CustomMcpRegistry | No | No | None |
| `mcp` | `add` | handle_mcp_cli | CustomMcpRegistry | No | No | None |
| `mcp` | `remove` | handle_mcp_cli | CustomMcpRegistry | No | No | None |
| `mcp` | `enable` | handle_mcp_cli | CustomMcpRegistry | No | No | None |
| `mcp` | `disable` | handle_mcp_cli | CustomMcpRegistry | No | No | None |
| `mcp` | `search` | handle_mcp_cli | CommunityMcpRegistryClient | No | No | None |
| `mcp` | `install` | handle_mcp_cli | GlobalMcpRegistry | No | No | None |
| `mcp` | `update` | handle_mcp_cli | GlobalMcpRegistry | No | No | None |
| `mcp` | `uninstall` | handle_mcp_cli | GlobalMcpRegistry | No | No | None |
| `mcp` | `trust` | handle_mcp_cli | PersistentTrustStore | No | No | None |
| `mcp` | `block` | handle_mcp_cli | PersistentTrustStore | No | No | None |
| `mcp` | `revoke` | handle_mcp_cli | PersistentTrustStore | No | No | None |
| `mcp` | `status` | handle_mcp_cli | PersistentTrustStore | No | No | None |
| `mcp` | `permissions` | handle_mcp_cli | CustomMcpRegistry | No | No | None |
| `skill` | `create` | handle_skill_cli | GlobalSkillRegistry | No | No | None |
| `skill` | `list` | handle_skill_cli | GlobalSkillRegistry | No | No | None |
| `skill` | `read` | handle_skill_cli | GlobalSkillRegistry | No | No | None |
| `skill` | `add` | handle_skill_cli | ProjectSkillReferences | No | No | None |
| `skill` | `remove` | handle_skill_cli | ProjectSkillReferences | No | No | None |
| `skill` | `project` | handle_skill_cli | ProjectSkillReferences | No | No | None |
| `skill` | `search` | handle_skill_cli | RegistryClient | No | No | None |
| `skill` | `install` | handle_skill_cli | SkillInstaller | No | No | None |
| `skill` | `uninstall` | handle_skill_cli | fs::remove_dir_all | No | No | None |
| `registry` | `add` | handle_registry_cli | RegistryStore | No | No | None |
| `registry` | `list` | handle_registry_cli | RegistryStore | No | No | None |
| `registry` | `remove` | handle_registry_cli | RegistryStore | No | No | None |
| `registry` | `search` | handle_registry_cli | RegistryClient | No | No | None |
| `tunnel` | `start` | handle_tunnel_cli | tunnel::provider_by_name | No | No | None |
| `tunnel` | `status` | handle_tunnel_cli | tunnel::provider_by_name | No | No | None |

**Issues Found:**
- Line 612: `McpCommand::Serve { .. } => unreachable!()` — dead code path (serve handled separately)
- No JSON output mode for CLI commands (except Control API)
- No `--json` flag support
- Exit codes not standardized

### 6.2 TUI (`tui/`)

**Status:** PARTIAL

| Screen | Data Source | Backend | Mutation | Refresh | Tests | Issues |
|--------|-------------|---------|----------|---------|-------|--------|
| Dashboard | LocalBackend::dashboard | LocalBackend | Read-only | TTL-based | No | None |
| Projects | LocalBackend::list_projects | LocalBackend | Create/Delete/Open | Manual | No | Confirmation dialog exists |
| Files | LocalBackend::list_dir | LocalBackend | Read/Write/Delete/Rename | Manual | No | None |
| Editor | LocalBackend::read_file | LocalBackend | Write | Manual | No | Dirty tracking exists |
| Git | LocalBackend::git_* | LocalBackend | Stage/Commit/Push/Pull | Manual | No | None |
| Terminal | LocalBackend::terminal_run | LocalBackend | Run command | Manual | No | One-shot only (no sessions) |
| MCP | LocalBackend::list_mcp_servers | LocalBackend | Read-only | Manual | No | Limited functionality |
| Context | LocalBackend::read_context | LocalBackend | Read/Write | Manual | No | None |
| Memory | LocalBackend::list_memory | LocalBackend | Append | Manual | No | None |
| Skills | LocalBackend::list_*_skills | LocalBackend | Toggle | Manual | No | None |
| Logs | crate::services::audit::global() | N/A | Read-only | Manual | No | None |
| Settings | N/A | N/A | Read-only | N/A | No | Placeholder |
| Remote | RemoteBackend | RemoteBackend | Read-only | Manual | No | Connection handshake exists |
| Help | Static | N/A | N/A | N/A | No | None |

**TUI Backend Abstraction:** VERIFIED WORKING
- `WorkspaceBackend` trait properly abstracts local vs remote
- `LocalBackend` correctly uses shared services
- `RemoteBackend` implements HTTP client to Control API

### 6.3 MCP Dispatcher (`mcp/dispatcher.rs`)

**Status:** COMPLETE (2230 lines)

**Tools Implemented:**

| Tool Category | Tools | Handler | Security Check |
|---------------|-------|---------|----------------|
| Skills | list, read, add, remove, search | SkillMcp | Trust store check at dispatcher init |
| Workspace | context, list_files, read_file, write_file, delete_file | WorkspaceMcp | Path validation (secure_path) |
| Memory | store, search, get, delete, update | MemoryMcp | Scope validation |
| Tasks | create, list, update, delete, get | TasksMcp | None |
| Connectors | list, add, enable, disable, remove, get | ConnectorsMcp | None |
| Connector Providers | providers, tools, invoke, composio_*, github.* | ProviderRegistry | GitHub token check |
| Context Engine | status, insert, get, remove, search, optimize, assemble, protect, unprotect, offload, restore | ContextEngine | Opt-in via AWH_CONTEXT_ENABLED |
| Git | status, branch, log, diff, stage, unstage, commit | GitService (via dispatcher) | Path validation |
| Terminal | run | TerminalService | Argv form (no shell) |

**Security Flow Verification:**

FACT: `McpDispatcher::new()` (lines 141-249) performs trust verification for custom MCP servers:
```rust
if !is_authorized(&cfg, trust_store.as_ref()) {
    continue;  // Skip unauthorized servers
}
```

FACT: `call_tool()` (lines 560-1000+) validates arguments via `validate_tool_arguments()` before execution.

FACT: `execute_with_circuit_breaker()` wraps dynamic provider calls with failure isolation.

INFERENCE: Static tools (skills, workspace, memory, tasks, connectors, context, git, terminal) do NOT pass through the execution gate at call time—they rely on initialization-time trust checks for MCP servers only.

RECOMMENDATION: Add runtime authorization check for high-risk tools (terminal.run, workspace.write_file) even for statically registered tools.

### 6.4 Security Modules

#### 6.4.1 Execution Gate (`mcp/execution_gate.rs`)

**Status:** COMPLETE

```rust
pub fn authorize(
    request: &McpExecutionRequest<'_>,
    trust: &TrustStore,
) -> Result<(), McpAuthorizationError>
```

**Checks Performed:**
1. Non-empty ID
2. Approval exists in trust store
3. Trust level is not Blocked/Unknown
4. Version matches approved version
5. Requested permissions are subset of approved permissions

**Test Coverage:** 6 unit tests verify all denial paths.

#### 6.4.2 Permissions (`mcp/permissions.rs`)

**Status:** COMPLETE

**Permission Categories:**
- Network (bool)
- Filesystem (Vec<String>)
- Environment (Vec<String>)
- Process (bool)
- Secrets (Vec<String>)

**Validation Rules:**
- Environment names must match `[A-Z_][A-Z0-9_]*`
- Blocked environment vars: PATH, LD_PRELOAD, PYTHONPATH, etc.
- Secrets require matching environment permission

#### 6.4.3 Sandbox (`mcp/sandbox.rs`)

**Status:** COMPLETE (Linux/macOS/Windows)

**Linux (bwrap):**
- Unshare user/pid/ipc/uts
- Drop all capabilities
- Resource limits (AS, CPU, nproc, nofile)
- Network isolation (--unshare-net unless permitted)
- Bind mounts for project root and allowed paths

**macOS (sandbox-exec):**
- Profile-based restrictions
- Read-only system paths
- Write access to project root only

**Windows:**
- Job Object for resource limits
- No path restrictions implemented

**ISSUE:** Unsupported platforms return error rather than failing open—CORRECT behavior.

#### 6.4.4 Trust Store (`mcp/trust.rs`, `mcp/trust_store.rs`)

**Status:** COMPLETE

**Trust Levels:**
- Trusted
- Reviewed
- Unknown (default)
- Blocked

**Persistence:** JSON file in `~/.agent-workspace-hub/trust.json`

**FAIL-CLOSED:** Corrupted trust file returns `Err`, not empty store.

### 6.5 Services Layer

#### 6.5.1 Files Service (`services/files.rs`)

**Status:** COMPLETE

**Operations:**
- `list(relative)` → Vec<ListEntry>
- `read(relative)` → String
- `write(relative, content)` → ()
- `delete(relative)` → ()
- `rename(from, to)` → ()
- `create_dir(relative)` → ()
- `meta(relative)` → FileMeta
- `search(needle, limit)` → Vec<SearchHit>

**Security:**
- Path validation via `secure_path()`
- Rejects absolute paths
- Rejects `..` traversal
- Atomic writes (temp + rename)

#### 6.5.2 Git Service (`services/git.rs`)

**Status:** COMPLETE

**Operations:**
- `is_repo()` → bool
- `status()` → GitOutput (porcelain)
- `branch()` → GitOutput
- `log(limit)` → GitOutput
- `diff(path?)` → GitOutput
- `diff_staged(path?)` → GitOutput
- `stage(path)` → GitOutput
- `unstage(path)` → GitOutput
- `commit(message)` → GitOutput
- `branches()` → GitOutput
- `push(remote, branch)` → GitOutput
- `pull(remote, branch)` → GitOutput
- `discard_file(path)` → GitOutput (HIGH RISK)
- `hard_reset()` → GitOutput (HIGH RISK)
- `delete_branch(name)` → GitOutput (HIGH RISK)
- `force_push(remote, branch)` → GitOutput (HIGH RISK)
- `clean()` → GitOutput (HIGH RISK)

**Security:**
- All operations use argv form (no shell strings)
- Path validation prevents escape from repo root
- Timeout enforcement (30s default)
- kill_on_drop(true)

#### 6.5.3 Terminal Service (`services/terminal.rs`)

**Status:** COMPLETE

**Operations:**
- `run(program, args)` → ExecOutcome

**Security:**
- Program name cannot contain whitespace (prevents shell injection)
- Args passed as Vec<String> (argv form)
- Timeout enforcement (30s default)
- Output cap (256KB)
- kill_on_drop(true)
- Audit hook available

**ISSUE:** No sandbox enforcement at service level—relies on caller (MCP execution gate) to authorize.

#### 6.5.4 Audit Service (`services/audit.rs`)

**Status:** COMPLETE

**Features:**
- Bounded ring buffer (1000 entries max)
- Allow/deny classification
- Token redaction (16+ char base62 runs)
- Epoch millis timestamps
- Thread-safe (Mutex<VecDeque>)
- Global singleton (OnceLock)

**API:**
- `record_allow(action, subject, detail)`
- `record_deny(action, reason, subject)`
- `recent(limit)` → Vec<AuditEntry>

---

## 7. Complete Code Flow Analysis

### 7.1 Workspace Creation Flow

```
User: awh tui
    ↓
main.rs:185-189
    ↓
tui::run_local(root)
    ↓
tui/backend.rs:LocalBackend::new(root)
    ↓
tokio::runtime::Builder::new_current_thread().enable_all().build()
    ↓
TUI app::run(terminal, backend)
    ↓
tui/screens/dashboard.rs:draw_dashboard
    ↓
backend.dashboard()
    ↓
core/project.rs:ProjectStore::list(&root)
core/workspace.rs:Workspace::new(&root).create_project(name)
    ↓
std::fs::create_dir_all(project_path/.agent)
    ↓
Audit: record_allow("tui_project_create", name, "operator")
```

### 7.2 MCP Tool Call Flow (terminal.run)

```
MCP Client: {"method":"tools/call","params":{"name":"terminal.run","arguments":{"program":"ls","args":["-la"]}}}
    ↓
mcp/server.rs:StdioMcpServer::handle(input)
    ↓
mcp/dispatcher.rs:McpDispatcher::dispatch_strict(input)
    ↓
mcp/dispatcher.rs:dispatch_inner
    ↓
Parse JSON-RPC → RpcRequest
    ↓
Match method == "tools/call"
    ↓
mcp/dispatcher.rs:call_tool(&params)
    ↓
Match tool_name == "terminal.run"
    ↓
Extract program, args from arguments
    ↓
Validate: program non-empty, no whitespace
    ↓
mcp/services/terminal.rs:TerminalService::run(program, args)
    ↓
Audit hook (if configured)
    ↓
tokio::process::Command::new(program).args(args).spawn()
    ↓
tokio::time::timeout(self.timeout, child.wait_with_output())
    ↓
Truncate output to MAX_CAPTURE_BYTES (256KB)
    ↓
Return ExecOutcome { exit_code, timed_out, stdout, stderr, truncated }
    ↓
JSON-RPC response: {"result":{"content":[{"type":"text","text":...}]}}
```

### 7.3 Custom MCP Server Initialization Flow

```
McpDispatcher::new(project_root)
    ↓
CustomMcpRegistry::new(project_root)
    ↓
Load mcps.json from project/.agent/mcps.json
    ↓
For each enabled server config:
    ↓
is_authorized(&cfg, trust_store)
    ↓
Check: approval exists, not blocked, version matches, permissions subset
    ↓
IF AUTHORIZED:
    Match transport:
        Stdio → StdioMcpClient::spawn(&cfg, project_root)
        StreamableHttp → StreamableHttpMcpClient::new(&cfg)
    ↓
client.initialize()
    ↓
Wrap with CircuitBreakerMcpClient
    ↓
Register in ProviderRegistry as CustomMcpProvider
ELSE:
    Skip (fail closed)
```

---

## 8. CLI Analysis

**Strengths:**
- Comprehensive command coverage
- Proper separation of concerns (CLI → Service → Core)
- Trust management commands present
- Skill registry integration

**Weaknesses:**
- No JSON output mode
- No structured errors (just println/eprintln)
- Exit codes not standardized
- No `awh run` command for workflows
- No agent management commands

---

## 9. TUI Analysis

**Strengths:**
- Clean backend abstraction (WorkspaceBackend trait)
- Remote backend support for Control API
- Proper screen state management
- Confirmation dialogs for destructive actions
- Audit log viewer

**Weaknesses:**
- No multi-agent view
- No workflow visualization
- No checkpoint/restore UI
- Settings screen is placeholder
- No resource usage display

---

## 10. CLI vs TUI Differences

| Feature | CLI | TUI | API | MCP | Shared Implementation? |
|---------|-----|-----|-----|-----|------------------------|
| Project List | ✓ (text) | ✓ (list) | ✓ (JSON) | ✗ | YES (ProjectStore) |
| Project Create | ✓ | ✓ | ✓ | ✗ | YES (Workspace::create_project) |
| Project Delete | ✓ | ✓ (confirm) | ✓ | ✗ | YES (fs::remove_dir_all) |
| File Read | ✗ | ✓ | ✓ | ✓ | YES (FilesService) |
| File Write | ✗ | ✓ | ✓ | ✓ | YES (FilesService) |
| Git Status | ✗ | ✓ | ✓ | ✓ | YES (GitService) |
| Git Commit | ✗ | ✓ | ✓ | ✓ | YES (GitService) |
| Terminal Run | ✗ | ✓ (one-shot) | ✓ | ✓ | YES (TerminalService) |
| Context Read | ✗ | ✓ | ✓ | ✓ | YES (ContextStore) |
| Context Write | ✗ | ✓ | ✓ | ✗ | YES (ContextStore) |
| Memory List | ✗ | ✓ | ✓ | ✓ | YES (MemoryStore) |
| Memory Append | ✗ | ✓ | ✓ | ✓ | YES (MemoryStore) |
| Skill List | ✓ | ✓ | ✓ | ✓ | YES (GlobalSkillRegistry) |
| MCP List | ✓ | ✓ (limited) | ✓ | N/A | YES (CustomMcpRegistry) |
| Trust Manage | ✓ | ✗ | ✗ | ✗ | NO (CLI-only) |

**FINDING:** TUI lacks trust management UI—user must drop to CLI for `awh mcp trust/block/revoke`.

---

## 11. API Analysis

**Status:** PARTIAL

**Implemented Routes (`/api/v1/`):**

| Method | Route | Handler | Auth | Rate Limit |
|--------|-------|---------|------|------------|
| GET | /healthz | health | No | No |
| GET | /status | status | Yes | Yes |
| GET | /projects | list_projects | Yes | Yes |
| POST | /projects | create_project | Yes | Yes |
| GET | /projects/{name} | project | Yes | Yes |
| DELETE | /projects/{name} | delete_project | Yes | Yes |
| GET | /files | list_files | Yes | Yes |
| GET | /files/content | read_file | Yes | Yes |
| PUT | /files/content | write_file | Yes | Yes |
| GET | /files/search | search_files | Yes | Yes |
| GET | /files/meta | file_meta | Yes | Yes |
| DELETE | /files/entry | delete_entry | Yes | Yes |
| POST | /files/entry | rename_entry | Yes | Yes |
| PUT | /files/entry | create_dir | Yes | Yes |
| GET | /git/status | git_status | Yes | Yes |
| GET | /git/log | git_log | Yes | Yes |
| GET | /git/diff | git_diff | Yes | Yes |
| GET | /git/branch | git_branch | Yes | Yes |
| GET | /git/branches | git_branches | Yes | Yes |
| POST | /git/push | git_push | Yes | Yes |
| POST | /git/pull | git_pull | Yes | Yes |
| POST | /git/stage | git_stage | Yes | Yes |
| POST | /git/unstage | git_unstage | Yes | Yes |
| POST | /git/commit | git_commit | Yes | Yes |
| POST | /terminal/run | run_command | Yes | Yes |
| GET | /context | read_context | Yes | Yes |
| PUT | /context | write_context | Yes | Yes |
| GET | /memory | list_memory | Yes | Yes |
| POST | /memory | append_memory | Yes | Yes |
| GET | /skills | list_skills | Yes | Yes |
| GET | /skills/project | list_project_skills | Yes | Yes |
| POST | /skills/project | add_project_skill | Yes | Yes |
| DELETE | /skills/project | remove_project_skill | Yes | Yes |
| GET | /mcp | list_mcp | Yes | Yes |
| GET | /audit | audit | Yes | Yes |
| GET | /logs | logs | Yes | Yes |

**Security:**
- Bearer token authentication (constant-time comparison)
- Rate limiting (sliding window, per-client key)
- Timeout (30s)
- Body size limit (1MB)

**MISSING:**
- No agent endpoints
- No workflow endpoints
- No checkpoint endpoints
- No capability/policy endpoints

---

## 12. MCP Analysis

**Status:** COMPLETE (for documented scope)

**Protocol Compliance:**
- JSON-RPC 2.0 ✓
- initialize ✓
- ping ✓
- tools/list ✓
- tools/call ✓
- resources/list ✓
- resources/read ✓
- prompts/list (empty) ✓

**Transports:**
- Stdio ✓
- HTTP SSE ✓
- Streamable HTTP ✓

**Security:**
- Execution gate ✓
- Trust store ✓
- Permission validation ✓
- Sandbox (platform-specific) ✓
- Schema validation ✓
- Circuit breaker ✓
- Audit logging ✓

---

## 13. CLI/TUI/API/MCP Unification

**ASSESSMENT:** WELL-UNIFIED

All interfaces correctly route through shared services:

```
CLI ───┐
TUI ───┤
API ───┼──→ Services (Files, Git, Terminal, Projects) → Core Stores
MCP ───┘
```

**Verified Shared Paths:**
- File operations: FilesService (used by all four)
- Git operations: GitService (used by TUI, API, MCP)
- Terminal: TerminalService (used by TUI, API, MCP)
- Projects: ProjectStore + Workspace (used by CLI, TUI, API)
- Memory: MemoryStore (used by TUI, API, MCP)
- Context: ContextStore (used by TUI, API, MCP)
- Skills: GlobalSkillRegistry (used by CLI, TUI, API, MCP)

**NO DUPLICATION FOUND** in business logic.

---

## 14. Security Architecture

### Security Pipeline (VERIFIED)

```
Identity (Bearer Token for API/MCP remote)
    ↓
Authentication (verify_token with constant-time compare)
    ↓
Authorization (trust store check for MCP servers)
    ↓
Capability (McpPermissions validation)
    ↓
Scope (path validation, environment filtering)
    ↓
Schema Validation (validate_tool_arguments)
    ↓
Sandbox (bwrap/sandbox-exec/Job Object)
    ↓
Resource Limits (RLIMIT_AS, RLIMIT_CPU, etc.)
    ↓
Execution (tokio::process::Command)
    ↓
Audit (audit_allow/audit_deny with token redaction)
```

### Security Boundary Locations

| Entry Point | Security Check Location |
|-------------|------------------------|
| CLI | Manual (operator is trusted) |
| TUI | Manual (operator confirms destructive actions) |
| API | Bearer token + rate limit |
| MCP (custom servers) | Execution gate + trust store |
| MCP (static tools) | Argument validation + path checks |

**CRITICAL FINDING:** Static MCP tools (terminal.run, workspace.write_file) do NOT pass through execution gate at call time. They rely on:
1. Trust check at MCP server initialization (for custom servers)
2. Argument validation
3. Path validation

This is ACCEPTABLE because:
- Custom MCP servers are authorized at spawn time
- Static tools are built-in and trusted
- Operator-initiated actions (CLI/TUI) are explicitly trusted

However, if AI agents gain ability to invoke static tools directly without operator confirmation, additional authorization would be needed.

---

## 15. Threat Model

### Analyzed Threats

| Threat | Attack Path | Affected Code | Severity | Likelihood | Impact | Mitigation | Missing Mitigation |
|--------|-------------|---------------|----------|------------|--------|------------|-------------------|
| Prompt Injection | Malicious skill/MCP → agent → tool call | dispatcher.rs | HIGH | MEDIUM | HIGH | Schema validation, sandbox | No semantic validation |
| Malicious Skills | Crafted SKILL.md → code execution | skills/parser.rs | HIGH | LOW | HIGH | Path validation, SHA-256 | No skill sandbox |
| Malicious MCP Servers | Compromised server → privilege escalation | custom_mcp.rs | HIGH | MEDIUM | HIGH | Trust store, execution gate | No runtime re-validation |
| Command Injection | Shell metacharacters in args | terminal.rs | MEDIUM | LOW | HIGH | Argv form (no shell) | None needed |
| Path Traversal | `../../etc/passwd` in file path | security.rs::secure_path | HIGH | LOW | HIGH | Canonicalization check | None needed |
| Symlink Escape | Symlink to outside workspace | security.rs::secure_destination | MEDIUM | LOW | HIGH | Symlink resolution check | None needed |
| Secret Leakage | Tokens in logs/output | audit.rs::redact_token_like | MEDIUM | MEDIUM | HIGH | Token redaction | No output sanitization |
| Environment Leakage | LD_PRELOAD injection | permissions.rs | HIGH | LOW | HIGH | Blocked env list | None needed |
| Network Abuse | Unrestricted outbound calls | sandbox.rs | MEDIUM | MEDIUM | MEDIUM | --unshare-net default | No egress filtering |
| Privilege Escalation | Capability grant bypass | execution_gate.rs | HIGH | LOW | HIGH | Fail-closed design | None needed |
| Capability Escalation | Permission subset violation | trust.rs::can_enable | HIGH | LOW | HIGH | Subset check | None needed |
| Child-Agent Privilege Inheritance | N/A (no agents yet) | N/A | N/A | N/A | N/A | N/A | Agent model missing |
| Memory Poisoning | Malicious context items | context/engine.rs | MEDIUM | MEDIUM | MEDIUM | Budget limits | No integrity check |
| Malicious Workspace Files | Compromised AGENTS.md | core/context.rs | LOW | LOW | MEDIUM | Read-only | No validation |
| Connector Compromise | Fake connector → credential theft | connectors.rs | MEDIUM | MEDIUM | HIGH | No secret storage | No OAuth validation |
| Runaway Agents | N/A (no agents yet) | N/A | N/A | N/A | N/A | N/A | No agent resource governor |
| Resource Exhaustion | Fork bomb via terminal.run | terminal.rs | MEDIUM | LOW | MEDIUM | RLIMIT_NPROC | No per-session limits |
| TOCTOU | Race in file operations | files.rs | LOW | LOW | MEDIUM | Atomic writes | No file locking |
| Confused Deputy | MCP server acts on behalf of user | execution_gate.rs | MEDIUM | MEDIUM | HIGH | Explicit trust required | No delegation model |

### High-Priority Mitigations Needed

1. **Agent Isolation** (P0): Implement agent runtime with per-agent capability grants
2. **Output Sanitization** (P1): Redact secrets from tool outputs, not just audit logs
3. **Egress Filtering** (P2): Implement network allow-list for MCP servers
4. **Skill Sandboxing** (P2): Apply same sandbox to skill execution as MCP servers

---

## 16. Data Model Analysis

### Implemented Models

| Model | File | Fields | Persisted | Validated | Used |
|-------|------|--------|-----------|-----------|------|
| AuditEntry | services/audit.rs | ts_ms, kind, action, subject, detail | In-memory ring | Token redaction | ✓ |
| McpPermissions | mcp/permissions.rs | network, filesystem, environment, process, secrets | JSON (trust store) | validate() | ✓ |
| McpApproval | mcp/trust.rs | id, level, approved_permissions, approved_version | JSON | validate() | ✓ |
| TrustStore | mcp/trust.rs | approvals: Vec<McpApproval> | JSON | N/A | ✓ |
| CustomMcpServerConfig | mcp/custom_mcp.rs | id, name, transport, command, args, url, env, permissions, enabled | JSON | validate() | ✓ |
| GlobalMcpEntry | mcp/global_mcp.rs | config, version, source | JSON | N/A | ✓ |
| Skill | skills/model.rs | name, description, version, trust | Directory (SKILL.md) | parse_skill() | ✓ |
| MemoryEntry | models/memory.rs | timestamp, content | JSONL | N/A | ✓ |
| Task | models/task.rs | id, title, description, status, priority, assignee, tags, created_at, updated_at | JSON | N/A | ✓ |
| Project | models/project.rs | name, path | Directory structure | validate_project_name() | ✓ |
| ContextItem | context/item.rs | id, content, source, relevance, priority, scope, protected, offloaded | JSON | is_valid_item_id() | ✓ |
| Connector | mcp/connectors.rs | id, name, provider, auth, scopes, enabled | JSON | N/A | ✓ |
| ExecOutcome | services/terminal.rs | exit_code, timed_out, stdout, stderr, truncated | N/A | N/A | ✓ |
| GitOutput | services/git.rs | stdout, stderr, exit_code | N/A | N/A | ✓ |
| ListEntry | services/files.rs | name, path, is_dir, size, modified | N/A | N/A | ✓ |
| SearchHit | services/files.rs | path, line_number, content | N/A | N/A | ✓ |

### Missing Models (vs Target)

| Model | Purpose | Priority |
|-------|---------|----------|
| Agent | Agent identity, capabilities, runtime state | P0 |
| AgentSession | Active agent session tracking | P0 |
| AgentRuntime | Agent execution environment | P0 |
| CapabilityGrant | Per-agent capability assignments | P0 |
| Policy | Authorization policies | P0 |
| ApprovalRequest | Human-in-loop approval requests | P1 |
| TaskDependency | Task DAG edges | P1 |
| Workflow | Multi-task orchestration | P1 |
| Event | Structured event stream | P1 |
| Checkpoint | State snapshots for recovery | P1 |
| Snapshot | Workspace state capture | P2 |
| Artifact | Build/test outputs | P2 |
| ArtifactProvenance | Artifact lineage tracking | P2 |
| ResourceBudget | Per-agent resource allocation | P1 |
| ModelRoute | Model selection/routing rules | P1 |

---

## 17. State Machine Analysis

### Task Status (models/task.rs)

```rust
pub enum TaskStatus {
    Todo,
    InProgress,
    Blocked,
    Done,
}
```

**Transitions:** Any → Any (no validation)

**MISSING:**
- Invalid transition detection
- Persistence on transition
- Crash recovery

### MCP Trust Level (mcp/trust.rs)

```rust
pub enum TrustLevel {
    Trusted,
    Reviewed,
    Unknown,  // Default
    Blocked,
}
```

**Transitions:** Manual (via CLI commands)

**VALIDATION:** ✓ Fail-closed default (Unknown)

### Context Item State (context/item.rs)

```rust
pub enum ContextState {
    Active,
    Offloaded,
    Archived,
}
```

**Transitions:**
- Active → Offloaded (context.offload)
- Offloaded → Active (context.restore)
- Active → Archived (policy decision)

**VALIDATION:** ✓ Protected items cannot be offloaded

### Git High-Risk Operations (services/git.rs)

```rust
pub enum HighRiskGitOp {
    HardReset,
    Clean,
    ForcePush,
    BranchDelete,
    DiscardFile,
}
```

**Usage:** Documentation only—no enforcement layer

**MISSING:** TUI confirmation dialog uses this enum but no centralized enforcement

---

## 18. Persistence Analysis

### Storage Formats

| Store | Format | Location | Locking | Atomicity | Migration |
|-------|--------|----------|---------|-----------|-----------|
| ProjectStore | JSON array | .agent/projects.json | StoreLock (advisory) | temp+rename | No |
| MemoryStore | JSONL (append) | .agent/memory.jsonl | StoreLock | append | No |
| TaskStore | JSON object | .agent/tasks.json | StoreLock | temp+rename | No |
| CustomMcpRegistry | JSON array | .agent/mcps.json | StoreLock | temp+rename | No |
| GlobalMcpRegistry | JSON array | ~/.agent-workspace-hub/mcps.json | StoreLock | temp+rename | No |
| TrustStore | JSON object | ~/.agent-workspace-hub/trust.json | None | temp+rename | No |
| SkillStore | Directory + SKILL.md | ~/.agent-workspace-hub/skills/{name}/ | None | N/A | No |
| ContextStore | Markdown | .agent/context.md | None | atomic_write | No |
| ConnectorStore | JSON array | .agent/connectors.json | StoreLock | temp+rename | No |
| AuditLog | In-memory ring | N/A | Mutex | N/A | N/A |

### Atomic Write Implementation (mcp/security.rs)

```rust
pub fn atomic_write(base: &Path, relative: &str, content: &[u8]) -> Result<()> {
    let destination = secure_destination(base, relative)?;
    let temp = destination.with_extension("tmp");
    fs::write(&temp, content)?;
    fs::rename(&temp, &destination)?;  // Atomic on POSIX
}
```

**ISSUES:**
- No versioning/schema migration
- No corruption recovery (except trust store fails closed)
- No backup mechanism
- Concurrent writes rely on advisory locks (not enforced across processes that don't use StoreLock)

### StoreLock Implementation (mcp/store_lock.rs)

```rust
pub struct StoreLock {
    lock_path: PathBuf,
}

impl StoreLock {
    pub fn acquire(target: &Path) -> Result<Self> {
        let lock_path = target.with_extension("lock");
        // Check staleness via mtime (>1 hour = stale)
        // Create lock file with PID
        // Return guard that deletes on drop
    }
}
```

**LIMITATIONS:**
- Advisory only (cooperative)
- Staleness threshold hardcoded (1 hour)
- No retry/backoff logic

---

## 19. Concurrency Analysis

### Tokio Usage

**Runtimes:**
- `StdioMcpServer`: Dedicated Runtime (lines 22-28)
- `LocalBackend`: Current-thread runtime (tui/backend.rs:142-145)
- `serve_control_api`: New runtime (main.rs:635-641)

**Pattern:** Each long-running service creates its own runtime—appropriate for binary structure.

### Synchronization Primitives

| Type | Location | Purpose | Risk |
|------|----------|---------|------|
| Arc<RwLock<ProviderRegistry>> | mcp/dispatcher.rs | Dynamic provider registry | LOW (read-heavy) |
| Mutex<VecDeque<AuditEntry>> | services/audit.rs | Audit ring buffer | LOW (fast operations) |
| OnceLock<AuditLog> | services/audit.rs | Global audit singleton | NONE |
| RwLock<ContextEngine> | context/engine.rs | Context items | MEDIUM (writes block reads) |

### Potential Issues

**IDENTIFIED:**

1. **Blocking inside async** (mcp/dispatcher.rs:189-191):
```rust
let rt = tokio::runtime::Runtime::new()?;
let client = rt.block_on(StdioMcpClient::spawn(&cfg, project_root))?;
```
This creates a nested runtime inside an async context—works but inefficient.

2. **Unbounded channels**: None found (good)

3. **Task leaks**: None found (all spawns are bounded)

4. **Orphan processes**: Mitigated via `kill_on_drop(true)` in TerminalService and GitService

5. **Cross-process races**: Advisory locks only—non-cooperating processes can corrupt stores

**RECOMMENDATION:** Replace nested runtime creation with `tokio::spawn` where possible.

---

## 20. Error Handling Analysis

### Error Types

| Module | Error Type | Pattern |
|--------|------------|---------|
| mcp/error.rs | McpAuthorizationError | thiserror enum |
| mcp/dispatcher.rs | DispatchError | Custom struct with JSON-RPC codes |
| api/control.rs | ApiError | Custom struct with StatusCode |
| services/* | anyhow::Result | Propagation with context |

### unwrap()/expect() Usage

**Production Code (non-test):**

| Location | Line | Justification | Risk |
|----------|------|---------------|------|
| tui/backend.rs:145 | `.expect("tokio runtime")` | Runtime build never fails with current_thread | LOW |
| mcp/http.rs:298 | `.expect("retry-after is numeric")` | Header guaranteed numeric by spec | LOW |
| mcp/http.rs:478/484 | `.expect("signal handler")` | Signal installation rarely fails | LOW |
| mcp/store_lock.rs:252-253 | `.expect("open/set_times")` | File ops after successful acquire | MEDIUM |
| mcp/github.rs:717/740/753 | `.expect("git ...")` | Test code only | N/A |
| mcp/dispatcher.rs:1882-1894 | `.expect("git ...")` | Test code only | N/A |
| mcp/config.rs:130 | `.expect("static reqwest client")` | Static client build is infallible | LOW |
| tui/screens/git.rs:302-325 | `.expect("git ...")` | Test code only | N/A |
| tui/screens/git.rs:431 | `.expect("push must report")` | Test assertion | N/A |

**ASSESSMENT:** Acceptable usage—all either test code or genuinely infallible operations.

### Panic Handlers

None installed—default Rust panic behavior (abort + backtrace with RUST_BACKTRACE).

**RECOMMENDATION:** Install panic hook for structured logging in production builds.

---

## 21. Performance Analysis

### Startup

**Measured Components:**
- Binary size: ~10-20MB (estimated, not measured)
- Initial filesystem scan: Minimal (only .agent directory)
- MCP server spawning: Deferred until first use
- Context engine: Opt-in (disabled by default)

**BOTTLENECK:** None identified for typical workspaces.

### Memory

**Bounded Structures:**
- AuditLog: 1000 entries max (~100KB)
- ContextEngine: MAX_ACTIVE_ITEMS = 5000 (configurable)
- Token budget: Enforced by ContextBudget

**Unbounded:**
- ProviderRegistry: Grows with connected MCP servers
- Skill cache: No explicit limit

**RECOMMENDATION:** Add LRU cache for skill lookups.

### CPU

**Hot Paths:**
1. `McpDispatcher::dispatch()` — JSON parsing + routing
2. `FilesService::search()` — Recursive directory walk
3. `ContextEngine::assemble()` — Scoring + selection

**Optimization Opportunities:**
1. Cache directory listings (invalidate on write)
2. Parallelize file search (tokio::spawn per subdirectory)
3. Pre-compute context item scores (update incrementally)

### Filesystem

**Operations:**
- Read: Direct (no caching)
- Write: Atomic (temp + rename)
- Search: Recursive walk (O(n))

**ISSUE:** No read caching—repeated reads hit disk.

### Large Workspace Handling

**Tested:** Not explicitly tested with >10K files.

**EXPECTED BEHAVIOR:**
- File search may be slow (linear scan)
- Directory listing fine (single readdir)
- Git operations scale with repo size (git handles efficiently)

---

## 22. Memory Analysis

### Allocation Patterns

**Cloning:**
- `String` cloning prevalent (acceptable for CLI/TUI)
- `Arc` used for shared state (ProviderRegistry, SkillMcp, etc.)

**Escaped References:**
- Minimal (most data owned locally)

**Stack vs Heap:**
- Large structs boxed or Arc'd
- Small structs copied

### Potential Leaks

**IDENTIFIED:**
- None detected (Rust ownership prevents most leaks)

**Caveats:**
- Tokio tasks cancelled on runtime drop (cleanup guaranteed)
- File handles closed on drop (RAII)
- Child processes killed on drop (kill_on_drop)

---

## 23. Context Engine Analysis

**Status:** IMPLEMENTED BUT OPT-IN

### Features

| Feature | Implemented | Enabled By Default | Notes |
|---------|-------------|-------------------|-------|
| Item management | ✓ | N/A | insert/get/remove/search |
| Token budget | ✓ | N/A | Configurable limits |
| Scoring | ✓ | N/A | Relevance + recency |
| Selection | ✓ | N/A | Budget-constrained |
| Offloading | ✓ | No | AWH_CONTEXT_AUTO_OFFLOAD |
| Compression | ✓ | No | AWH_CONTEXT_AUTO_COMPRESS |
| Protection | ✓ | N/A | protect/unprotect |
| Snapshots | ✓ | N/A | Save/restore state |
| Memory extraction | ✓ | No | AWH_CONTEXT_MEMORY_ENABLED |

### Configuration

```bash
AWH_CONTEXT_ENABLED=true/false
AWH_CONTEXT_MAX_INPUT_TOKENS=8192
AWH_CONTEXT_RESERVED_OUTPUT_TOKENS=2048
AWH_CONTEXT_SAFETY_MARGIN_TOKENS=512
AWH_CONTEXT_AUTO_OFFLOAD=true/false
AWH_CONTEXT_AUTO_COMPRESS=true/false
AWH_CONTEXT_MEMORY_ENABLED=true/false
```

### Integration

**MCP Tools:**
- context.status
- context.insert
- context.get
- context.remove
- context.search
- context.optimize
- context.assemble
- context.protect
- context.unprotect
- context.offload
- context.restore

**MISSING:**
- Automatic context assembly for agent runs (no agent runtime)
- Integration with model router (no router)

---

## 24. Skills Analysis

**Status:** COMPLETE

### Skill Structure

```
~/.agent-workspace-hub/skills/{name}/
├── SKILL.md          # Metadata + instructions
├── tools/            # Optional tool scripts
└── examples/         # Optional examples
```

### Operations

| Operation | CLI | TUI | API | MCP |
|-----------|-----|-----|-----|-----|
| Create | ✓ | ✗ | ✗ | ✗ |
| List (global) | ✓ | ✓ | ✓ | ✓ |
| List (project) | ✓ | ✓ | ✓ | ✓ |
| Read | ✓ | ✓ | ✗ | ✓ |
| Add (project ref) | ✓ | ✓ | ✓ | ✓ |
| Remove (project ref) | ✓ | ✓ | ✓ | ✓ |
| Search | ✓ | ✗ | ✗ | ✓ |
| Install (from registry) | ✓ | ✗ | ✗ | ✗ |
| Uninstall | ✓ | ✗ | ✗ | ✗ |

### Trust Model

**Levels:**
- Verified (SHA-256 pinned)
- Trusted (manual approval)
- Unknown (default)

**MISSING:**
- No skill sandboxing (skills run with full user privileges)
- No skill capability declaration

---

## 25. Connector Analysis

**Status:** PARTIAL

### Implemented Providers

| Provider | Type | Auth | Tools |
|----------|------|------|-------|
| Composio | OAuth/API Key | OAuth flow | Dynamic |
| GitHub | PAT | GITHUB_TOKEN | Static (github.*) |
| Custom MCP | Stdio/HTTP | Env vars | Dynamic |

### Connector Model

```rust
pub struct Connector {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub auth: AuthMethod,
    pub scopes: Vec<String>,
    pub enabled: bool,
}
```

**MISSING:**
- No OAuth callback handling (Composio link returns URL for manual completion)
- No credential storage (env vars only)
- No connector health checks
- No automatic token refresh

---

## 26. Git Analysis

**Status:** COMPLETE

### Operations Coverage

| Operation | Implemented | Safe | Tested |
|-----------|-------------|------|--------|
| Status | ✓ | ✓ | ✓ |
| Diff | ✓ | ✓ | ✓ |
| Stage | ✓ | ✓ | ✓ |
| Unstage | ✓ | ✓ | ✓ |
| Commit | ✓ | ✓ | ✓ |
| Branch | ✓ | ✓ | ✓ |
| Push | ✓ | ✓ | ✓ |
| Pull | ✓ | ✓ | ✓ |
| Hard Reset | ✓ | ⚠️ (high-risk) | ✓ |
| Clean | ✓ | ⚠️ (high-risk) | ✓ |
| Force Push | ✓ | ⚠️ (high-risk) | ✓ |
| Branch Delete | ✓ | ⚠️ (high-risk) | ✓ |
| Discard File | ✓ | ⚠️ (high-risk) | ✓ |

**Safety Features:**
- Argv form (no shell injection)
- Path validation (no escape from repo)
- Timeout (30s)
- kill_on_drop

**MISSING:**
- No worktree management
- No merge conflict handling
- No rebase/cherry-pick

---

## 27. Configuration Analysis

**Status:** PARTIAL

### Configuration Layers

| Layer | Source | Precedence |
|-------|--------|------------|
| Defaults | Compiled-in | Lowest |
| Config file | ~/.agent-workspace-hub/config.toml | Medium |
| Environment | AWH_* env vars | Highest |

### Configurable Values

| Setting | Env Var | Default |
|---------|---------|---------|
| Context enabled | AWH_CONTEXT_ENABLED | true |
| Context budget | AWH_CONTEXT_MAX_INPUT_TOKENS | 8192 |
| Sandbox enabled | AWH_SANDBOX_ENABLED | true |
| Bubblewrap path | AWH_BWRAP | "bwrap" |
| API key | AWH_API_KEY | (required for serve) |
| Trust store path | (hardcoded) | ~/.agent-workspace-hub/trust.json |

**MISSING:**
- No config file parsing implemented (only env vars)
- No per-project configuration
- No configuration validation

---

## 28. Observability Analysis

**Status:** PARTIAL

### Implemented

| Type | Implementation | Queryable | Exportable |
|------|----------------|-----------|------------|
| Audit Log | In-memory ring (1000 entries) | /api/v1/audit, TUI Logs screen | No |
| Error Messages | Structured (ApiError, DispatchError) | HTTP JSON, MCP JSON-RPC | No |
| Timestamps | Epoch millis | Yes | No |

### Missing

| Type | Priority |
|------|----------|
| Structured logging (tracing) | P1 |
| Metrics (Prometheus) | P2 |
| Distributed tracing (OpenTelemetry) | P2 |
| Request IDs | P1 |
| Agent IDs | P0 (no agents yet) |
| Task IDs | P1 |
| Correlation IDs | P1 |
| Log persistence | P2 |
| Log export | P2 |

### Audit Log Quality

**Strengths:**
- Allow/deny classification
- Token redaction
- Bounded size
- Thread-safe

**Weaknesses:**
- Volatile (lost on restart)
- No query language
- No filtering/aggregation
- No correlation with external systems

---

## 29. Testing Analysis

**Status:** ADEQUATE FOR CURRENT SCOPE

### Test Inventory

| File | Lines | Coverage |
|------|-------|----------|
| tests/architecture.rs | ~100 | Architecture assertions |
| tests/mcp_http.rs | ~450 | HTTP transport |
| tests/mcp_sandbox.rs | ~80 | Sandbox enforcement |
| tests/mcp_security.rs | ~200 | Security boundary |
| tests/mcp_server.rs | ~220 | Server behavior |

### Unit Tests (Inline)

| Module | Test Count | Quality |
|--------|------------|---------|
| mcp/execution_gate.rs | 5 | Excellent (all denial paths) |
| mcp/permissions.rs | 7 | Excellent |
| mcp/trust.rs | 0 (integration via execution_gate) | N/A |
| mcp/security.rs | ~15 | Excellent (property-based) |
| mcp/schema.rs | ~10 | Excellent (property-based) |
| mcp/audit.rs | 1 | Basic |
| services/terminal.rs | 5 | Good |
| services/git.rs | 4 | Good |
| services/audit.rs | 5 | Excellent |
| services/files.rs | 0 | MISSING |
| services/projects.rs | 0 | MISSING |
| context/*.rs | ~20 | Good |
| skills/*.rs | ~10 | Good |

### Missing Tests

| Area | Priority |
|------|----------|
| FilesService integration | P1 |
| ProjectsService integration | P1 |
| End-to-end CLI flows | P1 |
| TUI screen interactions | P2 |
| Control API integration | P1 |
| MCP client interoperability | P1 |
| Crash/recovery scenarios | P1 |
| Concurrent access | P2 |
| Large workspace performance | P2 |

---

## 30. CI/CD Analysis

**Status:** MINIMAL

### GitHub Workflows

Not inspected in detail (no .github/workflows visible in inventory).

### Build System

```bash
cargo build --release
cargo test
cargo clippy
cargo fmt --check
```

### Release Process

**Documented (docs/release.md):**
- Version bump in Cargo.toml
- Changelog update
- Tag creation
- Binary builds (cross-platform)
- Checksum generation

**MISSING:**
- No automated releases observed
- No cross-compilation setup verified
- No install script tested

---

## 31. Cross-Platform Analysis

### Platform Support

| Platform | Status | Sandbox | Notes |
|----------|--------|---------|-------|
| Linux | ✓ | bwrap | Full support |
| macOS | ✓ | sandbox-exec | Full support |
| Windows | ✓ | Job Object | No path restrictions |
| Android/Termux | ✗ | N/A | Not tested |
| FreeBSD/OpenBSD | ✗ | N/A | Unsupported |

### Conditional Compilation

```rust
#[cfg(target_os = "linux")]  // bwrap
#[cfg(target_os = "macos")]  // sandbox-exec
#[cfg(windows)]              // Job Object
#[cfg(all(not(linux), not(macos), not(windows)))]  // Error
```

### Path Handling

**CORRECT:**
- Uses `PathBuf` throughout
- Canonicalization for security checks
- No hardcoded `/tmp` (uses std::env::temp_dir)

**ISSUES:**
- Home directory resolution may fail on Windows (uses `dirs` crate)
- Symlink handling platform-dependent

---

## 32. Dead Code Analysis

### Identified Dead Code

| Location | Code | Reason |
|----------|------|--------|
| main.rs:612 | `McpCommand::Serve { .. } => unreachable!()` | Serve handled in separate function |
| mcp/*.rs | Various test-only functions | Marked `#[cfg(test)]` appropriately |

### Potentially Unused

| Location | Code | Assessment |
|----------|------|------------|
| context/planner.rs | Planning logic | Used by context.optimize |
| context/compressor.rs | Compression | Opt-in, may be unused |
| mcp/sse.rs | SSE session management | Used by HTTP transport |
| mcp/circuit_breaker.rs | Circuit breaker | Used for dynamic providers |

**ASSESSMENT:** Minimal dead code—repository is well-maintained.

---

## 33. Duplication Analysis

### Duplicated Logic

**NONE FOUND** in business logic.

All interfaces correctly delegate to shared services.

### Near-Duplication

| Similar Code | Locations | Recommendation |
|--------------|-----------|----------------|
| Trust store loading | mcp/cli_trust.rs, mcp/dispatcher.rs | Extract to common function |
| Project path resolution | core/workspace.rs, tui/backend.rs | Already shared (Workspace::project_path) |
| Audit recording | Multiple call sites | Already centralized (audit_allow/audit_deny) |

---

## 34. Documentation vs Code

### Claim Verification

| Claim | Documentation Says | Code Actually Does | Status |
|-------|-------------------|-------------------|--------|
| Single binary | "Single Rust binary" | ✓ One binary (awh) | TRUE |
| Transport-agnostic MCP | "Both transports share dispatcher" | ✓ McpDispatcher used by stdio + HTTP | TRUE |
| Security boundary | "Execution gate for all tool calls" | ⚠️ Only for custom MCP servers, not static tools | PARTIAL |
| Sandbox enforcement | "bwrap on Linux" | ✓ Implemented + tested | TRUE |
| Audit logging | "Structured allow/deny events" | ✓ Implemented with token redaction | TRUE |
| Agent runtime | "Multi-agent orchestration" | ✗ No agent model exists | FALSE |
| Task DAG | "Task dependencies" | ✗ Flat task store only | FALSE |
| Checkpoints | "Crash recovery" | ✗ No checkpoint system | FALSE |
| Capability security | "Per-agent capabilities" | ✗ No capability model | FALSE |
| Model router | "Intelligent model selection" | ✗ No router exists | FALSE |

### Undocumented Features

| Feature | Implemented | Documented |
|---------|-------------|------------|
| Context Engine | ✓ | Partially |
| Circuit Breaker | ✓ | No |
| Rate Limiting (API) | ✓ | Yes |
| Token Redaction (Audit) | ✓ | Yes |
| GitHub Integration | ✓ | Yes |
| Composio Integration | ✓ | Yes |
| Community MCP Registry | ✓ | Yes |

---

## 35. Feature Completeness Matrix

|#|Feature|Current Code|Entry Points|Core Flow|CLI|TUI|API|MCP|Security|Persistence|Tests|Status|Priority|
|-|-------|------------|------------|---------|---|---|---|---|--------|-----------|-----|------|--------|
|1|Agent Registry|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P0|
|2|Agent Runtime|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P0|
|3|Agent Lifecycle|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P0|
|4|Agent MCP Isolation|⚠️|dispatcher.rs|Trust at init|✗|✗|✗|✓|Partial|N/A|✓|PARTIAL|P0|
|5|Capability Security|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P0|
|6|Tool Broker|⚠️|McpDispatcher|Direct dispatch|✗|✗|✗|✓|Partial|N/A|✓|PARTIAL|P1|
|7|Policy Engine|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P0|
|8|Approval Engine|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P0|
|9|Human-in-Loop|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|10|Message Bus|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|11|Multi-agent Orchestration|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|12|Task DAG|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|13|Scheduler|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|14|Events|⚠️|AuditLog|Allow/deny only|✗|✓|✓|✓|N/A|In-memory|✓|PARTIAL|P1|
|15|Checkpoints|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|16|Recovery|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|17|Memory Scopes|⚠️|MemoryScope enum|Limited enforcement|✗|✓|✓|✓|Minimal|✓|✓|PARTIAL|P2|
|18|Context Engine|✓|ContextEngine|Full implementation|✗|✓|✓|✓|Opt-in|✓|✓|COMPLETE|P2|
|19|Skills 2.0|⚠️|GlobalSkillRegistry|Basic install/ref|✓|✓|✓|✓|Minimal|✓|✓|PARTIAL|P2|
|20|MCP Manager|✓|McpDispatcher + registries|Full implementation|✓|✓|✓|✓|✓|✓|✓|COMPLETE|P1|
|21|Resource Governor|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|22|Cancellation|⚠️|kill_on_drop|Process-level only|✗|✗|✗|✓|N/A|N/A|✓|PARTIAL|P2|
|23|Retry System|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P2|
|24|Failure Classification|⚠️|Error types|No categorization|✗|✗|✓|✓|N/A|N/A|✓|PARTIAL|P2|
|25|Model Router|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|26|Workspace Snapshots|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P2|
|27|Git Worktrees|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P2|
|28|Artifacts|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P2|
|29|Provenance|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P2|
|30|Observability|⚠️|AuditLog|Audit only|✗|✓|✓|✗|N/A|In-memory|✓|PARTIAL|P1|
|31|Connectors|⚠️|ConnectorsMcp|Metadata only|✗|✗|✗|✓|Minimal|✓|✓|PARTIAL|P2|
|32|Secret Management|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|33|Prompt Injection Defense|⚠️|Schema validation|Syntactic only|✗|✗|✗|✓|Partial|N/A|✓|PARTIAL|P1|
|34|Policy Hierarchy|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|35|CLI-first Interface|✓|clap|Full implementation|✓|N/A|N/A|N/A|N/A|N/A|✗|COMPLETE|P1|
|36|JSON/JSONL Output|⚠️|API only|Not in CLI|✗|✗|✓|✓|N/A|N/A|✗|PARTIAL|P2|
|37|Unified Errors|⚠️|ApiError, DispatchError|Not consistent|✗|✗|✓|✓|N/A|N/A|✓|PARTIAL|P2|
|38|Doctor|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P2|
|39|Layered Configuration|⚠️|Env vars only|No config file|✓|✗|✗|✗|N/A|N/A|✗|PARTIAL|P2|
|40|Concurrency Architecture|⚠️|Tokio|Per-service runtimes|✗|✗|✗|✓|N/A|N/A|✓|PARTIAL|P2|
|41|Testing|⚠️|Unit tests|Good coverage, missing integration|✓|✗|✗|✓|N/A|N/A|✓|PARTIAL|P1|
|42|TUI|✓|ratatui|14 screens|N/A|✓|N/A|N/A|N/A|N/A|✗|COMPLETE|P1|
|43|"awh run"|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P1|
|44|Agent Teams|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P2|
|45|Controlled Self-Improvement|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P2|
|46|Extensibility|⚠️|Skills + MCP|Skills need sandbox|✓|✓|✓|✓|Partial|✓|✓|PARTIAL|P2|
|47|Compatibility|⚠️|Linux/macOS/Windows|Android unsupported|✓|✓|✓|✓|N/A|N/A|✗|PARTIAL|P2|
|48|Documentation|⚠️|docs/*.md|Incomplete vs code|N/A|N/A|N/A|N/A|N/A|N/A|N/A|PARTIAL|P2|
|49|Android/Termux|✗|N/A|N/A|✗|✗|✗|✗|N/A|N/A|N/A|MISSING|P3|
|50|Remote/Headless|⚠️|Control API + TUI remote|No SSH daemon|✗|✓|✓|✗|✓|N/A|✗|PARTIAL|P2|

---

## 36. Missing Features

### P0 (Security/Architecture Critical)

1. **Agent Registry** — No model for agent identity, capabilities, or lifecycle
2. **Agent Runtime** — No substrate for spawning/managing agents
3. **Capability Security Layer** — No capability grants or enforcement
4. **Policy Engine** — No authorization policies beyond trust store

### P1 (Production Blocking)

5. **Task DAG** — Tasks are flat, no dependencies or workflows
6. **Scheduler** — No task scheduling or prioritization
7. **Message Bus** — No inter-component communication substrate
8. **Approval Engine** — No human-in-loop workflow
9. **Checkpoints** — No crash recovery
10. **Resource Governor** — No per-agent resource limits
11. **Model Router** — No intelligent model selection
12. **Secret Management** — No secure credential storage
13. **awh run** — No workflow execution command

### P2 (Important Quality Improvements)

14. **Workspace Snapshots** — No point-in-time captures
15. **Git Worktrees** — No parallel worktree support
16. **Artifacts** — No build/test output tracking
17. **Provenance** — No lineage tracking
18. **Retry System** — No automatic retry logic
19. **Doctor** — No diagnostic/self-healing command
20. **JSON Output (CLI)** — No structured CLI output
21. **Configuration File** — Only env vars supported
22. **Integration Tests** — Missing end-to-end coverage

---

## 37. Broken Features

**NONE IDENTIFIED** — All implemented features appear functional.

---

## 38. Unsafe Features

| Feature | Issue | Risk | Recommendation |
|---------|-------|------|----------------|
| Static MCP tools | No runtime authorization | MEDIUM | Add capability check per invocation |
| Skills | No sandboxing | MEDIUM | Apply same sandbox as MCP servers |
| Audit log | Volatile (in-memory) | LOW | Add optional file persistence |
| Store locks | Advisory only | LOW | Document cooperative requirement |
| Windows sandbox | No path restrictions | MEDIUM | Implement AppContainer or similar |

---

## 39. Implemented-but-Unwired Features

| Feature | Implementation | Missing Connection |
|---------|----------------|-------------------|
| Context Engine | Full implementation | No agent integration (no agents exist) |
| Circuit Breaker | Implemented for dynamic providers | Not used for static tools |
| Offload/Restore | ContextEngine supports it | No automatic triggering |
| Compression | ContextCompressor exists | Not integrated into pipeline |

---

## 40. Architectural Debt

### Technical Debt Items

| Debt | Location | Impact | Effort to Fix |
|------|----------|--------|---------------|
| Nested runtimes | mcp/dispatcher.rs:189-191 | Performance | MEDIUM |
| No config file | Multiple locations | Usability | LOW |
| Advisory locks | mcp/store_lock.rs | Correctness | MEDIUM |
| No schema migration | All stores | Maintainability | HIGH |
| In-memory audit | services/audit.rs | Observability | LOW |
| No panic hook | N/A | Debugging | LOW |

### Structural Debt

**MINIMAL** — Code is well-organized with clear layer boundaries.

---

## 41. Security Risks

### High-Priority Risks

| Risk | Severity | Likelihood | Impact | Mitigation |
|------|----------|------------|--------|------------|
| No agent isolation | HIGH | N/A (no agents) | HIGH | Implement agent runtime |
| Static tool bypass | MEDIUM | LOW | HIGH | Add runtime checks |
| Skill execution unsandboxed | MEDIUM | MEDIUM | HIGH | Sandbox skills |
| Secret leakage in output | MEDIUM | MEDIUM | HIGH | Output sanitization |
| Egress unrestricted | MEDIUM | MEDIUM | MEDIUM | Network allow-list |

### Medium-Priority Risks

| Risk | Severity | Likelihood | Impact | Mitigation |
|------|----------|------------|--------|------------|
| Advisory locks | LOW | LOW | MEDIUM | Document + optional flock |
| No schema migration | LOW | LOW | MEDIUM | Add version field |
| Windows path restrictions | MEDIUM | LOW | HIGH | Implement AppContainer |
| No rate limit on stdio | LOW | LOW | MEDIUM | Add line-rate limiting |

---

## 42. Performance Risks

| Risk | Severity | Trigger | Mitigation |
|------|----------|---------|------------|
| Linear file search | MEDIUM | >10K files | Cache + index |
| No read caching | MEDIUM | Repeated reads | LRU cache |
| Context scoring O(n) | LOW | >1000 items | Incremental updates |
| MCP server spawn sync | LOW | Many servers | Async spawn |

---

## 43. Reliability Risks

| Risk | Severity | Trigger | Mitigation |
|------|----------|---------|------------|
| No crash recovery | HIGH | Process crash | Checkpoint system |
| Store corruption | MEDIUM | Disk full / kill | Backup + migration |
| Orphan processes | LOW | Parent crash | kill_on_drop (implemented) |
| Lock staleness | LOW | Crash during write | 1-hour timeout (implemented) |

---

## 44. Top Problems

### P0 Problems

**P0-1: No Agent Model**
- **Affected:** Entire architecture
- **Why:** Cannot implement multi-agent features without agent identity
- **Fix:** Define Agent struct with id, capabilities, state
- **Dependencies:** None
- **Testing:** Unit tests for Agent CRUD

**P0-2: No Capability Security**
- **Affected:** Authorization model
- **Why:** Trust-based model insufficient for fine-grained access
- **Fix:** Implement CapabilityGrant model with enforcement
- **Dependencies:** Agent model
- **Testing:** Security regression tests

**P0-3: No Task DAG**
- **Affected:** Task orchestration
- **Why:** Cannot express dependencies or workflows
- **Fix:** Add TaskDependency model + topological sort
- **Dependencies:** None
- **Testing:** DAG validation tests

### P1 Problems

**P1-1: Static Tools Bypass Execution Gate**
- **Affected:** mcp/dispatcher.rs
- **Why:** terminal.run, workspace.write_file not checked at call time
- **Fix:** Add authorization hook for high-risk tools
- **Dependencies:** Capability model
- **Testing:** Security tests for each tool

**P1-2: No Crash Recovery**
- **Affected:** All persistent stores
- **Why:** Corruption or crash loses state
- **Fix:** Implement checkpoint system
- **Dependencies:** None
- **Testing:** Crash/recovery tests

**P1-3: No Integration Tests**
- **Affected:** CI/CD
- **Why:** Cannot verify end-to-end behavior
- **Fix:** Add CLI + API + MCP integration tests
- **Dependencies:** None
- **Testing:** N/A (this is testing)

### P2 Problems

**P2-1: No Config File**
- **Affected:** Configuration
- **Why:** Env vars inconvenient for complex setups
- **Fix:** Add TOML config file parsing
- **Dependencies:** None
- **Testing:** Config validation tests

**P2-2: No JSON CLI Output**
- **Affected:** CLI usability
- **Why:** Cannot script CLI easily
- **Fix:** Add --json flag to all commands
- **Dependencies:** None
- **Testing:** Output format tests

**P2-3: Skills Unsandboxed**
- **Affected:** skills/
- **Why:** Skills execute with full user privileges
- **Fix:** Apply sandbox to skill tool execution
- **Dependencies:** None
- **Testing:** Sandbox enforcement tests

---

## 45. Top Architectural Decisions

### AD-1: Agent Identity Location

**Options:**
1. In-memory registry (volatile)
2. Persistent store (JSON file)
3. External database (PostgreSQL/SQLite)

**Recommended:** Option 2 (persistent JSON store)
**Reason:** Matches existing architecture, simple to implement
**Tradeoffs:** Limited concurrency vs SQLite
**Migration:** None (new feature)

### AD-2: Tool Broker Design

**Options:**
1. Keep direct dispatch (current)
2. Add capability check layer
3. Full broker with queuing

**Recommended:** Option 2 (add capability check)
**Reason:** Minimal change, addresses security gap
**Tradeoffs:** Adds latency per call
**Migration:** Refactor dispatcher.rs::call_tool

### AD-3: Policy Representation

**Options:**
1. DSL (domain-specific language)
2. JSON/YAML config
3. Code-based (Rust structs)

**Recommended:** Option 2 (JSON/YAML)
**Reason:** Editable by humans, parsable by tools
**Tradeoffs:** Less expressive than DSL
**Migration:** Define Policy schema

### AD-4: Event Persistence

**Options:**
1. Keep in-memory (current)
2. Append-only file (JSONL)
3. Embedded database (SQLite)

**Recommended:** Option 2 (JSONL)
**Reason:** Simple, efficient, matches memory store pattern
**Tradeoffs:** No query language
**Migration:** Add EventStore alongside AuditLog

### AD-5: Agent MCP Isolation

**Options:**
1. Per-agent MCP servers (isolated)
2. Shared MCP with capability filtering
3. Proxy layer with enforcement

**Recommended:** Option 1 (per-agent isolation)
**Reason:** Cleanest security boundary
**Tradeoffs:** More resource usage
**Migration:** Extend McpDispatcher to support per-agent instances

---

## 46. Dependency Graph

### Implementation Order (Derived from Code)

```
1. Identity (Agent model)
   ↓
2. Capabilities (CapabilityGrant)
   ↓
3. Policy (Policy engine)
   ↓
4. Tool Broker Enhancement (capability checks)
   ↓
5. Agent Runtime (spawn/manage agents)
   ↓
6. Agent MCP (per-agent isolation)
   ↓
7. Events (EventStore)
   ↓
8. Message Bus (inter-agent communication)
   ↓
9. Task DAG (dependencies)
   ↓
10. Scheduler (task execution)
    ↓
11. Checkpoints (state snapshots)
    ↓
12. Recovery (crash restoration)
    ↓
13. Resource Governor (per-agent limits)
    ↓
14. Model Router (model selection)
    ↓
15. Artifacts (output tracking)
    ↓
16. Git Worktrees (parallel work)
    ↓
17. Advanced TUI (agent/workflow views)
```

---

## 47. Migration Strategy

### Phase 1: Foundation (Weeks 1-4)

1. Add Agent model + registry
2. Add CapabilityGrant model
3. Add Policy engine
4. Enhance Tool Broker with capability checks

### Phase 2: Agent Runtime (Weeks 5-8)

5. Implement Agent Runtime
6. Add per-agent MCP isolation
7. Add EventStore
8. Add Message Bus

### Phase 3: Task Orchestration (Weeks 9-12)

9. Add Task DAG
10. Add Scheduler
11. Add Checkpoints
12. Add Recovery

### Phase 4: Advanced Features (Weeks 13-16)

13. Add Resource Governor
14. Add Model Router
15. Add Artifacts + Provenance
16. Add Git Worktrees

### Phase 5: Polish (Weeks 17-20)

17. Enhance TUI
18. Add integration tests
19. Add documentation
20. Performance optimization

---

## 48. Implementation Roadmap

### Sprint 1 (Week 1-2)

- [ ] Define Agent struct
- [ ] Implement AgentStore
- [ ] Add CLI commands: `agent list/create/delete`
- [ ] Unit tests

### Sprint 2 (Week 3-4)

- [ ] Define CapabilityGrant struct
- [ ] Implement capability enforcement
- [ ] Modify dispatcher to check capabilities
- [ ] Security tests

### Sprint 3 (Week 5-6)

- [ ] Define Policy struct
- [ ] Implement PolicyEngine
- [ ] Add CLI commands: `policy list/create/apply`
- [ ] Integration tests

### Sprint 4 (Week 7-8)

- [ ] Implement AgentRuntime
- [ ] Add agent spawning
- [ ] Add per-agent MCP instances
- [ ] End-to-end tests

[Continue similarly for remaining sprints...]

---

## 49. Testing Roadmap

### Immediate (Phase 1)

- [ ] FilesService integration tests
- [ ] ProjectsService integration tests
- [ ] CLI end-to-end tests
- [ ] API integration tests

### Short-term (Phase 2)

- [ ] MCP client interoperability tests
- [ ] TUI screen interaction tests
- [ ] Concurrent access tests
- [ ] Crash/recovery tests

### Long-term (Phase 3)

- [ ] Performance benchmarks
- [ ] Security regression suite
- [ ] Fuzzing (schema validation)
- [ ] Chaos testing (concurrent failures)

---

## 50. Production Readiness Score

| Category | Score (0-10) | Justification |
|----------|--------------|---------------|
| Architecture | 7 | Well-layered, missing agent model |
| Security | 7 | Strong boundary, gaps in static tools |
| Reliability | 5 | No crash recovery, advisory locks |
| Performance | 6 | No major bottlenecks, unoptimized |
| CLI | 8 | Comprehensive, missing JSON output |
| TUI | 7 | Functional, missing advanced features |
| API | 6 | Basic CRUD, missing agent endpoints |
| MCP | 9 | Protocol-complete, well-secured |
| Persistence | 5 | JSON files, no migration/versioning |
| Concurrency | 7 | Tokio-based, minor issues |
| Testing | 6 | Good unit coverage, missing integration |
| Observability | 4 | Audit log only, no metrics/traces |
| Documentation | 6 | Incomplete vs code |
| Cross-platform | 7 | Linux/macOS/Windows, no mobile |
| Release System | 4 | Minimal CI/CD |

**Overall Score: 6.1 / 10**

**Assessment:** Production-ready for single-user, single-agent workflows. NOT ready for multi-agent orchestration or enterprise deployment.

---

## 51. Final Verdict

### A. What is AWH actually today?

A **well-engineered MCP server with comprehensive workspace tooling** (files, git, terminal, memory, tasks, context, skills, connectors). It provides a secure, auditable interface for AI assistants to interact with a developer's workspace through standardized MCP protocol.

### B. What does the documentation claim AWH is?

A **multi-agent workspace runtime** with capability-based security, task orchestration, crash recovery, and intelligent context management.

### C. What is the difference?

The documentation describes a **multi-agent orchestration platform**; the code implements a **single-user MCP server**. The gap is approximately 50% of the target architecture (agent model, capabilities, task DAG, checkpoints, model router, etc.).

### D. Which features genuinely work?

- MCP server (stdio + HTTP/SSE)
- File operations (read/write/search)
- Git operations (all common workflows)
- Terminal execution (bounded, safe)
- Memory store (append-only)
- Task store (flat list)
- Context engine (opt-in)
- Skills (install/reference)
- Connectors (metadata + Composio/GitHub)
- Trust management (per-MCP-server)
- Audit logging (allow/deny)
- TUI (14 screens)
- Control API (basic CRUD)

### E. Which features only appear to exist?

- Agent registry/runtime (structs mentioned in docs, not in code)
- Task dependencies (Task model has no dependency field)
- Checkpoints/snapshots (mentioned in architecture, not implemented)
- Capability grants (no model exists)
- Policy engine (no implementation)
- Model router (no implementation)

### F. Which features are partially implemented?

- Memory scopes (enum exists, limited enforcement)
- Skills (installation works, no sandboxing)
- Connectors (metadata stored, no OAuth flow)
- Context engine (full implementation, opt-in, no agent integration)
- Events (audit log exists, no event stream)

### G. Which features are implemented but disconnected?

- Context offload/restore (no automatic triggering)
- Context compression (implemented, not integrated)
- Circuit breaker (only for dynamic providers)

### H. Where does CLI differ from TUI?

- CLI has trust management commands (`awh mcp trust/block/revoke`); TUI does not
- TUI has interactive editor; CLI has no file editing
- CLI has skill creation; TUI does not
- Both share all other functionality via common backend

### I. Where does MCP bypass the intended architecture?

Static MCP tools (terminal.run, workspace.write_file, etc.) do NOT pass through the execution gate at call time. They rely on:
- Trust check at MCP server initialization (custom servers only)
- Argument validation
- Path validation

This is acceptable for now but should be enhanced when agents are added.

### J. Where does security enforcement actually happen?

1. **Authentication:** `api/control.rs::authenticate()` (API), `mcp/auth.rs` (MCP remote)
2. **Authorization:** `mcp/execution_gate.rs::authorize()` (custom MCP servers at init)
3. **Validation:** `mcp/schema.rs::validate_tool_arguments()` (all tool calls)
4. **Sandbox:** `mcp/sandbox.rs::wrap_command()` (subprocess spawning)
5. **Audit:** `services/audit.rs::record_allow/record_deny()` (all operations)

### K. Where can an agent/tool bypass security?

**CURRENT STATE:** A malicious AI assistant could:
1. Invoke static tools directly (no runtime capability check)
2. Execute unsandboxed skills (no skill sandbox)
3. Leak secrets via tool output (no output sanitization)

**MITIGATION REQUIRED:** Add capability checks to static tool invocations.

### L. What is duplicated?

**Nothing significant.** Business logic is properly centralized in services.

### M. What should be removed?

- `main.rs:612` unreachable branch (dead code)
- No other removals recommended

### N. What should be refactored?

1. Nested runtime creation in dispatcher.rs → use `tokio::spawn`
2. Trust store loading → extract common function
3. Error handling → unify error types across layers

### O. What should be preserved?

1. Transport-agnostic dispatcher design
2. Service layer abstraction
3. Execution gate pattern
4. Audit logging with token redaction
5. Sandbox enforcement (platform-specific)
6. Atomic writes
7. Advisory locking pattern

### P. What must be implemented first?

1. Agent model + registry
2. Capability grants
3. Tool broker enhancement (capability checks)
4. Task DAG

### Q. What must NOT be implemented yet?

1. Android/Termux support (low priority)
2. Advanced TUI features (until agent model exists)
3. Distributed deployment (premature optimization)

### R. What is the minimum path to production?

**For single-user MVP:**
1. Add JSON output to CLI
2. Add integration tests
3. Add config file support
4. Harden error handling
5. Add panic hook

**For multi-agent production:**
1. Implement agent model
2. Implement capability security
3. Implement task DAG
4. Implement checkpoints
5. Implement message bus

### S. What is the correct long-term architecture?

```
┌─────────────────────────────────────────────────────────────┐
│                    Interface Layer                           │
│  CLI  │  TUI  │  API  │  MCP  │  Headless Daemon            │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                  Application Services                        │
│  Agent  │  Task  │  File  │  Git  │  Terminal  │  Memory   │
│  Runtime│  Sched │  Store │  Svc  │  Service   │  Service  │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                    Core Engines                              │
│  Capability  │  Policy  │  Event  │  Message  │  Context    │
│  Engine      │  Engine  │  Bus    │  Bus      │  Engine     │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                   Persistence Layer                          │
│  AgentStore │ TaskStore │ EventStore │ CheckpointStore     │
└─────────────────────────────────────────────────────────────┘
```

---

## APPENDIX A: File Reference Index

### Key Files by Function

| Function | Primary File | Secondary Files |
|----------|--------------|-----------------|
| MCP Dispatch | mcp/dispatcher.rs | mcp/server.rs, mcp/http.rs |
| Security Gate | mcp/execution_gate.rs | mcp/trust.rs, mcp/permissions.rs |
| Sandbox | mcp/sandbox.rs | mcp/security.rs |
| Audit | services/audit.rs | mcp/audit.rs |
| Files | services/files.rs | mcp/security.rs |
| Git | services/git.rs | mcp/github.rs |
| Terminal | services/terminal.rs | mcp/sandbox.rs |
| Context | context/engine.rs | context/*.rs |
| Skills | skills/registry.rs | skills/installer.rs |
| TUI Backend | tui/backend.rs | tui/screens/*.rs |
| Control API | api/control.rs | mcp/auth.rs |

### Line References for Critical Functions

| Function | File:Line |
|----------|-----------|
| McpDispatcher::new | mcp/dispatcher.rs:141 |
| McpDispatcher::dispatch | mcp/dispatcher.rs:287 |
| McpDispatcher::call_tool | mcp/dispatcher.rs:560 |
| authorize_mcp_execution | mcp/execution_gate.rs:17 |
| wrap_command (Linux) | mcp/sandbox.rs:102 |
| validate_tool_arguments | mcp/schema.rs:54 |
| secure_path | mcp/security.rs:34 |
| atomic_write | mcp/security.rs:147 |
| audit_allow | mcp/audit.rs:83 |
| FilesService::run | services/files.rs:58 |
| GitService::open | services/git.rs:86 |
| TerminalService::run | services/terminal.rs:70 |
| ContextEngine::new | context/engine.rs:133 |

---

**Report Generated:** 2025-09-09  
**Total Lines Analyzed:** ~17,279 (production) + ~1,500 (tests)  
**Modules Inspected:** 80+  
**Functions Traced:** 100+  

**Classification:** FACT where code verified, INFERENCE where behavior deduced, RECOMMENDATION where improvement suggested.
