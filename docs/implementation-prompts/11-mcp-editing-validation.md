# Prompt 11 — MCP Editing + Real Client Validation (AWE-009 / AWE-016)

## Mission

Implement, harden, and prove the canonical AWH editing boundary exposed through MCP.

This prompt owns two closely related outcomes:

1. **AWE-009 — MCP editing:** expose the existing canonical edit transaction/service through the MCP tool surface without creating a second editor, transaction model, authorization model, snapshot path, rollback engine, or audit store.
2. **AWE-016 — real-client validation:** prove the MCP editing contract through the real protocol and transport boundary, using the repository's existing real-binary/interop harness where possible and extending it only where evidence is missing.

The implementation must treat the current `rust` branch as the source of truth. Historical issue-resolving and Trust-Wedge documents are requirements evidence only; current code and tested behavior win when they differ.

The desired architecture is:

```text
real MCP client
      |
      v
MCP transport/session/protocol
      |
      v
agent route + caller/session/workspace binding
      |
      v
existing MCP dispatcher / execution gate
      |
      v
existing capability + policy authorization
      |
      v
canonical EditService / EditTransaction
      |
      +--> snapshot / recovery boundary
      +--> provenance boundary
      +--> persistent audit boundary
      |
      v
workspace filesystem
      |
      v
post-edit verification / structured result
```

The MCP layer is an adapter and protocol boundary. It is not a second implementation of editing semantics.

---

## 1. Product boundary

AWH is an agent-agnostic, local-first workspace runtime for existing coding agents. External agents own reasoning, planning, model selection, and agent-specific orchestration. AWH owns controlled workspace mutation, capability/policy enforcement, snapshots, provenance, rollback, audit, and their shared interfaces.

For this prompt:

- MCP owns protocol handling, transport/session behavior, tool discovery, argument validation, request-to-service adaptation, and protocol-level error mapping.
- The existing editing service owns edit semantics.
- The existing authorization boundary owns capability/policy decisions.
- The existing agent/session/routing boundary owns caller identity and workspace binding.
- The existing snapshot/provenance boundary owns recovery capture and provenance.
- The existing audit boundary owns durable audit events.
- The filesystem owns actual bytes.
- The MCP client must observe the same result as any other AWH interface.

Do not move domain semantics into MCP merely because the request arrived through MCP.

---

## 2. Current repository contract

Before changing code, inspect the current `rust` branch rather than assuming historical state.

At minimum inspect:

```text
docs/roadmap/GROWTH_STRATEGY.md
docs/roadmap/PROJECT_ROADMAP.md
docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
docs/FEATURES.md
docs/PROJECT_CONTEXT.md
docs/architecture.md
docs/security.md
docs/threat-model.md
docs/mcp.md
docs/implementation-prompts/README.md
docs/implementation-prompts/01-init-runtime-contracts.md
docs/implementation-prompts/02-agent-runtime-identity.md
docs/implementation-prompts/03-mcp-routing-and-security.md
docs/implementation-prompts/04-edit-transaction-model.md
docs/implementation-prompts/05-edit-operation-engine.md
docs/implementation-prompts/06-edit-safety.md
docs/implementation-prompts/07-edit-authorization.md
docs/implementation-prompts/08-snapshots-provenance.md
docs/implementation-prompts/09-rollback-recovery.md
docs/implementation-prompts/10-persistent-audit.md
docs/implementation-prompts/12-cli-editing.md
docs/implementation-prompts/16-testing-and-acceptance.md
```

Also inspect the historical collections when available:

```text
docs/trust-wedge/
docs/issue-resolving-prompts/
```

Search historical material specifically for:

```text
AWE-009
AWE-016
filesystem.patch
filesystem.replace
filesystem.insert
filesystem.delete_range
filesystem.apply_diff
filesystem.rollback
MCP editing
MCP client
real-client validation
interop
tools/list
tools/call
schema validation
agent-specific MCP
```

Historical material must not cause a duplicate subsystem to be introduced.

---

## 3. Required source forensics

Inspect the complete current implementations, not only search excerpts.

### MCP boundary

Inspect:

```text
src/mcp/mod.rs
src/mcp/dispatcher.rs
src/mcp/server.rs
src/mcp/http.rs
src/mcp/sse.rs
src/mcp/schema.rs
src/mcp/error.rs
src/mcp/auth.rs
src/mcp/tls.rs
src/mcp/execution_gate.rs
src/mcp/permissions.rs
src/mcp/trust_store.rs
src/mcp/observability.rs
src/mcp/config.rs
src/mcp/store_lock.rs
```

Inspect any additional MCP routing/session/agent modules that exist on the current branch.

Determine:

- how `initialize` is enforced;
- how protocol versions are negotiated;
- how `tools/list` is constructed;
- how `tools/call` is dispatched;
- how notifications differ from requests;
- how malformed JSON and malformed envelopes map to JSON-RPC errors;
- how schema validation runs before handlers;
- how authentication is represented;
- how agent route identity is represented;
- how session identity is represented;
- how workspace identity is bound;
- how concurrent sessions share the dispatcher;
- how stdio differs from HTTP/SSE;
- how request-size and timeout limits work;
- how authorization is invoked;
- how audit events are emitted;
- how structured errors are sanitized.

Do not replace working protocol infrastructure with a new implementation.

### Editing boundary

Inspect:

```text
src/services/edit.rs
src/services/files.rs
src/services/authorization.rs
src/services/snapshot.rs
src/services/provenance.rs
src/services/audit.rs
src/core/errors.rs
```

Also inspect any current rollback/recovery module.

Identify the canonical types and methods for:

- `EditTransaction`;
- `EditOperation`;
- expected state;
- path validation;
- replacement;
- insertion;
- line deletion;
- multi-operation patch;
- unified diff application;
- conflict detection;
- atomic commit;
- verification;
- rollback/recovery;
- snapshot capture;
- provenance;
- audit.

If the current service already implements a requested behavior, MCP must call it rather than reproduce it.

### Tests and interop

Inspect:

```text
tests/mcp_protocol.rs
tests/mcp_server.rs
tests/mcp_executable.rs
tests/mcp_builtin_tool_gate.rs
tests/mcp_sandbox.rs
examples/mcp-interop/
```

Inspect any current test or example covering the official `@modelcontextprotocol/sdk`, MCP Inspector, stdio, SSE, HTTP, agent routes, or real compiled-binary interoperability.

The existing repository already contains real-binary stdio and an MCP interoperability harness. Extend those assets rather than creating a competing harness unless the existing one cannot express the required evidence.

---

## 4. Current AWE-009 tool surface

The current repository already contains MCP editing tool identifiers including:

```text
filesystem.replace
filesystem.insert
filesystem.delete_range
filesystem.patch
filesystem.apply_diff
filesystem.rollback
```

The current dispatcher also contains an `EditService` instance and routes the `filesystem.*` mutation tools toward it.

Treat that as existing implementation, not as permission to assume it is complete.

For each exposed tool, verify all of the following against current source:

1. tool is present in the canonical tool catalog;
2. tool has an accurate `inputSchema`;
3. schema is validated before handler execution;
4. tool name maps to exactly one authorization action;
5. authorization occurs before mutation;
6. caller/session/workspace context cannot be replaced by tool arguments;
7. path/resource validation is performed by the canonical service;
8. expected-state/conflict behavior is preserved;
9. snapshot/recovery semantics are preserved;
10. provenance correlation is preserved;
11. audit correlation is preserved;
12. result serialization is deterministic;
13. service errors become safe structured MCP errors;
14. no secrets or raw file contents leak through MCP errors or audit calls;
15. transport behavior is identical across supported MCP transports except for transport-specific envelopes.

---

## 5. Canonical tool-to-service mapping

The implementation must establish one explicit mapping table.

At minimum:

| MCP tool | Canonical action/service |
|---|---|
| `filesystem.replace` | `EditAction::Replace` → canonical `EditService` replace path |
| `filesystem.insert` | `EditAction::Insert` → canonical `EditService` insert path |
| `filesystem.delete_range` | `EditAction::DeleteRange` → canonical `EditService` delete-range path |
| `filesystem.patch` | `EditAction::Patch` → canonical multi-operation edit path |
| `filesystem.apply_diff` | `EditAction::ApplyDiff` → canonical unified-diff path |
| `filesystem.rollback` | `EditAction::Rollback` → canonical rollback/recovery path |

Do not introduce an MCP-specific `McpEditService`, `McpPatchEngine`, `McpRollbackService`, or equivalent.

If the current code has multiple adapters, they must converge on one canonical service rather than wrapping one another indefinitely.

---

## 6. Tool schema contract

Every editing tool must advertise a schema that accurately describes what the handler accepts.

Validate:

- required fields;
- field types;
- integer ranges;
- string limits;
- array limits;
- operation object shape;
- mutually exclusive operation variants;
- optional expected-state metadata;
- optional occurrence;
- path syntax;
- diff size;
- nesting depth;
- unknown/malformed values;
- empty operations;
- duplicate or ambiguous fields.

Schema validation must happen before the edit handler performs filesystem work.

A malformed request must not:

- read an arbitrary file;
- create a snapshot;
- mutate the filesystem;
- invoke rollback;
- consume unbounded memory;
- bypass authorization;
- create a misleading success audit event.

For malformed requests use the repository's existing JSON-RPC invalid-parameters/schema-error conventions. Do not invent a second MCP error vocabulary.

---

## 7. Protocol lifecycle

Validate editing through the complete MCP lifecycle.

The real client sequence must be equivalent to:

```text
connect
→ initialize
→ negotiate protocol version
→ receive capabilities
→ tools/list
→ identify editing tool
→ tools/call
→ receive result/error
→ continue or close session
```

Editing calls made before initialization must be rejected by the existing session gate.

Validate supported protocol versions using the repository's current advertised set rather than hard-coding an obsolete version.

Do not weaken lifecycle enforcement merely to make an editing test pass.

---

## 8. Tool discovery contract

The editing tools must be discoverable through the same canonical `tools/list` path as every other AWH built-in tool.

Verify:

- names are unique;
- names are stable;
- descriptions do not claim unsupported behavior;
- schemas match handlers;
- capability/agent filtering is respected where the current routing contract requires it;
- disabled/inactive agent routes do not accidentally expose tools;
- a denied high-risk operation is not made executable merely because it appears in the catalog;
- discovery metadata does not leak credentials, filesystem secrets, or internal host paths.

Where the current architecture distinguishes discovery from invocation authorization, preserve that distinction:

```text
tools/list
   ≠
authorization to execute
```

A tool may be discoverable while invocation is denied.

If the current agent-aware contract filters tools by capability/policy, test that behavior explicitly. If discovery is intentionally broader and authorization is invocation-time, document and test that contract instead of inventing a new one.

---

## 9. Caller, agent, session, and workspace binding

The MCP URL namespace or transport connection is routing information, not authorization.

For agent-specific endpoints, verify:

```text
route agent
   ↓
resolved agent identity
   ↓
MCP protocol session
   ↓
AWH AgentSession where available
   ↓
workspace
   ↓
capability/policy
   ↓
edit service
```

The following substitution attacks must fail:

- route says agent A but request claims agent B;
- session belongs to workspace A but path targets workspace B;
- session belongs to agent A but request supplies agent B;
- inactive agent attempts to invoke an edit route;
- unknown agent attempts to invoke an edit route;
- one agent reuses another agent's session identifier;
- one workspace's edit transaction is presented to another workspace;
- a tool argument attempts to override authenticated identity;
- a URL path attempts to grant permissions;
- a client changes agent/workspace metadata between `tools/list` and `tools/call`.

Never infer authorization from the string `/{agent}/mcp`.

---

## 10. Authorization ordering

For every consequential edit, the observable ordering must remain:

```text
transport validation
→ protocol/session validation
→ route/identity validation
→ tool lookup
→ argument/schema validation
→ capability/policy authorization
→ canonical edit validation
→ snapshot/recovery preparation
→ canonical edit execution
→ post-edit verification
→ provenance/audit integration
→ protocol result
```

Do not perform mutation before authorization.

Do not perform a compensating mutation after a denial.

Do not turn an authorization failure into a generic successful tool response.

The MCP adapter must use the current authorization boundary and current `EditAction` mapping. Do not duplicate capability or policy logic inside dispatcher branches.

---

## 11. High-risk default-deny behavior

Editing tools are consequential operations.

Verify the current capability/policy rules for:

```text
filesystem.replace
filesystem.insert
filesystem.delete_range
filesystem.patch
filesystem.apply_diff
filesystem.rollback
```

A missing, malformed, unavailable, expired, or out-of-scope authorization state must fail closed according to the existing authorization contract.

For a denial:

- no edit service mutation;
- no snapshot capture for the denied operation;
- no rollback;
- no partial file change;
- no success audit event;
- no secret-bearing error;
- deterministic machine-readable denial;
- safe human-readable explanation where the existing protocol supports it.

Preserve the current distinction between capability denial, policy denial, invalid request, and internal service failure.

---

## 12. Editing semantics must remain service-owned

MCP must not decide what constitutes a successful edit.

The canonical service remains responsible for:

- occurrence semantics;
- line indexing;
- line insertion;
- line deletion;
- diff parsing;
- diff context matching;
- expected-state matching;
- path containment;
- symlink/escape protection;
- atomic commit;
- multi-operation preparation;
- post-commit verification;
- recovery;
- structured edit status;
- conflict detection.

The dispatcher should construct the canonical request and invoke the service.

If the MCP adapter needs to translate JSON into a Rust type, that translation must be lossless with respect to the canonical service contract.

---

## 13. Expected-state and stale-state behavior

MCP clients frequently operate on files after an earlier read. Therefore stale-state handling must be preserved end-to-end.

Test:

1. client reads file;
2. another actor changes file;
3. client submits an edit based on stale expected state;
4. AWH detects the conflict;
5. no stale overwrite occurs;
6. MCP returns the canonical structured conflict/error;
7. audit records the conflict where the audit contract requires it;
8. filesystem remains unchanged by the rejected edit.

Do not convert stale-state conflicts into generic `-32603` errors if the current service contract provides a structured domain error that can be safely represented.

Do not expose the entire conflicting file when the error can be expressed with hashes, paths, state identifiers, or bounded metadata.

---

## 14. Multi-operation patch behavior

For `filesystem.patch` verify:

- operations are parsed deterministically;
- unsupported operation shapes are rejected;
- empty operation arrays are rejected if the service contract requires;
- all target paths remain workspace-relative;
- all preparation happens before mutation;
- one failed operation prevents unsafe partial application according to the edit service contract;
- expected state is evaluated against the correct live state;
- operation ordering is preserved;
- duplicate paths follow the canonical transaction semantics;
- successful results identify the canonical edit transaction;
- verification occurs before success is reported.

Do not reimplement multi-file transaction semantics in the MCP handler.

---

## 15. Unified diff behavior

For `filesystem.apply_diff` verify:

- malformed diff is rejected before mutation;
- binary patches are rejected when unsupported by the canonical service;
- path headers cannot escape the workspace;
- context mismatches produce a conflict/error;
- multi-file diffs preserve canonical transaction semantics;
- no blind whole-file replacement occurs as a fallback;
- no partial mutation is reported as success;
- newline/no-final-newline behavior remains owned by the edit service;
- exact bytes are preserved where supported by the service.

The MCP layer should pass the diff to the canonical service rather than parse/apply it itself.

---

## 16. Rollback exposure

If `filesystem.rollback` is currently exposed on the MCP surface, it must call the canonical rollback/recovery boundary.

Verify:

- rollback target is a real edit transaction;
- rollback authorization is checked through the existing `EditAction::Rollback` path;
- snapshot/provenance records are resolved through the canonical stores;
- produced-state conflict protection is preserved;
- another actor's post-edit modification prevents unsafe overwrite;
- exact pre-edit bytes are restored where the snapshot contract allows;
- created files are removed only under the existing produced-state safety rule;
- post-rollback verification is performed;
- repeated rollback follows the canonical idempotence/error contract;
- rollback audit/provenance correlation is preserved.

Do not add a second MCP rollback algorithm.

If the tool is not actually implemented on the current branch, the prompt must require completing the integration only after the canonical rollback service exists; do not fabricate a protocol-level success path.

---

## 17. Snapshot, provenance, and audit correlation

MCP must preserve the existing downstream chain:

```text
MCP request
  → edit id / request correlation
  → authorization decision
  → snapshot
  → edit
  → verification
  → provenance
  → audit
```

The MCP layer may add transport/request correlation metadata, but it must not create a second provenance or audit store.

Do not log full file contents.

Do not include bearer tokens, API keys, private keys, passwords, authorization headers, or other secrets in MCP errors, traces, or audit details.

Do not expose unnecessary absolute filesystem paths.

Where the persistent audit contract is available, verify that denied, conflicted, successful, failed, and rollback outcomes are distinguishable.

---

## 18. Error mapping

Define and test one deterministic mapping from canonical AWH failures to MCP results.

At minimum distinguish:

| Failure | Required MCP behavior |
|---|---|
| malformed JSON | existing parse-error contract |
| malformed JSON-RPC envelope | existing invalid-request contract |
| unknown method | existing method-not-found contract |
| unknown tool | existing tool-not-found contract |
| invalid tool arguments | existing invalid-params/schema contract |
| authentication failure | existing authentication contract |
| unknown/inactive agent | existing routing/identity denial |
| missing capability | existing authorization denial |
| policy denial | existing authorization denial |
| invalid path | canonical edit/path error, safely mapped |
| stale expected state | canonical conflict result |
| malformed diff | canonical edit error |
| snapshot/recovery preparation failure | mutation blocked; safe service error |
| edit application failure | canonical service failure |
| verification failure | canonical verification/recovery result |
| rollback conflict | canonical rollback conflict |
| unexpected internal failure | sanitized internal-error result |

Never expose Rust backtraces, filesystem secrets, environment variables, authorization material, or raw internal storage paths to an MCP client.

Never turn a failed mutation into a success merely because the JSON-RPC transport itself succeeded.

---

## 19. JSON-RPC result integrity

Verify:

- request IDs are preserved;
- response IDs match the originating request;
- notifications do not receive responses where the protocol forbids them;
- errors use the correct JSON-RPC envelope;
- successful tool results use the current MCP tool-result structure;
- structured service information is serialized deterministically;
- malformed service output cannot produce invalid JSON;
- multiple concurrent requests cannot cross-wire response IDs;
- stdout remains protocol-clean for stdio;
- tracing/logging stays off the protocol stdout channel.

A real MCP client must be able to parse every response.

---

## 20. Transport coverage

At minimum cover the transports actually supported by the current branch.

### Stdio

Use the real compiled `awh` binary where the repository's harness supports it.

Verify:

- process starts cleanly;
- initialize works;
- tools/list exposes editing tools;
- tools/call invokes editing;
- malformed input produces valid JSON-RPC errors;
- stdout contains protocol output only;
- process exits cleanly at EOF;
- multiple sequential calls preserve session state.

### HTTP/SSE

Where supported, verify:

- authentication;
- initialization;
- session assignment;
- agent route binding;
- tools/list;
- tools/call;
- malformed requests;
- denied edits;
- successful edits;
- concurrent sessions;
- disconnect/reconnect behavior;
- response correlation;
- request/body limits;
- timeout behavior.

Do not claim HTTP/SSE coverage if the required test environment is unavailable. Record the exact limitation.

---

## 21. Real-client interoperability

AWE-016 requires protocol-level evidence from a real client, not only direct calls to Rust functions.

Prefer the repository's existing:

```text
examples/mcp-interop/
tests/mcp_executable.rs
```

and the official `@modelcontextprotocol/sdk` reference client where already configured.

The real-client test should perform the actual sequence:

```text
spawn real awh
→ connect using MCP client implementation
→ initialize
→ tools/list
→ locate filesystem editing tool
→ tools/call
→ inspect result
→ inspect filesystem
→ perform denied/malformed/conflict case
→ inspect error
→ close
```

Where the official SDK or another standards-compliant client is available, use it rather than a hand-written protocol simulator.

A raw JSON-RPC test is still useful for byte-level coverage, but it is not sufficient by itself for the AWE-016 evidence requirement.

---

## 22. Real-client test matrix

Build a matrix with at least:

| Scenario | Client boundary | Expected evidence |
|---|---|---|
| initialize | real client | successful negotiated session |
| tools/list | real client | editing tools discoverable according to policy contract |
| replace | real client | file changes through canonical service |
| insert | real client | exact line semantics |
| delete range | real client | exact range semantics |
| patch | real client | multi-operation transaction |
| apply diff | real client | unified diff reaches canonical service |
| malformed args | real client | invalid-params response; no mutation |
| unauthorized edit | real client | denial; no mutation |
| wrong workspace | real client | containment/identity denial |
| inactive/unknown agent | agent route | deterministic rejection |
| stale state | real client | conflict; no stale overwrite |
| snapshot failure | fault injection where practical | mutation blocked |
| verification failure | fault injection where practical | no false success |
| rollback | real client | canonical recovery semantics |
| concurrent clients | real transport | isolated sessions and correctly correlated responses |
| restart | real binary | durable downstream records remain correlated |

Only mark a row passed when the evidence comes from the requested boundary.

---

## 23. Agent-specific endpoint validation

When the current MCP routing implementation supports agent-specific routes, test at least:

```text
/claude/mcp
/claude/sse
/qwen/mcp
/qwen/sse
/opencode/mcp
/opencode/sse
```

Use only routes actually configured by the current repository.

For each active route verify:

- route resolves to the intended agent;
- inactive agents are rejected;
- route names are validated;
- the same underlying dispatcher/service semantics are used;
- tool discovery is correctly scoped;
- invocation cannot substitute another agent;
- workspace/session binding is preserved;
- concurrent agent sessions do not share mutable session state;
- audit events retain the correct caller correlation.

Do not duplicate the edit service per agent.

---

## 24. Concurrent-client isolation

The dispatcher is shared across concurrent sessions, so editing tests must prove that session-specific state is not accidentally shared.

Run at least two concurrent clients and verify:

- distinct request IDs remain distinct;
- session state does not cross;
- agent identity does not cross;
- workspace roots do not cross;
- one client's authorization cannot grant another client's request;
- one client's edit cannot silently target another workspace;
- response ordering may vary but response IDs remain correct;
- audit/provenance correlation remains attributable;
- no global mutable MCP request context is used as a shortcut.

If the existing architecture uses immutable/shared dispatcher state plus per-session state, preserve that design.

---

## 25. Resource limits and abuse resistance

MCP is an externally reachable protocol boundary.

For every editing tool enforce the existing configured limits for:

- request body;
- JSON depth;
- string sizes;
- array sizes;
- operation count;
- diff size;
- path length;
- error size;
- response size;
- request timeout.

Test adversarial inputs:

- enormous replacement strings;
- huge patch arrays;
- deeply nested operation objects;
- excessively long paths;
- oversized unified diffs;
- invalid UTF-8 at the transport boundary where applicable;
- repeated malformed calls;
- many concurrent calls;
- pathological occurrence values;
- integer overflow candidates;
- duplicate operation fields.

A rejected request must not partially mutate the filesystem.

Do not add arbitrary new limits without first checking the existing configuration contract. If a limit is required to close a demonstrated resource-exhaustion gap, document the chosen bound and why it is compatible with existing clients.

---

## 26. Path and filesystem safety

MCP arguments are untrusted input.

The canonical edit service must remain responsible for secure path handling.

Test:

```text
absolute paths
../ traversal
nested traversal
encoded traversal where decoded by an upstream layer
dot segments
empty paths
workspace root itself
symlink escapes
symlink replacement races where the current platform permits testing
paths from a different workspace
Windows-style separators where relevant
NUL/control characters where relevant
very long paths
```

Never normalize a path in MCP in a way that weakens the canonical service's security checks.

Never accept a client-supplied absolute path as an authorization substitute.

---

## 27. No hidden mutation on validation failure

For every validation/authorization failure verify filesystem state before and after.

The invariant is:

```text
invalid request
   ⇒ no snapshot
   ⇒ no edit
   ⇒ no rollback
   ⇒ no partial file mutation
```

For authorization denial:

```text
denied request
   ⇒ no mutation
```

For stale-state conflict:

```text
conflict
   ⇒ no stale overwrite
```

For preparation failure:

```text
preparation failure
   ⇒ no committed edit
```

For post-commit verification failure, follow the existing edit/recovery contract rather than inventing a dispatcher-level compensation mechanism.

---

## 28. Audit semantics at the MCP boundary

Use the canonical audit API.

At minimum distinguish:

- tool request accepted;
- authorization denied;
- malformed/invalid tool request where the audit contract requires;
- edit committed;
- edit conflict;
- edit failed;
- rollback requested;
- rollback denied;
- rollback committed;
- rollback conflict.

Audit records must contain bounded, non-secret metadata.

Never pass:

- bearer tokens;
- API keys;
- authorization headers;
- private keys;
- passwords;
- complete file contents;
- giant diffs;
- arbitrary request bodies.

The persistent audit implementation from Prompt 10 is authoritative when present. Do not introduce an MCP audit ring.

---

## 29. Compatibility with existing MCP clients

The implementation must preserve compatibility with the existing MCP protocol contract.

Check:

- current supported protocol versions;
- current `tools/list` response structure;
- current `tools/call` result structure;
- current JSON Schema subset;
- current error codes;
- current session lifecycle;
- current authentication headers;
- current SSE semantics;
- current stdio framing;
- current optional metadata behavior.

Do not make schemas unnecessarily strict in ways that reject standards-compliant clients.

Do not make schemas so permissive that malformed mutations reach the service.

Use the repository's existing schema validator and error mapping.

---

## 30. Do not duplicate MCP infrastructure

Before adding any code, search for existing:

```text
McpDispatcher
StdioMcpServer
SSE server
HTTP server
tools_list_static
schema validator
execution gate
EditService
EditAction
audit functions
agent/session routing
interop harness
```

There must remain:

- one canonical dispatcher;
- one canonical edit service;
- one canonical authorization boundary;
- one canonical snapshot store;
- one canonical provenance store;
- one canonical persistent audit writer;
- one canonical MCP protocol/session implementation per transport abstraction;
- one canonical real-client interoperability harness.

If an existing adapter is duplicated, consolidate it rather than adding another wrapper.

---

## 31. Required implementation sequence

Implement in this exact linear order.

### Step 1 — Repository contract audit

Record the current implementation of:

- MCP lifecycle;
- tool registry;
- editing tools;
- schema validation;
- authorization;
- agent routing;
- session binding;
- workspace binding;
- edit service;
- snapshot/provenance;
- audit;
- real-client harness.

### Step 2 — Tool contract reconciliation

For every editing tool compare:

```text
tool name
description
inputSchema
authorization action
dispatcher branch
canonical service method
result schema
error mapping
```

Fix only demonstrated mismatches.

### Step 3 — Thin-adapter hardening

Ensure dispatcher branches:

- validate arguments;
- resolve caller context;
- authorize;
- construct the canonical transaction;
- invoke the canonical service;
- serialize the canonical result.

Remove domain logic that duplicates `EditService`.

### Step 4 — Security boundary hardening

Verify route/session/workspace/agent binding and fail-closed authorization.

### Step 5 — Protocol error hardening

Ensure service errors map deterministically to safe MCP errors without leaking internals.

### Step 6 — Transport tests

Extend existing stdio/HTTP/SSE tests as required.

### Step 7 — Real-client interoperability

Extend the existing official SDK/reference-client harness to exercise editing.

### Step 8 — Adversarial and concurrency tests

Add malformed, denial, stale-state, isolation, size-limit, and concurrent-client cases.

### Step 9 — Evidence review

Run the full verification suite and inspect actual filesystem/audit/provenance results.

### Step 10 — Documentation only where required by implementation

Update implementation-facing documentation only if the code change makes existing documentation materially false. Do not expand scope into a documentation rewrite.

---

## 32. Required tests — MCP tool schemas

Add or strengthen tests for every editing tool.

Verify:

- required arguments;
- wrong argument types;
- missing arguments;
- invalid integer values;
- malformed operation objects;
- invalid path values;
- oversized values;
- unsupported operation variants;
- malformed diffs;
- schema rejection before handler execution.

A schema failure must not mutate the filesystem.

---

## 33. Required tests — service mapping

For every tool prove that the MCP request reaches the canonical service.

Use observable behavior or targeted test seams to demonstrate:

```text
filesystem.replace → EditService
filesystem.insert → EditService
filesystem.delete_range → EditService
filesystem.patch → EditService
filesystem.apply_diff → EditService
filesystem.rollback → canonical rollback service
```

Do not assert private implementation details unnecessarily; prove the architectural boundary through behavior and focused seams.

---

## 34. Required tests — authorization

Cover:

- authorized edit;
- missing capability;
- policy denial;
- expired grant;
- out-of-scope grant;
- unavailable authorization state;
- rollback denial;
- agent mismatch;
- session mismatch;
- workspace mismatch;
- inactive agent;
- unknown agent.

For each denied case prove no filesystem mutation.

---

## 35. Required tests — editing correctness

Through MCP, not only direct service calls, cover:

- replacement;
- occurrence selection;
- ambiguous occurrence;
- insertion;
- deletion;
- multi-operation patch;
- unified diff;
- multi-file diff;
- expected-state match;
- expected-state mismatch;
- empty file;
- no-final-newline;
- LF;
- CRLF;
- Unicode;
- Devanagari;
- emoji;
- large but permitted files;
- workspace-relative paths.

---

## 36. Required tests — rollback/recovery

Through MCP where rollback is exposed:

- successful rollback;
- unknown edit ID;
- wrong workspace;
- unauthorized rollback;
- produced-state mismatch;
- concurrent modification;
- repeated rollback;
- missing snapshot;
- corrupt snapshot;
- post-rollback verification.

Do not duplicate rollback logic in tests; drive the canonical service through MCP.

---

## 37. Required tests — protocol correctness

Cover:

- initialize;
- supported protocol version;
- unsupported protocol version;
- pre-initialize tool call;
- malformed JSON;
- malformed JSON-RPC envelope;
- unknown method;
- unknown tool;
- invalid params;
- request IDs;
- notifications;
- concurrent requests;
- clean EOF;
- transport disconnect.

---

## 38. Required tests — real-client evidence

The real-client harness must produce explicit evidence for:

1. initialize succeeded;
2. editing tool discovered;
3. authorized edit succeeded;
4. filesystem changed exactly as expected;
5. malformed edit was rejected;
6. unauthorized edit was denied;
7. stale edit was rejected;
8. rollback, when exposed and available, followed canonical semantics;
9. transport remained protocol-valid.

If the repository's official SDK harness supports multiple transports, run the editing matrix over each supported transport.

If an external client such as OpenCode, Codex, Claude Code, Qwen Code, or OpenHands is available in the test environment, use it as an additional interoperability check. Do not make a vendor-specific client mandatory if it cannot be installed or executed in the CI environment; record that limitation and retain standards-compliant client evidence.

---

## 39. Required tests — stdout/protocol hygiene

For stdio:

- stdout contains only MCP protocol data;
- logs/traces are not written to stdout;
- stderr may carry diagnostics according to the current server contract;
- a client can parse every response;
- one malformed request does not corrupt subsequent framing.

This must be tested against the compiled binary, not only an in-process dispatcher.

---

## 40. Required tests — concurrent sessions

Run at least two independent MCP sessions concurrently.

Use different:

- request IDs;
- workspaces where supported;
- agent identities where supported;
- edit targets.

Verify:

- no cross-session state;
- no response cross-talk;
- no identity confusion;
- no workspace escape;
- no corrupted files;
- no duplicate edit IDs caused by the MCP adapter;
- audit/provenance correlations remain correct.

---

## 41. Required tests — failure injection

Where the current architecture provides test seams, inject failures at:

- authorization lookup;
- snapshot capture;
- edit preparation;
- atomic write;
- verification;
- provenance write;
- audit write;
- transport disconnect.

For each case verify the actual edit outcome and the MCP response separately.

In particular, do not let an audit/provenance observer failure incorrectly turn a committed filesystem edit into a false success/failure representation unless the current service contract explicitly makes that subsystem part of the transaction's commit point.

---

## 42. Required security invariants

The completed implementation must satisfy all of these:

1. MCP is never an authorization bypass.
2. URL route names are never treated as authorization.
3. Every consequential edit passes the existing authorization boundary.
4. Editing semantics exist in exactly one canonical service.
5. Snapshot/recovery semantics exist in exactly one canonical store/service.
6. Rollback semantics exist in exactly one canonical service.
7. Audit persistence exists in exactly one canonical audit subsystem.
8. Schema validation occurs before edit execution.
9. Invalid requests do not mutate the workspace.
10. Denied requests do not mutate the workspace.
11. Stale expected state does not overwrite newer state.
12. Workspace paths cannot escape the authorized workspace.
13. Agent/session/workspace identity cannot be substituted through tool arguments.
14. Concurrent sessions remain isolated.
15. Secrets and full file contents do not leak through MCP errors or audit metadata.
16. stdio stdout remains protocol-clean.
17. Response IDs remain correctly correlated.
18. Resource limits remain bounded.
19. Unsupported behavior is never reported as implemented.
20. Real-client evidence is based on actual transport/protocol execution.

---

## 43. Relationship to Prompt 03

Prompt 03 defines the MCP routing/security boundary. This prompt must consume the current implementation of that boundary.

Do not:

- create another route validator;
- create another session store;
- create another authentication mechanism;
- replace the existing execution gate;
- move agent authorization into editing handlers.

If current code reveals a security defect required for AWE-009/AWE-016 correctness, fix the smallest canonical boundary necessary and document why it belongs in this implementation rather than creating a parallel subsystem.

The prompt is independently executable against current `rust`; do not require the executor to merge another prompt PR first.

---

## 44. Relationship to Prompts 04–10

Use the current source contracts for:

- edit transaction/model;
- edit execution/safety;
- authorization;
- snapshots/provenance;
- rollback;
- persistent audit.

Do not reimplement any of them.

The executor may need to fix an integration seam that is demonstrably missing, but the result must still converge on the existing canonical service/store.

Prompt 11 is not allowed to become a second implementation of those prompts.

---

## 45. Relationship to Prompt 12

Prompt 12 owns the CLI editing interface.

Prompt 11 must not add CLI commands or duplicate CLI editing behavior.

Both interfaces must ultimately reach the same application service:

```text
MCP ──┐
CLI ──┼──> canonical AWH editing service
TUI ──┘
```

Only MCP protocol adaptation belongs here.

---

## 46. Relationship to Prompt 16

Prompt 16 owns broad acceptance evidence.

Prompt 11 must still include focused MCP editing and real-client tests required to prove AWE-009/AWE-016. Do not defer essential MCP correctness to Prompt 16.

Prompt 16 may later aggregate these tests; it must not become a hidden prerequisite for this prompt.

---

## 47. Non-goals

Do not implement:

- model routing;
- agent reasoning;
- planner/orchestrator;
- subagent spawning;
- generic workflow engine;
- new MCP protocol version unrelated to editing;
- new transport solely for this feature;
- WebSocket transport unless separately required by the current roadmap;
- second editing service;
- second authorization system;
- second snapshot store;
- second provenance store;
- second rollback engine;
- second audit store;
- generic event bus;
- analytics platform;
- remote SIEM;
- Git reset/revert implementation;
- worktree implementation;
- CLI editing commands;
- TUI editing screens;
- vendor-specific agent orchestration.

---

## 48. Performance discipline

MCP adaptation should add minimal overhead.

Avoid:

- reading the same file solely to construct MCP metadata when the edit service already reads it;
- parsing a diff twice;
- serializing/deserializing the same transaction repeatedly;
- duplicating large file contents in error messages;
- unbounded cloning of patch arrays;
- per-request construction of heavyweight global services;
- blocking the async runtime with synchronous network or filesystem work where the current architecture already provides async boundaries.

Measure or reason about the dominant cost before optimizing.

Correctness and security take priority over micro-optimizations.

---

## 49. Documentation and interoperability claims

Only document an editing tool as implemented when all required layers exist:

```text
schema
→ dispatcher
→ authorization
→ canonical service
→ filesystem mutation
→ verification
→ required recovery/provenance/audit
→ protocol test
→ real-client evidence
```

Do not mark a feature complete because:

- a tool name appears in `tools/list`;
- a dispatcher branch exists;
- a unit test calls the Rust method directly;
- a help message mentions the tool;
- a historical report says it existed;
- an in-process fake client succeeds.

Real-client validation is part of the acceptance evidence for AWE-016.

---

## 50. Verification gates

Before declaring Prompt 11 complete, run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Also run the repository's focused MCP/interoperability commands when available, including:

```text
mcp protocol tests
mcp executable tests
mcp server tests
mcp built-in tool gate tests
mcp interop harness
```

Record exact commands and outcomes.

If a real-client test cannot run because a required external dependency, client binary, or transport environment is unavailable, state exactly what was unavailable and do not convert that absence into a passing claim.

---

## 51. Completion criteria

Prompt 11 is complete only when:

- every supported MCP editing tool has one canonical service mapping;
- schemas accurately describe accepted input;
- schema validation happens before handler execution;
- authorization is enforced through the existing boundary;
- agent/session/workspace binding is preserved;
- route names cannot grant authorization;
- editing uses the canonical `EditService`;
- expected-state/conflict semantics are preserved;
- snapshots/recovery are preserved;
- provenance is preserved;
- persistent audit correlation is preserved;
- rollback uses the canonical rollback service;
- errors are structured and sanitized;
- stdio protocol output is clean;
- supported transports behave consistently;
- concurrent sessions are isolated;
- resource limits are enforced;
- malformed and adversarial inputs are tested;
- stale edits cannot overwrite newer state;
- real compiled-binary evidence exists;
- a standards-compliant real MCP client successfully discovers and invokes an editing tool;
- denied/malformed/stale cases are proven at the protocol boundary;
- no duplicate subsystem was introduced;
- full verification gates pass;
- documentation does not claim unsupported behavior.

---

## 52. Independence rule

This prompt must be executable against the current `rust` branch as a standalone implementation task.

Do not require:

- another implementation-prompt PR to merge first;
- a historical branch;
- an old commit;
- an unavailable issue;
- an external client that is not necessary for standards-compliant evidence;
- assumptions that contradict current source.

If current `rust` has already implemented part of AWE-009/AWE-016, preserve it and harden only the missing contract.

If current `rust` has advanced beyond the historical requirements, update the implementation plan to match the current architecture rather than regressing to historical designs.

---

## 53. Final implementation report

The executor must finish with a concise evidence report containing:

1. repository/source forensics performed;
2. current MCP architecture used;
3. canonical editing service used;
4. tool-to-service mapping;
5. schema validation behavior;
6. authorization and identity binding;
7. workspace/path security;
8. snapshot/provenance/rollback integration;
9. audit integration;
10. protocol/error mapping;
11. transport coverage;
12. real-client evidence;
13. adversarial tests;
14. concurrency/isolation tests;
15. resource-limit tests;
16. failure-injection results;
17. exact verification commands and outcomes;
18. changed files;
19. any environment/client limitations;
20. explicit confirmation that no duplicate editor, authorization system, snapshot store, rollback engine, or audit store was introduced.

The report must distinguish:

```text
implemented and tested
implemented but environment-limited
existing and reused
not in scope
```

Never claim real-client interoperability without actual client/transport evidence.

---

## 54. Strict scope

Implementation work should be limited to the smallest set of source/test/example files required to complete AWE-009 and AWE-016.

Do not modify `docs/implementation-prompts/11-mcp-editing-validation.md` during execution unless the task explicitly asks to revise the prompt.

Do not modify other implementation prompts as part of this task.

Do not perform unrelated refactors.

If a broader architectural defect blocks MCP editing, make the smallest canonical fix necessary and explain why it is required for this prompt.

The final change set must make the MCP editing boundary more correct, more secure, and more demonstrably interoperable without creating a parallel AWH subsystem.
