# Recommended Product Roadmap (Claude's opinionated recommendation)

**Status:** This is an opinionated recommendation, not a directive. It sits alongside the original vision docs (`AWH_project_report.md`, `what_is_AWH.md`, `docs/MASTER_PROMPT.md`) and `docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md` rather than replacing them. It answers a direct question: "if you were building this, what would your roadmap be?"

## North Star

**"The local runtime for your coding agent."** One static binary that replaces the memory server + gateway + sandbox + git-tooling stack a solo developer or small team currently assembles by hand around Claude Code, Codex CLI, OpenCode, Aider, or Qwen Code. Not an MCP Gateway company. Not a memory-server company. Not an orchestration framework. The bundle is the product - see `docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md` Section 5 for the market evidence behind this positioning.

---

## Horizon 0 - Foundation (mostly done; finish it, then stop touching it)

Already solid: MCP infrastructure, workspace/file ops, Git, Context Engine, Skills, and the in-flight security phases (1-3 merged, 4a specified).

- Finish **Phase 4a** (ToolBroker consolidation) as already scoped - cheap, low-risk, closes a real self-inflicted gap (the manual gate+policy call pairing).
- Do **Phase 4b** (custom/dynamic MCP path unification) in **reduced form only**: just enough to make per-agent tool scoping possible in Horizon 2. Do not chase feature parity with agentgateway/Octelium on this layer - that race already has a result forming.
- After that, this layer is hygiene, not the pitch. Stop investing further engineering time here beyond maintenance.

---

## Horizon 1 - "The Daily Driver"

**Goal:** a developer can install this in one minute and it is immediately better than the 3-4 separate MCP servers they were stitching together.

1. **Distribution, before any new feature.** Static binaries for Linux/macOS/Windows published on tag push, a Homebrew tap, a one-line install script, an npx-style wrapper for discoverability. If this does not already exist, it outranks everything else in this document.
2. **`awh init`** - zero-to-working `.agent/` setup in one command, no hand-written config file required first.
3. **Agent-agnostic file-edit rollback**: `awh workspace undo`, automatic snapshot-before-write, independent of git commits. Gives any MCP-connected agent a safety net that today only exists inside specific vendor products, exposed standalone for the first time.
4. **Git-worktree-per-agent as a first-class primitive**: `awh agent create --worktree` automatically creates an isolated branch, ties provenance to commits, and detects merge conflicts before they happen. This is the single feature in this roadmap worth betting on hardest - no direct competitor was found packaging this as a built-in mechanic.
5. **Memory, repositioned rather than abandoned.** Do not compete with Vestige/mem0/Letta on retrieval sophistication - that race is lost. The actual differentiator: memory access, file access, and terminal access share one audit trail and one policy store, which no point-solution memory server can offer since none of them touch your files or terminal.
6. **A genuinely good local TUI.** Every competitor found in market research pushes a web dashboard or a cloud console. An SSH-friendly, zero-browser TUI running on the same machine as the agent is rarer than anything in the gateway or memory space and directly serves the "local-first" positioning.

**Success metric:** a solo developer chooses AWH over "point-solution memory server + point-solution gateway + point-solution sandbox" because it is one thing to install instead of four.

---

## Horizon 2 - "Multi-Agent, Locally"

Only pursued after Horizon 1 has real users. Explicitly **not** a distributed swarm platform.

1. Real Agent lifecycle (spawn/stop/pause/resume), deliberately capped at "a handful of agents on one machine."
2. **Session-to-agent binding.** This is the step that finally makes Phase 1's CapabilityGrant model mean something rather than being unused data (closes Issue #1 from `ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md`).
3. Task delegation scoped **only** to the git-worktree model from Horizon 1: a Manager agent assigns worktree-scoped work to sub-agents. Not a general-purpose message bus.
4. A minimal local approval queue: `awh approvals list|approve|deny`. Cheap to build for a single local user and a real gap versus the crowded memory/gateway tools, most of which do not handle this well for solo/small-team use.

**Success metric:** a small team runs 2-3 agents against one repository safely, without needing Kubernetes or a hosted control plane.

---

## Horizon 3 - "Plug into the ecosystem, don't compete with it"

1. Expose AWH's own workspace/memory/security tools **as an MCP server** so LangGraph-, CrewAI-, or Claude Agent SDK-based systems can consume it directly. This turns the layer AWH cannot realistically win (general orchestration) into a customer instead of a rival.
2. Track MCP's own evolving auth/policy primitives (OAuth scopes, incremental consent) and build adapters to them rather than diverging further from the standard as it matures.
3. Checkpoint/recovery scoped narrowly to "resume this local session after a crash" - not a distributed durable-execution platform, which is exactly LangGraph's core value proposition and not winnable from scratch.

---

## Explicit cuts - do not build these unless Horizons 1-2 prove out real usage first

- **Full Global to Task policy hierarchy.** Overkill for the actual target persona (solo developer / small team). Keep it to workspace rules plus the agent-scoping added in Horizon 2, and stop there.
- **A competing Workflow Engine, DAG Scheduler, or Model Router.** LangGraph and Microsoft Agent Framework have multi-year, funded head starts with production deployments at companies like Uber and LinkedIn. Not winnable from scratch, not worth the attempt.
- **A general-purpose multi-agent message bus.** Build only what the worktree-delegation pattern in Horizon 2 actually needs.
- **A Command Center / web dashboard.** This directly contradicts the "local-first, terminal-native" positioning meant to be the differentiator - a web dashboard is exactly what the competing gateways and orchestration platforms already offer.
- **Built-in Agent Teams, a skill/agent marketplace, or controlled self-improvement.** Interesting long-term ideas, but a resource sink today for a persona and usage pattern that does not exist yet. Revisit only after Horizon 1 has demonstrated real, retained users.

---

## Why this ordering, in one paragraph

Every individual layer this project could compete on (MCP Gateway, cross-agent memory, multi-agent orchestration) already has funded or foundation-backed leaders as of September 2026. The only combination not found occupied during this review is the full local bundle - workspace, memory, security, and git together in one binary with a terminal-native interface - for the specific persona of a developer already using a terminal coding agent who wants one thing to install instead of several. This roadmap is built to reach that specific, checkable claim as directly as possible, and to deliberately avoid spending effort on categories where the market has already picked winners this project cannot realistically catch.
