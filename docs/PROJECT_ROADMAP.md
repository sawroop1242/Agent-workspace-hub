# Agent Workspace Hub — Final Product Roadmap

> **Status:** Canonical forward-looking architecture, feature set, CLI contract, dependency graph, and implementation order.
>
> **Branch:** `rust`
>
> Historical status/evidence documents remain historical snapshots. This document is the authoritative build plan for future implementation.

## 1. Product Boundary

AWH is an **agent-agnostic, local-first workspace runtime for coding agents**. It is infrastructure for existing AI agents, not another agent framework, model router, or general-purpose workflow engine.

### AWH owns

- Workspace and filesystem state
- Controlled, agent-grade file editing
- Git and first-class agent worktrees
- Capabilities and policy enforcement
- Snapshots, undo, rollback, and provenance
- Context and project state
- Persistent developer-oriented memory
- Skills and capability packages
- Agent profiles and sessions
- Tasks and runtime state
- Audit and observability
- MCP, CLI, TUI, and Control API interfaces
- Optional remote runtime and integrations

### External agents own

- Reasoning
- Planning
- Model selection
- Agent intelligence
- Agent-specific orchestration

The operating system/container/VM layer remains below AWH. AWH snapshots and policy are not replacements for OS/container/VM sandboxing.

## 2. Final Architecture

```text
                         External Agents
        Claude / OpenCode / OpenHands / Qwen / Codex / etc.
                              │
                              ▼
                    ┌─────────────────────┐
                    │  MCP / Control API  │
                    │  CLI / TUI          │
                    └──────────┬──────────┘
                               │
                     ┌─────────▼─────────┐
                     │ Agent Router      │
                     │ + Agent Registry  │
                     └─────────┬─────────┘
                               │
                     ┌─────────▼─────────┐
                     │ Session Runtime   │
                     └─────────┬─────────┘
                               │
              ┌────────────────▼────────────────┐
              │ Capability + Policy Engine     │
              └────────────────┬────────────────┘
                               │
        ┌──────────────────────┼──────────────────────┐
        ▼                      ▼                      ▼
 Workspace/FS             Git/Worktrees          Runtime/Tools
        │                      │                      │
        └──────────────────────┼──────────────────────┘
                               ▼
                 Snapshot / Provenance / Audit
                               │
                 ┌─────────────┴─────────────┐
                 ▼                           ▼
          Context / Memory / Skills    Collaboration
```

### Shared-service rule

CLI, MCP, TUI, and Control API are **interfaces over the same AWH application services**. They must not implement separate filesystem, Git, policy, editing, snapshot, or runtime semantics.

### Configuration rule

```text
TOML / env / CLI overrides
          ↓
Configuration
          ↓
AgentProfile / RuntimeConfig
          ↓
AgentRegistry + PolicyEngine
```

TOML is declarative configuration; it is never the authoritative authorization engine.

## 3. Agent Profiles & Policy-Routed MCP

Named agent profiles are first-class runtime identities.

```text
/{agent}/mcp
/{agent}/sse
```

Example:

```text
/claude/mcp
/claude/sse
/qwen/mcp
/qwen/sse
/opencode/mcp
/opencode/sse
```

The URL namespace identifies routing only. It is **not authorization**.

Request flow:

```text
/{agent}/mcp
   ↓
AgentRegistry
   ↓
AgentSession
   ↓
CapabilityContext
   ↓
PolicyEngine
   ↓
Tool Registry
   ↓
AWH application service
   ↓
Audit / provenance
```

If a tool is denied, execution must not occur and a structured `PermissionDenied` result is returned.

### Agent lifecycle

```bash
awh agent list
awh agent show <name>
awh agent start <name>...
awh agent start --all
awh agent stop <name>
awh agent restart <name>
awh agent run <name>
awh agent status
```

Starting only `claude` registers only Claude's configured active routes. An inactive agent must not have an active agent-specific MCP route.

## 4. Final CLI Contract

The following is the **final target CLI surface**. A command is not considered implemented merely because it appears in help; its underlying service, policy checks, tests, terminal validation, and failure/recovery behavior must exist.

```text
awh
├── init
├── version
├── status
├── doctor
├── config
├── agent
│   ├── list
│   ├── show
│   ├── start
│   ├── stop
│   ├── restart
│   ├── run
│   └── status
├── mcp
│   ├── serve
│   ├── list
│   ├── add
│   ├── remove
│   ├── inspect
│   ├── test
│   └── logs
├── workspace
│   ├── create
│   ├── list
│   ├── open
│   ├── info
│   └── remove
├── fs
│   ├── read
│   ├── write
│   ├── stat
│   ├── search
│   ├── patch
│   ├── replace
│   ├── insert
│   ├── delete-range
│   ├── apply-diff
│   ├── hash
│   ├── verify
│   ├── history
│   └── rollback
├── git
│   ├── status
│   ├── diff
│   ├── staged-diff
│   ├── log
│   ├── branch
│   ├── branches
│   ├── worktree
│   ├── stage
│   ├── unstage
│   ├── commit
│   ├── push
│   ├── pull
│   ├── reset
│   ├── clean
│   └── validate
├── worktree
│   ├── create
│   ├── list
│   ├── inspect
│   ├── remove
│   ├── merge
│   └── status
├── capability
│   ├── list
│   ├── show
│   ├── grant
│   ├── revoke
│   └── check
├── policy
│   ├── list
│   ├── show
│   ├── check
│   ├── validate
│   └── explain
├── snapshot
│   ├── create
│   ├── list
│   ├── show
│   ├── restore
│   ├── delete
│   └── diff
├── context
│   ├── show
│   ├── save
│   ├── update
│   ├── clear
│   └── search
├── memory
│   ├── list
│   ├── get
│   ├── search
│   ├── add
│   ├── update
│   └── delete
├── skill
│   ├── list
│   ├── show
│   ├── install
│   ├── remove
│   ├── enable
│   └── disable
├── session
│   ├── list
│   ├── show
│   ├── create
│   ├── stop
│   └── status
├── task
│   ├── list
│   ├── show
│   ├── create
│   ├── update
│   ├── cancel
│   └── assign
├── audit
│   ├── list
│   ├── show
│   ├── search
│   └── export
├── logs
│   ├── show
│   ├── follow
│   └── clear
├── terminal
│   ├── run
│   ├── list
│   └── kill
├── connector
│   ├── list
│   ├── add
│   ├── remove
│   ├── inspect
│   ├── test
│   └── invoke
├── collaboration
│   ├── agents
│   ├── status
│   ├── handoff
│   ├── assign
│   ├── conflicts
│   └── events
├── api
│   ├── serve
│   ├── status
│   ├── tokens
│   └── logs
├── tui
└── completion
    ├── bash
    ├── zsh
    ├── fish
    └── powershell
```

The complete command semantics and dependency table live in `docs/CLI.md`.

## 5. Phase Map

| Phase | Area | Final CLI families |
|---|---|---|
| 0 | Foundation & release | root, `init`, `version`, `status`, `doctor`, `config` |
| 1 | MCP infrastructure | `mcp *`, agent routing foundation |
| 2 | Workspace runtime | `workspace *`, basic `fs *` |
| 3 | Git & isolation | `git *`, `worktree *` |
| 4 | Capability & policy | `capability *`, `policy *`, agent authorization |
| 5 | Snapshots/undo/provenance | `snapshot *`, `fs history`, `fs rollback` |
| 6 | Context engine | `context *` |
| 7 | Developer memory | `memory *` |
| 8 | Skills | `skill *` |
| 9 | Sessions & agent runtime | `agent *`, `session *`, `task *` |
| 10 | Audit/observability | `audit *`, `logs *`, terminal foundation |
| 11 | TUI | `tui` |
| 12 | Multi-agent collaboration | `collaboration *` |
| 13 | Control API | `api *` |
| 14 | Remote AWH | remote operation behind API/runtime abstractions |
| 15 | Ecosystem/integrations | `connector *`, agent/IDE adapters, completion |
| 16 | Advanced infrastructure | sandbox/resource/secrets adapters behind existing interfaces |

## 6. Dependency-Driven Build Order

### Stage A — Foundation

```text
CLI framework
→ configuration
→ init/version/status/doctor
→ storage/state
→ structured logging
```

### Stage B — MCP

```text
MCP protocol/server
→ mcp serve
→ tool/resource registry
→ mcp inspect/test
→ lifecycle/logging
```

### Stage C — Workspace and filesystem

```text
workspace service
→ workspace CLI
→ fs read/stat/search/hash
→ secure path resolution
```

### Stage D — Agent-grade editing

```text
EditTransaction
→ replace/insert/delete-range
→ patch
→ apply-diff
→ context validation
→ conflict detection
→ atomic commit
→ verification
→ history/rollback
```

All editing interfaces must use the same `EditService`.

### Stage E — Security and agent routing

```text
Capability model
→ PolicyEngine
→ AgentProfile
→ AgentRegistry
→ AgentSession
→ agent-specific MCP routes
→ policy-enforced tool discovery/invocation
```

### Stage F — Git isolation

```text
git read operations
→ git mutations
→ worktree service
→ agent/session/worktree association
→ guarded merge/reset/clean/push/pull
```

### Stage G — Reversibility and observability

```text
snapshots
→ provenance
→ audit events
→ logs
→ rollback/recovery
```

### Stage H — Runtime state

```text
sessions
→ tasks
→ context
→ memory
→ skills
```

### Stage I — High-risk operations

```text
policy + capability + session + audit
→ terminal
→ connectors
```

### Stage J — Collaboration and control plane

```text
agent identity
→ sessions
→ worktrees
→ tasks
→ conflict/events
→ collaboration
→ Control API
→ TUI
```

### Stage K — Ecosystem and release

```text
connectors/integrations
→ shell completion
→ documentation
→ end-to-end acceptance
→ release
```

## 7. Critical Dependency Graph

```text
Foundation
   │
   ├───────────────┐
   ▼               ▼
Workspace         MCP
   │               │
   ▼               ▼
Filesystem      Agent Profiles
   │               │
   ▼               ▼
EditService ───► PolicyEngine
   │               │
   ├───────┬───────┘
   ▼       ▼
 Git    Sessions
   │       │
   ▼       ▼
Worktrees Tasks
   │       │
   └───┬───┘
       ▼
Snapshots / Provenance
       │
       ▼
Audit / Observability
       │
       ├──────────────┐
       ▼              ▼
 Context            Terminal
       │              │
       ▼              ▼
 Memory           Connectors
       │
       └──────┬───────┘
              ▼
        Collaboration
              │
              ▼
         Control API
              │
              ▼
             TUI
              │
              ▼
       Ecosystem/Release
```

## 8. Security Ordering

Security-sensitive mutations must not be implemented before their enforcement layer exists.

```text
PolicyEngine
   ↓
Capability checks
   ↓
Session identity
   ↓
Audit
   ↓
Dangerous operation
```

This applies especially to:

- `git push`
- `git reset`
- `git clean`
- `snapshot restore`
- `terminal run`
- `capability grant`
- `capability revoke`
- `connector invoke`

Route namespace, CLI arguments, TOML, or MCP discovery must never bypass authorization.

## 9. Phase Exit Rules

Every phase must satisfy:

```text
implementation
→ unit tests
→ integration tests
→ real terminal validation
→ failure/recovery validation
→ documentation update
```

For MCP-facing features also require real-client validation where practical.

A feature is **implemented** only when the behavior exists and is validated; planned commands must not be documented as currently available.

## 10. Final Acceptance Workflow

The core product acceptance path is:

```text
awh init
  ↓
awh workspace create
  ↓
awh agent start claude
  ↓
awh mcp serve
  ↓
Claude → /claude/mcp
  ↓
filesystem.read
  ↓
filesystem.patch
  ↓
PolicyEngine
  ↓
EditTransaction
  ↓
Snapshot
  ↓
Verification
  ↓
Audit
  ↓
Git diff
  ↓
Git commit
```

Additional acceptance checks:

```text
awh doctor
awh status
awh policy check
awh capability check
awh snapshot list
awh audit search
awh logs follow
```

## 11. Strategic Guardrails

1. **Do not become an agent framework.** AWH provides runtime infrastructure; agents own reasoning and planning.
2. **Do not become a generic workflow/DAG platform.** Add orchestration only when required by an AWH workflow.
3. **Do not become a generic model router.** Provider/model selection remains above AWH.
4. **Do not claim snapshots replace sandboxing.** Use OS/container/VM isolation where required.
5. **MCP-first, not MCP-only.** CLI, TUI, and Control API are first-class interfaces over the same core.
6. **Prefer composable adapters.** Remote, connector, sandbox, and ecosystem features should not contaminate the core runtime.
7. **Validate complete workflows, not only unit tests.**
8. **Coherence over feature count.** Later phases must not destabilize the core runtime.
9. **Distribution is a product feature.** Cross-platform binaries, install/upgrade/uninstall, checksums, and completion are part of release quality.
