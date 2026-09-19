# AWH — Reconciled Roadmap (v2)

**Supersedes for planning purposes:** `docs/RECOMMENDED_PRODUCT_ROADMAP.md` (PR #41's horizon framing is preserved as strategic context, not discarded)
**Reconciles:** `docs/PROJECT_ROADMAP.md`, `docs/PROJECT_ROADMAP_STATUS.md`, `docs/ROADMAP_GAP_MATRIX.md`, the AWE-001..004 issue-resolving prompts, and the verified security-phase work (PRs #18-#21)
**Correction to prior claim:** `docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md` (PR #40) stated Phase 4a had no PR yet. This was wrong: **PR #21 is merged**, CI green (fmt/clippy/build x3 OS/dependency audit).

---

## 1. Why a v2 was needed

`docs/PROJECT_ROADMAP.md` and its companion status/gap-matrix docs are more thorough than the roadmap in PR #41 in one specific way: they enumerate concrete CLI commands and sub-features (`awh worktree merge`, `awh explain <file>`, `filesystem.apply_diff`, session lifecycle, shell completion, Android/Termux distribution) that PR #41 only gestured at as "horizons." That granularity is genuinely useful and is adopted here.

However, `PROJECT_ROADMAP_STATUS.md`'s completion percentages were not verified against source the way every other claim in this review has been. A direct tree search for `snapshot`, `worktree`, `session`, `provenance`, and `undo` modules found only `src/context/snapshot.rs` (an unrelated context-budget mechanism). The claimed 65-75% (Snapshots), 60-70% (Sessions), and 25-35% (Multi-Agent) are corrected downward below to reflect what actually exists, using the same verification standard applied to every other status claim in this repository's review history.

---

## 2. Corrected status, phase by phase

| Phase (per `PROJECT_ROADMAP.md` numbering) | Self-reported | Source-verified correction | Reasoning |
|---|---|---|---|
| 0 — Foundation & Release | 85% | **~60%** | Rust architecture/config/errors/logging/CI are real and strong. No verified release workflow producing installable binaries was found — distribution (Issue #5, prior report) remains open. |
| 1 — MCP Infrastructure | 90-95% | **~95%** (agree) | Matches independent verification across this entire review — the strongest, most mature subsystem. |
| 2 — Workspace Runtime | 75-80% | **~78%** (agree, minor) | Filesystem/security foundations verified solid. Agent-grade patch editing (AWE-001..004) is designed but unexecuted — see Section 3. |
| 3 — Git & Isolation | 55-60% | **~45%** | Base Git service (status/diff/commit/branch) verified solid and substantial. First-class worktrees do not exist in source at all (zero `worktree` matches in the tree) — the status doc's own text agrees this is "a major gap," so the number is corrected down to reflect that the headline feature of this phase is fully unbuilt, not partially built. |
| 4 — Capability & Policy | 70-75% | **~40%** | This phase's own definition requires "one authoritative enforcement path across filesystem, Git, process, network, MCP and secrets." Verified reality: built-in tools are gated (PRs #19-21), workspace-scoped deny policy exists for 3 tools (PR #20), but the custom/dynamic MCP path remains a separate mechanism (Phase 4b, not started), and there is no per-agent enforcement (no session-to-agent binding exists). Real progress, but well short of "one boundary." |
| 5 — Snapshots/Undo/Provenance | 65-75% | **~5%** | No `snapshot` (workspace-level), `provenance`, or `undo` module exists anywhere in `src/`. `src/context/snapshot.rs` is a distinct, unrelated mechanism (context token-budget state, not file-change history). This phase is effectively unbuilt; only the AWE editing prompts (unexecuted) touch adjacent ground. |
| 6 — Context Engine | 75-85% | **~75%** (agree) | Independently verified as complete/mature earlier in this review (11 modules: budget, compressor, planner, scoring, selector, snapshot, tokens). |
| 7 — Developer Memory | 65-70% | **~40%** (agree with prior review) | Basic memory CRUD exists; the cross-linking to Agent/Session/Git/Snapshot this phase requires does not, since Session and Snapshot themselves do not yet exist. |
| 8 — Skills & Capability Packages | 75-80% | **~70%** (agree) | Matches prior verification. Unsandboxed execution remains a real open issue (Issue #3, prior report). |
| 9 — Sessions & Agent Management | 60-70% | **~20%** | `Agent`/`AgentStatus`/`AgentStore` exist and are tested (PR #18). No distinct `Session` model exists anywhere in source — zero matches for a session module separate from MCP protocol sessions. The phase's own stated goal ("AWH-native session model distinct from MCP protocol sessions") is unbuilt. |
| 10 — Audit & Observability | 75-80% | **~35%** (corrected down) | Basic `audit_allow`/`audit_deny` events exist and were meaningfully improved in PRs #19-20 (distinguishable event names per denial source). The "unified provenance schema across all runtime entities" this phase requires cannot exist yet because Session, Worktree, and Snapshot — three of the entities it needs to correlate — do not exist. |
| 11 — TUI | 55-65% | **~40%** (agree with prior review) | Real, working TUI with 14 screens confirmed. Not yet a unified runtime control center because the runtime model it would surface (Session/Worktree/Snapshot) isn't built. |
| 12 — Multi-Agent Collaboration | 25-35% | **~2%** | Only the `Agent` data model (PR #18) exists, with zero per-agent enforcement, zero worktree isolation, zero handoff/ownership/conflict detection. This phase is correctly listed as low-priority (P2) in the gap matrix, but its self-reported percentage overstates what exists. |
| 13 — Control API | 60-70% | **~35%** (agree with prior review) | `src/api/control.rs` is substantial but has no agent-management endpoints, matching earlier verification. |
| 14 — Remote AWH | 30-40% | **~35%** (agree) | HTTP/SSE/TLS/bearer-auth foundations verified real; full remote session lifecycle unbuilt. |
| 15 — Ecosystem & Integrations | 35-45% | **~35%** (agree) | MCP interop and Composio/GitHub connectors verified present. |
| 16 — Advanced Infrastructure | 10-20% | **~10%** (agree) | Sandbox primitives for custom MCP servers verified real; Docker/WASM adapters and enterprise RBAC unbuilt. |

**The pattern in every correction**: phases whose headline feature is a *new, named subsystem* that doesn't exist in source at all (worktrees, snapshots/undo/provenance, sessions) were self-reported at 55-75% but are closer to 0-20% in reality. Phases whose headline feature is an extension of something *already independently verified* (MCP, Context Engine, Skills, Control API) have self-reported numbers that hold up.

---

## 3. What actually happened since the last status doc (PR #40)

- **PR #21 merged**: Phase 4a (ToolBroker consolidation) is done. CI green across fmt/clippy/3-OS build/dependency audit. Not yet independently line-reviewed with the same depth as PRs #18-20 — flagged here as an open verification item, not a rejection.
- **AWE-001 through AWE-004 exist as fully-specified, unexecuted prompts** (`docs/issue-resolving-prompts/`), targeting the exact gap this document's own Section 2 identifies as the largest one relative to the roadmap's stated priorities: agent-grade patch editing (`EditTransaction`, `EditOperation`, contextual replacement, line-range insert/delete, multi-file patch), aimed at future PRs #22-25.

---

## 4. The reconciled roadmap, with explicit reasoning for every inclusion and cut

### Keep as immediate next priority: AWE-001..004 (agent-grade editing)

**Why kept, and promoted ahead of my own previously-proposed Horizon 1 items:** `ROADMAP_GAP_MATRIX.md` independently arrives at the same conclusion my own market analysis did from a different angle — full-file rewrites are the weakest part of how AWH currently exposes editing to agents, and a patch-oriented transaction model is both P0 in the gap matrix and a genuine product differentiator (it directly enables the Horizon-1 "file-edit rollback" feature from PR #41's roadmap once combined with Section 5 below). These prompts are already well-scoped, follow the same "smallest coherent patch, read-first, don't duplicate existing abstractions" discipline used in my own Phase 1-4a prompts, and should be executed next, in order, exactly as `AWE-SEQUENTIAL-IMPLEMENTATION-MASTER-PROMPT.md` specifies.

### Keep, re-scoped smaller: Snapshots/Undo/Provenance (Phase 5)

**Why kept:** this is the single most-cited "core differentiator" across all of `PROJECT_ROADMAP.md`, `ROADMAP_GAP_MATRIX.md`, and my own PR #41 (as "agent-agnostic file-edit rollback"). Three independent planning passes converged on this without copying each other — that's a strong signal it's real.
**Why re-scoped:** build it as the direct output of AWE-001..004 (snapshot-before-patch, tied to `EditTransaction`), not as a separate, larger subsystem first. Building snapshots before the edit model that would use them risks the same "orphaned abstraction" failure mode `ROADMAP_GAP_MATRIX.md` itself warns about in its "Cross-Phase Critical Gaps" section.

### Keep, but sequenced later than the gap matrix suggests: first-class worktrees (Phase 3) and a distinct Session model (Phase 9)

**Why kept:** both are real, both are currently at true 0% despite self-reported numbers suggesting otherwise, and both are genuinely necessary before per-agent capability enforcement (the actual definition of a working "Capability System") can exist.
**Why sequenced after editing/snapshots rather than before, diverging from `ROADMAP_GAP_MATRIX.md`'s stated dependency chain:** the gap matrix places Agent/Session identity before agent-grade editing. This document instead sequences editing first, because it is fully specified and ready to execute today (AWE-001..004), while Session/Worktree design work has not yet started at all. Shipping the ready work first is lower-risk than blocking on unstarted design.

### Cut, or rather deferred indefinitely: full Global-to-Task policy hierarchy, Allow rules, formal approval workflow

**Why cut:** unchanged reasoning from PR #41 — Agent and Task policy scopes require a real Session model to mean anything, and building them before Session/Worktree exist would create policy rules that bind to nothing. `PROJECT_ROADMAP.md`'s own Phase 4 description lists "temporary permissions" and "approval requests" as aspirational checklist items without committing to a sequence — this document commits to deferring them explicitly rather than leaving them ambiguously "someday."

### Cut: distributed multi-agent swarm, general-purpose message bus, DAG workflow engine, generic model router

**Why cut:** all three source documents (`PROJECT_ROADMAP.md`'s guardrails, `ROADMAP_GAP_MATRIX.md`'s "do not" list, and my own market analysis in PR #40) independently arrive at the same cut for the same underlying reason — LangGraph, Microsoft Agent Framework, and dedicated model routers already have multi-year, funded head starts with production deployments. Three independent analyses agreeing on a cut is a stronger signal than any one of them alone.

### Kept, with a correction to priority: distribution and release infrastructure

**Why kept and elevated:** `PROJECT_ROADMAP.md` Phase 0 explicitly lists installer, one-line install, GitHub Releases, checksums, and shell completion as required for its own "exit condition" of Phase 0. My own market analysis (PR #40) reached the same conclusion independently: a security tool nobody can trivially install will not get adopted regardless of internal quality. This document keeps distribution as a P0 blocking item, not a nice-to-have, matching the corrected ~60% status above (down from a self-reported 85% that assumed release infrastructure existed without a verified check).

### Kept as-is: MCP-first, agent-agnostic, local-first positioning; Rust-native single-binary approach

**Why kept:** this is the one area where all sources - the original vision docs, `PROJECT_ROADMAP.md`, and my own market analysis - agree completely, and it remains the most externally defensible positioning found across the entire competitive review in `docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md`.

---

## 5. Immediate next three actions, in order

1. **Execute AWE-001 through AWE-004 sequentially**, exactly as `AWE-SEQUENTIAL-IMPLEMENTATION-MASTER-PROMPT.md` specifies. This is ready today and was the single largest concrete gap in prior planning.
2. **Verify PR #21 with the same line-level rigor applied to PRs #18-20** before treating Phase 4a as fully trusted, not just CI-green.
3. **Resolve the distribution question** (does a release workflow producing installable binaries exist?) before any further feature work — this is now confirmed as a real gap from two independent angles (market analysis and `PROJECT_ROADMAP.md`'s own Phase 0 exit condition), not a hypothetical one.
