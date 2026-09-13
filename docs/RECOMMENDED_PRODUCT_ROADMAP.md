# Recommended Product Roadmap (Historical Strategic Recommendation)

> **Status: SUPERSEDED for implementation planning.** The canonical final architecture, feature set, CLI contract, dependency graph, and build order are now defined in [`docs/PROJECT_ROADMAP.md`](PROJECT_ROADMAP.md), [`docs/CLI.md`](CLI.md), and [`docs/FEATURES.md`](FEATURES.md).
>
> This document is retained for historical market/positioning context. Its earlier horizon sequencing is not the current build plan.

---

## North Star

**"The local runtime for your coding agent."** One static binary that replaces the memory server + gateway + sandbox + git-tooling stack a solo developer or small team currently assembles by hand around Claude Code, Codex CLI, OpenCode, Aider, or Qwen Code. Not an MCP Gateway company. Not a memory-server company. Not an orchestration framework. The bundle is the product.

---

## Historical Horizon 0 - Foundation

Already solid at the time of this recommendation: MCP infrastructure, workspace/file ops, Git, Context Engine, Skills, and the in-flight security phases.

- Finish ToolBroker consolidation.
- Reduce custom/dynamic MCP path work to what is required for per-agent tool scoping.
- Treat the foundation as hygiene after those gaps close.

---

## Historical Horizon 1 - "The Daily Driver"

**Goal:** a developer can install this in one minute and it is immediately better than several separate MCP servers.

1. Distribution before new features.
2. `awh init` for zero-to-working setup.
3. Agent-agnostic file-edit rollback and automatic snapshots.
4. Git-worktree-per-agent as a first-class primitive.
5. Coding-workflow memory integrated with file/terminal audit and policy.
6. A local, SSH-friendly TUI.

**Historical success metric:** a solo developer chooses AWH over several point-solution servers because it is one thing to install.

---

## Historical Horizon 2 - "Multi-Agent, Locally"

Only after real users. Explicitly not a distributed swarm platform.

1. Agent lifecycle.
2. Session-to-agent binding.
3. Worktree-scoped task delegation.
4. Minimal local approval queue.

---

## Historical Horizon 3 - "Plug into the ecosystem"

1. Expose AWH workspace/memory/security tools as MCP.
2. Track evolving MCP auth/policy primitives.
3. Narrow crash-recovery/checkpoint support.

---

## Historical explicit cuts

- Full global-to-task policy hierarchy unless validated.
- Competing workflow engine, DAG scheduler, or model router.
- General-purpose multi-agent message bus.
- Web command center/dashboard as the default UX.
- Built-in agent teams, marketplace, or controlled self-improvement before real usage validates them.

---

## Historical rationale

The recommendation was based on the view that the combination of local workspace, memory, security, Git and terminal-native UX was more coherent than competing independently on gateways, memory, or orchestration. The current roadmap retains that product-boundary principle but replaces the old horizon sequencing with the dependency-driven final plan in `docs/PROJECT_ROADMAP.md`.
