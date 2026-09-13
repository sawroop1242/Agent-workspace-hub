# AWE Sequential Implementation Master Prompt — Forensic-Aligned Build Plan

## Purpose

Use this document to drive an AI coding agent through the AWH implementation backlog **one issue at a time**. The current repository is authoritative. PR #43's forensic report is the latest architectural reality check; it confirms that the MCP transport/protocol layer is strong, but agent-grade editing, file snapshots, provenance, agent identity enforcement, and worktree isolation are not yet wired.

This is an orchestration prompt. Detailed implementation rules live in the individual AWE prompt files. Do not collapse all issues into one rewrite.

## Architectural boundary

AWH is an **agent-agnostic, MCP-first workspace runtime**, not an AI agent.

Do not add LLM reasoning, autonomous planning, model routing, prompt orchestration, or agent workflow intelligence.

External agents decide what should happen. AWH provides controlled workspace state, capabilities, mutations, verification, reversibility, isolation, and observability.

## Current forensic facts that must remain true until implementation proves otherwise

- `services/edit.rs` is a model/scaffold with no production executor.
- No `filesystem.*` edit MCP tools exist yet.
- No file snapshots or file rollback exist; context snapshots are a different subsystem.
- Agent records and capability grants are currently not consulted by tool execution.
- Built-in Medium/High-risk authorization is currently opt-in/default-allow; this is a security priority.
- Remote MCP can bind publicly without TLS; this is a security priority.
- The existing whole-file workspace write has no stale-state precondition.
- MCP has divergent filesystem/memory/task/connector implementations instead of one canonical service layer.
- Audit is a volatile process-local ring without caller identity.
- Worktrees and agent isolation are absent.

Never claim any of these as fixed without source + tests proving it.

---

# Phase A — Immediate security gates

Before exposing new mutation surfaces, resolve the two cheap P0 security findings from PR #43:

### M1 — Built-in High-risk default deny
Change the authorization policy so High-risk built-ins such as `terminal.run`, high-risk GitHub mutations, and `connector.invoke` require explicit authorization. Preserve compatibility deliberately for lower-risk tools where appropriate. Add regression tests and clear onboarding/error messages.

### M2 — Public MCP TLS guard
Refuse non-loopback MCP HTTP/SSE binds without TLS. Loopback development remains usable. Add tests for loopback/plaintext and public/plaintext/public+TLS combinations.

If these are tracked outside the AWE-001…018 series, create or use dedicated issues before implementation. Do not hide them inside an unrelated edit issue.

---

# Phase B — Agent-grade editing core

Execute in this exact order:

1. AWE-001 / #22 — canonical transaction model
2. AWE-002 / #23 — safe contextual replacement
3. AWE-003 / #24 — line-range insert/delete
4. AWE-004 / #25 — multi-operation patch
5. AWE-005 / #26 — unified diff
6. AWE-006 / #27 — atomic/rollback-safe transaction
7. AWE-007 / #28 — stale-state/conflict enforcement
8. AWE-008 / #29 — post-edit verification

AWE-002 and AWE-003 may proceed in parallel after AWE-001, but AWE-004 must wait for both.

For each issue:
1. Read its prompt file.
2. Read its GitHub issue.
3. Inspect current source and tests.
4. Determine what is already implemented.
5. Make the smallest coherent patch.
6. Add behavior-focused tests.
7. Run the full verification gate.
8. Inspect `git diff` and repository status.
9. Commit only that issue's work.
10. Stop on failure; do not continue automatically.

Use the dedicated AWE-004 plan/checklists for AWE-004.

---

# Phase C — Exposure and recovery

After the core is verified:

9. AWE-009 / #30 — MCP editing tools
10. AWE-010 / #31 — explicit edit rollback
11. AWE-011 / #32 — capability/policy enforcement on all edit paths
12. AWE-012 / #33 — file snapshots and provenance
13. AWE-013 / #34 — persistent correlated audit
14. AWE-014 / #35 — CLI editing commands
15. AWE-015 / #36 — complete edit test suite

Important dependency refinement from the forensic report:

- Caller identity is foundational for trustworthy audit and per-agent capability scoping.
- Snapshot storage is a file-recovery subsystem, not the existing context snapshot subsystem.
- The transaction model must remain the single mutation identity used by all surfaces.
- MCP and CLI must call the same EditService.

AWE-011 and AWE-013 may be developed in parallel with snapshot work after the transaction identity exists, but no surface may bypass the authoritative policy boundary.

---

# Phase D — Acceptance and contract

16. AWE-016 / #37 — real MCP client interoperability
17. AWE-017 / #38 — full end-to-end acceptance workflow
18. AWE-018 / #39 — stable editing contract documentation

AWE-017 is the gate for saying **agent-grade editing is implemented**.

The canonical acceptance workflow is:

```text
Agent
  ↓
Session / Caller identity
  ↓
Read / locate
  ↓
Prepare edit
  ↓
Capability + Policy
  ↓
Expected-state conflict check
  ↓
Snapshot
  ↓
Apply
  ↓
Verify
  ↓
Provenance + persistent audit
  ↓
Result
  ↓
Optional rollback
  ↓
Verify restoration
```

---

# Phase E — Post-edit architectural foundations

After AWE-017, implement the broader forensic critical path:

```text
caller identity
→ unified store layer
→ edit transaction
→ snapshots/provenance
→ persistent audit
→ per-agent capability scoping
→ filesystem mutation safety
→ worktree isolation
```

Priority items from PR #43:
- unify MCP/core filesystem, memory, and task abstractions instead of creating more duplicates;
- fix final-component filesystem TOCTOU behavior;
- add per-path mutation coordination where justified;
- bind agent → session → workspace → worktree;
- implement first-class worktrees only after identity and edit safety are real;
- harden Control API transport/auth scopes;
- add multi-OS/Android/Termux validation;
- add dependency auditing.

Do **not** prioritize richer multi-agent orchestration, cloud control planes, connector expansion, or LLM planner integration before these foundations.

---

# Agent Profiles / Policy-Routed MCP requirement

The final architecture includes multiple simultaneous agents with explicit configuration and lifecycle control.

Use a declarative TOML source such as:

```toml
[server]
host = "127.0.0.1"
port = 9000

[agents.claude]
enabled = true
mcp = true
sse = true

[agents.claude.permissions]
tools = ["filesystem.read", "filesystem.search", "filesystem.patch", "git.status", "git.diff"]

[agents.qwen]
enabled = true
mcp = true
sse = true

[agents.qwen.permissions]
tools = ["filesystem.read", "filesystem.search", "filesystem.patch", "terminal.execute"]
```

The architecture is:

```text
TOML
 ↓
Config / AgentProfile
 ↓
AgentRegistry
 ↓
AgentSession
 ↓
PolicyEngine / Capability
 ↓
Tool Registry
 ↓
Canonical services
```

Routes may be:

```text
/{agent}/mcp
/{agent}/sse
```

but the URL namespace is **identity/routing only, never authorization**.

CLI lifecycle target:

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

Starting only `claude` must register only Claude's active routes; inactive agents must not receive MCP traffic. Policy checks still run after routing.

This architecture belongs after the editing/security foundations and before full Phase-12 collaboration. Do not turn AWH into an agent framework.

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

Then inspect the issue-specific source and tests. Search for existing implementations before adding anything.

Prefer minimal patches. Never blindly overwrite whole source files.

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

If Cargo is unavailable in the current environment, state that explicitly and use repository CI as the verification authority; do not invent local success.

---

# Commit discipline

One focused commit per issue or tightly coupled implementation stage.

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

Do not mix unrelated cleanup into implementation commits.

---

# Absolute rules

1. Repository code is authoritative.
2. PR #43 is a forensic reality check, not a feature specification.
3. Execute issue prompts sequentially unless a documented dependency allows safe parallel work.
4. Never silently overwrite stale content.
5. Never mutate during preparation.
6. Never duplicate canonical edit algorithms.
7. Never create a second policy engine.
8. Never treat URL namespaces as authorization.
9. Never claim context snapshots are file snapshots.
10. Never claim rollback, provenance, or agent isolation before their acceptance tests pass.
11. Never weaken tests or CI to obtain green status.
12. Never claim a test passed unless it ran.
13. Preserve MCP-first, agent-agnostic architecture.
14. Keep AWH out of LLM reasoning and autonomous agent orchestration.
15. Stop on security, compilation, test, or invariant failures.
