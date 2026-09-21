# TW-003 — Agent-Specific MCP Routing + Authorization

## Master implementation prompt

Implement agent-scoped MCP routing and one authoritative authorization path as a standalone Trust-Wedge slice. The issue must work against the current `rust` branch without requiring another unmerged TW issue or PR.

### 1. Repository forensics

Inspect the current implementation before changing it:

- `src/mcp/server.rs`
- `src/mcp/sse.rs`
- `src/mcp/tool_broker.rs`
- `src/mcp/execution_gate.rs`
- `src/services/authorization.rs`
- `src/core/capability_grants.rs`
- `src/core/policy.rs`
- current agent/profile/session code;
- MCP tool discovery and invocation;
- audit/observability hooks;
- relevant CLI/configuration/tests.

Search for existing route parsing, caller context, capability checks, policy decisions, execution gates, and MCP session types before introducing abstractions. Reuse the existing security choke point.

### 2. Canonical request flow

The security-critical flow must be explicit:

`transport -> resolve agent -> resolve active AWH session -> construct caller/capability context -> evaluate capability -> evaluate policy -> execution gate/tool broker -> tool`

The exact modules may differ if the repository has evolved; preserve the invariant rather than forcing filenames.

Routing identifies the intended agent. **A route, URL segment, header, or tool name is never authorization by itself.**

The authorization decision must happen before the underlying tool is invoked.

### 3. Agent-scoped routes

Support the repository's appropriate agent-scoped MCP/SSE route shape, such as:

- `/{agent}/mcp`
- `/{agent}/sse`

Use the current transport conventions rather than adding incompatible parallel servers.

Route handling must:

- parse and validate the agent identifier;
- reject unknown agents;
- reject inactive/disabled agents;
- resolve an active AWH session bound to the same agent/workspace;
- preserve caller identity for downstream authorization and audit;
- prevent one agent's route from resolving another agent's session.

Do not infer authorization merely because a route was successfully parsed.

### 4. Capability and policy enforcement

Authorization must fail closed.

Reject before tool execution when:

- capability is absent;
- capability is expired;
- capability scope does not cover the requested operation/resource;
- workspace scope mismatches;
- agent/session scope mismatches;
- policy denies the operation;
- caller/session is invalid or inactive.

Policy denial must override an otherwise present capability grant.

If authorization is denied, the underlying tool must not be invoked and no protected mutation may occur.

Use existing capability/policy types and semantics where they exist. Do not hard-code Claude, Codex, OpenCode, or other agent-specific privileges.

### 5. Tool broker and execution gate

Extend the existing tool broker/execution gate instead of creating a second security gate.

The gate should receive enough structured context to make and record the decision:

- agent ID;
- session ID;
- workspace ID;
- tool/action;
- requested resource/scope;
- capability context;
- policy decision;
- correlation/request ID when available.

The tool must execute only after authorization succeeds.

Tool discovery/listing must follow the repository's security model. Do not expose protected tools merely because the transport can enumerate them.

### 6. Isolation invariants

Guarantee:

- unknown agent cannot reach a tool;
- inactive agent cannot reach a tool;
- agent A cannot use agent B's session;
- route identity cannot override caller/session identity;
- workspace mismatch fails before execution;
- expired/scope-mismatched capability fails before execution;
- policy denial cannot be bypassed by another adapter;
- CLI/MCP adapters do not create competing authorization rules;
- denied consequential operations cause zero protected mutation.

### 7. Error and observability behavior

Return structured, stable error categories for at least:

- unknown agent;
- inactive agent;
- missing/invalid session;
- missing capability;
- expired capability;
- scope mismatch;
- policy denial;
- invalid request/tool.

Do not leak secrets, capability material, or sensitive file contents through errors.

Preserve caller identity and authorization outcome for the existing audit/observability boundary. Do not scatter independent audit writes through every transport handler.

### 8. Tests

Add focused tests for:

- route parsing;
- unknown agent;
- inactive agent;
- active session resolution;
- agent/session mismatch;
- workspace mismatch;
- capability allow;
- missing capability;
- expired capability;
- capability scope mismatch;
- policy allow/deny;
- policy override of grant;
- route-name-is-not-authorization;
- authorized tool discovery/call;
- denied call proving the underlying tool was not invoked;
- two-agent isolation;
- CLI/MCP adapter equivalence where both exist.

For denial tests, instrument the tool or use a real side effect and assert that it did not execute.

### 9. Security gates

Before completion verify:

- exactly one authoritative authorization path exists;
- every consequential MCP operation passes through it;
- default behavior is deny when authority is absent;
- transport identity cannot become authority;
- capability and policy checks happen before mutation/execution;
- caller identity is preserved through execution;
- no new unrestricted route or remote-control surface was introduced;
- no agent-specific hard-coded bypass exists.

### 10. Definition of done

Complete only when:

- real MCP/SSE runtime paths use the agent-scoped authorization flow;
- authorization decisions are enforced before tool execution;
- positive and negative tests prove the behavior;
- multi-agent isolation is executable;
- relevant formatting, compilation, tests, and security checks pass;
- documentation reflects the actual route/auth contract;
- no unrelated TW work is included.

Final report must list changed files, canonical authorization path, route behavior, denial guarantees, tests, commands/results, and remaining limitations.

### Non-goals

Do not redesign the capability model, implement the full edit/snapshot/rollback/audit system, build a second MCP server, add external-agent orchestration, or introduce agent-specific hard-coded permissions.