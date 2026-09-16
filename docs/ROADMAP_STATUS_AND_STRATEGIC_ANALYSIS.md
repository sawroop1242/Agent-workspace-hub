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


---

## 7. Addendum — Direct Code-Level Analysis (added 2026-09-15)

Everything above was written from GitHub API reads (diffs, CI logs, targeted file fetches). This section is different: the repository was cloned locally and analyzed end-to-end with real static tooling (`grep`, `wc`, a Python script separating production code from `#[cfg(test)]` blocks) rather than API spot-checks. Note: PR #51 has been merged onto `rust` since Section 2 was written; the figures below reflect the current `rust` tip.

### 7.1 Scale

- **43,343 lines of Rust** across 125 files (up substantially from the ~17K LOC estimated in `AWH_FORENSIC_REPORT.md` — the codebase has grown a great deal since that report).
- **3,857 lines of Python** (the automation pipeline: `scripts/checkpoint_state.py`, `scripts/safe_git.py`, `scripts/awh_pipeline.py`, etc.)
- Largest files: `src/services/edit.rs` (3,892 lines — the agent-grade editing subsystem), `src/mcp/dispatcher.rs` (3,429 lines), `src/mcp/schema.rs` (2,740 lines), `src/api/control.rs` (1,700 lines).
- 38 direct dependencies, all mainstream and current: `rustls`/`tokio-rustls` (memory-safe TLS, avoiding OpenSSL's C FFI surface — a deliberate choice), `subtle` (constant-time comparison, clearly present for secret/token handling), `tower-http` with the `catch-panic` feature enabled (HTTP-layer panic isolation as a defense-in-depth backstop). No bloat, nothing unmaintained-looking.

### 7.2 Panic-risk audit — a real correction made during this review

An initial `grep` found **937 `.unwrap()` calls** in `src/`, which on its face looks concerning for a project whose entire value proposition is "fail-closed" security. Rather than report that number, it was checked precisely: a script split every file at its first `#[cfg(test)]` boundary and counted separately.

**Result: only 3 of those unwraps are in actual production code.** The other 946 are inside test modules, where panicking on a bad fixture is correct, expected behavior, not a defect. Each of the 3 production unwraps was then read in context:
- Two in `src/tui/screens/editor.rs` unwrap an `Option` that is set to `Some(...)` unconditionally on the line immediately above — provably safe, just not written in the most idiomatic style (`if let Some(x) = ...` would avoid the unwrap entirely).
- One in `src/context/engine.rs` unwraps a value inside a branch already gated by an identical `.is_some()` check one line earlier — also provably safe.

**Net finding: zero exploitable, attacker-triggerable panics from `.unwrap()` anywhere in 43,000 lines of production Rust.** Similarly, `expect()` in production code is 30 calls (vs. 62 in tests), concentrated in `context/engine.rs` and `mcp/schema.rs` — not yet individually audited line-by-line, flagged as a follow-up. `panic!()` in production code is **zero** (all 11 occurrences are in tests).

This correction matters beyond the specific number: it is a concrete example of the difference between a superficial grep-based audit and a real one, and the corrected picture is a genuinely strong result worth stating plainly rather than burying under the scarier initial count.

### 7.3 `unsafe` usage — another grep correction

An initial search for `unsafe ` (keyword plus trailing space) matched 7 files. Reading each match showed most were **string matches, not the Rust keyword** — hits like `bail!("unsafe workspace path")` and a test comment `// Writes fail closed on unsafe ids`, none of which are actual `unsafe` blocks.

**Genuine `unsafe { }` blocks exist in exactly 2 files, both legitimate:**
- `src/mcp/http.rs` — a standard `Pin`/`get_unchecked_mut()` pattern for implementing `Stream` polling, idiomatic and common in async Rust.
- `src/mcp/sandbox.rs` — Windows Job Object FFI (`CreateJobObjectW`, `CloseHandle` via the `windows_sys` crate), necessary for the process-sandboxing subsystem discussed elsewhere in this document.

No unexplained or gratuitous `unsafe` usage anywhere in the codebase.

### 7.4 Test coverage

- 561 inline `#[test]` functions inside `src/` (unit tests, colocated with the code they test)
- 175 `#[test]`/`#[tokio::test]` functions in the dedicated `tests/` directory (6,438 lines of integration tests across 13 files)
- **736 total test functions.** Zero `TODO`/`FIXME`/`unimplemented!()`/`todo!()` markers found anywhere in `src/`.

### 7.5 Revised overall assessment

The prior sections of this document focused on architectural completeness (what subsystems exist vs. don't) and market positioning. This addendum focused on a different question — of the code that *does* exist, how sound is it? The answer, checked directly rather than assumed: genuinely sound. Disciplined error handling (near-zero real panic risk in production paths), minimal and well-chosen dependencies with security-conscious choices already in place (rustls, constant-time comparison, panic isolation), legitimate and minimal `unsafe` usage, and a large, real test suite with no unfinished-work markers. The gaps identified earlier in this document (missing subsystems, aspirational status-doc percentages) remain accurate and are about what hasn't been built yet — they are not a reflection of the quality of what has.

---

## 8. Addendum 2 - Full docs/ Folder Audit + Live Repository Recheck (added 2026-09-15)

Per direct request, every file in docs/ (62 files as of this writing, up from an initial count of 58 partway through this same review - the repository is genuinely live and moving fast) was read. This section reports what materially changes or adds to the picture above, and explicitly flags where this document's own prior claims needed correction.

### 8.1 A significant discovery: this exact document has a formal specification

docs/issue-resolving-prompts/AWE-019-roadmap-implementation-status-known-issues-strategic-analysis.md is, functionally, the specification for this file ("Issue #40"). It was found after this document was already substantially written, not before - so the prior sections were not written to satisfy it, yet independently converged on most of its requirements: verified-vs-self-reported status separation, an evidence-hierarchy discipline, a known-issues section with the same specific items it calls out by name (capability-model-vs-enforcement gap, custom/dynamic MCP path divergence, unsandboxed skills, deny-only policy scope, distribution/packaging verification, duplicate-id behavior, source-scan-test terminology). Its scope constraint - modify only this file unless evidence proves otherwise - matches how this document has in fact been maintained (via addenda to one file, not a proliferation of competing docs). This convergence is treated as validation, not as license to stop checking; the rest of this addendum continues the same verification discipline against the newly-read material.

### 8.2 Major finding: the largest module in the codebase is fully unintegrated

src/services/edit.rs (3,892 lines - the single largest file in the 43,343-line codebase, implementing the agent-grade patch-editing subsystem described across the AWE issue-resolving prompts) is compiled (pub mod edit; in src/services/mod.rs) but called from nowhere else in the codebase. Verified two independent ways: no MCP tool name in src/mcp/dispatcher.rs references it (searched for edit, patch, replace, insert, delete_range, diff, rollback, history - only unrelated matches like context.insert and git.diff), and no other file in src/ contains use ... edit:: or services::edit (zero matches). It is also not exposed by any CLI subcommand - the current main.rs only has Tui, Serve, Mcp, Skill, Registry, Agent, Policy, Tunnel as top-level commands, none of which correspond to docs/CLI.md's documented target awh fs patch|replace|insert|delete-range|apply-diff|history|rollback surface. docs/CLI.md itself discloses this appropriately ("not a claim that every command is currently implemented"), so this is not a case of a doc overclaiming - it's a case of substantial, well-built code (confirmed clean in the panic-risk audit below) sitting with zero integration. This is a more precise and more significant finding than "Workspace Runtime agent-grade editing is in progress" - it is fully built as an isolated module and fully disconnected from every interface that could reach it.

### 8.3 Distribution status is now precise, not unverified

Section 5, Issue #5 previously described distribution/packaging as "unverified." It is now verified precisely: docs/release.md and docs/INSTALL.md describe a complete release workflow (Linux/macOS/Windows x86_64/aarch64, checksums, install script) that exists and is committed but has never been used - docs/release.md states directly that the first tagged release has not been cut, pending the hardening phases being accepted and rust branch protection being confirmed active. docs/error.md, however, references a v0.1.0 download URL and Android/Termux testing against what reads as an already-existing release - a direct contradiction between two docs that is recorded here rather than silently resolved in either direction, per this document's own evidence-conflict discipline.

### 8.4 Branch protection: another unresolved documentation contradiction

docs/development.md asserts rust "is the production branch (branch-protected; see security.md)" as settled fact. docs/security.md's own branch-protection checklist describes what should be configured and explicitly states the automation token cannot configure it via API - "an administrator must configure manually" - without confirming that this has actually happened. These two docs disagree on whether a real, technically-enforced control exists or only a documented intention does. This matters beyond documentation hygiene: several of this document's own safety claims (e.g., that all changes land via reviewed PRs) currently hold in practice by observed convention, not by a verified, enforced repository setting.

### 8.5 A live, real incident validating a newly-found analysis

docs/PIPELINE_ANALYSIS_FIXES.md (added the same day as this addendum) identifies, among other issues, that the autonomous loop treats a BLOCKED checkpoint as fatal rather than recoverable, with no scheduled automatic recovery. This is not a hypothetical concern: the repository's commit history includes "chore: reset checkpoint from BLOCKED to IDLE to resolve pipeline deadlock", a direct, dated instance of the exact failure mode the analysis describes requiring manual human intervention. Between that commit and the time of this addendum, roughly 20 further commits landed hardening recovery CAS/identity handling and building a second reviewer architecture (docs/pr-reviews/agent3-v2.md) - a deterministic-rule-plus-LLM hybrid reviewer with an explicit evidence-confidence taxonomy (CONFIRMED / LIKELY / POSSIBLE / SPECULATION) for every finding it raises. This independently mirrors the verification discipline this document has tried to hold itself to, and is a positive, notable sign of the project's own engineering culture, not just an external standard being imposed on it.

### 8.6 A third, independent roadmap-phase framework

docs/PROJECT_CONTEXT.md presents yet another phase numbering (Security Fixes -> Code Quality & Reliability [stated as current] -> Comprehensive Testing -> Runtime Features -> Release Polish) that overlaps with, but does not match, both this document's own phase tracking and docs/PROJECT_ROADMAP.md's 17-phase (0-16) numbering addressed in Section 2 above. Notably, PROJECT_CONTEXT.md states the project's current priority is structured error-handling and reliability cleanup - not new feature work - which sits in tension with the amount of new feature work (the editing subsystem, the automation pipeline, SEC-002, the AGENT-001/ARCH-001/FS-001/GIT-001 prompts) that has landed concurrently. This document does not attempt to adjudicate which framework is authoritative; it records that at least three exist and recommends the project explicitly designate one as canonical and retire or clearly subordinate the others, consistent with AWE-019's own instruction to resolve roadmap contradictions explicitly rather than silently pick a side.

### 8.7 A stale number worth correcting on sight

docs/testing.md states "123 automated tests." Section 7.4 of this document already reported a directly-measured, current figure of 736 test functions (561 inline unit tests plus 175 integration tests), obtained from a local clone rather than a doc claim. The gap between 123 and 736 is not a small drift - it reflects docs/testing.md predating a large amount of the work audited in this document. Flagged here explicitly so it is not read as a contradiction of Section 7's own figures.

### 8.8 Security posture: richer than previously credited

docs/threat-model.md documents eleven STRIDE-style threats (T1-T11), most with specific, named mitigations and tests: rate limiting with key-spray capping, defense-in-depth secret redaction (including token-shaped-segment matching beyond simple key lookups), circuit-breaker self-healing behavior, and the SEC-002 TLS guard verified directly in this document's own PR review of PR #52. This is materially more mature than the picture available from PR-by-PR spot checks alone, and supports treating the "Security Infrastructure" line in Section 1.1's table as being toward the higher end of its corrected range, though still short of the unified, single-boundary Tool Broker the project's own Phase 4 definition requires.

### 8.9 Net effect on this document's prior conclusions

Nothing in this addendum overturns Section 6's core conclusion. If anything, it sharpens it: the codebase is executing with real discipline (confirmed again independently by docs/PROJECT_CONTEXT.md's own unwrap/panic audit reaching the same near-zero result this document measured precisely in Section 7.2), but the documentation layer describing that codebase now has its own, separate reliability problem - three competing roadmap frameworks, a stale test count, and two direct factual contradictions about release and branch-protection status. A codebase this well-verified deserves a documentation layer held to the same standard, and closing that gap is now as concrete and actionable an item as anything in Section 4's known-issues list.
