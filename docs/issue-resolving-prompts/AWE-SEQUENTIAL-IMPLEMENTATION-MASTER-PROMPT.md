# AWE Sequential Implementation Master Prompt — Current-State / Forensic-Aligned Build Plan

## Purpose

Use this document to drive AWH implementation **one issue at a time**. The current `rust` branch, actual source, tests, CI, and GitHub issue state are authoritative. Individual prompt files are implementation instructions, not evidence that an issue is complete.

The prompt program has now been expanded through AWE-019 and the post-edit architecture sequence. The repository currently contains **29 files in `docs/issue-resolving-prompts/`**: **25 canonical issue prompts**, this master prompt, and 3 legacy/support AWE-004 prompt files. The 25 canonical issue prompts cover AWE-001..019, SEC-001..002, AGENT-001, ARCH-001, FS-001, and GIT-001.

---

## Current issue-resolution status

### Important distinction

- **Prompt prepared:** the issue has a detailed master prompt in this directory.
- **Issue resolved:** the GitHub issue's implementation/acceptance criteria have been satisfied and the issue can be closed with evidence.
- These are not the same state.

### Current verified status

- **Canonical issue prompts prepared:** 25/25.
- **Canonical issues verified closed/resolved:** **0/25**.
- The tracked AWE/SEC/architecture issues remain open in GitHub; for example AWE-001/#22, SEC-001/#44, SEC-002/#45, and AWE-019/#40 are explicitly still open.
- Therefore the project must **not** claim that the AWE editing workstream or security foundation is complete merely because the prompt files exist.

### PR #52 / PR #53 reconciliation

PR #52 and PR #53 were competing implementations of the same SEC-002 issue. Both were merge-conflicted against the current `rust` branch, so merging both would have duplicated/competed over the same security boundary rather than producing a clean implementation.

Their intended changes were reconciled directly into `rust`:

- hardened SEC-002 loopback/TLS policy from PR #53;
- policy enforcement before TCP listener creation;
- IP-semantic loopback detection;
- conservative `localhost` resolution;
- fail-closed unknown hostnames;
- focused policy tests;
- real `serve()` startup-boundary rejection tests;
- invalid TLS validation regression coverage;
- current MCP rate-limiter/session behavior preserved.

PR #52 and PR #53 are therefore **superseded, not independently merged**. Do not count either PR as a separately resolved issue. They both map to the single SEC-002/#45 issue.

The reconciliation commits on `rust` are:

- `061bc96d7d86d17893356cfa166b4af8f6167c7c` — SEC-002 HTTP guard reconciliation.
- `05203ead3d97c5370b85cfa689ae252029d7aeb8` — export the SEC-002 policy helper for focused tests.
- `60f626e0250a06c827af55d4398227c946da25d0` — replace the weak SEC-002 regression suite with policy and startup-boundary tests.

**SEC-002/#45 remains open until the resulting branch passes the required CI/acceptance gate and the issue is explicitly closed.**

---

## Architectural boundary

AWH is an **agent-agnostic, MCP-first workspace runtime**, not an AI agent.

Do not add LLM reasoning, autonomous planning, model routing, prompt orchestration, or generic agent workflow intelligence.

External agents decide what should happen. AWH provides controlled workspace state, capabilities, mutations, verification, reversibility, isolation, identity, and observability.

---

# Phase A — Security prerequisites

Security is a hard gate before post-edit architecture work.

1. **SEC-001 / #44 — OPEN / NEXT:** deny High-risk built-in MCP tools by default.
2. **SEC-002 / #45 — IMPLEMENTATION RECONCILED / VALIDATION PENDING:** require TLS for non-loopback MCP HTTP/SSE binds.

### Security gate rule

Do **not** start AGENT-001/#46 while either SEC-001/#44 or SEC-002/#45 remains unverified or open.

If SEC-002 validation fails, fix SEC-002 before proceeding to SEC-001-dependent architecture work. If SEC-001 remains unresolved after SEC-002 is validated, **SEC-001 becomes the next implementation issue**.

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

AWE-002 and AWE-003 may proceed in parallel only after AWE-001 is satisfied. AWE-004 waits for both.

For every issue:

1. Read its prompt and GitHub issue.
2. Inspect current source/tests and existing abstractions.
3. Determine what is already implemented.
4. Make the smallest coherent patch.
5. Add behavior-focused tests.
6. Run the verification gate.
7. Inspect `git diff` and repository status.
8. Commit only that issue's work.
9. Stop.

---

# Phase C — Exposure, recovery, policy, identity prerequisites, and observability

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

AWE-017 is the behavioral gate for claiming agent-grade editing is implemented. AWE-018 documents only implementation-backed behavior. AWE-019 is documentation-only and must never become an implementation umbrella.

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

---

# Phase E — Post-edit architecture foundations

Only begin this phase after the security gate and the complete editing acceptance gate are satisfied.

22. **AGENT-001 / #46** — connect caller identity, AgentProfile, AgentRegistry, AgentSession, workspace, and policy-routed MCP.
23. **ARCH-001 / #47** — unify MCP and service/core stores.
24. **FS-001 / #48** — close final-component TOCTOU and coordinate concurrent mutations.
25. **GIT-001 / #49** — implement agent worktree isolation and safe multi-agent Git coordination.

**AGENT-001 is the first post-edit architecture issue. After completing AGENT-001, HARD STOP before ARCH-001.**

Intended identity architecture:

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

Routes such as `/{agent}/mcp` and `/{agent}/sse` may provide routing/identity, but the URL namespace is **never authorization**.

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

Do not implement worktree isolation before caller identity and conflict semantics are real.

---

# Unresolved-work rule

If the current issue is found to be already implemented, **verify it against the issue acceptance criteria instead of duplicating it**.

If an issue is partially implemented:

1. retain the working parts;
2. identify the exact unmet acceptance criteria;
3. convert those gaps into the current issue's remaining implementation work;
4. do not silently skip to a later issue.

If the current issue is blocked, the blocker becomes the immediate next action. Do not mark the issue resolved.

If a future issue depends on an unresolved prerequisite, the future issue is blocked and must not be implemented merely because its prompt exists.

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

Then inspect issue-specific source/tests. Search for existing implementations before adding new abstractions.

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

For documentation-only work such as AWE-019, use documentation/static validation and repository evidence; do not claim Rust CI unless it actually ran.

For SEC-002 specifically, the minimum acceptance matrix is:

```text
127.0.0.1 + plaintext       → allow
127.x.x.x + plaintext       → allow
::1 + plaintext             → allow
localhost + plaintext       → allow only when all resolved addresses loop back
0.0.0.0 + plaintext        → reject before bind
:: + plaintext              → reject before bind
non-loopback IP + plaintext → reject before bind
unknown hostname + plaintext→ fail closed
non-loopback + valid TLS    → policy allow, then TLS validation/startup applies
non-loopback + invalid TLS  → reject during TLS validation
```

---

# Acceptance and evidence discipline

Every completion claim must identify its evidence level:

- **implemented and tested on current branch**;
- **implemented but partially validated**;
- **in progress**;
- **planned/not started**;
- **blocked/unresolved**;
- **outside current roadmap**.

Never convert an issue prompt, roadmap item, or PR description into proof of implementation.

For security, authorization, editing, rollback, identity, and isolation claims, prefer actual source + tests + CI evidence.

For strategic/market claims, separate repository facts, technical assessment, market observations, hypotheses, and recommendations.

---

# Commit discipline

Use one focused commit per issue/stage. Do not mix unrelated cleanup into implementation commits.

A conflict-resolution/reconciliation commit is acceptable only when multiple branches implement the **same issue** and their changes must be consolidated. It must document which behavior was retained and why.

Never count competing PRs for the same issue as multiple resolved issues.

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
20. Do not treat PR #52 and PR #53 as separate SEC-002 issues; they are superseded competing implementations of #45.
21. Do not proceed to AGENT-001 while SEC-001/#44 or SEC-002/#45 remains unresolved.
22. After SEC-002 validation, the next unresolved security issue is SEC-001/#44.
23. After the complete editing/security gates pass, the next architecture issue is AGENT-001/#46.
24. After AGENT-001, stop before ARCH-001/#47.

---

# Final reporting protocol

At the end of each issue, report:

- issue number/title;
- current issue state;
- files changed;
- implementation or documentation outcome;
- tests/verification actually executed;
- CI evidence if used;
- known limitations/blockers;
- exact diff scope;
- commit SHA when available;
- whether the GitHub issue can now be closed.

Explicitly state whether any file outside the issue's intended scope changed.

**HARD STOP after the current issue. Wait for the next explicit instruction before proceeding.**
