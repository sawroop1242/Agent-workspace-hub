# AWH Roadmap — Phase-by-Phase Gap Matrix

> **Status snapshot:** 2026-09-12  
> **Branch:** `rust`  
> **Source:** `docs/PROJECT_ROADMAP.md` + `docs/PROJECT_ROADMAP_STATUS.md`  
> **Purpose:** Convert the roadmap status into an actionable dependency-aware gap matrix.

## How to Read This Matrix

- **Current implementation** = what is already materially present in the repository.
- **Remaining gaps** = work required to reach the roadmap target, not merely missing nice-to-have features.
- **Dependencies** = capabilities that should exist before the phase can be considered reliably complete.
- **Priority** = relative engineering priority for the current AWH strategy.
- **Recommended next milestone** = the smallest meaningful milestone that moves the phase toward completion.

Priority levels:

- **P0 — Critical:** directly required for the core AWH runtime/product boundary.
- **P1 — High:** required for a coherent production-quality system after P0.
- **P2 — Medium:** important expansion after the core runtime is coherent.
- **P3 — Later:** valuable ecosystem/remote expansion.
- **P4 — Demand-driven:** implement only when real use cases justify it.

## Master Gap Matrix

| Phase | Current implementation | Remaining gaps | Dependencies | Priority | Recommended next milestone |
|---|---|---|---|---|---|
| **0 — Foundation & Release** | Modular Rust architecture, config, structured errors, tracing/logging, CI, release workflow, installer, docs and automated tests are established. | Production release gates, distribution/package support, Android/Termux verification, upgrade/uninstall validation, release hardening. | Stable core APIs; reproducible builds; cross-platform CI. | **P0** | Ship one reproducible release candidate with Linux/macOS/Windows plus verified Android/Termux binary/install path. |
| **1 — MCP Infrastructure** | Mature stdio + HTTP/SSE MCP stack with auth, sessions, permissions/trust, discovery, limits/timeouts, audit, skills, tasks, memory, connectors and management commands. | Real-client interoperability validation, SSE/resource-boundary hardening, platform sandbox probes, connection reliability. | Stable workspace/capability APIs; real MCP client test matrix. | **P0** | Pass an end-to-end MCP interoperability suite against OpenCode, OpenHands, Claude Code, Qwen Code/Codex where supported. |
| **2 — Workspace Runtime** | Workspace/filesystem services, secure path handling, atomic writes and file operations are present. | Agent-grade line-wise/diff editing, context matching, conflict detection, verification and rollback integration. | Capability/policy engine; snapshots; audit/provenance; workspace locking. | **P0** | Implement `filesystem.patch/replace/insert/delete_range/apply_diff/rollback` using `read → locate → validate → patch → verify`. |
| **3 — Git & Isolation** | Substantial Git service: status, staging, commit, log, branches, diff, push/pull, reset, clean, branch deletion, high-risk classification and validation. | First-class AWH worktrees; worktree lifecycle; Agent/Session/Workspace/Worktree linkage; safe merge/remove workflows. | Workspace model; agent/session model; capability/policy; Git service. | **P0** | Deliver `awh worktree create/list/inspect/merge/remove` with isolated agent workspaces. |
| **4 — Capability & Policy** | Capability grants, policy structures, agents and MCP permission/trust enforcement exist. | One authoritative enforcement path across filesystem, Git, process, network, MCP and secrets; inheritance/scoping; temporary grants. | Agent/session identity; resource model; tool broker; audit. | **P0** | Route every privileged AWH operation through `Identity → Agent → Session → Workspace → Capability → Resource → Policy → Tool`. |
| **5 — Snapshots/Undo/Provenance** | Snapshot subsystem exists and is substantial; security/audit events already exist. | Automatic pre-change snapshots, change provenance, file/session/action history, restore/undo APIs and explainability. | Patch editing; unified audit model; workspace/Git identity. | **P0** | Complete one reversible edit transaction: snapshot → patch → verify → provenance → audit → undo/restore. |
| **6 — Context Engine** | Budgeting, token accounting, scoring, selection, compression, offloading, planning, policy and snapshot support exist. | Runtime-aware context assembly from workspace/Git/session/memory/skills/tool results; stale-context detection; default agent integration. | Agent/session model; workspace/worktree; memory; audit/change events. | **P1** | Build `awh context build` for a real agent session that automatically selects relevant files, Git changes and recent runtime state within a token budget. |
| **7 — Developer Memory** | Memory subsystem and MCP memory operations exist; isolation/size controls are established. | Strong links between memory and project/workspace/Git/agent/session/files/snapshots; decision/architecture records; lifecycle/forget semantics. | Stable agent/session/workspace IDs; provenance; context engine. | **P1** | Implement project-scoped memory records that can be traced to an Agent/Session/Workspace and consumed by context building. |
| **8 — Skills & Capability Packages** | Skills subsystem, registry and MCP skill operations exist; privilege separation is present. | Standard package metadata, requested capabilities, policy requirements, activation lifecycle, versioning and capability-aware loading. | Capability/policy engine; registry; agent/session identity. | **P1** | Define and enforce a canonical skill manifest: purpose + tools + capabilities + policy + inputs/outputs + version. |
| **9 — Agents & Sessions** | Agent structures and MCP session handling exist. | AWH-native session model distinct from MCP protocol sessions; lifecycle commands; linkage to worktree/capabilities/context/memory/snapshots/audit. | Worktrees; capability engine; unified runtime state; Control API. | **P0** | Implement `Agent → Session → Workspace/Worktree` as the canonical runtime identity and expose lifecycle through CLI/API/MCP. |
| **10 — Audit & Observability** | Audit service and structured security/tracing events exist; MCP audit operations are available. | Unified provenance schema across all runtime entities, query/filter commands, change history, explainability, metrics and event correlation. | Agent/session IDs; tool broker; snapshots; capability/policy decisions. | **P1** | Produce a single correlated event stream for an agent edit, including tool, capability decision, file change, snapshot and Git state. |
| **11 — TUI** | Real TUI subsystem and remote TUI infrastructure exist. | Unified runtime-state presentation; actions against the same Control API/core state; workspace/agent/session/worktree/capability/snapshot/audit views. | Unified runtime model; Control API; audit/events. | **P1** | Turn TUI into a read/write control center for one complete agent session rather than adding isolated screens. |
| **12 — Multi-Agent Collaboration** | Agent foundation and isolation primitives exist; early collaboration infrastructure is present. | Worktree-backed handoff, task ownership, conflict detection, merge notifications and agent events. | Phase 3 worktrees; Phase 9 sessions; Phase 10 events/audit. | **P2** | Support two agents working on isolated worktrees with explicit ownership, handoff and merge/conflict events. |
| **13 — Control API** | `src/api/control.rs` is already substantial and exposes meaningful runtime control. | Stable API contract, complete domain coverage, scoped auth, capability authorization, event streaming and remote hardening. | Unified runtime model; capability/policy; audit/events. | **P1** | Freeze a versioned Control API covering workspace, agent, session, worktree, capability, snapshot, Git, memory and audit operations. |
| **14 — Remote AWH** | HTTP/SSE, TLS, bearer auth, Control API and remote TUI foundations exist. | Full remote workspace/session lifecycle, secure synchronization, reconnection/recovery, remote audit/control UX. | Stable Control API; capability authorization; local runtime coherence. | **P3** | Demonstrate one secure remote session controlling an AWH workspace with reconnect and scoped authorization. |
| **15 — Ecosystem & Integrations** | MCP interoperability, Composio/connectors, GitHub integration and custom MCP infrastructure exist. | Broad vendor/client verification, polished adapters and documentation for major coding agents/IDEs. | Stable MCP, Control API, capabilities and runtime model. | **P2** | Publish integration conformance guides/tests for OpenCode, OpenHands, Claude Code, Qwen Code, Codex and major editor workflows. |
| **16 — Advanced Infrastructure** | Some sandbox/security/resource/network/secrets infrastructure exists. | Docker/WASM adapters, modern platform sandboxing, safe-open primitives, remote execution, advanced secret providers, distributed workspace, enterprise RBAC. | Mature policy/capability model; stable Control API; proven runtime semantics. | **P4** | Add one demand-driven sandbox adapter with explicit capability enforcement and end-to-end audit rather than expanding all advanced targets at once. |

## Dependency-Critical Ordering

The phases are numbered, but they should **not** be implemented as a purely linear checklist. The most important dependency chain is:

```text
Phase 0 Foundation
      ↓
Phase 9 Agent/Session identity
      ↓
Phase 4 Capability/Policy
      ↓
Phase 2 Agent-grade editing
      ↓
Phase 5 Snapshot/Provenance
      ↓
Phase 3 Worktree isolation
      ↓
Phase 10 Unified Audit
      ↓
Phase 6 Context integration
      ↓
Phase 11 TUI / Phase 13 Control API
      ↓
Phase 12 Multi-agent
      ↓
Phase 14 Remote AWH
```

**Phase 1 MCP** cuts across this chain as the primary external integration surface rather than being a prerequisite for every internal component.

**Phase 7 Memory** and **Phase 8 Skills** should plug into the unified runtime model instead of evolving as isolated subsystems.

## Cross-Phase Critical Gaps

### 1. No single authoritative runtime identity

The largest architectural gap is not another tool. AWH needs one canonical relationship:

```text
Agent
  -> Session
    -> Workspace
      -> Worktree
        -> Capability/Policy
          -> Context
          -> Memory
          -> Snapshots
          -> Tools
          -> Audit
```

Without this relationship, existing subsystems remain individually useful but operationally fragmented.

### 2. Editing is not yet an agent-grade transaction

AWH should not depend on agents repeatedly rewriting entire files. The core editing primitive should be patch-oriented and verifiable:

```text
read
  → locate
  → validate context
  → apply minimal patch
  → verify result
  → snapshot/provenance
  → audit
```

This is one of the highest-value missing runtime capabilities.

### 3. Worktrees need to become first-class

Git support is already substantial, but AWH's product value increases significantly when each agent/session can receive an explicit isolated worktree with a known lifecycle.

### 4. Capability enforcement needs one boundary

MCP permission checks, capability grants, policy decisions and future process/network/secrets controls should converge on the same authorization model. Avoid separate authorization logic for every subsystem.

### 5. Audit must become provenance, not only logging

The useful question is not merely:

> "What happened?"

It is:

> "Which agent, in which session/worktree, invoked which capability/tool, changed which resource, under which policy decision, with which snapshot, and what was the resulting Git state?"

### 6. TUI, CLI, MCP and Control API should expose the same core

The interfaces should be different views/control surfaces over one AWH runtime, not four partially independent implementations.

## Recommended Milestone Sequence

### Milestone A — Safe Agent Editing

Complete Phase 2 + the minimum pieces of Phases 4/5/10 required for a transaction-safe edit.

**Acceptance flow:**

```text
Agent session
→ capability check
→ read file
→ minimal patch
→ automatic snapshot
→ verify
→ audit/provenance
→ Git diff
→ undo
```

### Milestone B — Isolated Agent Workspaces

Complete the core of Phase 3 + Phase 9.

**Acceptance flow:**

```text
awh agent add
→ awh session start
→ awh worktree create
→ agent edits isolated tree
→ inspect diff
→ merge/remove
```

### Milestone C — Unified Runtime Provenance

Complete the critical parts of Phases 5, 9 and 10.

**Acceptance target:**

```text
awh explain src/main.rs
```

can answer which agent/session/worktree changed the file, which tool and capability were used, what policy decision occurred, which snapshot exists, and what Git change resulted.

### Milestone D — Runtime-Aware Context

Integrate Phase 6 with the canonical Agent/Session/Workspace/Worktree model and Phase 7 memory.

### Milestone E — One Control Surface

Make CLI, MCP, TUI and Control API operate against the same runtime state and authorization layer.

### Milestone F — Multi-Agent Runtime

Only after worktree isolation and provenance are reliable, add handoff, ownership, conflict and merge events.

### Milestone G — Remote AWH

Only after the local runtime is coherent and the Control API is stable, expose the same model securely over the network.

## Definition of "Roadmap Complete"

A phase should not be marked complete merely because its module or commands exist. A phase is complete when:

1. the implementation exists;
2. authorization/capability boundaries are enforced;
3. the feature integrates with the canonical runtime model;
4. failures and recovery paths are tested;
5. CLI/MCP/API behavior is consistent where applicable;
6. audit/provenance is available for consequential operations;
7. real end-to-end workflows pass, not only unit tests;
8. documentation and operational behavior match the implementation.

This prevents AWH from accumulating many technically implemented but disconnected subsystems.

## Immediate Engineering Focus

For the current roadmap state, the recommended order is:

1. **Agent-grade patch editing**
2. **First-class agent worktrees**
3. **Canonical Agent/Session runtime model**
4. **Unified capability/policy enforcement**
5. **Snapshot + provenance + audit transaction**
6. **Runtime-aware context/memory integration**
7. **TUI/Control API convergence**
8. **Small, worktree-based multi-agent collaboration**
9. **Remote AWH**
10. **Advanced infrastructure only when demanded by real users**

The core strategy is to make AWH **coherent before making it larger**.
