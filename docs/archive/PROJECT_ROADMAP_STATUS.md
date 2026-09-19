# AWH Project Roadmap Status

> **Status snapshot:** 2026-09-12  
> **Branch audited:** `rust`  
> **Purpose:** Track the current implementation state against `docs/PROJECT_ROADMAP.md`.

## Executive Summary

AWH has moved beyond the early infrastructure stage. The repository already contains substantial implementations for MCP, workspace/filesystem services, Git operations, capabilities/policies, snapshots, context, memory, skills, agents/sessions, audit/observability, TUI, Control API, and remote infrastructure.

The main engineering problem is now **integration and coherence**, not raw feature count.

The target runtime model is:

```text
Agent
  -> Session
    -> Workspace
      -> Worktree
        -> Capabilities / Policy
          -> Context / Memory
            -> Tool invocation
              -> Snapshot / Provenance
                -> Audit / Observability
```

AWH should remain **MCP-first, agent-agnostic, local-first infrastructure**. External agents own reasoning and planning; AWH owns the controlled workspace/runtime around them.

## Roadmap Completion Estimate

These percentages are roadmap-level estimates, not claims that every implemented component is production-ready.

| Phase | Area | Estimated completion | Current assessment |
|---|---|---:|---|
| 0 | Foundation & Release Infrastructure | **85%** | Strong Rust foundation, configuration, errors, logging, CI/release infrastructure and testing. Distribution/Android release hardening remains. |
| 1 | MCP Infrastructure | **90–95%** | Strongest subsystem. stdio/HTTP/SSE, auth, sessions, permissions, trust, discovery, limits, audit and MCP management are implemented. More client interoperability and hardening remain. |
| 2 | Workspace Runtime | **75–80%** | Workspace/filesystem/security foundations are mature. Agent-grade patch/edit primitives are the main missing high-value layer. |
| 3 | Git & Workspace Isolation | **55–60%** | Git service is substantial. First-class AWH-managed agent worktrees and agent/session/workspace/worktree linkage remain a major gap. |
| 4 | Capability & Policy Engine | **70–75%** | Capability grants, policy and MCP permission enforcement exist. The broader capability model still needs unification across all tools and resources. |
| 5 | Snapshots, Undo & Provenance | **65–75%** | Snapshot infrastructure exists. Full automatic pre-change snapshot → patch → verify → provenance → audit → undo flow remains to be completed. |
| 6 | Context Engine | **75–85%** | Real context subsystem with budgeting, scoring, selection, compression, offloading, planning, policy and snapshots. Deeper runtime integration remains. |
| 7 | Developer-Oriented Memory | **65–70%** | Memory subsystem and MCP operations exist. Cross-linking memory with agent/session/workspace/Git/files/snapshots needs completion. |
| 8 | Skills & Capability Packages | **75–80%** | Skills, registry and MCP operations are present. Capability-aware activation/package semantics need strengthening. |
| 9 | Sessions & Agent Management | **60–70%** | Agent/session foundations exist. AWH's central session model must be made distinct from MCP protocol sessions and linked to runtime state. |
| 10 | Audit & Observability | **75–80%** | Audit and structured security/tracing events exist. Unified provenance across all runtime entities remains the target. |
| 11 | Terminal UI | **55–65%** | Real TUI and remote TUI infrastructure exist. The TUI needs to become the unified live control center for AWH runtime state. |
| 12 | Multi-Agent Collaboration | **25–35%** | Foundations exist, but worktree isolation, handoff, task ownership, conflict detection and merge events need implementation. |
| 13 | Control API | **60–70%** | `src/api/control.rs` is substantial. Contract stabilization, scoped authorization and complete domain/event APIs remain. |
| 14 | Remote AWH | **30–40%** | HTTP/SSE, TLS, bearer auth, Control API and remote TUI foundations exist. Full remote workspace/session/synchronization product remains. |
| 15 | Ecosystem & Integrations | **35–45%** | MCP interoperability, Composio, GitHub and connector infrastructure exist. Broader vendor-specific verification/integration remains. |
| 16 | Advanced Infrastructure | **10–20%** | Several sandbox/security primitives exist, but Docker/WASM adapters, advanced secrets, remote execution, distributed workspace and enterprise RBAC remain. |

## Phase-by-Phase Status

### Phase 0 — Foundation & Release Infrastructure

**Current state: ~85%**

Implemented areas include modular Rust architecture, configuration, structured errors, tracing/logging, CI, release workflow, installation infrastructure, documentation and automated testing.

Remaining priorities:
- production release process
- branch protection/release gates
- broader package/distribution support
- Android/Termux release verification
- upgrade/uninstall verification
- final cross-platform release validation

### Phase 1 — MCP Infrastructure

**Current state: ~90–95%**

This is currently the most mature subsystem. Existing implementation covers stdio and HTTP/SSE transport, authentication, sessions, permissions/trust, filesystem security, skills, tasks, memory, connectors, audit, configuration, schema validation, request limits/timeouts, circuit-breaker behavior and MCP management/discovery.

Remaining priorities:
- verify against real OpenCode/Codex/other MCP clients
- deepen SSE/resource-boundary testing
- dedicated macOS/Windows sandbox runtime probes
- resolve real-world connection failures rather than adding more MCP features

**Direction:** feature-count expansion should now slow down; integration reliability should become the priority.

### Phase 2 — Workspace Runtime

**Current state: ~75–80%**

Workspace/filesystem services and atomic/security-aware writes are present.

Highest-value missing layer: controlled agent-grade editing:

- `filesystem.patch`
- `filesystem.replace`
- `filesystem.insert`
- `filesystem.delete_range`
- `filesystem.apply_diff`
- `filesystem.rollback`

The preferred editing transaction is:

```text
read -> locate -> validate -> patch -> verify
```

This should integrate directly with snapshots, provenance and audit rather than becoming another isolated file API.

### Phase 3 — Git & Workspace Isolation

**Current state: ~55–60%**

The Git service already supports status, staging/unstaging, commit, log, branches, diff, push/pull, reset, clean, branch deletion, high-risk operation classification, repository validation and command timeouts.

Major missing feature: first-class AWH-managed agent worktrees:

```text
awh worktree create
awh worktree list
awh worktree inspect
awh worktree merge
awh worktree remove
```

Target relationship:

```text
Agent -> Session -> Workspace -> Worktree -> Branch
```

### Phase 4 — Capability & Policy Engine

**Current state: ~70–75%**

Capability grants, policies, agent structures and MCP permission/trust enforcement already exist.

The next step is a single capability model:

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

Capabilities should cover at least:

- `filesystem.*`
- `git.*`
- `process.execute`
- `network.request`
- `mcp.invoke`
- `secrets.read`

Skills and agents must not silently gain capabilities.

### Phase 5 — Snapshots, Undo & Provenance

**Current state: ~65–75%**

Snapshot infrastructure exists and is substantial.

Target transaction:

```text
Agent action
  -> automatic pre-change snapshot
  -> controlled patch
  -> verification
  -> provenance record
  -> audit event
  -> reversible undo
```

Target user-facing operations include:

```text
awh snapshot create/list/inspect/restore/delete
awh workspace diff/history/undo/restore
awh explain <file>
```

Snapshots/provenance are workspace safety and reversibility mechanisms; they are **not** a replacement for VM/container/OS-level sandboxing.

### Phase 6 — Context Engine

**Current state: ~75–85%**

The context subsystem already contains budget management, compression, token accounting, scoring, selection, offloading, planning, policy and snapshot support.

Next priority is runtime integration with:

- workspace state
- Git changes
- sessions
- agent identity
- memory
- skills
- MCP tool results
- recent actions

The goal is context that is automatically relevant to the current AWH runtime state rather than a standalone retrieval subsystem.

### Phase 7 — Developer-Oriented Memory

**Current state: ~65–70%**

Memory CRUD and MCP memory operations exist.

AWH should deliberately avoid trying to beat dedicated memory servers on every dimension. The differentiator is project/runtime integration:

```text
Project <-> Workspace <-> Git <-> Agent <-> Session
          <-> Files <-> Snapshots <-> Decisions
```

### Phase 8 — Skills & Capability Packages

**Current state: ~75–80%**

Skills, registry and MCP operations are implemented.

Next priorities:
- explicit skill metadata
- requested capabilities
- required policy
- inputs/outputs
- capability-aware activation
- safe enable/disable lifecycle

### Phase 9 — Sessions & Agent Management

**Current state: ~60–70%**

Agent and session foundations exist, but the central AWH session must not be confused with an MCP protocol session.

Target:

```text
Agent
  -> Session
    -> Workspace/Worktree
    -> Capabilities
    -> Context
    -> Memory
    -> Snapshots
    -> Audit
```

### Phase 10 — Audit & Observability

**Current state: ~75–80%**

Audit and structured security/tracing infrastructure are present.

The target is one unified event/provenance model containing at least:

- timestamp
- agent
- session
- workspace
- worktree
- tool
- capability
- resource
- action
- policy decision
- result
- duration
- snapshot/change information

This should enable high-value commands such as:

```text
awh audit
awh history
awh explain src/main.rs
```

### Phase 11 — Terminal UI

**Current state: ~55–65%**

A real TUI subsystem and remote TUI infrastructure exist.

The next step is not adding disconnected screens. The TUI should consume the unified runtime model and expose:

- workspace/files
- agents
- sessions
- worktrees/Git
- MCP/tools
- capabilities/policies
- snapshots/undo
- memory/context
- audit/logs

### Phase 12 — Multi-Agent Collaboration

**Current state: ~25–35%**

Keep this deliberately small and developer-focused:

- agent registry/status
- isolated worktrees
- shared project state
- session handoff
- task ownership
- conflict detection
- merge notifications
- agent events

Do **not** turn AWH into a distributed agent swarm, generic message bus, autonomous scheduler or general-purpose DAG workflow engine.

### Phase 13 — Control API

**Current state: ~60–70%**

The Control API is already a substantial subsystem.

Remaining priorities:
- stable public contract
- scoped authentication
- capability-based authorization
- complete domain APIs
- event streaming
- local-only and remote modes
- remote hardening

### Phase 14 — Remote AWH

**Current state: ~30–40%**

HTTP/SSE, TLS, bearer authentication, Control API and remote TUI foundations exist.

Remaining:
- complete remote workspace/session model
- secure remote operations
- synchronization
- reconnection/recovery
- remote audit/control experience

Remote AWH should remain secondary to the local-first product until local workflows are excellent.

### Phase 15 — Ecosystem & Integrations

**Current state: ~35–45%**

Existing direction includes MCP interoperability, Composio/connectors, GitHub integration and custom MCP infrastructure.

Priority integrations include:

- OpenCode
- OpenHands
- Claude Code
- Qwen Code
- Codex
- Aider
- Gemini CLI
- GitHub/GitLab/Gitea
- VS Code/Neovim/Acode/JetBrains

AWH should consume and expose MCP infrastructure rather than attempting to replace every MCP server.

### Phase 16 — Advanced Infrastructure

**Current state: ~10–20%**

Some sandbox/security infrastructure already exists, including platform-specific process/resource controls and network/secrets policy mechanisms.

Remaining demand-driven work:

- Docker sandbox adapter
- WASM sandbox adapter
- modern macOS sandbox integration
- safe-open primitives
- remote execution
- advanced secret providers
- distributed workspace
- enterprise RBAC

## Current Highest-Priority Work

### P0 — Agent-Grade Editing

Build controlled line-wise/diff-based editing rather than full-file rewrites.

```text
read
 -> locate
 -> validate
 -> patch
 -> verify
 -> snapshot/provenance/audit
```

### P0 — First-Class Worktrees

Make agent isolation a first-class AWH primitive.

```text
awh worktree create/list/inspect/merge/remove
```

### P0 — Unify Capability Enforcement

Make every AWH operation pass through the same capability/policy model.

### P1 — Unify Runtime Provenance

Connect Agent, Session, Workspace, Worktree, File, Git, Tool, Capability, Snapshot, Memory and Audit.

### P1 — Turn TUI Into the Runtime Control Center

The TUI should display the same state exposed by MCP, CLI and Control API.

### P2 — Multi-Agent Collaboration

Implement handoff, ownership, conflict detection and merge events only after worktree isolation is reliable.

### P2 — Remote AWH

Harden the local product first; then expose the same runtime through secure remote APIs.

## Strategic Guardrails

1. **Do not become an agent framework.** External agents own reasoning and planning.
2. **Do not become a LangGraph-style workflow platform.** Keep workflows runtime-oriented and minimal.
3. **Do not become a generic model router.** Model/provider routing is outside the core product boundary.
4. **Do not claim snapshots/policies are equivalent to OS/container sandboxing.** Use real sandbox adapters where isolation is required.
5. **Treat distribution as a product feature.** Installation, upgrades, Android/Termux support and reliable releases matter.
6. **MCP-first, not MCP-only.** CLI, TUI and Control API should expose the same runtime model.
7. **Prefer composable adapters.** Integrate existing ecosystems rather than rebuilding every surrounding tool.
8. **Test complete workflows.** A feature is not complete because unit tests pass; test actual terminal commands, MCP clients and failure/recovery paths.
9. **Optimize for coherence over feature count.** The strongest differentiation is the integrated runtime model, not isolated primitives.

## Product Differentiation

The relevant competitive comparison is not only individual products. AWH is primarily competing with the developer's current glue stack:

```text
Coding Agent
+ multiple MCP servers
+ Git
+ shell scripts
+ workspace conventions
+ snapshots/backups
+ permissions
+ logs
+ ad-hoc integrations
```

AWH should win by making this stack **coherent, local-first, controlled, reversible and easy to operate** from one runtime.

Persistent memory, MCP gateways, sandboxing, agent orchestration and workspace tools are all active/open categories. Therefore AWH should not market any single primitive as uncontested. The differentiation is the integrated runtime boundary between an external AI agent and the developer's real workspace.

## Definition of the Next Milestone

The next major milestone should be a complete end-to-end workflow:

```text
External Agent
  -> AWH Agent/Session
    -> isolated Worktree
      -> capability/policy check
        -> context selection
          -> line-wise/diff edit
            -> automatic snapshot
              -> verification
                -> provenance/audit
                  -> Git diff
                    -> undo/restore when requested
```

If this workflow is reliable through **MCP + CLI + TUI**, AWH will have a much stronger product foundation than simply adding more isolated tools.
