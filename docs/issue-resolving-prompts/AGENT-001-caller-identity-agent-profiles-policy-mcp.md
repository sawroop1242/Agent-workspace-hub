# AGENT-001 / #46 — Caller Identity, Agent Profiles, Policy-Routed MCP — Full Issue-Resolution Master Prompt

> **Branch:** `rust`  
> **Issue:** #46 — `AGENT-001: Connect caller identity, agent profiles, and policy-routed MCP`  
> **Status:** Issue-resolving master prompt

## Mission

Connect AWH's persisted `Agent`/`CapabilityGrant` model to real runtime caller identity, MCP sessions, agent profiles, workspace context, capability/policy authorization, tool discovery/invocation, lifecycle control, and audit.

Target identity path:

```text
TOML → Config → AgentProfile → AgentRegistry → AgentSession → Workspace → Capability / Policy → Tool Registry → canonical service → Audit / result
```

The implementation must make agent identity real at the authorization boundary. Persisted agent records must no longer be bookkeeping-only, and MCP sessions must no longer be anonymous with respect to agent authorization.

Do not build LLM reasoning, autonomous planning, workflow orchestration, a model router, or a second authorization system.

---

## 1. Forensic baseline and dependencies

Before changing code, verify the current `rust` branch rather than trusting this prompt or the issue description.

Known forensic findings to reconfirm against source/tests:

- `Agent` and `CapabilityGrant` records exist but are not yet fully connected to actual tool authorization.
- MCP protocol sessions are not yet authoritative `AgentSession` identities.
- Tasks may still use free-string assignees.
- Audit does not yet consistently carry agent/session identity.
- Multiple simultaneous agents require explicit identity separation.
- Agent lifecycle and route activation must be distinct from authorization.
- `/{agent}/mcp` and `/{agent}/sse` may select an agent context, but the URL itself is never authorization.

Dependencies:

- SEC-001 / #44 — deny High-risk built-in MCP tools by default.
- SEC-002 / #45 — require TLS for non-loopback MCP HTTP/SSE binds.
- AWE-011 / #32 — authoritative capability/policy enforcement.
- AWE-018 / #39 — documented editing contract.
- AWE-019 / #40 — roadmap/status/strategic analysis documentation.

Worktree isolation / GIT-001 depends on this identity boundary.

If any dependency is incomplete, preserve its existing fail-closed behavior and do not duplicate it inside AGENT-001.

---

## 2. Mandatory repository preflight

Before implementation:

1. Confirm branch `rust`.
2. Inspect `git status`, recent commits, and current diff.
3. Read `AGENTS.md`, `Cargo.toml`, relevant project-status docs, and `docs/issue-resolving-prompts/AWE-SEQUENTIAL-IMPLEMENTATION-MASTER-PROMPT.md`.
4. Read AWE-011 and the relevant security prompts.
5. Read in full, where present:
   - `src/core/agents.rs`
   - `src/core/capability_grants.rs`
   - `src/core/policy.rs`
   - `src/mcp/dispatcher.rs`
   - `src/mcp/tool_broker.rs`
   - MCP session/transport modules
   - configuration/profile modules
   - audit/observability modules
   - workspace/session modules
   - CLI agent commands
   - Control API/TUI agent lifecycle adapters
6. Search for existing `AgentProfile`, `AgentRegistry`, `AgentSession`, caller, capability, policy, session, and audit abstractions before adding types.
7. Inspect tests for MCP sessions, policy, capabilities, tools, audit, and agent commands.
8. Inspect the actual merged/current state of #44, #45, and AWE-011 where relevant.

Do not create a parallel identity abstraction when an existing canonical service can be extended.

---

## 3. Canonical identity model

### AgentProfile

Represents declarative configuration for an enabled/disabled agent. Use existing schema conventions. It may contain stable ID, enabled state, route/runtime configuration, workspace/resource scope, and declared capability references where appropriate.

TOML is configuration input, **not** the policy engine.

### AgentRegistry

Authoritative resolver for configured agent profiles.

Requirements:

- deterministic lookup by stable ID;
- reject unknown agents;
- distinguish configured from active;
- prevent duplicate active identities;
- never turn arbitrary URL strings into authorization identities;
- provide the profile used to establish an `AgentSession`.

### AgentSession

Authoritative active caller binding containing, as appropriate:

- stable agent ID;
- session ID;
- workspace context;
- transport/connection identity;
- lifecycle state;
- creation/activation metadata;
- authorization context.

Do not authorize a connection merely because it reached an agent route.

### Caller identity

Every protected tool invocation must carry enough structured identity to answer:

> Which agent, which session, which workspace, and which authorization context requested this operation?

Avoid hidden process-global `current_agent` state.

---

## 4. Authoritative authorization boundary

Canonical flow:

```text
MCP request
→ resolve caller/session
→ resolve agent
→ resolve workspace
→ capability evaluation
→ policy evaluation
→ Tool Registry metadata
→ canonical service/tool execution
→ audit/result
```

Reconcile the exact ordering with AWE-011 and current security gates; do not blindly replace tested authorization code.

Non-negotiable behavior:

- agent identity reaches the authoritative authorization engine;
- capability grants are consulted;
- grant expiry/revocation is enforced;
- resource/workspace scope is enforced where defined;
- policy remains authoritative after routing;
- route names are never authorization credentials;
- direct/internal protected execution cannot silently bypass the same boundary;
- denied requests do not execute the underlying tool;
- unknown/inactive agents fail closed;
- missing identity fails closed where identity is required;
- existing custom-MCP trust semantics are preserved.

Do not create another policy or capability engine.

---

## 5. CapabilityGrant enforcement

Integrate the existing capability model. Verify and enforce the fields actually represented by the current schema, including where applicable:

- agent binding;
- capability/tool identity;
- resource/path scope;
- expiry (`expires_at`);
- revoked/disabled state;
- workspace scope;
- Tool Registry required permissions.

Required outcomes:

```text
valid grant + valid policy → allowed
missing grant → denied
expired grant → denied
revoked grant → denied
wrong-agent grant → denied
wrong workspace/resource → denied
policy denial → denied
```

Do not reduce a scoped capability to a bare string match.

---

## 6. Agent-specific MCP discovery and invocation

For `tools/list`:

- resolve the active agent/session;
- expose only tools the caller may discover under the authoritative authorization model;
- avoid unnecessary privileged metadata leakage;
- preserve stable MCP schemas.

For `tools/call`:

- resolve the same identity;
- re-authorize using current capability/policy state;
- never treat a previous discovery result as authorization;
- pass identity into the canonical service/tool layer;
- emit the appropriate audit correlation.

Invocation must always perform the final authorization check.

---

## 7. Routing and transport

Supported routes may include:

```text
/{agent}/mcp
/{agent}/sse
```

The route can select an agent context, but:

> **URL namespace ≠ authorization.**

Required behavior:

- unknown agent → rejected;
- disabled/inactive agent → rejected;
- active agent → correct `AgentSession` established/used;
- forged route cannot impersonate another agent;
- arbitrary request parameters cannot switch established identity;
- MCP lifecycle remains protocol-correct;
- supported transports retain equivalent authorization semantics;
- SEC-002 TLS requirements remain intact.

No transport-specific authorization engine.

---

## 8. Agent lifecycle CLI

Implement or complete the existing CLI architecture for:

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

Use existing command conventions if equivalent commands already exist.

Semantics:

- `start <agent>` activates only that configured agent;
- `start --all` activates all enabled profiles;
- inactive agents receive no new MCP traffic;
- `stop` prevents new authorized traffic and invalidates appropriate active session state;
- `restart` cannot leave stale authorization state;
- `run` must not create an execution path bypassing policy/capability;
- `status` distinguishes configured/active/stopped/failed states where supported.

Do not build unrelated process supervision.

---

## 9. Simultaneous-agent isolation

At least two agents must coexist without identity confusion:

```text
Agent A / Session A → A capabilities/policy/audit
Agent B / Session B → B capabilities/policy/audit
```

Prove that A never inherits B's identity, workspace, grants, or audit correlation.

Test interleavings such as:

- A connects, B connects, A calls, B calls;
- A disconnects while B remains active;
- A restarts while B remains active;
- A has an expired grant while B remains authorized;
- A and B invoke the same tool with different permissions.

Never use process-global mutable caller identity.

---

## 10. Audit and observability

Integrate with the existing audit service. Do not create an agent-specific audit store.

Security-sensitive invocation results should correlate, where supported:

```text
agent_id
session_id
workspace_id
operation/tool
policy decision
result
```

Preserve AWE-013 persistent audit semantics if present. Never record secrets merely to establish caller correlation.

---

## 11. Tasks and identity references

If this issue touches task ownership, prefer stable agent identity over free-form strings, but do not perform an unrelated task-system rewrite.

If migration is required, preserve backward compatibility and test it. Otherwise document the boundary and leave the unrelated subsystem unchanged.

---

## 12. Security and failure behavior

Test at minimum:

- unknown agent;
- inactive agent;
- forged route;
- missing session identity;
- expired grant;
- revoked grant;
- grant for another agent;
- wrong workspace/resource;
- unauthorized tool;
- privileged tool without capability;
- policy denial;
- direct/internal bypass attempt;
- simultaneous-agent identity confusion;
- session reuse after stop/restart;
- malformed identity/session metadata.

Protected operations must fail closed and must not execute on authorization failure.

Do not weaken MCP trust, sandbox, path, policy, capability, or TLS checks.

---

## 13. Testing requirements

Use real service boundaries and real MCP/session behavior where practical. Mocks are supplemental only.

### Unit tests

Cover profile validation, registry lookup, enabled state, session identity/lifecycle, capability lookup, expiry, scope, authorization decisions, identity propagation, and audit correlation.

### Integration tests

At minimum:

1. authorized agent invokes authorized tool;
2. unauthorized tool rejected;
3. expired grant rejected;
4. inactive agent receives no MCP traffic;
5. unknown agent rejected;
6. agent-specific `tools/list` filtered correctly;
7. `tools/call` re-authorizes current state;
8. two simultaneous agents remain isolated;
9. stop/restart invalidates stale session state;
10. policy denial reaches client as stable structured error;
11. audit records correct agent/session identity;
12. existing custom-MCP authorization remains intact.

### Regression tests

Prove no regression in MCP `initialize`, `tools/list`, `tools/call`, built-in authorization, custom MCP trust, policy enforcement, capability persistence, TLS requirements, and existing CLI behavior.

---

## 14. Compatibility and migration

If existing configurations lack agent identity:

- define migration/default behavior explicitly;
- never silently grant privileged access;
- fail closed where identity is required;
- add compatibility tests.

Configuration schema changes must follow existing AWH conventions. TOML remains configuration, never authorization policy.

---

## 15. Architecture guardrails and non-goals

Do not use AGENT-001 to build:

- LLM reasoning;
- autonomous planning;
- workflow/DAG orchestration;
- model routing;
- prompt management;
- agent-to-agent intelligence;
- a second policy engine;
- a second capability store;
- a second MCP dispatcher;
- a second audit system;
- worktree isolation;
- unrelated TUI redesign;
- unrelated Control API redesign;
- broad task migration.

AWH remains agent-agnostic and MCP-first: external agents decide what should happen; AWH supplies controlled identity, policy, capabilities, workspace state, tool access, and observability.

---

## 16. Verification gate

Run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Then inspect:

```bash
git status
git diff --stat
git diff
```

Never claim a command passed unless it actually ran. If Cargo is unavailable, state that and use actual CI evidence rather than inventing success.

---

## 17. Definition of Done

- [ ] AgentProfile has a clear configuration/runtime boundary.
- [ ] AgentRegistry is the authoritative profile resolver.
- [ ] AgentSession binds an MCP/session caller to stable agent identity.
- [ ] Identity reaches the authoritative capability/policy boundary.
- [ ] Capability grants are enforced, including expiry and applicable scope.
- [ ] Agent-specific discovery respects authorization.
- [ ] Invocation re-checks authorization using current state.
- [ ] Unknown/inactive agents cannot receive unauthorized MCP traffic.
- [ ] Routes are never authorization credentials.
- [ ] Two simultaneous agents remain isolated.
- [ ] Audit carries agent/session/workspace correlation.
- [ ] CLI lifecycle uses the same canonical identity model.
- [ ] Existing security gates remain intact.
- [ ] Unauthorized, expired-grant, inactive-route, and simultaneous-agent tests exist.
- [ ] Applicable Rust verification passes or limitations are explicitly reported.
- [ ] No duplicate authorization/session/audit architecture exists.
- [ ] Worktree isolation remains a downstream issue.

---

## 18. Final implementation report

Report exact files changed, identity model implemented, authorization boundary used, CLI/MCP lifecycle behavior, capability/policy enforcement, audit correlation, tests executed, verification results, known limitations, and exact final diff scope.

Explicitly state whether any file outside the intended AGENT-001 implementation scope changed.

---

## HARD STOP

After AGENT-001:

1. Do not begin ARCH-001, FS-001, GIT-001, or any later issue in the same run.
2. Do not implement worktree isolation.
3. Do not redesign policy beyond the existing authorization boundary required here.
4. Do not build agent reasoning or orchestration.
5. Do not create alternate MCP/session/authorization implementations.
6. Stop after AGENT-001 verification and report the final state.
