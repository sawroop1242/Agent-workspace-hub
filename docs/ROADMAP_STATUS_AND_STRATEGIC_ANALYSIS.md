# AWH — Roadmap, Implementation Status, Known Issues & Strategic Analysis

**Prepared by:** Claude (Anthropic), acting as external technical reviewer and roadmap author for the security-foundation track on the `rust` branch.
**Date:** 2026-09-12
**Status of this document:** Synthesis, not a replacement. It sits alongside `docs/PROJECT_STATUS.md`, `docs/completeness-audit.md`, `AWH_FORENSIC_REPORT.md`, and `RUFLO_AWH_ARCHITECTURE_ANALYSIS.md`, updating them with everything verified and shipped since those were written, plus a market analysis those documents do not attempt.

**Methodology note:** every status claim below was checked against live GitHub Actions CI logs (exact test counts pulled from raw job output, not summaries), direct reads of the source files in question, and real merge-status checks against the GitHub API. Market/competitive claims are sourced from web research current to September 2026 and are flagged with explicit confidence levels rather than presented as settled fact. Nothing here is asserted on the strength of a PR description alone.

---

## 1. Current Implementation Status

### 1.1 Subsystem completion table (updated against the original 21-row estimate)

| # | Subsystem | Prior estimate | Current estimate | What changed |
|---|---|---|---|---|
| 1 | MCP Infrastructure | 85% (target 100%) | 100% (frozen) | No change this cycle |
| 2 | Workspace / File System | 80% | 80% | No change this cycle |
| 3 | Git | 80% | 80% | No change this cycle |
| 4 | Skills | 70% | 70% | Unchanged — still executes unsandboxed (Issue #3) |
| 5 | Context Engine | 65% | 65% | No change this cycle |
| 6 | Security Infrastructure | 60% | **~78%** | Verified P1 gate-bypass bug closed (PR #19); workspace policy layer added (PR #20) |
| 7 | Capability System | 10% | **~25%** | `Agent` / `CapabilityGrant` data model + CLI shipped (PR #18) — **not yet enforced per-agent** (Issue #1) |
| 8 | Policy Engine | 0% | **~30%** | Working deny-only, workspace-scoped engine for 3 tools (PR #20) — no hierarchy, no Allow rules, no approval flow yet |
| 9 | Tool Broker | 0% | **~5%** | Phase 4a fully specified; no code merged yet |
| 10 | CLI | 35% | **~40%** | `awh agent *` and `awh policy *` command families added |
| 11 | Control API | 35% | 35% | No change this cycle |
| 12 | Memory | 40% | 40% | No change this cycle |
| 13 | Observability | 30% | **~32%** | Gate denials and policy denials now produce distinguishable audit event names |
| 14 | Agent Runtime | 0% | 0% | Not started |
| 15 | Multi-Agent Messaging | 0% | 0% | Not started |
| 16 | Workflow Engine | 0% | 0% | Not started |
| 17 | DAG Scheduler | 0% | 0% | Not started |
| 18 | Checkpoint / Recovery | 0% | 0% | Not started |
| 19 | Artifact / Provenance | 0% | 0% | Not started |
| 20 | Model Router | 0% | 0% | Not started |
| 21 | TUI | 40% | 40% | No change this cycle |

**Equal-weighted average across all 21 rows: ~30% → ~34%.** This understates what remains: the seven rows still at 0% (Agent Runtime, Multi-Agent Messaging, Workflow Engine, DAG Scheduler, Checkpoint/Recovery, Artifact/Provenance, Model Router) are individually larger in scope than everything shipped in this cycle combined.

---

## 2. Security-Foundation Roadmap — Phase Status

| Phase | Scope | Status | PR | Verified test result |
|---|---|---|---|---|
| 1 | `Agent` + `CapabilityGrant` models, persistence, CLI — zero enforcement, purely additive | Merged | #18 | CI green (fmt/clippy/build x3 OS/audit); ~570 tests passing, 16 new unit + 3 new integration tests, 0 regressions |
| 2 | Built-in tool execution gate (`awh.builtin` reserved trust identity) closing the verified static-tool bypass | Merged | #19 | CI green; **598/598** tests passing, exact match confirmed against raw job log |
| 3 | Workspace-scoped, deny-only policy rules for `workspace.write_file` / `workspace.delete_file` / `terminal.run` | Merged | #20 | CI green (2 workflow runs); **620/620** tests passing, exact match confirmed |
| 4a | `ToolBroker` consolidation — single `authorize_tool()` entry point + declarative `RESOURCE_SCOPED_TOOLS` table | Prompt delivered, no PR yet | — | — |
| 4b | Unify the custom/dynamic MCP server authorization path under the same broker as built-in tools | Not started | — | — |
| 5 | Agent Runtime — spawn/stop/pause/resume/kill, session-to-agent binding, first real per-agent capability enforcement | Not started | — | — |
| 6 | Event Bus + Task DAG + Scheduler | Not started | — | — |
| 7 | Multi-agent messaging + Workflow Engine | Not started | — | — |
| 8 | Checkpoint/Recovery + Resource Governor | Not started | — | — |
| 9 | Artifacts/Provenance + Git worktrees-per-agent | Not started | — | — |
| 10 | Model Router + secret manager + `awh run` + CLI polish | Not started | — | — |

**3 of 11 phases merged and independently verified. 1 in flight. 7 not started.** Phases 1-4a were deliberately sized as the smallest safe slice; Phase 5 onward each represent substantially larger scope with no code written yet.

---

## 3. Full Feature Ledger

### 3.1 Already built - pre-existing baseline
- MCP protocol layer: stdio + HTTP/SSE on one shared `McpDispatcher`, ~70 tools, JSON-RPC correctness, SDK/Inspector interop-tested
- MCP server trust (custom/external servers only): bearer auth, `TrustStore`/`PersistentTrustStore`, `authorize_mcp_execution`, circuit breaker, TLS, rate limiting
- Sandboxing: bwrap (Linux, verified), sandbox-exec (macOS), Job Objects (Windows) — for custom MCP server subprocesses
- Tool Registry: per-tool `risk` (Low/Medium/High) + `required_permissions` metadata for every tool — inert until Phase 2 gave it enforcement teeth
- Audit/observability basics: `audit_allow`/`audit_deny`, secret redaction
- Context Engine (11 modules) — complete
- Core stores: workspace, project, memory, tasks, files — JSON-persisted, path-traversal-safe
- Skills system (13 modules) — runs unsandboxed (Issue #3)
- Services: Git, Terminal — structured argv, no shell strings, timeouts
- TUI: ratatui, 14 screens, local + remote backend
- Control API: `/api/v1` axum router — no agent-management endpoints until Phase 1
- Tunnel: ngrok provider abstraction
- CI: fmt + clippy + build/test x3 OS + `cargo audit`, green

### 3.2 Added via Phases 1-3 (merged)
- `Agent` model + `AgentStatus` lifecycle, `AgentStore`; `CapabilityGrant` model + `CapabilityGrantStore`; `awh agent create|list|inspect|grant|revoke`
- Built-in tool execution gate closing the verified P1 bypass (terminal.run, workspace.write_file/delete_file, git.commit, ~34 total Medium/High-risk tools); `awh mcp trust awh.builtin`; `awh mcp permissions awh.builtin`; terminal.run/git.commit now audited for the first time; source-scan regression test guarding against a future ungated high-risk tool
- Workspace-scoped `PolicyRule`/`PolicyStore` (deny-only, 3 tools); boundary-safe path-prefix + exact-command matching; `awh policy deny|list|remove`; distinct audit trail; atomic writes + file locking; fails closed only on genuine corruption

### 3.3 In progress
- Phase 4a: ToolBroker consolidation (prompt delivered, not yet implemented)

### 3.4 Upcoming (Phases 4b-10)
- Tool Broker unification with the custom/dynamic MCP path
- Agent Runtime: real lifecycle, session-to-agent binding, per-agent MCP tool scoping
- Event Bus, Task DAG, Scheduler
- Multi-agent messaging, Workflow Engine
- Checkpoint/Recovery, Resource Governor
- Artifacts/Provenance, Git worktrees-per-agent
- Model Router, secret manager, `awh run`, `awh doctor`, JSON/JSONL CLI output, config file

### 3.5 Beyond the current roadmap (in the vision docs, not yet phase-scoped)
- Human Approval/HITL (`awh approvals list|approve|deny`)
- Full Global to Task policy hierarchy with Allow rules
- Skills sandboxing
- Built-in Agent Teams
- Controlled self-improvement, human-gated
- Command Center dashboard, full TUI expansion
- Emergency stop with full execution-tree cancellation propagation

---

## 4. Known Issues & Gaps, with resolution prompts

### Issue #1 - Capability System has no per-agent enforcement yet
**What's true today:** `CapabilityGrant` (Phase 1) is a real, tested data model, but nothing consults it. Phase 2/3's gates are per-workspace/per-machine (`awh.builtin`), not per-agent. This is by design (Agent Runtime doesn't exist yet), not an oversight.
**Resolution:** This is Phase 5 by design.
**Prompt when ready:** "Design and implement session-to-agent binding for McpDispatcher: each active session is associated with an Agent id at connection time; authorize_tool additionally consults CapabilityGrantStore::list_for_agent for that session's agent before falling through to the existing awh.builtin/policy checks. Read src/core/agents.rs, src/core/capability_grants.rs, and src/mcp/tool_broker.rs (post Phase 4a) in full before starting."

### Issue #2 - Custom/dynamic MCP server path is not unified with the built-in gate
**What's true today:** is_authorized()/authorize_mcp_execution()/dispatch_custom_mcp() remain a separate, older mechanism from the authorize_builtin/authorize_policy pair built in Phases 2-3.
**Resolution:** Phase 4b, deliberately scoped separately since this is the most mature part of the existing security model.
**Prompt when ready:** "Read src/mcp/dispatcher.rs's dispatch_custom_mcp() and is_authorized() in full, current state. Design how ToolBroker::authorize_tool (from Phase 4a) can also cover the dynamic/custom-provider path without changing authorize_mcp_execution's existing tested behavior for currently-registered custom servers. Do not merge the two trust stores without explicit justification in the PR description."

### Issue #3 - Skills execute unsandboxed
**What's true today:** The skills system is mature but installed skills run with full process privilege, not routed through mcp/sandbox.rs.
**Resolution:** Classify executable vs. instruction-only skill capabilities before sandboxing either.
**Prompt when ready:** "Audit src/skills/*.rs to classify which skill capabilities involve actual code/command execution versus pure instruction/prompt content. For the executable subset only, route execution through the existing mcp/sandbox.rs primitives. Produce the classification as a table in the PR description before writing enforcement code."

### Issue #4 - Policy Engine is deny-only, workspace-scoped, single-level
**What's true today:** Real and tested, but far short of the vision docs' Global to Task hierarchy with Allow rules and approval semantics.
**Resolution:** Intentionally deferred until Agent Runtime (Phase 5) and an approval channel exist.
**Prompt when ready:** "Extend src/core/policy.rs's PolicyRule with an optional effect: Allow | Deny field (default Deny for backward compatibility) and a scope_level once Agent Runtime exists to populate it meaningfully. Do not build Agent/Task scope levels before Phase 5 ships."

### Issue #5 - Distribution/packaging story is unverified
**What's true today:** Not confirmed whether AWH ships as a one-line-installable static binary. Given the competitive landscape (Section 6), this may block adoption more than any remaining phase.
**Resolution:** Verify current release process; prioritize above Phase 4b if absent.
**Prompt when ready:** "Audit .github/workflows/ for any release/binary-publishing job. If none exists, add a release workflow producing static binaries for Linux/macOS/Windows on tag push, plus a Homebrew tap formula. Ship independently of any in-flight phase branch."

### Issue #6 - Duplicate agent/grant ids silently overwrite (non-blocking, noted at PR #18 review)
Consistent with existing TaskStore behavior elsewhere; revisit only if Phase 5 or 4b needs multiple concurrent grants of the same permission type.

### Issue #7 - Coverage tests are source-text scans, not true mutation tests (terminology note from PR #19/#20 review)
The protective effect is real; past PR descriptions overstated the technique as "mutation-tested." Cosmetic fix only: rename to "source-scan regression test" going forward.

---

## 5. Market & Competitive Analysis Summary

The underlying thesis is validated by concrete 2026 incidents: an OpenAI evaluation agent reached Hugging Face production infrastructure in July 2026 via a sandbox gap, and Microsoft disclosed prompt-injection-to-RCE CVEs in Semantic Kernel in May 2026. MCP is now a de facto standard, present in over 80% of observed cloud environments per Wiz, with 97M+ monthly downloads by mid-2026.

However, the specific niches this roadmap targets are not empty:
- "MCP Gateway"/Tool Broker is a mature, named 2026 category with a Rust-native, Linux Foundation-backed leader (agentgateway), plus Octelium, Lunar MCPX, Composio (SOC 2 certified), and gateway offerings from Docker, Microsoft, IBM, and Kong. Cisco announced dedicated MCP security tooling at RSA 2026.
- Cross-session/cross-agent memory for coding CLI agents is, if anything, more crowded: Vestige and akitaonrails/ai-memory ship the nearly identical "single Rust binary, local-first, MCP-native memory server" pitch already, alongside mem0, Letta, Zep, Supermemory, Cognee, and many smaller entrants.
- Orchestration (LangGraph, CrewAI, Microsoft Agent Framework, OpenAI Agents SDK, Google ADK) is a $7.38B+ market with production deployments at Uber, LinkedIn, and Replit.
- Anthropic's own Claude Agent SDK already ships a permission system and subagent machinery for anyone building on Claude specifically.

**Recommended strategic repositioning:** not "best MCP Gateway" or "best memory layer" - those races have visible leaders. The more defensible position is the bundle: one static Rust binary combining workspace + context + memory + skills + git + security + TUI, targeted at developers already using terminal coding agents who are currently stitching together several separate point-solution MCP servers by hand.
- **Keep as-is:** MCP infra, workspace/file ops, Git basics, Context Engine, Skills
- **Near-term priority:** finish security hygiene as table stakes, not a moat; fix distribution (Issue #5); agent-agnostic file-edit rollback; git-worktree-per-agent
- **Medium-term:** minimal local Agent Runtime (a handful of agents on one machine, not a distributed swarm); a terminal-native TUI for observability (a genuinely rarer feature than the others found)
- **Do not build from scratch:** a competing Workflow Engine, DAG Scheduler, or Model Router - interoperate with LangGraph/CrewAI instead of racing them

---

## 6. Conclusion

The security-foundation work completed in this cycle (Phases 1-3, PRs #18-#20) is real, independently verified against live CI and source, not just self-reported, and it correctly followed the project's own architectural rule of building Capability and Policy before anything resembling Agent Runtime. That is a genuinely solid foundation.

But completing the full 11-phase security roadmap does not, by itself, make AWH competitive with where the market has already moved in 2026. Roughly a third of the full 21-subsystem vision is in place, the hardest and largest remaining subsystems are entirely unbuilt, and the specific product categories the roadmap has been aiming at (MCP Gateway, cross-agent memory) both have funded, in some cases foundation-backed, competitors already shipping equivalent or more mature versions of the same idea.

The path forward that gives AWH real potential is not building every phase in the original vision docs faster. It is narrowing the target to the specific bundle nothing else offers - a single local Rust binary that replaces several stitched-together point solutions for developers using terminal coding agents - finishing the security work as necessary hygiene rather than a headline feature, and deliberately not competing on orchestration breadth where the market has already picked winners. That is a smaller, more honest, and more achievable definition of success than the original all-encompassing "AI OS" framing, and it is the one this document recommends adopting going forward.
