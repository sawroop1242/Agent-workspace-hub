# Prompt 03 — Agent-Scoped MCP Routing, Authorization & Transport Security (TW-003 / SEC-001 / SEC-002)

## Mission

Implement one authoritative security boundary for agent-scoped MCP routing and consequential MCP tool execution.

Target path:

~~~
untrusted MCP request
  → transport authentication / protocol validation
  → agent route resolution
  → trusted AWH caller context
  → canonical tool registry / discovery
  → capability + policy authorization
  → existing execution gate / AWH service
  → execution + audit
~~~

The route namespace identifies an agent for routing. **The route is never authorization.** A client that knows `/{agent}/mcp` or `/{agent}/sse` must still authenticate, bind to a trusted agent/session/workspace context, and pass invocation-time authorization.

This prompt is standalone and must be executable against the current `rust` branch. It must not require another prompt, another PR, or a future implementation.

---

## 1. Product boundary

AWH is an agent-agnostic, local-first workspace runtime for coding agents.

AWH owns workspace/runtime state, controlled tool execution, MCP/CLI/TUI/Control API boundaries, trust/capability/policy enforcement, agent/session identity, and security/audit boundaries.

External agents own reasoning, planning, model selection, provider orchestration, and agent intelligence.

Do not turn this task into an LLM runtime, model router, autonomous scheduler/swarm, second MCP implementation, second policy/authorization engine, or OS/container sandbox replacement.

---

## 2. Required first action — repository forensics

Before changing code, inspect the current branch and establish what is actually implemented.

~~~
git status --short --branch
git log --oneline -n 20
cargo metadata --no-deps
cargo check --all-targets
~~~

Inspect, at minimum:

~~~
Cargo.toml
README.md
AGENTS.md                         (if present)
docs/FEATURES.md
docs/PROJECT_CONTEXT.md
docs/architecture.md
docs/security.md
docs/threat-model.md
docs/mcp.md
docs/roadmap/GROWTH_STRATEGY.md
docs/roadmap/PROJECT_ROADMAP.md
docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
docs/implementation-prompts/README.md
docs/implementation-prompts/01-*.md
docs/implementation-prompts/02-*.md
docs/implementation-prompts/04-*.md
docs/implementation-prompts/07-*.md
docs/implementation-prompts/11-*.md
docs/implementation-prompts/15-*.md
~~~

Inspect:

~~~
src/mcp/mod.rs
src/mcp/http.rs
src/mcp/dispatcher.rs
src/mcp/server.rs
src/mcp/custom_mcp.rs
src/mcp/auth.rs
src/mcp/tls.rs
src/mcp/tool_registry.rs
src/mcp/execution_gate.rs
src/mcp/error.rs
src/mcp/permissions.rs
src/mcp/schema.rs
src/core/agents.rs
src/core/identity.rs
src/core/policy.rs
src/core/capability_grants.rs
src/services/authorization.rs
src/main.rs
~~~

Search before creating abstractions:

~~~
rg -n "SessionLifecycle|session_id|AgentId|AgentSession|AgentStore|CapabilityGrant|PolicyStore|PolicyEngine|authorize|execution_gate|tool_registry|build_router|/mcp|/sse|Bearer|TLS|dispatch|tools/list|tools/call" src tests docs
~~~

Inspect existing MCP/security tests, especially `tests/mcp_builtin_tool_gate.rs`, `tests/mcp_policy_gate.rs`, `tests/mcp_dynamic_tools.rs`, `tests/sec_002_public_mcp_tls_guard.rs`, `tests/mcp_security.rs` if present, and other `tests/*mcp*`.

Historical `docs/issue-resolving-prompts/` and `docs/trust-wedge/` material is reference material only if it still exists or is available in repository history. The consolidated `docs/implementation-prompts/` collection, current source, roadmap, feature documentation, and tested behavior are the active contract.

Do not assume that a roadmap statement means a feature is already implemented.

---

## 3. Current implementation constraints

The repository already has substantial MCP infrastructure. Reuse it.

Known concepts that must not be duplicated:

- MCP dispatcher/session lifecycle;
- HTTP/SSE transport;
- bearer-token authentication;
- TLS validation;
- custom MCP trust/permission machinery;
- canonical MCP tool registry;
- built-in execution gate;
- MCP schema validation;
- workspace/path security;
- workspace-local policy storage;
- capability grant storage;
- existing agent records;
- typed identity primitives;
- existing authorization services;
- existing audit/observability hooks.

Keep these distinctions explicit:

~~~
MCP protocol session ≠ AWH AgentSession
agent route       ≠ authorization
AgentStore member ≠ capability grant
capability grant  ≠ policy approval
transport auth    ≠ agent identity
tool discovery    ≠ permission
~~~

If a canonical agent/session implementation is already present on the current branch, consume it. If absent, do not create a competing full identity subsystem merely for this prompt. Build the smallest explicit adapter around the identity primitives actually available.

---

## 4. Target architecture

Converge on:

~~~
External MCP client
      ↓
transport authentication / protocol validation
      ↓
AgentRouteResolver
      ↓
trusted CallerContext {agent, session, workspace}
      ↓
canonical ToolRegistry
      ↓
discovery filtering
      ↓
invocation authorization
      ↓
existing specialized execution gates
      ↓
canonical AWH service/tool
      ↓
audit / observability
~~~

A denial must stop the request before consequential execution.

---

## 5. Agent route contract

Support:

~~~
/{agent}/mcp
/{agent}/sse
~~~

Follow the current Axum/router architecture.

The path segment is only a routing selector. Resolve it through the canonical agent store/registry.

Reject:

- unknown agent;
- disabled/inactive agent;
- empty ID;
- `.` or `..`;
- path separators/traversal;
- control characters;
- ambiguous identifiers;
- unbounded input.

Reuse the repository's existing agent-ID validation. Do not silently transform an unsafe ID into another identity.

Never use these as proof of identity:

- route name alone;
- Host header;
- User-Agent;
- MCP `clientInfo.name`;
- client-supplied `agent_id`;
- display name;
- client-supplied workspace path;
- arbitrary URL metadata.

---

## 6. Trusted caller context

Identify or minimally introduce one transport-independent caller context.

Example:

~~~
CallerContext
├── agent_id
├── session_id (when available)
├── workspace_id/root
└── safe transport correlation metadata
~~~

Never put bearer tokens, secrets, or raw credentials in it.

Resolve in this order:

1. validate transport/protocol request;
2. authenticate remote transport where required;
3. resolve route agent;
4. resolve canonical agent record;
5. check activation/lifecycle state;
6. resolve trusted workspace;
7. bind/validate MCP protocol session;
8. construct CallerContext;
9. pass CallerContext to authorization;
10. dispatch only after authorization succeeds.

A failed step stops the request. Do not reconstruct identity later from untrusted request fields.

---

## 7. Prevent identity/session/workspace substitution

The following must fail:

- route substitution: a session for agent A used through agent B's route;
- agent substitution: client-supplied `agent_id` changes identity;
- session substitution: a valid session ID obtains another agent's authority;
- workspace substitution: a request selects another workspace by path;
- capability substitution: agent A inherits grants from agent B.

A protocol session must be bound to the trusted agent/workspace context when established.

A session that is unknown, closed, failed, expired, or bound to another agent/workspace cannot execute consequential tools.

Use canonical workspace containment/path validation. Do not duplicate filesystem security in the router.

---

## 8. MCP protocol session relationship

Reuse the existing MCP `SessionLifecycle`/dispatcher machinery.

Do not rename it into AWH AgentSession or create a second state machine.

Relationship:

~~~
MCP connection
  ↓
MCP protocol session
  ↓
trusted AWH caller context
  ↓
AgentId + WorkspaceId
  ↓
authorization
~~~

One agent may have multiple protocol connections. Connections must not accidentally share mutable state.

---

## 9. Per-agent activation

Implement agent-specific activation without creating a second MCP server.

Required semantics:

~~~
awh agent start claude
awh agent start claude qwen
awh agent start --all
~~~

Only requested enabled agents become active.

An inactive route must not behave as an active server with merely denied tools; follow the repository's lifecycle contract.

Stopping A must not stop B.

Do not require a separate OS process per agent unless the current architecture requires it.

Use shared runtime services for CLI, MCP, TUI, and API behavior.

---

## 10. Agent-aware discovery

For `tools/list`:

1. resolve trusted caller;
2. resolve effective capability/policy context;
3. enumerate the canonical tool registry;
4. filter tools the caller is not allowed to invoke;
5. return safe metadata.

Discovery filtering is **not authorization**.

A client may call a hidden tool directly, so every `tools/call` must authorize again.

Do not create a second per-agent tool catalog that can drift from the canonical registry.

---

## 11. Invocation authorization

Every agent-scoped consequential invocation must pass one authoritative authorization path.

Conceptual request:

~~~
ToolAuthorizationRequest
├── agent
├── session
├── workspace
├── tool
├── resource/operation
└── validated arguments
~~~

Required logic:

~~~
authenticated caller
AND valid agent
AND valid session/route
AND correct workspace
AND registered tool
AND required capability
AND policy allows
AND existing security gates pass
        ↓
      ALLOW
~~~

Otherwise: **DENY**.

No tool may bypass the path because it is built-in, dynamic, called over stdio, called over HTTP/SSE, or reached by an internal caller.

Where an existing specialized gate exists, compose it instead of copying it.

---

## 12. Preserve SEC-001

The repository already has a canonical tool registry and built-in execution gate. Reuse them.

Preserve:

- unregistered tool → deny;
- unavailable/corrupt authorization state → fail closed;
- High-risk built-ins → deny by default until explicitly authorized;
- required permissions must be covered;
- blocked trust state → deny;
- trust/version mismatch → deny;
- existing Low/Medium behavior.

Do not copy `authorize_builtin_tool` into the agent router.

Do not replace agent-specific authorization with a global `awh.builtin` allow. That would erase agent isolation.

If the current gate must be extended for agent context, extend the canonical authorization path while retaining the existing default-deny behavior.

---

## 13. Preserve policy and capability enforcement

Inspect `src/core/policy.rs` and `src/services/authorization.rs` before changing authorization.

Do not create a second MCP-only PolicyEngine.

Where an existing authorization service owns a domain, adapt MCP caller identity into it.

Preserve:

~~~
policy deny → DENY
no policy deny → capability/scope evaluation
capability/scope failure → DENY
otherwise → ALLOW
~~~

Explicit policy denial overrides capability allowance.

Corrupt/unreadable required security state fails closed.

Do not redesign the entire policy language, hierarchy, approvals, or human-in-the-loop system.

---

## 14. Capability isolation

Evaluate grants for the resolved agent only:

~~~
required capability
  ↓
grants belonging to resolved agent
  ↓
scope/expiry validation
  ↓
policy
  ↓
decision
~~~

Never scan all workspace grants and choose a matching one.

Preserve existing grant expiry and scope semantics. Never infer capability from route, profile name, MCP metadata, or discovery.

---

## 15. HTTP/SSE integration

Use the existing HTTP/Axum architecture.

For each agent route:

- validate route;
- authenticate transport;
- resolve agent;
- verify activation;
- establish route/session binding;
- dispatch through the shared MCP dispatcher;
- preserve request-size limits;
- preserve timeouts;
- preserve CORS/origin controls;
- preserve structured errors.

Do not bypass the dispatcher.

For `/{agent}/sse`:

1. authenticate;
2. resolve agent;
3. verify active state;
4. create the existing MCP protocol session;
5. bind trusted agent/workspace context;
6. return the normal MCP session identifier;
7. require the same binding for subsequent messages;
8. reject mismatched/unknown/closed sessions.

A later request must not replace the original agent binding.

---

## 16. Authentication and SEC-002 TLS

Preserve existing bearer authentication:

- missing/invalid bearer → reject;
- valid bearer → continue;
- credential never logged/audited;
- credential never returned in errors.

Preserve SEC-002:

~~~
loopback
  → plaintext may be allowed by documented contract

non-loopback/public bind
  → TLS required
  → missing/invalid TLS = startup failure before listener availability
~~~

Do not start a public plaintext listener and merely warn.

Do not weaken TLS to make agent routing work.

---

## 17. Stdio semantics

Keep existing stdio MCP compatibility.

Do not infer an agent from `clientInfo.name`.

If current CLI/runtime provides explicit agent-scoped stdio execution, establish agent context before serving the session.

A compatibility `awh mcp serve` mode must not silently inherit an arbitrary configured agent's privileges.

---

## 18. Tool registry and dynamic MCP

The canonical tool registry remains the source of truth for built-in tools.

Every callable built-in retains stable name, schema, risk, required permissions, and canonical execution behavior.

Unknown tools fail closed.

Do not create a second tool catalog.

Distinguish:

~~~
agent-scoped AWH MCP route
    ≠
custom external MCP server
~~~

Existing custom MCP trust remains required. A custom server reached through an agent route must satisfy both server trust and agent/session authorization.

A trusted custom server must not bypass the agent boundary.

---

## 19. Error model

Reuse existing structured MCP/security errors where possible.

At minimum distinguish these conditions or equivalent existing codes:

~~~
unknown_agent
agent_disabled
agent_inactive
invalid_agent_id
missing_authentication
invalid_authentication
unknown_session
session_agent_mismatch
session_workspace_mismatch
invalid_workspace
unknown_tool
tool_not_authorized
capability_denied
policy_denied
invalid_arguments
transport_security_error
internal_security_error
~~~

Never expose bearer tokens, secret values, complete trust/capability records, private filesystem paths, or remote stack traces.

Extend existing error types instead of creating duplicates when possible.

---

## 20. Zero-mutation denial invariant

For every authorization failure:

~~~
DENY
  ↓
no consequential service/tool execution
  ↓
no file mutation
no Git mutation
no process launch
no connector mutation
no secret resolution
no external side effect
~~~

Tests must prove observable zero side effects, not merely assert an error.

---

## 21. Audit and observability

Reuse existing audit/observability.

Safe correlation fields may include:

- agent ID;
- session ID;
- workspace/correlation ID;
- transport;
- route;
- tool;
- decision;
- reason;
- timestamp.

Never record credentials, secret values, or unredacted sensitive arguments.

Observability is not an authorization authority. A logging failure cannot turn DENY into ALLOW.

---

## 22. CLI/runtime integration

Use shared runtime services.

Preserve current `awh agent create/list/inspect/grant/revoke` behavior.

Where supported by the current CLI contract, lifecycle semantics include:

~~~
awh agent list
awh agent show <name>
awh agent start <name>
awh agent start --all
awh agent stop <name>
awh agent restart <name>
awh agent status
awh agent run <name>
~~~

Do not put MCP authorization logic directly into CLI handlers.

Do not create separate identity semantics for CLI and MCP.

---

## 23. Configuration

Use the repository's existing configuration mechanism.

Minimum agent MCP configuration, if missing, should cover only:

- agent identity;
- enabled/disabled state;
- MCP route activation;
- workspace association where required.

Conceptual shape:

~~~
[agents.claude]
enabled = true

[agents.claude.mcp]
enabled = true
sse = true
~~~

Do not create a parallel configuration file.

Configuration describes runtime intent; it must not silently grant high-risk capabilities unless the canonical authorization contract explicitly permits it.

Malformed security configuration fails closed.

---

## 24. Concurrent-agent isolation

Prove at least two concurrent agents:

~~~
Claude → Session A → Workspace/Context A → /claude/mcp
Qwen   → Session B → Workspace/Context B → /qwen/mcp
~~~

Prove:

- starting A does not activate B;
- stopping A does not stop B;
- A cannot use B's session;
- B cannot use A's session;
- A cannot use B's grants;
- B cannot use A's grants;
- workspace bindings remain independent;
- discovery can differ by permissions;
- invocation authorization remains independent;
- concurrent requests do not leak state.

Use deterministic synchronization, not sleep-based race assumptions.

---

## 25. Security ordering

For consequential requests, enforce this conceptual order:

~~~
1. transport security
2. request-size / structural validation
3. MCP protocol/session validation
4. agent route validation
5. agent/session/workspace binding
6. tool registry lookup
7. schema/argument validation
8. capability resolution
9. policy evaluation
10. existing specialized execution gate
11. canonical service/tool
12. audit/observability
~~~

Existing abstractions may combine steps, but no side effect may occur before all mandatory checks succeed.

Discovery is never a substitute for invocation authorization.

---

## 26. Tests — unit and adversarial

Add/update tests for:

### Route

- valid route;
- unknown agent;
- disabled/inactive agent;
- unsafe ID;
- traversal;
- control characters.

### Identity binding

- route → agent;
- route/session mismatch;
- session/agent mismatch;
- session/workspace mismatch;
- unknown/closed session;
- workspace substitution;
- forged client `agent_id`;
- clientInfo impersonation.

### Authorization

- allowed capability;
- missing capability;
- expired capability;
- out-of-scope capability;
- policy deny overrides grant;
- corrupt authorization state fails closed;
- unknown tool fails closed;
- High-risk default-deny remains active.

### Discovery

- authorized tool visible;
- unauthorized tool hidden;
- direct invocation of hidden tool denied.

### Side effects

- denied file operation changes nothing;
- denied Git operation changes nothing;
- denied process operation does not spawn;
- denied connector operation does not execute.

---

## 27. Tests — HTTP/SSE/TLS

Use actual handlers/integration tests or the established harness.

Cover:

### Authentication

- missing bearer;
- wrong bearer;
- valid bearer;
- malformed Authorization header.

### Routing

- valid `/{agent}/mcp`;
- unknown agent;
- disabled agent;
- session mismatch;
- wrong workspace;
- unauthorized tool;
- authorized tool.

### SSE

- valid connection;
- session creation;
- stored agent binding;
- matching subsequent message;
- unknown session;
- wrong-agent session;
- closed/failed session;
- disconnect.

### TLS

- documented loopback plaintext behavior;
- public plaintext rejected before bind;
- missing certificate/key rejected;
- malformed certificate/key rejected;
- valid TLS accepted;
- bearer required over TLS.

### Concurrency

Run two agents concurrently and prove independent sessions and decisions.

---

## 28. Real-client interoperability

Extend existing MCP interoperability tests where available.

Verify:

~~~
connect
initialize
tools/list
tools/call
unknown tool
invalid session
wrong bearer
missing bearer
disconnect
~~~

For agent-scoped endpoints, prove discovery and invocation use the correct agent-specific authorization context.

A successful connection alone is not sufficient.

---

## 29. Regression requirements

Do not remove or weaken tests for:

- SEC-001 High-risk default deny;
- canonical tool registry;
- schema validation;
- trust-store corruption fail-closed;
- bearer authentication;
- SEC-002 TLS guard;
- resource limits/timeouts;
- workspace/path containment;
- secret redaction.

If a contract genuinely changes, document the change and replace the test with an equivalent or stronger security assertion. Never delete a failing security test merely to make CI green.

---

## 30. Scope boundary

### Owns

- `/{agent}/mcp`;
- `/{agent}/sse`;
- route validation/resolution;
- trusted route-to-agent binding;
- MCP session-to-agent/workspace binding;
- agent-aware discovery;
- invocation-time authorization integration;
- capability/policy integration;
- transport security;
- SEC-001 preservation;
- SEC-002 preservation;
- structured security errors;
- cross-agent isolation tests;
- MCP interoperability tests;
- safe audit correlation.

### Does not own

- full AgentProfile/AgentRegistry/AgentSession redesign;
- autonomous execution;
- LLM/model routing;
- swarm scheduling/handoff;
- worktree implementation;
- edit transaction redesign;
- snapshots/rollback;
- persistent audit redesign;
- connector architecture;
- Control API redesign;
- TUI redesign;
- OS/container sandbox implementation;
- secret-provider architecture;
- general policy-language redesign.

If an out-of-scope subsystem blocks progress, use the smallest compatibility adapter possible and report the limitation.

---

## 31. Independence rule

This prompt must not require another prompt, another PR, a prescribed merge order, or a future AgentSession implementation.

Inspect and reuse whatever is currently present.

If a required identity abstraction is absent:

1. use the strongest existing identity primitive;
2. keep the adapter explicit and local;
3. do not build a competing full identity subsystem;
4. document the limitation;
5. still implement every security property supported by the current branch.

---

## 32. Linear implementation sequence

### Step 1 — Baseline and mapping

- inspect current branch;
- run baseline checks;
- trace MCP request paths;
- list existing authorization checkpoints;
- identify authoritative owners.

### Step 2 — Trusted caller contract

- identify/reuse caller context;
- ensure identity is not derived from untrusted metadata;
- document context ownership.

### Step 3 — Route resolver

- parse/validate agent path;
- resolve canonical agent;
- reject unknown/disabled/inactive agents;
- bind route to trusted state.

### Step 4 — Protocol-session binding

- reuse MCP session lifecycle;
- attach agent/workspace context;
- reject substitution;
- preserve protocol compatibility.

### Step 5 — HTTP/SSE integration

- add/extend `/{agent}/mcp`;
- add/extend `/{agent}/sse`;
- preserve auth, TLS, limits, timeouts, CORS, and dispatcher behavior.

### Step 6 — Discovery

- use canonical tool registry;
- filter by effective authorization;
- keep discovery separate from enforcement.

### Step 7 — Invocation gate

- resolve trusted caller;
- validate tool/arguments;
- evaluate capability;
- evaluate policy;
- invoke existing execution gates;
- execute canonical service/tool only after ALLOW.

### Step 8 — SEC-001/SEC-002 regression

- prove High-risk default deny;
- prove agent isolation cannot bypass it;
- prove public plaintext remains impossible;
- prove valid TLS works.

### Step 9 — Adversarial tests

- route/session/agent substitution;
- workspace substitution;
- capability substitution;
- hidden-tool direct call;
- denied side effects;
- concurrent agents.

### Step 10 — Real-client tests

Use the existing interoperability harness.

### Step 11 — Full verification

Run all required gates and inspect the diff.

### Step 12 — Scope audit

Confirm no duplicate MCP server, duplicate policy engine, model router, swarm scheduler, or unrelated Trust Wedge feature was introduced.

---

## 33. Verification gates

Run:

~~~
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
~~~

Also run focused MCP/security tests and any available real-client interoperability tests.

If a command cannot run because of environment limitations, report **NOT VERIFIED**. Never claim a green verification gate that was not actually run.

---

## 34. Completion criteria

Complete only when all applicable items are true:

- agent routes resolve through canonical agent state;
- route names are not authorization;
- unknown/disabled/inactive agents fail closed;
- MCP sessions bind to trusted agent/workspace context;
- session substitution fails;
- workspace substitution fails;
- capability checks use the resolved agent;
- policy denial overrides capability allowance;
- discovery is filtered but invocation is independently authorized;
- unknown tools fail closed;
- SEC-001 High-risk default-deny remains intact;
- SEC-002 blocks public/non-loopback plaintext;
- bearer credentials remain protected;
- denied operations have zero consequential side effects;
- concurrent agents remain isolated;
- existing MCP compatibility tests pass;
- real-client verification passes where available;
- no duplicate security authority was introduced;
- verification gates pass or are explicitly marked NOT VERIFIED;
- scope is limited to Prompt 03.

Do not claim the entire Trust Wedge is complete merely because Prompt 03 is complete.

---

## 35. Required final report

Report:

### Prompt 03 — Result

~~~
Status: COMPLETE / PARTIAL / BLOCKED
Baseline commit:
Final commit:
~~~

### Security path

Describe the actual:

~~~
transport
→ authentication
→ route
→ agent
→ session
→ workspace
→ registry
→ capability/policy
→ execution gate
→ service/tool
~~~

### Routing

State `/{agent}/mcp` and `/{agent}/sse` behavior, activation rules, and rejection behavior.

### Identity

State agent/session/workspace binding and substitution protection.

### Authorization

State capability/policy source, discovery filtering, invocation enforcement, SEC-001 behavior, and zero-side-effect evidence.

### Transport

State bearer/TLS/loopback/public-bind behavior.

### Tests

List focused, integration, adversarial, concurrent-agent, and real-client tests.

### Verification

Give exact results for:

~~~
cargo fmt
cargo check
cargo test
cargo clippy
git diff --check
~~~

### Files changed

List every changed file and its purpose.

### Compatibility

State which existing MCP contracts were preserved.

### Limitations

State missing infrastructure honestly.

### Scope confirmation

Explicitly confirm that no model router, swarm scheduler, duplicate MCP server, duplicate policy engine, or unrelated Trust Wedge feature was introduced.
