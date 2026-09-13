# AWE Sequential Implementation Master Prompt — Forensic-Aligned Build Plan

## Purpose

Use this document to drive an AI coding agent through the AWH implementation backlog **one issue at a time**. The current repository is authoritative. PR #43 is the latest forensic reality check: the MCP transport/protocol layer is strong, but agent-grade editing, file snapshots, provenance, agent identity enforcement, and worktree isolation are not yet wired.

This is an orchestration prompt. Detailed implementation rules live in the individual prompt files. Do not collapse all issues into one rewrite.

## Architectural boundary

AWH is an **agent-agnostic, MCP-first workspace runtime**, not an AI agent.

Do not add LLM reasoning, autonomous planning, model routing, prompt orchestration, or agent workflow intelligence.

External agents decide what should happen. AWH provides controlled workspace state, capabilities, mutations, verification, reversibility, isolation, and observability.

## Current forensic facts

- `services/edit.rs` is a model/scaffold with no production executor.
- No `filesystem.*` edit MCP tools exist yet.
- No file snapshots or file rollback exist; context snapshots are a different subsystem.
- Agent records and capability grants are currently not consulted by tool execution.
- Built-in High-risk authorization is currently opt-in/default-allow.
- Public MCP HTTP/SSE can currently run without TLS.
- Existing whole-file workspace writes have no stale-state precondition.
- MCP has divergent filesystem/memory/task/connector implementations instead of one canonical service layer.
- Audit is a volatile process-local ring without caller identity.
- Worktrees and agent isolation are absent.

Never claim any of these as fixed without source + tests proving it.

---

# Phase A — Security prerequisites

Execute these first:

1. **SEC-001 / #44** — deny High-risk built-in MCP tools by default.
2. **SEC-002 / #45** — require TLS for non-loopback MCP HTTP/SSE binds.

Both have dedicated prompt files. They may be implemented independently, but neither should be hidden inside an editing issue.

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

AWE-002 and AWE-003 may proceed in parallel after AWE-001, but AWE-004 waits for both.

For each issue:
1. Read its prompt file and GitHub issue.
2. Inspect current source and tests.
3. Determine what is already implemented.
4. Make the smallest coherent patch.
5. Add behavior-focused tests.
6. Run the verification gate.
7. Inspect `git diff` and repository status.
8. Commit only that issue's work.
9. Stop on failure.

Use the dedicated AWE-004 plan/checklists for AWE-004.

---

# Phase C — Exposure, identity, recovery, and observability

11. AWE-009 / #30 — MCP editing tools
12. AWE-010 / #31 — explicit edit rollback
13. AWE-011 / #32 — capability/policy enforcement on all edit paths
14. AWE-012 / #33 — file snapshots and provenance
15. AWE-013 / #34 — persistent correlated audit
16. AWE-014 / #35 — CLI editing commands
17. AWE-015 / #36 — complete edit test suite

Identity and policy are foundational. Do not expose a new mutation path that bypasses the authoritative capability boundary.

---

# Phase D — Acceptance and contract

18. AWE-016 / #37 — real MCP client validation
19. AWE-017 / #38 — complete editing acceptance workflow
20. AWE-018 / #39 — stable editing contract documentation

AWE-017 is the gate for saying **agent-grade editing is implemented**.

Canonical workflow:

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

21. **AGENT-001 / #46** — connect caller identity, AgentProfiles, AgentRegistry, AgentSession, and policy-routed MCP.
22. **ARCH-001 / #47** — unify MCP and service/core stores.
23. **FS-001 / #48** — close final-component TOCTOU and coordinate concurrent mutations.
24. **GIT-001 / #49** — implement agent worktree isolation and safe multi-agent Git coordination.

The intended AgentProfile architecture is:

```text
TOML
→ Config/AgentProfile
→ AgentRegistry
→ AgentSession
→ PolicyEngine/Capability
→ Tool Registry
→ canonical service
```

Routes may be `/{agent}/mcp` and `/{agent}/sse`, but the URL namespace is **routing/identity only, never authorization**.

Target lifecycle:

```text
awh agent list
awh agent show <agent>
awh agent start <agent>
awh agent start <agent> <agent>
awh agent start --all
awh agent stop <agent>
awh agent restart <agent>
awh agent run <agent>
awh agent status
```

`awh agent start claude` must activate only Claude's configured routes; policy checks still run after routing.

Do not build worktrees before identity and conflict semantics are real.

---

# Repository inspection protocol

Before every issue inspect:

```text
AGENTS.md
Cargo.toml
README.md
docs/PROJECT_ROADMAP.md
docs/CLI.md
docs/FEATURES.md
```

Then inspect issue-specific source/tests. Search for existing implementations before adding anything.

Prefer minimal patches. Never blindly rewrite complete source files.

---

# Verification gate

Run after every implementation issue:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Also:

```bash
git status
git diff --stat
git diff
```

Never report a command as passed unless it actually ran successfully.

If Cargo is unavailable in the current environment, state that explicitly and use repository CI as verification authority; never invent local success.

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
```

---

# Absolute rules

1. Repository code is authoritative.
2. PR #43 is a forensic baseline, not a feature specification.
3. Execute prompts sequentially unless a dependency explicitly permits safe parallel work.
4. Never silently overwrite stale content.
5. Never mutate during preparation.
6. Never duplicate canonical edit algorithms.
7. Never create a second policy engine.
8. Never treat URL namespaces as authorization.
9. Never treat context snapshots as file snapshots.
10. Never claim rollback, provenance, or agent isolation before acceptance tests pass.
11. Never weaken tests or CI to obtain green status.
12. Never claim a test passed unless it ran.
13. Preserve MCP-first, agent-agnostic architecture.
14. Keep AWH out of LLM reasoning and autonomous agent orchestration.
15. Stop on security, compilation, test, or invariant failures.
