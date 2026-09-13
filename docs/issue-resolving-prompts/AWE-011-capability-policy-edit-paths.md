# AWE-011 / #32 — Capability and Policy Enforcement on Every Edit Path

## Mission

Implement production-grade authorization for the canonical AWH editing system so that **every consequential edit and rollback operation crosses one authoritative capability-and-policy boundary before mutation**.

This issue is the security bridge between the agent/session identity model and the canonical `EditService`. The objective is not to create another authorization subsystem. The objective is to make the existing capability, trust, policy, audit, and service-layer concepts actually participate in one deterministic decision path.

The required conceptual path is:

```text
Caller / Agent
    → Session
    → Workspace
    → Capability
    → Resource
    → Policy
    → EditService
    → filesystem mutation
```

The implementation must preserve transport independence: MCP, CLI, TUI, and Control API must ultimately obtain the same authorization decision for the same principal, workspace, capability, resource, operation, and policy state.

---

## 1. Issue and dependency contract

Implement **only AWE-011 / GitHub issue #32**.

Required dependencies:

- AWE-009 / #30 — MCP agent-grade editing.
- AWE-010 / #31 — edit-level rollback.
- Existing capability-grant infrastructure.
- Existing trust / permission / execution-gate infrastructure.
- Existing policy infrastructure.
- Existing audit infrastructure.

AWE-011 must not redesign AWE-009 or AWE-010. It must place an authoritative authorization checkpoint around them.

Do not prematurely implement AWE-012 snapshot/provenance storage or AWE-013 persistent audit redesign. AWE-011 may emit the existing audit events and may expose stable authorization metadata needed by those later milestones.

Identity work may prepare seams for the later `TOML → AgentProfile → AgentRegistry → AgentSession → PolicyEngine` architecture, but TOML parsing must not be embedded inside the policy evaluator.

---

## 2. Repository preflight — inspect before editing

Before writing code, perform a forensic repository audit on the current `rust` branch.

Inspect at minimum:

- `src/services/edit.rs`
- `src/models/capability_grant.rs`
- `src/core/capability_grants.rs`
- `src/core/policy.rs`
- `src/mcp/execution_gate.rs`
- `src/mcp/permissions.rs`
- MCP dispatcher and built-in tool dispatch paths.
- CLI edit/file/tool paths.
- TUI paths that can initiate mutations.
- Control API paths that can initiate mutations.
- Agent/session/workspace identity code that already exists.
- Existing audit implementation.
- Existing secure filesystem/path helpers.
- Existing tests covering authorization, trust, permissions, and editing.
- `docs/PROJECT_ROADMAP.md`, `docs/PROJECT_STATUS.md`, architecture/security documentation, and prior AWE prompts where necessary.

Do not trust stale documentation over executable code. Determine what is actually wired on the `rust` branch.

Record the actual current architecture before implementing anything. In particular, identify every route by which an edit or rollback can reach mutation code and every route that currently performs authorization independently.

### Known forensic baseline

The repository currently contains:

- `CapabilityGrant` with `agent_id`, `permission`, optional `scope`, `granted_at`, and optional `expires_at`.
- `CapabilityGrantStore` persisting grants under `.agent/capabilities`.
- `PolicyStore` persisting policy rules under `.agent/policy.json`.
- An MCP execution gate/trust path.
- Built-in-tool authorization that historically allowed Medium/High tools when no built-in trust record existed for backward compatibility.
- A narrow policy implementation supporting only a limited set of built-in tools.
- Agent capability grants that are recorded but not yet authoritative for edit authorization.
- The canonical `EditTransaction`/`EditOperation` model in `src/services/edit.rs`.

These facts must be revalidated against the current source before implementation; do not blindly reproduce them if the branch has changed.

---

## 3. Core security invariant

Establish this invariant:

> **No edit or rollback mutation may occur unless the caller has passed the single authoritative authorization boundary for that exact operation and resource.**

This includes, at minimum:

- `Replace`
- `Insert`
- `DeleteRange`
- `Patch`
- `ApplyDiff`
- edit-level rollback from AWE-010
- every future operation that is routed through the canonical edit service

The authorization decision must happen **before any mutation**, including temporary-file writes that are part of the mutation transaction.

Authorization failure must be zero-mutation:

- no target-file mutation
- no partial transaction mutation
- no rollback mutation
- no hidden fallback to an unrestricted service path
- no capability escalation through an alternate transport
- no authorization based solely on URL/route naming

Audit logging is permitted/required where the existing architecture supports it, but audit emission must not itself mutate the protected resource.

---

## 4. One authoritative authorization service

Create or adapt **one shared authorization boundary** rather than adding independent checks in MCP, CLI, TUI, or individual edit operations.

The shared decision should conceptually receive a request containing enough information to answer:

```text
principal / agent identity
session identity
workspace identity
operation / tool
capability required
resource / path
requested scope
risk level where applicable
edit identity where applicable
rollback identity where applicable
current policy context
```

The exact Rust type names are implementation-dependent, but the contract must be explicit, typed, deterministic, and transport-independent.

A reasonable conceptual result is:

```text
Allow
Deny { reason, rule/grant metadata as appropriate }
```

Do not expose internal secrets or sensitive policy data unnecessarily in denial responses.

### Decision ordering

The authorization pipeline should be deterministic and fail closed. A suitable ordering is:

1. Resolve authenticated caller/principal.
2. Resolve agent identity when the caller is an agent.
3. Resolve session identity.
4. Resolve workspace identity and verify the requested resource belongs to that workspace.
5. Resolve the canonical operation/tool identity.
6. Resolve required capability/capabilities.
7. Resolve the agent's active grants.
8. Validate grant expiry.
9. Validate grant scope against the canonical resource.
10. Evaluate policy rules.
11. Apply any existing trust/execution constraints that remain authoritative.
12. Produce one allow/deny result.
13. Audit the decision using existing audit infrastructure.
14. Only after allow, invoke `EditService` mutation/rollback logic.

Do not allow a lower layer to silently bypass an earlier required authorization stage.

---

## 5. Capability-grant enforcement

The existing `CapabilityGrant` model must become an actual authorization input rather than passive persisted metadata.

For an agent-scoped request, evaluate grants using:

- exact `agent_id` ownership
- required `Permission`
- resource scope
- grant validity
- expiration
- workspace/resource containment

### Expiration

`expires_at` is security-sensitive and must be enforced.

Rules:

- `None` means no expiry, subject to revocation/policy.
- A grant with an expiry at or before the authorization evaluation time is denied.
- Invalid/unparseable expiry data must fail closed rather than silently becoming non-expiring.
- Time comparison must use a consistent UTC/RFC-3339 representation.
- Do not depend on string comparison unless the representation is first normalized and correctness is demonstrated.
- Avoid race-prone behavior where a grant can expire between validation and mutation without an explicit boundary decision.

Tests must cover:

- valid future expiry
- exact expiry boundary
- expired grant
- malformed expiry
- no expiry

### Scope

If a grant contains `scope`, it must constrain the authorized resource.

For filesystem edits:

- canonicalize/normalize the resource using the repository's existing secure path model;
- enforce workspace containment;
- compare scopes using path-component semantics, not naive string-prefix semantics;
- prevent `src/foo` from authorizing `src/foobar`;
- prevent `..`, absolute paths, symlink escapes, and equivalent traversal representations;
- ensure a grant cannot authorize a resource outside its intended workspace.

Do not invent a second path-security implementation if a canonical secure filesystem helper already exists. Reuse the established security boundary.

An unscoped grant may only be treated as unrestricted if that meaning is explicitly part of the capability contract and remains safe for the relevant permission. Do not infer broad authority from missing scope accidentally.

---

## 6. Agent/session/workspace identity

Authorization must know **who** is requesting the mutation.

The decision path must carry enough identity to distinguish at least:

- human/operator caller
- agent identity
- session identity
- workspace identity

Do not use an MCP route or URL as authorization by itself.

For example, a route such as:

```text
/{agent}/mcp
```

may identify/routinely select an agent session, but the string `agent` in the URL must never itself constitute proof that the caller owns that identity or has the requested capability.

Where the current branch lacks a complete `AgentSession` abstraction, introduce the smallest explicit typed seam required for authorization. Do not create an unrelated full agent runtime in this issue.

Prepare the architecture for:

```text
TOML
  ↓
AgentProfile
  ↓
AgentRegistry
  ↓
AgentSession
  ↓
Authorization / PolicyEngine
```

Do not parse TOML inside `PolicyEngine` or the low-level edit service.

The edit service should receive an already-resolved authorization context rather than discovering identity from transport-specific state.

---

## 7. Policy enforcement

Unify policy evaluation with capability evaluation at the same authorization boundary.

Existing `PolicyStore` semantics must be respected rather than silently replaced.

At minimum, support the currently implemented policy model correctly for every edit path that it covers.

The implementation must make clear the distinction between:

- **capability** — what the principal is allowed to possess/use;
- **resource scope** — where that capability applies;
- **policy** — contextual rules that can deny or otherwise constrain an operation;
- **trust** — existing trust/approval semantics for MCP or built-in tool execution;
- **filesystem security** — canonical path/workspace/symlink protections.

Do not collapse all of these into arbitrary booleans.

### Deny precedence

Any applicable explicit policy denial must result in denial even if a capability grant exists.

Do not implement policy as an allow-list that accidentally grants authority simply because no deny rule exists unless that is explicitly the established policy contract.

The implementation must preserve the repository's current policy semantics while making their relationship to capability authorization explicit.

### Future policy extensibility

Design the authorization request so additional policy rules can later evaluate:

- agent
- session
- workspace
- tool/operation
- resource
- capability
- risk
- time

without changing every transport and edit operation again.

Do not implement a generic workflow/DAG policy engine here.

---

## 8. Existing MCP trust and execution gate

AWH already has MCP trust/permission enforcement. Do not create a second parallel trust system.

Determine exactly where the existing MCP `ExecutionGate` is authoritative and how it interacts with the new edit authorization boundary.

The resulting architecture should have one coherent security chain rather than:

```text
MCP gate → tool → separate edit gate → separate capability check → mutation
```

with inconsistent semantics.

Instead, compose the existing mechanisms deliberately:

```text
transport authentication/trust
        ↓
canonical authorization context
        ↓
capability + scope + policy decision
        ↓
canonical EditService
```

If an existing MCP trust decision is still required for the caller/tool, preserve it. If a trust check is redundant after the new shared authorization boundary, document the exact ownership instead of duplicating the decision.

Do not weaken existing MCP fail-closed behavior.

---

## 9. Built-in default-allow migration

The current built-in authorization path has a compatibility behavior where absence of the reserved built-in trust record can mean unrestricted legacy behavior.

AWE-011 must address this deliberately for **edit-capable/high-risk mutation paths**.

Do not silently turn every historical tool into deny-by-default without analyzing compatibility. Instead:

1. Identify every Medium/High edit-capable built-in tool.
2. Determine which are already governed by the canonical authorization boundary.
3. Remove any edit-path bypass that relies solely on the absence of a trust record.
4. Establish an explicit authorization rule for legacy callers.
5. If a migration compatibility mode is necessary, make it explicit, narrow, observable, and documented in code/tests.
6. Do not let compatibility behavior become an invisible permanent security hole.

The security target is:

> New edit tools must not inherit unrestricted default access merely because no authorization record exists.

Prefer explicit authorization semantics over implicit default allow.

---

## 10. Every edit operation must use the same boundary

All canonical operations must authorize consistently.

### Replace

Authorize the target resource before reading/applying the mutation. Authorization must cover the actual resolved path, not only the tool name.

### Insert

Authorize the target path and requested mutation before insertion.

### DeleteRange

Authorize the target path and deletion operation before mutation.

### Patch

Authorize the target resource before applying the patch.

### ApplyDiff

For multi-file diffs, authorize **every affected resource** before any file is mutated. A single denied file must prevent the whole transaction from entering the mutation phase.

Do not authorize only the first diff path.

### Rollback

AWE-010 rollback is a consequential mutation and must be authorized too.

Rollback authorization must include:

- caller/agent identity
- session/workspace
- edit identity
- target resource(s)
- rollback capability
- policy evaluation
- current state/conflict conditions as defined by AWE-010

Do not assume that possession of an `EditId` itself grants rollback authority.

---

## 11. Multi-file transaction authorization

Authorization must occur at the transaction boundary for multi-operation edits.

Required sequence:

```text
Receive transaction
    ↓
Resolve every affected resource
    ↓
Build complete authorization request set
    ↓
Validate every capability/grant/scope
    ↓
Evaluate policy for every resource/operation
    ↓
If ANY denial → zero mutation
    ↓
Only then prepare/apply EditTransaction
    ↓
Verify
    ↓
Commit
```

Do not partially mutate files and then discover that a later operation is unauthorized.

Do not use rollback as the normal way to compensate for authorization that should have been checked during preparation.

Rollback remains a recovery mechanism, not an authorization substitute.

---

## 12. Direct/internal service-call security

One of the most important acceptance criteria is:

> No internal direct service path can be added without an explicit authorization contract.

Review every public mutation entry point on the service layer.

Decide explicitly whether authorization is:

- mandatory inside the canonical `EditService`,
- guaranteed by a service-layer authorization wrapper, or
- guaranteed by a higher shared mutation boundary with a technically enforced contract.

Do not rely on comments saying “callers must authorize first.”

The architecture must make bypasses difficult or impossible.

If a lower-level helper is intentionally mutation-capable without authorization because it is an internal primitive, make that boundary explicit and ensure it cannot be reached by MCP/CLI/TUI/API request handling without the authoritative authorization step.

Never add transport-specific privileged helper functions that bypass the shared boundary.

---

## 13. MCP / CLI / TUI / Control API equivalence

MCP and CLI are explicit acceptance targets, and TUI/Control API must not become hidden bypasses.

For an equivalent request:

```text
same principal
same agent
same session
same workspace
same operation
same resource
same capability
same policy state
```

all interfaces must receive the same authorization result.

The interface may format the error differently, but the underlying decision and reason must be equivalent.

Test at least:

- MCP allow
- MCP deny
- CLI allow
- CLI deny
- equivalent MCP/CLI denial for insufficient capability
- equivalent MCP/CLI denial for expired grant
- equivalent MCP/CLI denial for wrong workspace/resource
- rollback denial through each supported interface

Do not duplicate policy logic in interface handlers.

---

## 14. Zero-side-effect denial guarantee

For every denied edit or rollback:

- no target bytes change;
- no target file is created/deleted/replaced;
- no temporary mutation is committed to the target;
- no other transaction operation is applied;
- no partial multi-file state is observable;
- AWE-010 rollback is not invoked as a compensating action for a pre-mutation denial.

Audit events may be emitted, but the protected filesystem state must remain unchanged.

Write tests that compare exact pre-request file bytes and relevant metadata against post-denial state.

---

## 15. Authorization result and error contract

Define structured, machine-readable authorization failures.

At minimum distinguish causes such as:

- missing identity
- unknown agent
- unknown session
- wrong workspace
- missing capability
- expired capability grant
- out-of-scope resource
- explicit policy denial
- trust/approval denial
- unsupported operation
- malformed authorization context
- authorization infrastructure unavailable

Do not leak sensitive policy internals, secrets, or unrelated workspace data.

Errors must be deterministic enough for MCP/CLI clients to react programmatically.

Avoid generic `permission denied` for every case when a safe structured reason can be supplied.

Infrastructure failures must fail closed rather than being converted into allow.

---

## 16. Audit integration

Every consequential authorization decision should integrate with the repository's existing audit mechanism.

At minimum capture, subject to existing privacy/security rules:

- allow/deny
- principal/agent identity when safe
- session identity when safe
- workspace identity when safe
- operation/tool
- resource identifier or safe normalized resource representation
- capability/permission
- policy decision
- denial reason
- timestamp
- edit ID where available

Do not wait for AWE-013 to make the decision observable.

However, do not implement AWE-013's full persistent provenance architecture here. Use the current audit interface and leave stable seams for later enrichment.

Never log secrets, raw tokens, credentials, or sensitive file contents merely to make authorization auditable.

---

## 17. Security requirements

The implementation must preserve and integrate with existing security helpers.

### Paths

- reject absolute paths where the canonical edit model requires relative workspace paths;
- reject `..` traversal;
- reject platform prefix/root escape forms;
- prevent symlink-based escape from the workspace;
- enforce canonical workspace containment;
- do not use raw route strings as resource authorization;
- do not use naive string prefixes for security-sensitive scope checks.

### TOCTOU

AWE-011 cannot magically eliminate every filesystem TOCTOU race.

Document the actual security boundary and rely on AWE-006 atomic/recovery semantics and existing locks where available.

Do not claim that checking a path and later opening it are inherently race-free.

Where the existing implementation has a mutation lock/transaction boundary, perform authorization and resource resolution as close to the mutation boundary as the architecture safely permits.

### Fail closed

If authorization state is unavailable, corrupt, malformed, ambiguous, or cannot be safely evaluated:

```text
DENY
```

Never:

```text
authorization_error → allow
```

---

## 18. Resource and scope normalization

Use one canonical representation for authorization resources.

For filesystem edits, distinguish clearly between:

- transport path
- workspace-relative path
- canonical filesystem path
- capability scope

Authorization must compare equivalent resources consistently.

Test adversarial cases including:

```text
src/main.rs
src//main.rs
src/./main.rs
src/../src/main.rs
src/main.rs/..
../src/main.rs
absolute/path
symlink-to-outside/file
src/foobar.rs when scope is src/foo
```

Only safe equivalents should normalize to the same resource.

Do not normalize away a security boundary.

---

## 19. Capability composition

If an operation requires multiple capabilities, authorization must require all necessary capabilities unless an explicit policy model says otherwise.

Examples:

- filesystem mutation
- process execution
- network
- environment access
- secrets

Do not treat one broad capability as implicitly granting unrelated permissions.

Reuse the repository's existing `Permission` vocabulary and MCP permission model where appropriate rather than creating duplicate enums.

---

## 20. Testing strategy

Tests must prove the authorization boundary, not merely that helper functions return booleans.

### Unit tests

Cover:

- matching agent grant
- wrong agent
- missing grant
- valid grant
- expired grant
- malformed expiry
- unscoped grant
- correctly scoped grant
- out-of-scope resource
- path-component boundary
- workspace mismatch
- explicit policy denial
- policy allow/no-deny behavior according to the established contract
- trust denial
- authorization infrastructure failure
- deterministic error classification
- route/URL identity cannot substitute for authorization

### Edit integration tests

For every operation:

- authorized edit succeeds;
- missing capability is denied;
- expired grant is denied;
- wrong workspace is denied;
- wrong resource scope is denied;
- policy denial is denied;
- denial causes zero filesystem mutation.

Cover:

- Replace
- Insert
- DeleteRange
- Patch
- ApplyDiff
- rollback

### Multi-file tests

At least:

1. all files authorized → transaction can proceed;
2. one file denied → zero files mutate;
3. one operation expired → zero files mutate;
4. one policy denial → zero files mutate;
5. mixed resources/scopes are evaluated independently and deterministically.

### MCP/CLI equivalence tests

Run the same logical request through both paths and compare the underlying authorization outcome.

### Bypass tests

Attempt to reach the edit service through every discovered direct/internal route that could otherwise avoid authorization.

The test must demonstrate that protected external request paths cannot bypass the shared boundary.

### Rollback tests

Cover:

- authorized rollback;
- missing rollback authority;
- expired rollback capability;
- policy-denied rollback;
- wrong workspace;
- wrong agent;
- edit ID possession without authorization;
- rollback denial causes zero restoration mutation;
- existing AWE-010 conflict checks still execute correctly after authorization passes.

### Failure injection

Where practical, inject failures in:

- capability store read
- policy store read
- identity resolution
- authorization evaluation
- audit emission

Verify that security-sensitive failures fail closed and do not mutate files.

### Property/invariant tests

For arbitrary denied requests:

```text
filesystem_after == filesystem_before
```

For equivalent authorization inputs:

```text
decision(interface_A) == decision(interface_B)
```

---

## 21. Compatibility requirements

Preserve existing public MCP/CLI behavior where it does not conflict with the new security invariant.

Do not silently break low-risk read-only operations merely because they share infrastructure with mutation tools.

For existing built-in trust behavior, explicitly document any migration from legacy default-allow to explicit authorization.

Do not preserve compatibility by allowing an unauthorized edit.

Security takes precedence over convenience.

Any compatibility mode must be:

- explicit;
- narrowly scoped;
- test-covered;
- observable;
- documented;
- removable.

---

## 22. Performance and reliability

Authorization runs on a potentially hot path. Keep it deterministic and bounded.

Avoid:

- unbounded directory scans per request where a bounded store API can be used;
- repeated parsing of the same policy state unnecessarily;
- network calls during local edit authorization unless explicitly required by the architecture;
- expensive canonicalization repeated for every operation when a transaction-level result can safely be reused.

Do not sacrifice correctness for micro-optimizations.

Authorization cache design, if introduced, must account for:

- grant revocation;
- expiry;
- policy changes;
- workspace changes;
- session changes.

A stale allow decision must never survive a security-relevant authorization change.

---

## 23. Backward compatibility and serialization

Do not break existing persisted:

- `CapabilityGrant` JSON
- policy JSON
- trust store JSON
- edit transaction serialization

unless there is a demonstrated security defect requiring migration.

If a migration is unavoidable:

1. detect the old format;
2. migrate deterministically;
3. fail closed on ambiguous data;
4. test old and new representations;
5. document the compatibility boundary.

Do not silently reinterpret an existing grant as broader authority.

---

## 24. Documentation and architecture contract

Update documentation **only if the implementation itself requires documentation changes within the scope of this issue**. The issue-resolution prompt is not permission to rewrite unrelated documentation.

Document the final authoritative authorization path and explicitly state:

```text
Caller/Agent
→ Session
→ Workspace
→ Capability
→ Resource
→ Policy
→ EditService
```

Document that:

- URL/route identity is not authorization;
- capability grants are enforced;
- `expires_at` is enforced;
- resource scope is enforced;
- policy denial wins where applicable;
- all edit operations and rollback are protected;
- MCP and CLI share the same decision boundary;
- denied operations cause zero mutation;
- direct service paths require an explicit authorization contract.

Do not claim that AWE-011 implements the complete future AgentRuntime, AWE-012 snapshots, AWE-013 provenance, or worktree isolation.

---

## 25. Explicit non-goals

Do **not** use AWE-011 to implement:

- a generic agent framework;
- a workflow/DAG engine;
- a new model router;
- a new MCP protocol;
- a second policy engine;
- a second capability vocabulary;
- a replacement for existing MCP trust semantics without migration analysis;
- full AgentRuntime/AgentRegistry lifecycle management;
- TOML parsing inside policy evaluation;
- AWE-012 snapshot/provenance storage;
- AWE-013 persistent audit redesign;
- AWE-021+ worktree isolation;
- arbitrary filesystem sandboxing beyond the existing security model;
- unrelated UI redesign;
- unrelated documentation cleanup.

Keep the change narrowly focused on **authorization of edit and rollback mutation paths**.

---

## 26. Verification workflow

After implementation, run the full repository validation required by the project:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Then run the focused authorization/edit tests.

Perform real terminal validation, not only static/unit validation.

Verify at least:

1. create/prepare a workspace;
2. create an agent identity/grant using the repository's actual supported mechanism;
3. attempt an authorized edit;
4. verify the edit succeeds;
5. remove/revoke or expire the grant;
6. repeat the edit;
7. verify deterministic denial;
8. verify exact file bytes remain unchanged after denial;
9. create a scoped grant;
10. edit inside scope;
11. attempt edit outside scope;
12. verify denial and zero mutation;
13. configure a policy denial;
14. verify denial even when capability exists;
15. test rollback authorization;
16. test the same logical request through MCP and CLI where supported;
17. verify no route-only identity bypass exists.

If a test requires environment-specific credentials or unavailable external services, replace it with the strongest local integration test possible and document the limitation. Do not mark the security property verified when it was only inferred.

---

## 27. Definition of Done

AWE-011 is complete only when all of the following are true:

- [ ] One authoritative authorization boundary governs edit mutations.
- [ ] Capability grants are actually consulted.
- [ ] Agent identity reaches the decision point.
- [ ] Session identity reaches the decision point where supported.
- [ ] Workspace identity/resource containment is enforced.
- [ ] Grant `expires_at` is enforced.
- [ ] Grant scope is enforced with secure path semantics.
- [ ] Policy evaluation is part of the authoritative decision.
- [ ] Existing MCP trust semantics are preserved coherently without a parallel policy engine.
- [ ] Replace is protected.
- [ ] Insert is protected.
- [ ] DeleteRange is protected.
- [ ] Patch is protected.
- [ ] ApplyDiff is protected across every affected file.
- [ ] Rollback is protected.
- [ ] Multi-file transactions authorize all resources before mutation.
- [ ] Denied requests cause zero filesystem mutation.
- [ ] MCP and CLI share equivalent authorization semantics.
- [ ] TUI/Control API cannot silently bypass the boundary.
- [ ] URL/route identity is never treated as authorization.
- [ ] Authorization failures are structured and deterministic.
- [ ] Security-sensitive authorization failures fail closed.
- [ ] Existing audit infrastructure records allow/deny decisions appropriately.
- [ ] No direct internal mutation path bypasses the explicit authorization contract.
- [ ] Tests cover least privilege, expiry, wrong agent, wrong workspace, scope, policy denial, rollback denial, and MCP/CLI equivalence.
- [ ] Failure-injection tests demonstrate fail-closed behavior.
- [ ] Existing AWE-010 rollback/conflict semantics remain intact.
- [ ] Full Rust CI passes.
- [ ] Real terminal/integration validation has been performed.
- [ ] No unrelated feature has been introduced.

---

## 28. Final implementation report

At completion, report:

1. **Files changed** — exact paths only.
2. **Authorization architecture** — final decision path and ownership.
3. **Capability enforcement** — how grants, scope, and expiry are evaluated.
4. **Policy enforcement** — how policy interacts with capabilities and trust.
5. **Edit coverage** — Replace/Insert/DeleteRange/Patch/ApplyDiff/Rollback.
6. **Transport coverage** — MCP/CLI/TUI/Control API status.
7. **Security guarantees** — especially zero-mutation denial and fail-closed behavior.
8. **Tests added/updated** — exact test areas.
9. **Validation commands and results**.
10. **Known limitations** — only real, verified limitations.
11. **Compatibility/migration behavior** — if any.
12. **Confirmation that no unrelated subsystem was modified.**

Do not claim success based solely on compilation.

---

## 29. HARD STOP

When AWE-011 is fully implemented, tested, and verified:

**STOP.**

Do not implement AWE-012, AWE-013, AWE-014, worktree isolation, or any later roadmap item in the same task.

Do not modify unrelated repository files merely to improve architecture unless the file is directly required to implement and verify AWE-011.

The success criterion is not feature count. The success criterion is a coherent, enforceable, auditable authorization boundary protecting the canonical AWH edit and rollback system.