# AWE Sequential Implementation Master Prompt — Forensic-Aligned Build Plan

## Purpose

Use this document to drive an AI coding agent through the AWH implementation backlog **one issue at a time**. The current `rust` repository is authoritative. Detailed implementation rules live in the individual issue-resolving prompt files. Do not collapse multiple issues into one rewrite.

The sequence now includes the completed agent-grade editing documentation contract and AWE-019's roadmap/status report before moving into the post-edit identity architecture.

---

## Architectural boundary

AWH is an **agent-agnostic, MCP-first workspace runtime**, not an AI agent.

Do not add LLM reasoning, autonomous planning, model routing, prompt orchestration, or generic agent workflow intelligence.

External agents decide what should happen. AWH provides controlled workspace state, capabilities, mutations, verification, reversibility, isolation, identity, and observability.

---

## Current forensic reality

Treat source, tests, CI, and merged history as authoritative. The following findings must not be claimed as fixed without direct evidence:

- `services/edit.rs` originally provided the canonical edit vocabulary but required production executor work for the editing roadmap.
- Agent-grade editing must be considered complete only after the AWE acceptance workflow has passed.
- File snapshots are distinct from context-engine snapshots.
- Agent records and capability grants must be connected to real runtime authorization before they count as enforced.
- High-risk MCP authorization must remain fail-closed according to the security roadmap.
- Public non-loopback MCP HTTP/SSE must preserve the required TLS boundary.
- Whole-file writes must not bypass stale-state/conflict semantics where the applicable contract requires them.
- MCP and core/service implementations must converge on canonical service boundaries rather than accumulating duplicate semantics.
- Audit must preserve caller/session correlation where the applicable audit contract requires it.
- Worktree isolation must not be treated as complete before identity and conflict semantics are real.

Never infer completion from the existence of types, prompts, routes, or issue descriptions alone.

---

# Phase A — Security prerequisites

Execute first:

1. **SEC-001 / #44** — deny High-risk built-in MCP tools by default.
2. **SEC-002 / #45** — require TLS for non-loopback MCP HTTP/SSE binds.

They may be implemented independently, but neither should be hidden inside an editing or agent-identity issue.

---

# Phase B — Agent-grade editing core

Execute in this order:

3. AWE-001 / #22 — canonical transaction model
4. AWE-002 / #23 — safe contextual replacement
5. AWE-003 / #24 — line-range insert/delete
6. AWE-004 / #25 — multi-operation patch
7. AWE-005 / #26 — unified diff
8. AWE-006 / #27 — atomic/rollback-safe transaction
9. AWE-007 / #28 — stale-state/conflict enforcement
10. AWE-008 / #29 — post-edit verification

AWE-002 and AWE-003 may proceed in parallel after AWE-001 only when their explicit dependencies are satisfied; AWE-004 waits for both.

For every issue:

1. Read its prompt file and GitHub issue.
2. Inspect current source and tests.
3. Determine what is already implemented.
4. Make the smallest coherent patch.
5. Add behavior-focused tests.
6. Run the verification gate.
7. Inspect `git diff` and repository status.
8. Commit only that issue's work.
9. Stop on failure.

---

# Phase C — Exposure, identity prerequisites, recovery, and observability

11. AWE-009 / #30 — MCP editing tools
12. AWE-010 / #31 — explicit edit rollback
13. AWE-011 / #32 — capability/policy enforcement on all edit paths
14. AWE-012 / #33 — file snapshots and provenance
15. AWE-013 / #34 — persistent correlated audit
16. AWE-014 / #35 — CLI editing commands
17. AWE-015 / #36 — complete edit test suite

Identity and policy are foundational. Never expose a mutation path that bypasses the authoritative capability boundary.

---

# Phase D — Acceptance and contract

18. AWE-016 / #37 — real MCP client validation
19. AWE-017 / #38 — complete editing acceptance workflow
20. AWE-018 / #39 — stable editing contract documentation
21. AWE-019 / #40 — roadmap, implementation status, known issues, and strategic analysis report

AWE-017 is the behavioral gate for saying **agent-grade editing is implemented**. AWE-018 documents the implementation-backed editing contract. AWE-019 establishes an evidence-backed project/roadmap status reference and must remain documentation-only.

AWE-019 must not be used to turn strategic recommendations into implementation work.

Canonical editing workflow:

```text
Agent
→ caller/session identity
→ read/locate
→ prepare patch
→ capability/policy
→ expected-state check
→ snapshot
→ apply
→ verify
→ provenance/audit
→ result
→ optional rollback
→ verify restoration
```

For AWE-019 specifically, the required outcome is a trustworthy status document, not runtime behavior.

---

# Phase E — Post-edit architecture foundations

22. **AGENT-001 / #46** — connect caller identity, AgentProfiles, AgentRegistry, AgentSession, workspace, and policy-routed MCP.
23. **ARCH-001 / #47** — unify MCP and service/core stores.
24. **FS-001 / #48** — close final-component TOCTOU and coordinate concurrent mutations.
25. **GIT-001 / #49** — implement agent worktree isolation and safe multi-agent Git coordination.

AGENT-001 is the first post-edit architecture issue. Do not begin ARCH-001, FS-001, or GIT-001 in the same implementation run after AGENT-001; stop and wait for the next explicit instruction.

The intended identity architecture is:

```text
TOML
→ Config/AgentProfile
→ AgentRegistry
→ AgentSession
→ Workspace
→ Capability / Policy
→ Tool Registry
→ canonical service
→ Audit
```

Routes may be `/{agent}/mcp` and `/{agent}/sse`, but the URL namespace is **routing/identity only, never authorization**.

Target lifecycle:

```text
awh agent list
awh agent show <agent>
awh agent start <agent>
awh agent start --all
awh agent stop <agent>
awh agent restart <agent>
awh agent run <agent>
awh agent status
```

`awh agent start claude` activates only Claude's configured routes; policy and capability checks still run after routing.

Do not build worktrees before identity and conflict semantics are real.

---

# Issue execution protocol

Before every issue:

```text
Read prompt
→ read GitHub issue
→ inspect current source/tests
→ search for existing abstractions
→ identify dependencies
→ implement smallest coherent change
→ add behavior tests
→ run verification
→ inspect diff/status
→ commit only this issue
→ STOP
```

Do not assume an issue is still unimplemented merely because its prompt exists. Reconcile the prompt with the current branch first.

If the requested issue is already complete, verify it instead of duplicating implementation. If the issue is blocked by a missing dependency, stop and report the dependency.

---

# Repository inspection protocol

Before every implementation issue inspect, where present:

```text
AGENTS.md
Cargo.toml
README.md
docs/PROJECT_ROADMAP.md
docs/PROJECT_STATUS.md
docs/CLI.md
docs/FEATURES.md
docs/RECONCILED_ROADMAP_V2.md
docs/issue-resolving-prompts/
```

Then inspect issue-specific source/tests. Search for existing implementations before adding anything.

Prefer minimal patches. Never blindly rewrite complete source files.

---

# Verification gate

Run after every implementation issue:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also inspect:

```bash
git status
git diff --stat
git diff
```

Never report a command as passed unless it actually ran successfully.

If Cargo is unavailable, state that explicitly and use actual repository CI as verification evidence. Never invent local success.

For documentation-only issues such as AWE-019, use the strongest available documentation/static validation and repository evidence; do not claim a full Rust CI run unless it actually ran.

---

# Acceptance and evidence discipline

Every completion claim must identify its evidence level:

- implemented and tested on current branch;
- implemented but partially validated;
- in progress;
- planned/not started;
- blocked/unresolved;
- outside current roadmap.

Never convert an issue prompt, roadmap item, or PR description into proof of implementation.

For security, authorization, editing, rollback, identity, and isolation claims, prefer actual source + tests + CI evidence over documentation claims.

For strategic/market claims in AWE-019, clearly separate repository facts, technical assessment, market observations, hypotheses, and recommendations.

---

# Commit discipline

Use one focused commit per issue/stage. Do not mix unrelated cleanup into implementation commits.

Examples:

```text
feat(edit): add canonical edit transaction model
feat(edit): add safe contextual replacement
feat(edit): add safe line-range insert and delete operations
feat(edit): add multi-operation filesystem patch
feat(edit): add unified diff application
feat(edit): add transactional rollback safety
feat(security): deny high-risk built-ins by default
feat(mcp): require tls for public binds
docs(roadmap): add implementation status and strategic analysis
docs(agent): connect caller identity and policy-routed MCP
```

---

# Absolute rules

1. Repository code and actual CI/test evidence are authoritative.
2. PR #43 is a forensic baseline, not a feature specification.
3. Execute prompts sequentially unless a prompt explicitly permits safe parallel work.
4. Never silently overwrite stale content.
5. Never mutate during preparation.
6. Never duplicate canonical edit algorithms.
7. Never create a second policy engine.
8. Never treat URL namespaces as authorization.
9. Never treat context snapshots as file snapshots.
10. Never claim rollback, provenance, or agent isolation before the applicable acceptance tests pass.
11. Never weaken tests or CI to obtain green status.
12. Never claim a test passed unless it actually ran.
13. Preserve MCP-first, agent-agnostic architecture.
14. Keep AWH out of LLM reasoning and autonomous agent orchestration.
15. Keep documentation-only issues documentation-only.
16. Do not turn strategic recommendations into implementation scope without a separate issue.
17. Do not implement the next numbered issue in the same run after completing the current issue.
18. Stop on security, compilation, test, or invariant failures.
19. Do not modify unrelated files.
20. After AWE-019, the next implementation issue is AGENT-001 / #46; after AGENT-001, stop before ARCH-001 / #47.

---

# Final reporting protocol

At the end of each issue, report:

- issue number/title;
- files changed;
- implementation or documentation outcome;
- tests/verification actually executed;
- CI evidence if used;
- known limitations/blockers;
- exact diff scope;
- commit SHA when available.

Explicitly state whether any file outside the issue's intended scope changed.

**HARD STOP after the current issue. Wait for the next explicit instruction before proceeding.**
