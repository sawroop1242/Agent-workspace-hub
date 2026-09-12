# Agent Workspace Hub — Project Roadmap

> **Strategic direction:** AWH is an agent-agnostic, local-first workspace runtime for coding agents. It is infrastructure for existing AI agents, not an attempt to become another agent framework, model router, or general-purpose workflow engine.

## Product Boundary

AWH should own:

- Workspace and filesystem state
- Controlled file editing
- Git and agent worktrees
- Capabilities and policy
- Snapshots, undo, and provenance
- Context and project state
- Persistent developer-oriented memory
- Skills and capability packages
- Agent/session state
- Audit and observability
- MCP, CLI, TUI, and Control API interfaces

External agents should own:

- Reasoning
- Planning
- Model selection
- Agent intelligence
- Agent-specific orchestration

The operating system and infrastructure remain below AWH.

---

# Phase 0 — Foundation & Release Infrastructure

**Goal:** Make AWH installable, reproducible, cross-platform, and testable.

### Core

- [ ] Rust workspace architecture cleanup
- [ ] Configuration system
- [ ] Workspace/project discovery
- [ ] Cross-platform path abstraction
- [ ] Standardized error handling
- [ ] Structured logging
- [ ] Version/build information
- [ ] Storage abstraction
- [ ] Storage migrations
- [ ] Locking/concurrency primitives

### Distribution

- [ ] Linux x86_64
- [ ] Linux ARM64
- [ ] macOS ARM64
- [ ] macOS x86_64
- [ ] Windows x86_64
- [ ] Android/Termux ARM64
- [ ] Static/minimal binaries where practical
- [ ] One-line installer
- [ ] Upgrade command
- [ ] Uninstall command
- [ ] GitHub Releases
- [ ] Checksums
- [ ] Shell completion

### CLI baseline

```text
awh init
awh status
awh doctor
awh version
awh config
awh workspace
awh logs
```

### Quality gate

Every important CLI command should be validated through:

```text
implementation
  -> unit test
  -> integration test
  -> real terminal test
  -> failure/recovery test
```

**Exit condition:** a new user can install AWH and successfully run `awh init`, `awh status`, and `awh doctor`.

---

# Phase 1 — MCP Infrastructure

**Goal:** Make AWH a reliable MCP-first infrastructure layer.

### MCP server

- [ ] MCP initialization
- [ ] Tool discovery
- [ ] Resource discovery
- [ ] Prompt support
- [ ] Transport abstraction
- [ ] stdio transport
- [ ] HTTP transport
- [ ] SSE compatibility where required
- [ ] Authentication hooks
- [ ] Connection lifecycle management
- [ ] Graceful shutdown
- [ ] Concurrent clients
- [ ] Request cancellation
- [ ] Timeout handling
- [ ] Structured MCP errors
- [ ] Protocol/version negotiation

### MCP management

```text
awh mcp serve
awh mcp list
awh mcp add
awh mcp remove
awh mcp inspect
awh mcp test
awh mcp logs
```

### MCP security

- [ ] Tool allowlist
- [ ] Tool denylist
- [ ] Resource restrictions
- [ ] Session isolation
- [ ] Capability checks
- [ ] Request audit
- [ ] Rate limits
- [ ] Payload limits
- [ ] Timeout policies

### Interoperability

Validate AWH against multiple external MCP clients, including OpenCode, OpenHands, Claude Code, Qwen Code, and generic MCP clients where practical.

**Exit condition:** external agents can connect to AWH reliably without AWH-specific client hacks.

---

# Phase 2 — Workspace Runtime

**Goal:** Give agents a controlled, persistent workspace.

### Workspace lifecycle

```text
awh workspace create
awh workspace list
awh workspace open
awh workspace remove
awh workspace info
```

### Filesystem

- [ ] Read files
- [ ] Write files
- [ ] Append
- [ ] Create
- [ ] Delete
- [ ] Rename
- [ ] Move
- [ ] Copy
- [ ] Directory operations
- [ ] Recursive listing
- [ ] File metadata
- [ ] Binary file support
- [ ] Streaming
- [ ] File locking

### Controlled editing

Avoid unnecessary full-file rewrites. Provide a patch-oriented editing model:

```text
read
  -> locate region
  -> validate expected content
  -> patch
  -> verify
```

Support:

- [ ] Line-range edits
- [ ] Exact replacement
- [ ] Unified diff
- [ ] Patch application
- [ ] Insertions
- [ ] Deletions
- [ ] Context matching
- [ ] Conflict detection
- [ ] Atomic writes

---

# Phase 3 — Git & Workspace Isolation

**Goal:** Make AWH understand developer projects and safely isolate agent work.

### Git

- [ ] Repository detection
- [ ] Status
- [ ] Diff
- [ ] Log
- [ ] Branch management
- [ ] Commit
- [ ] Checkout
- [ ] Reset
- [ ] Stash
- [ ] Remote information
- [ ] Conflict detection

### First-class agent worktrees

```text
Project
 ├── main
 ├── agent-claude
 ├── agent-opencode
 ├── agent-qwen
 └── agent-experiment
```

Commands:

```text
awh worktree create <name>
awh worktree list
awh worktree inspect <name>
awh worktree remove <name>
awh worktree merge <name>
```

AWH should associate agent, session, workspace, worktree, branch, and modified files.

**Exit condition:** multiple agents can work on one project without accidentally sharing the same mutable workspace.

---

# Phase 4 — Capability & Policy Engine

**Goal:** Control exactly what an agent is allowed to do.

Core authorization model:

```text
Identity
  -> Agent
  -> Session
  -> Workspace
  -> Capability
  -> Resource
  -> Policy
  -> Tool invocation
```

### Capabilities

Examples:

```text
filesystem.read
filesystem.write
filesystem.delete
git.read
git.write
process.execute
network.request
mcp.invoke
secrets.read
```

### Policies

Support:

- [ ] Allow/deny rules
- [ ] Path restrictions
- [ ] Command restrictions
- [ ] Network restrictions
- [ ] Resource limits
- [ ] Session-specific policies
- [ ] Agent-specific policies
- [ ] Workspace-specific policies
- [ ] Temporary permissions
- [ ] Approval requests
- [ ] Policy inheritance

A skill or agent must not silently obtain additional capabilities.

---

# Phase 5 — Snapshots, Undo & Provenance

**Goal:** Make agent changes reversible and explainable.

Modification flow:

```text
Agent
  -> AWH
  -> Snapshot
  -> Modification
  -> Verification
```

### Snapshot commands

```text
awh snapshot create
awh snapshot list
awh snapshot inspect
awh snapshot restore
awh snapshot delete
```

### Workspace recovery

```text
awh workspace diff
awh workspace history
awh workspace undo
awh workspace restore
```

### Provenance

Track, where practical:

```text
file
agent
session
capability
tool
timestamp
before hash
after hash
snapshot
```

This enables workflows such as:

```text
awh explain src/main.rs
```

### Recovery features

- [ ] Undo last agent action
- [ ] Undo a session
- [ ] Restore a snapshot
- [ ] Restore an individual file
- [ ] Compare snapshots
- [ ] Recover deleted files
- [ ] Crash recovery

**Boundary:** AWH's snapshot/provenance layer is a workspace safety mechanism. It should not be marketed as a replacement for VM/container/OS sandboxing.

---

# Phase 6 — Context Engine

**Goal:** Give agents relevant project context without forcing them to rediscover the project repeatedly.

### Context sources

- Filesystem
- Git
- Workspace metadata
- Session history
- Previous changes
- Skills
- Project configuration
- Tool results
- User-provided context

### Operations

```text
awh context inspect
awh context search
awh context build
awh context summarize
awh context export
```

### Context capabilities

- [ ] Relevant-file discovery
- [ ] Dependency awareness
- [ ] Project structure
- [ ] Changed-file awareness
- [ ] Recent agent-action context
- [ ] Session context
- [ ] Context compression
- [ ] Token-budget-aware context
- [ ] Stale-context detection

---

# Phase 7 — Developer-Oriented Memory

**Goal:** Provide persistent project state integrated with workspace, Git, sessions, and provenance.

AWH should not try to win by implementing every possible memory feature. Focus on coding workflow memory.

### Memory types

```text
Project memory
Session memory
Agent memory
Developer preferences
Decision records
Architecture decisions
Known issues
Task state
```

### Commands

```text
awh memory add
awh memory search
awh memory list
awh memory forget
awh memory export
```

Memory should understand relationships between:

```text
Project
Workspace
Git
Agent
Session
Files
Snapshots
```

---

# Phase 8 — Skills & Capability Packages

**Goal:** Make AWH extensible without turning it into an agent framework.

A skill package should describe:

```text
What it does
Required tools
Required capabilities
Inputs
Outputs
Policy requirements
Instructions
```

Example:

```text
rust-development
 ├── cargo
 ├── rustfmt
 ├── clippy
 ├── filesystem
 └── git
```

### Management

```text
awh skill list
awh skill install
awh skill remove
awh skill inspect
awh skill enable
awh skill disable
```

### Security

```text
Skill
  -> requested capabilities
  -> policy evaluation
  -> approved capabilities
```

---

# Phase 9 — Sessions & Agent Management

**Goal:** Make AWH agent-aware without becoming the agent itself.

### Agent registry

```text
awh agent list
awh agent add
awh agent remove
awh agent inspect
awh agent connect
```

Target interoperability includes Claude Code, OpenCode, OpenHands, Qwen Code, Codex, Aider, Gemini CLI, and custom MCP agents where technically supported.

### Session model

```text
Agent
 └── Session
      ├── Workspace
      ├── Worktree
      ├── Capabilities
      ├── Context
      ├── Memory
      ├── Snapshots
      └── Audit
```

Commands:

```text
awh session list
awh session start
awh session inspect
awh session pause
awh session resume
awh session close
```

---

# Phase 10 — Audit & Observability

**Goal:** Make agent activity inspectable and diagnosable.

Central event model:

```text
timestamp
agent
session
workspace
tool
capability
resource
action
result
duration
policy decision
```

### CLI

```text
awh audit
awh audit agent
awh audit session
awh audit workspace
awh audit tool
```

### Observability

- [ ] Structured logs
- [ ] Event stream
- [ ] Metrics
- [ ] Tool latency
- [ ] Failures
- [ ] Policy denials
- [ ] File modifications
- [ ] Process execution
- [ ] MCP requests
- [ ] Resource usage

---

# Phase 11 — Terminal UI

**Goal:** Provide a live developer control center.

### TUI panels

- [ ] Workspace
- [ ] Files
- [ ] Agents
- [ ] Sessions
- [ ] MCP
- [ ] Tools
- [ ] Capabilities
- [ ] Policies
- [ ] Snapshots
- [ ] Git
- [ ] Memory
- [ ] Audit
- [ ] Logs

The TUI should prioritize local observability and control rather than becoming another full IDE.

---

# Phase 12 — Multi-Agent Collaboration

**Goal:** Support multiple agents on one machine/project without building a distributed agent framework.

### Initial capabilities

- [ ] Agent registry
- [ ] Agent status
- [ ] Isolated worktrees
- [ ] Shared project state
- [ ] Session handoff
- [ ] Task ownership
- [ ] Conflict detection
- [ ] Merge notifications
- [ ] Agent-to-agent event notifications

Example:

```text
Agent A -> feature/auth
Agent B -> feature/api
Agent C -> tests
```

AWH coordinates the workspace and state; agents remain responsible for reasoning.

### Explicitly defer

- [ ] Large distributed message bus
- [ ] Distributed agent swarm
- [ ] Autonomous agent scheduler
- [ ] General-purpose DAG workflow engine

---

# Phase 13 — Control API

**Goal:** Allow external applications to control AWH through a stable API.

Architecture:

```text
CLI
TUI
MCP
External UI
External Agent
       |
       v
 AWH Control API
       |
       v
    AWH Core
```

### API domains

- [ ] Workspace API
- [ ] Agent API
- [ ] Session API
- [ ] Snapshot API
- [ ] Git API
- [ ] Memory API
- [ ] Capability API
- [ ] Policy API
- [ ] Audit API
- [ ] Event streaming

### Security

- [ ] API authentication
- [ ] Scoped tokens
- [ ] Capability-based API access
- [ ] Local-only mode
- [ ] Remote mode
- [ ] TLS support

---

# Phase 14 — Remote AWH

**Goal:** Extend the local runtime to remote environments only after the local product is mature.

Potential architecture:

```text
Local Agent
     |
     v
 AWH Client
     |
     v
 Remote AWH
     |
     v
 Workspace
```

Potential features:

- [ ] Remote workspace
- [ ] Remote session
- [ ] Remote MCP
- [ ] Remote audit
- [ ] Remote TUI
- [ ] Secure authentication
- [ ] Workspace synchronization
- [ ] Connection recovery

Local-first remains the primary product model.

---

# Phase 15 — Ecosystem & Integrations

**Goal:** Make AWH easy to adopt with existing developer tooling.

### Agent integrations

- [ ] Claude Code
- [ ] OpenCode
- [ ] OpenHands
- [ ] Qwen Code
- [ ] Codex
- [ ] Aider
- [ ] Gemini CLI
- [ ] Custom agents

### Developer integrations

- [ ] GitHub
- [ ] GitLab
- [ ] Gitea
- [ ] VS Code
- [ ] Neovim
- [ ] Acode
- [ ] JetBrains IDEs

AWH should consume and expose MCP infrastructure rather than attempting to replace every MCP server or gateway.

---

# Phase 16 — Advanced Infrastructure

**Goal:** Add deeper isolation and enterprise capabilities only when validated by real users.

Potential adapters/features:

- [ ] Process sandbox adapters
- [ ] Docker integration
- [ ] WASM sandbox
- [ ] OS-level sandbox adapters
- [ ] Resource quotas
- [ ] Network policies
- [ ] Secrets-manager integrations
- [ ] Remote execution
- [ ] Snapshot deduplication
- [ ] Distributed workspace
- [ ] Enterprise RBAC

These should remain modular adapters around the AWH core rather than forcing the core to become a monolithic platform.

---

# Priority Model

| Priority | Phase | Importance |
|---|---|---:|
| P0 | Foundation & release infrastructure | Critical |
| P0 | MCP infrastructure | Critical |
| P0 | Workspace/file runtime | Critical |
| P0 | Git/worktrees | Critical |
| P0 | Capability + policy | Critical |
| P0 | Snapshots/undo/provenance | Core differentiator |
| P1 | Context engine | High |
| P1 | Developer-oriented memory | High |
| P1 | Skills | High |
| P1 | Sessions/agent management | High |
| P1 | Audit/observability | High |
| P2 | TUI | High UX value |
| P2 | Multi-agent basics | Medium |
| P2 | Control API | Medium |
| P3 | Remote AWH | Later |
| P3 | Ecosystem integrations | Later |
| P4 | Advanced sandbox/enterprise | Demand-driven |

---

# Strategic Guardrails

## 1. Do not become an agent framework

AWH should provide infrastructure to agents, not compete with their reasoning/planning layer.

## 2. Do not build a LangGraph-style workflow platform by default

Only add orchestration features when a concrete AWH workflow requires them.

## 3. Do not build a generic model router

Model selection and provider routing belong above AWH unless a future validated use case requires a small adapter.

## 4. Do not claim to replace real sandboxing

Snapshots, policies, capabilities, and process controls complement container/VM/OS sandboxing; they are not automatically equivalent to it.

## 5. Distribution is a product feature

AWH should remain easy to install and operate as a single local Rust binary wherever practical.

## 6. MCP-first, not MCP-only

MCP is the primary interoperability boundary, while CLI, TUI, and Control API provide direct local control.

## 7. Prefer composable adapters

Integrate with existing MCP gateways, sandbox runtimes, Git providers, memory systems, and agents where doing so is better than rebuilding them.

## 8. Validate the workflow, not just individual components

A feature is not complete merely because its Rust tests pass. The full agent workflow must be exercised through real CLI/MCP interactions and failure/recovery scenarios.

---

# Long-Term Product Shape

```text
                    AI / AGENT LAYER
 ┌─────────────────────────────────────────────────┐
 │ Claude │ Codex │ OpenCode │ Qwen │ OpenHands   │
 └───────────────────────┬─────────────────────────┘
                         │ MCP / API
                         ▼
              ┌───────────────────────┐
              │       AWH CORE        │
              │                       │
              │ Capability Engine     │
              │ Policy Engine         │
              │ Tool Broker            │
              │ Session Manager       │
              │ Context Engine        │
              │ Memory                │
              │ Skills                │
              │ Audit                 │
              └───────────┬───────────┘
                          │
              ┌───────────▼───────────┐
              │   WORKSPACE RUNTIME   │
              │                       │
              │ Filesystem            │
              │ File Patching         │
              │ Snapshots             │
              │ Git                   │
              │ Worktrees             │
              │ Processes             │
              │ Secrets               │
              └───────────┬───────────┘
                          │
             ┌────────────▼────────────┐
             │       OS / Runtime      │
             │ Linux / macOS / Windows │
             │ Android / Termux        │
             └─────────────────────────┘
```

## Success criterion

AWH succeeds when a developer can install one local binary, connect their preferred coding agent, and immediately gain a coherent workspace with persistent project state, controlled capabilities, reversible changes, Git isolation, context, auditability, and MCP interoperability — without requiring AWH to become the agent itself.
