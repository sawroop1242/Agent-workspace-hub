# Prompt 07 — Edit Caller Authorization & Trust-Wedge Enforcement (AWE-011 / TW-004)

## Mission

Implement and harden one authoritative, transport-independent authorization boundary for every consequential AWH filesystem edit and edit-level rollback.

The implementation must answer, before mutation:

> Who is calling, which agent/session/workspace does that caller belong to, what exact operation and resource are being requested, which capability grants apply, and does workspace policy permit it?

Authorization is a security boundary, not metadata. A request that cannot be attributed to a trusted caller, cannot be evaluated against the concrete resource, or encounters an authorization-store failure must not silently fall through to mutation.

This prompt is standalone. Work only from the current rust branch as the source of truth. Do not require another implementation-prompt PR, another branch, or a prescribed merge order.

---

## 1. Product and security boundary

AWH is an agent-agnostic workspace runtime for existing coding agents.

External agents own:

- reasoning;
- planning;
- model/provider selection;
- agent-specific orchestration.

AWH owns:

- workspace and filesystem state;
- controlled edits;
- capabilities and policy;
- snapshots/provenance;
- rollback/recovery;
- agent/session runtime identity;
- MCP/CLI/TUI/API boundaries;
- audit and observability.

The authorization layer sits between trusted runtime identity and consequential AWH mutation:

    transport / trusted caller resolution
                ↓
    canonical authorization request
                ↓
    capability evaluation
                ↓
    workspace policy evaluation
                ↓
    AuthorizationDecision
                ↓
    canonical edit / rollback service
                ↓
    mutation

Transport routing, an MCP URL, a CLI flag, an AgentProfile declaration, a configuration file, or a tool description is not itself authorization.

Do not weaken existing MCP trust/authentication or high-risk default-deny behavior while implementing this prompt.

---

## 2. Scope

### In scope

Implement or harden the single authorization boundary for:

- filesystem.replace;
- filesystem.insert;
- filesystem.delete_range;
- filesystem.patch;
- filesystem.apply_diff;
- filesystem.rollback.

The implementation must cover:

1. trusted caller/principal resolution;
2. agent/session/workspace binding;
3. operation-to-authorization mapping;
4. resource/path normalization and scope matching;
5. capability-grant evaluation;
6. grant expiry handling;
7. workspace policy evaluation;
8. deterministic deny precedence;
9. fail-closed behavior on authorization infrastructure failure;
10. authorization-before-mutation ordering;
11. identical semantics across transports;
12. structured allow/deny results;
13. zero mutation on denial;
14. authorization decision correlation with existing edit/snapshot/audit data where those facilities exist;
15. regression, concurrency, bypass, and adversarial security tests.

### Explicitly out of scope

Do not implement or redesign:

- AgentProfile/AgentRegistry/AgentSession as a new subsystem;
- a second identity system;
- a second capability store;
- a second policy engine;
- a second MCP trust engine;
- a second edit transaction model;
- a second edit executor;
- snapshot storage or retention;
- rollback implementation;
- persistent audit storage;
- Git worktrees;
- distributed locking;
- model routing;
- LLM orchestration;
- swarm scheduling;
- approvals/interactive human-review workflows unless an existing contract already requires them;
- CLI editing commands as a new product surface;
- MCP editing tools as a new product surface.

If a missing prerequisite is discovered, integrate with the existing contract or report the gap. Do not silently invent an adjacent subsystem.

---

## 3. Forensic-first procedure

Before changing code, inspect the current repository and establish the actual implementation boundary.

Read, at minimum, the current versions of:

### Product / roadmap

- docs/roadmap/GROWTH_STRATEGY.md
- docs/roadmap/PROJECT_ROADMAP.md
- docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
- docs/FEATURES.md
- docs/PROJECT_CONTEXT.md
- docs/architecture.md
- docs/security.md
- docs/threat-model.md

### Canonical implementation prompts

Inspect the current docs/implementation-prompts/ collection sufficiently to identify ownership and prevent duplication, especially:

- 01-init-runtime-contracts.md
- 02-agent-runtime-identity.md
- 03-mcp-routing-and-security.md
- 04-edit-transaction-model.md
- 05-edit-operation-engine.md
- 06-edit-safety.md
- 08-snapshots-provenance.md
- 09-rollback-recovery.md
- 10-persistent-audit.md
- 11-mcp-editing-validation.md
- 12-cli-editing.md
- 16-testing-and-acceptance.md
- 17-editing-contract-status-roadmap.md

The old docs/issue-resolving-prompts/ and docs/trust-wedge/ collections are historical source material where present. Search them for AWE-011, TW-004, authorization, capability, policy, caller identity, rollback authorization, bypass, and default-deny requirements. Do not copy obsolete implementation assumptions over current code.

### Current source

Inspect the actual authorization and callers, including as applicable:

- src/services/authorization.rs
- src/core/identity.rs
- src/core/agents.rs
- src/core/policy.rs
- capability-grant store/model modules;
- src/services/edit.rs
- src/services/files.rs
- src/services/snapshot.rs
- src/services/audit.rs
- src/mcp/dispatcher.rs
- MCP session/security/permission modules;
- Control API mutation entry points;
- current CLI mutation entry points;
- tests covering edits, MCP authorization, capabilities, policy, and rollback.

Then trace every edit and rollback call path.

Do not assume that a function is protected merely because its caller is expected to be trusted.

---

## 4. Baseline and ownership map

Before implementation, record:

- current authorization entry point(s);
- current AuthorizationRequest/principal/action/decision types;
- current capability-grant representation and persistence;
- current PolicyStore representation and matching semantics;
- current agent/session identity types;
- every filesystem edit entry point;
- every rollback entry point;
- every transport adapter that can reach them;
- existing MCP trust/authentication checks;
- existing path validation;
- existing audit/provenance hooks;
- existing tests proving authorization;
- known bypasses;
- known semantic mismatches;
- compatibility-sensitive public APIs.

Create a small ownership table in the implementation notes or final report:

| Concern | Canonical owner | Authorization responsibility |
|---|---|---|
| Agent identity | existing identity/runtime subsystem | consume trusted identity |
| Capability grants | existing capability store | evaluate grants |
| Policy rules | existing PolicyStore | evaluate deny rules |
| Edit model/execution | existing EditService | execute only after authorization |
| Filesystem safety | existing FilesService/path validation | validate and mutate safely |
| Snapshots | existing SnapshotStore | recovery/provenance boundary |
| Audit | existing audit service | record decisions where supported |
| MCP transport trust | existing MCP security/dispatcher | authenticate/trust transport |
| Authorization decision | this boundary | one authoritative decision |

If current code differs from this table, adapt to the actual contracts rather than creating duplicate owners.

---

## 5. Current implementation contract to preserve

The current rust branch already contains an EditAuthorizer and related contracts. Treat them as the starting point, not as permission to create another authorization service.

The current authorization model includes concepts equivalent to:

- EditAction;
- AuthorizingPrincipal;
- AuthorizationRequest;
- DenialReason;
- AuthorizationDecision;
- EditAuthorizer.

Current action-to-tool names include:

| Edit action | Policy/tool namespace |
|---|---|
| Replace | filesystem.replace |
| Insert | filesystem.insert |
| DeleteRange | filesystem.delete_range |
| Patch | filesystem.patch |
| ApplyDiff | filesystem.apply_diff |
| Rollback | filesystem.rollback |

Preserve stable names unless the current repository demonstrates a better established canonical contract.

The current capability model uses a filesystem permission plus optional scope and expiry. Preserve its intended security semantics:

- exact agent identity matching;
- filesystem permission required;
- scope is workspace-relative;
- scope matching is component-aware;
- expiry is evaluated using the repository's established time representation;
- malformed expiry fails closed;
- policy denial has precedence over a capability allow;
- unreadable capability/policy state fails closed.

---

## 6. Critical security correction: caller semantics

Audit the distinction between:

1. transport authentication;
2. agent identity;
3. operator/human direct invocation;
4. capability authorization;
5. workspace policy authorization.

Do not conflate them.

A remote MCP API key proves transport-level authentication; it does not automatically prove that the caller is a particular agent.

An /{agent}/mcp route identifies a routing namespace; it does not authorize that agent.

An AgentProfile says what an agent is configured as; it does not grant permission.

A session identifies runtime state; it does not itself grant filesystem capability.

A capability grant gives authority only within its defined scope and lifetime.

A policy deny must be able to narrow otherwise granted capability.

### Direct operator calls

The current implementation permits a principal with no agent_id to take an operator path. Preserve this only if the repository's actual security contract explicitly defines direct operator mutation as authorized.

Do not accidentally turn:

    missing agent identity
            ↓
          Allow

into a general-purpose bypass for agent-facing or remote mutation.

The implementation must explicitly distinguish an authenticated/trusted operator invocation from an ambiguous or missing caller identity.

If the existing API lacks enough information to make that distinction safely, introduce the smallest compatible caller-context field or boundary required to make it explicit, or fail closed for the ambiguous path. Do not infer trust from absence.

Required invariant:

> An untrusted or ambiguous caller must never become authorized merely because agent_id is absent.

---

## 7. Canonical authorization request

Every consequential mutation must be representable as one canonical authorization request containing, at minimum:

- action;
- trusted caller/principal;
- workspace identity;
- concrete workspace-relative resource;
- session identity when an agent session is claimed;
- transaction/edit identity when relevant to rollback.

The request must be constructed from trusted runtime state.

Never derive authorization identity from:

- URL path alone;
- user-controlled request body;
- arbitrary agent_id argument;
- arbitrary session_id argument;
- filesystem path;
- tool description;
- MCP metadata that has not been authenticated/bound;
- configuration supplied by the agent itself.

For agent-scoped MCP routing, the routed agent identity and authenticated runtime session must agree before the authorization request reaches the mutation service.

---

## 8. Identity binding invariants

For an agent-authenticated request, enforce:

    caller identity
        ==
    registered agent
        ==
    active/eligible session
        ==
    target workspace

where the current repository has contracts for each component.

At minimum:

1. unknown agent → deny;
2. disabled/stopped/invalid session → deny;
3. session bound to another agent → deny;
4. session bound to another workspace → deny;
5. route identity conflicting with trusted session identity → deny;
6. claimed workspace conflicting with trusted session workspace → deny;
7. missing required identity → deny;
8. forged identity supplied only in request parameters → never trusted.

Do not add a parallel registry lookup if the existing AgentRegistry/session subsystem already provides authoritative resolution.

---

## 9. Capability evaluation

Evaluate capability grants against the concrete operation and resource, not merely the broad tool category.

Required checks:

1. resolve the canonical agent identity;
2. load its grants from the existing store;
3. fail closed if the store is unreadable/corrupt;
4. require the appropriate filesystem permission;
5. require a grant whose scope covers the exact resource;
6. reject malformed grant data;
7. reject expired grants;
8. do not let an expired grant act as fallback;
9. do not let a grant for another agent authorize the request;
10. do not let a grant for another workspace authorize the request;
11. preserve deterministic behavior when multiple grants overlap.

### Scope semantics

Use component-aware path-prefix semantics.

For example:

- src/foo covers src/foo;
- src/foo covers src/foo/bar.rs;
- src/foo does not cover src/foobar.rs;
- src/foo does not cover other/src/foo.

Normalize equivalent separator representations according to the existing workspace path contract.

Do not allow:

- .. traversal;
- absolute-path confusion;
- empty/ambiguous scope semantics;
- separator tricks;
- Unicode normalization tricks that change the intended path boundary;
- symlink escape to transform an authorized path into an unauthorized target.

Path containment remains the filesystem service's responsibility, but authorization must consume the same canonical resource representation.

---

## 10. Expiry semantics

Grant expiry must be deterministic and fail closed.

| Grant state | Result |
|---|---|
| no expiry and otherwise valid | eligible |
| future expiry | eligible |
| expiry at/before current time | deny |
| malformed expiry | deny |
| unreadable grant store | deny |
| grant for wrong agent | deny |
| grant outside resource scope | deny |

Do not silently reinterpret malformed expiry as “never expires”.

Use the repository's established UTC/RFC 3339 handling and avoid local-time ambiguity.

Tests must avoid flaky wall-clock assumptions; use a testable clock or bounded timestamps where the current architecture permits.

---

## 11. Policy evaluation and deny precedence

Policy is an independent authorization layer.

The required logical relationship is:

    policy deny
         OR
    capability deny
         OR
    identity/session deny
         OR
    authorization infrastructure failure
             ⇒ DENY

Only when all required checks succeed may authorization return Allow.

Policy denial must take precedence over capability approval.

For example:

    filesystem.replace
    agent has valid Filesystem grant
    target is inside grant scope
    workspace policy denies src/secrets/**
             ↓
    Deny(PolicyDenied)

Never implement:

    capability allow
        ↓
    skip policy

Policy matching must use the canonical tool/action namespace and resource normalization already established by the repository.

Do not create a second policy matcher solely for authorization.

---

## 12. Fail-closed infrastructure behavior

Authorization is security-sensitive.

If the capability store, policy store, identity store, or required runtime security state is unreadable/corrupt, the result must not become Allow.

Required invariant:

    authorization infrastructure error → Deny

Use a stable reason such as InfrastructureUnavailable where that contract already exists.

Do not expose:

- raw store contents;
- secrets;
- capability payloads;
- internal stack traces;
- full sensitive file content;
- unnecessary host filesystem paths.

The internal logs may retain safe diagnostic context according to the existing observability policy, but client-visible denial details must remain bounded and non-sensitive.

---

## 13. One decision boundary

There must be exactly one authoritative authorization decision for edit/rollback mutation.

Adapters may:

- authenticate;
- resolve trusted transport context;
- construct an authorization request;
- call the authorizer;
- translate the decision into transport-specific output.

Adapters must not independently decide:

- capability validity;
- policy allow/deny;
- filesystem scope authorization;
- rollback authority.

The edit service may still enforce its own non-authorization safety invariants such as expected-state validation, path containment, and transaction correctness. Those are not duplicate authorization.

Likewise, the MCP trust gate remains responsible for MCP-server/tool trust and transport security. It is not a substitute for edit authorization.

---

## 14. Authorization-before-mutation invariant

For every consequential edit and rollback:

    request
      ↓
    trusted caller resolution
      ↓
    authorization
      ↓
    filesystem/path/state safety
      ↓
    preparation
      ↓
    snapshot/recovery boundary where required
      ↓
    mutation
      ↓
    verification

No consequential filesystem mutation may occur before the authoritative authorization decision.

A denial must result in:

- zero target-file mutation;
- zero rollback mutation;
- no snapshot pretending that an edit occurred;
- no false “committed” status;
- no success audit event;
- a deterministic denial result.

Do not rely only on tests of the happy path. Trace the call graph and prove that no direct service caller can mutate around the gate.

---

## 15. Edit and rollback coverage

The authorization boundary must cover every currently supported consequential edit action:

- replace;
- insert;
- delete range;
- patch;
- apply diff.

Rollback is also consequential mutation and must cross the same authorization boundary.

Rollback must authorize the actual resource(s) being restored and the rollback action itself.

Do not treat rollback as an internal implementation detail that is automatically trusted because it is initiated by an existing edit.

A valid edit capability does not automatically imply unrestricted rollback authority if the repository's policy model can distinguish filesystem.rollback.

Preserve the existing filesystem.rollback policy namespace.

---

## 16. Multi-file authorization

For multi-file edits or rollback:

1. enumerate all concrete affected resources before mutation;
2. validate every resource against workspace containment;
3. authorize the operation for every affected resource;
4. if any resource is unauthorized, deny the entire operation;
5. do not mutate authorized files while leaving unauthorized files untouched;
6. do not claim transaction success after a partial authorization decision.

If the current authorization API only accepts one resource, adapt it carefully so a multi-file transaction can be checked without weakening the single-decision invariant.

A safe model is:

    transaction
       ↓
    canonical set of affected resources
       ↓
    authorization decision over the complete set
       ↓
    allow only if every required resource is authorized

Do not authorize only the first path.

---

## 17. Route and transport equivalence

Authorization semantics must not depend on whether the same mutation arrives through:

- MCP stdio;
- MCP HTTP/SSE;
- Control API;
- CLI;
- an internal application-service caller that represents a trusted operator context.

Transport-specific authentication can differ, but once the canonical caller context is established, the mutation authorization decision must be equivalent for the same:

- workspace;
- agent/session;
- action;
- resource;
- capability state;
- policy state.

Required parity property:

    same trusted caller + same operation + same resource + same policy
                             ↓
                  same AuthorizationDecision

Do not duplicate authorization logic in each transport.

---

## 18. MCP-specific requirements

For agent-scoped MCP routing, preserve the security relationship:

    /{agent}/mcp
          ↓
    trusted session/caller resolution
          ↓
    canonical agent identity
          ↓
    authorization
          ↓
    filesystem service

The route namespace is not authority.

Test adversarial cases:

- route says agent A, trusted session says agent B;
- route says agent A, no trusted session;
- valid API key but unknown agent;
- valid agent but expired grant;
- valid agent/grant but policy deny;
- agent A attempts resource inside agent B's scope;
- client supplies forged agent/session fields in tool arguments;
- alternate MCP transport reaches the same service without the route adapter.

Every denied case must produce zero mutation.

Do not weaken SEC-001/SEC-002 or the existing MCP authentication/trust gate.

---

## 19. Authorization decision contract

Preserve one structured result type.

The decision must be machine-readable and stable enough for MCP/CLI/API adapters.

At minimum:

    Allow

or:

    Deny {
        reason: stable reason code,
        detail: safe human-readable detail
    }

Reason codes should distinguish at least:

- missing/ambiguous caller identity;
- unknown agent;
- invalid/disabled session where the existing contract supports it;
- missing capability;
- expired grant;
- out-of-scope resource;
- policy denial;
- infrastructure unavailable;
- unsupported operation.

Do not reveal whether a protected resource exists if that information is itself sensitive.

Do not return different security semantics merely because one adapter uses JSON-RPC and another uses CLI output.

---

## 20. Denial precedence

Define and test one deterministic precedence order.

A safe ordering is:

    invalid/untrusted caller context
            ↓
    workspace/session binding failure
            ↓
    policy infrastructure failure
            ↓
    policy deny
            ↓
    capability infrastructure failure
            ↓
    missing/expired/out-of-scope capability
            ↓
    allow

If the repository already has an established precedence contract, preserve it unless it is demonstrably unsafe.

The important invariant is:

> No lower-priority positive condition may override a higher-priority security failure or deny.

For example:

- valid capability never overrides policy deny;
- valid route never overrides wrong session identity;
- authenticated transport never overrides missing required agent authority;
- stale/invalid identity never becomes operator authority;
- corrupt authorization state never becomes allow.

---

## 21. Concurrency and race behavior

Authorization must remain safe under concurrent agents.

Test at least:

- two agents editing different authorized files;
- two agents editing the same authorized file;
- one agent with authorization and one without;
- grant revocation/expiry while requests are being evaluated where the current store architecture permits it;
- policy changes while requests are being evaluated where practical;
- concurrent authorization calls;
- concurrent denied requests;
- concurrent edit and rollback requests.

Do not claim authorization alone eliminates filesystem TOCTOU.

The edit-safety layer remains responsible for expected-state verification and safe commit/recovery behavior.

If grant/policy data is cached, define and test its invalidation/refresh semantics. Do not introduce a cache merely for performance.

---

## 22. TOCTOU boundary

Authorization and filesystem state are related but not identical.

The implementation must acknowledge:

    authorize(path, state)
          ↓
    filesystem may change
          ↓
    edit safety revalidates live state
          ↓
    commit

Do not claim that an authorization check proves the file cannot change afterward.

Use the existing edit-safety mechanisms for:

- live state reads;
- expected-state matching;
- path validation;
- atomic commit;
- post-commit verification;
- recovery conflict detection.

Authorization proves authority; edit safety proves that the mutation is still safe to apply.

---

## 23. Audit/provenance integration

Authorization should integrate with existing audit/provenance facilities without creating a new event store.

Where the current architecture supports it, correlate:

- workspace;
- agent;
- session;
- action;
- resource;
- edit/transaction ID;
- policy decision;
- capability decision;
- timestamp;
- final outcome.

Record denied authorization decisions where the existing audit boundary supports security events.

Do not record:

- capability secrets;
- bearer/API keys;
- full file contents;
- sensitive request bodies;
- unnecessary credentials.

A denial audit event must not itself become a mutation or leak mechanism.

Persistent audit redesign belongs to its existing owner; do not implement it here.

---

## 24. Error handling and information disclosure

Authorization errors must be structured and deterministic.

Client-visible errors should answer:

- allowed or denied;
- stable reason;
- safe explanation.

They should not expose:

- raw JSON from capability/policy stores;
- secret values;
- bearer tokens;
- environment variable contents;
- full filesystem contents;
- stack traces;
- internal panic messages;
- unnecessary absolute host paths.

Internal diagnostics may retain enough information to troubleshoot a configuration problem, subject to existing redaction rules.

Use the existing error taxonomy where possible rather than introducing another parallel error hierarchy.

---

## 25. Tests — unit level

Add or strengthen focused tests for pure authorization logic.

At minimum:

### Identity

- valid agent identity;
- unknown agent;
- missing agent where agent identity is required;
- valid operator context if explicitly supported;
- ambiguous caller is denied;
- session/agent mismatch;
- session/workspace mismatch;
- disabled/ineligible session.

### Capability

- valid filesystem grant;
- wrong agent;
- wrong permission;
- missing grant;
- expired grant;
- malformed expiry;
- future expiry;
- unscoped grant;
- scoped grant;
- exact scope match;
- child path scope match;
- prefix-collision rejection;
- traversal rejection;
- separator normalization;
- multiple grants;
- overlapping grants;
- corrupt store;
- unreadable store.

### Policy

- no matching deny;
- matching deny;
- policy deny overrides valid capability;
- policy store corruption fails closed;
- action namespace mapping;
- rollback policy denial.

### Decision stability

For identical inputs, the decision and reason must be deterministic.

---

## 26. Tests — mutation boundary

Use real temporary workspaces and real service boundaries.

For every edit operation, prove:

    unauthorized request
          ↓
        Deny
          ↓
    target bytes unchanged

Repeat for:

- replace;
- insert;
- delete range;
- patch;
- apply diff;
- rollback.

Also test:

- wrong workspace;
- wrong agent;
- wrong session;
- expired grant;
- out-of-scope path;
- policy denial;
- corrupt authorization state;
- route/session mismatch.

Do not use only mocked “authorized” booleans for these tests. Exercise the actual authorization service and actual mutation boundary.

---

## 27. Tests — zero-side-effect denial

For a denied request, assert all relevant side effects remain absent:

- target file unchanged;
- newly-created target not created;
- rollback target not restored;
- no partial multi-file mutation;
- no snapshot falsely recorded as pre-mutation success;
- no success provenance;
- no “committed” transaction state;
- no misleading allow audit event.

If a denial audit event is expected, assert that only the denial event is produced and it contains no sensitive material.

---

## 28. Tests — bypass resistance

Perform a repository-wide search for every direct invocation of edit/rollback mutation.

For each call site, answer:

> Where is the canonical authorization decision enforced?

Test or restructure any path that can reach mutation without it.

Explicitly inspect:

- MCP dispatcher;
- MCP tool handlers;
- Control API handlers;
- CLI service calls;
- internal helper functions;
- rollback helpers;
- test-only public methods that could accidentally become production bypasses.

Do not solve bypasses by adding scattered authorization checks.

If an existing service-level boundary is the correct choke point, put the canonical decision there and let transports remain thin.

---

## 29. Tests — adversarial cases

Include attacks such as:

1. ../../outside.txt;
2. absolute path;
3. backslash path;
4. repeated separators;
5. scope src/foo against src/foobar;
6. symlink escaping the workspace;
7. route agent A with session agent B;
8. forged agent_id in tool arguments;
9. forged session_id;
10. valid capability for a different agent;
11. expired capability;
12. malformed capability expiry;
13. corrupt capability store;
14. corrupt policy store;
15. policy deny hidden behind an apparently valid capability;
16. rollback without eligible authorization;
17. unknown rollback transaction;
18. missing caller identity;
19. authenticated remote caller with no agent authority;
20. concurrent requests attempting cross-agent access.

Every security failure must fail closed.

---

## 30. Property-style and boundary tests

Where practical, use property-style tests for:

- path-scope component boundaries;
- normalization idempotence;
- no scope widening during normalization;
- denial under malformed authorization state;
- deterministic action-to-tool mapping.

Important invariant:

    normalize(resource)
    must never transform an unauthorized resource
    into an authorized one.

If a fuzz/property framework is already used by the repository, reuse it. Do not introduce a large testing dependency solely for this prompt unless justified.

---

## 31. Compatibility

Preserve existing public behavior that is outside this security boundary.

When changing an API:

1. identify all callers;
2. migrate them to the canonical authorization contract;
3. avoid duplicate compatibility paths that can bypass authorization;
4. preserve serialization compatibility where practical;
5. update tests at the actual boundary.

Do not leave an old “legacy” mutation function callable without the authorization boundary merely to preserve compilation.

If a compatibility path cannot safely remain, make its failure explicit and fail closed.

---

## 32. Performance discipline

Authorization runs before every consequential edit.

Keep it deterministic and bounded.

Avoid:

- unbounded grant scans;
- repeated filesystem walks;
- loading entire workspace contents;
- unnecessary cryptographic work;
- network calls;
- model calls;
- arbitrary external services.

Do not add caching unless the current architecture requires it and cache invalidation can preserve security semantics.

Correctness and fail-closed behavior take precedence over micro-optimizations.

---

## 33. Implementation sequence

Execute the implementation in this exact linear sequence.

### Step 1 — Repository forensics

Read the current contracts and trace all edit/rollback callers.

Record existing ownership and bypasses.

### Step 2 — Freeze the canonical contract

Confirm the single action/request/principal/decision types.

Reuse existing types where possible.

Do not create parallel authorization structs.

### Step 3 — Establish trusted caller resolution

Ensure the authorization request receives identity from the trusted runtime/session boundary.

Reject forged route/request identity.

Explicitly distinguish trusted operator context from absent/ambiguous identity.

### Step 4 — Establish workspace/session binding

Validate agent/session/workspace relationships using the existing identity/runtime contracts.

Fail closed on mismatch or missing required context.

### Step 5 — Normalize the authorization resource

Use the same workspace-relative resource representation as the edit/filesystem service.

Reject path ambiguity before capability/policy approval.

### Step 6 — Evaluate policy

Use the existing PolicyStore.

Preserve deny precedence and fail-closed store behavior.

### Step 7 — Evaluate capability

Use the existing capability-grant store.

Check agent, permission, scope, and expiry.

Never widen authority during normalization.

### Step 8 — Produce one decision

Return one canonical AuthorizationDecision.

Do not allow transports to reinterpret an authorization failure as success.

### Step 9 — Place the choke point

Ensure every consequential edit and rollback reaches the canonical authorization boundary before mutation.

Remove or restructure bypass paths rather than adding scattered checks.

### Step 10 — Integrate MCP/CLI/API adapters

Make adapters construct trusted context and call the canonical boundary.

Do not reproduce policy/capability logic in adapters.

### Step 11 — Integrate rollback authorization

Ensure filesystem.rollback crosses the same boundary and is evaluated against the actual rollback resources.

### Step 12 — Integrate safe correlation

Attach existing edit/transaction/session identifiers to audit/provenance where supported.

Do not create a new event store.

### Step 13 — Add unit tests

Cover identity, capability, policy, expiry, scope, precedence, and corruption.

### Step 14 — Add real mutation-boundary tests

Use temporary workspaces and verify zero mutation on denial.

### Step 15 — Add adversarial and concurrency tests

Exercise route confusion, identity confusion, path confusion, corrupt state, and concurrent requests.

### Step 16 — Audit for duplicate mechanisms

Search the repository for:

- alternate authorize functions;
- direct capability checks;
- direct policy checks;
- transport-specific edit authorization;
- rollback-only authorization;
- bypass helper methods.

Consolidate only within this scope.

### Step 17 — Run verification gates

Run all applicable repository checks.

### Step 18 — Perform final security and scope audit

Confirm:

- one authorization decision boundary;
- no mutation before authorization;
- no ambiguous caller becomes allow;
- policy deny wins;
- capability failures fail closed;
- all resources in multi-file operations are covered;
- rollback is covered;
- no duplicate subsystem was introduced;
- no other implementation prompt was changed.

---

## 34. Required authorization truth table

The implementation must satisfy a table equivalent to this:

| Caller | Session | Capability | Policy | Expected |
|---|---|---|---|---|
| trusted operator | valid | operator contract | allow | Allow |
| ambiguous/no trusted caller | absent | grant exists | allow | Deny |
| known agent | valid | valid/in-scope | allow | Allow |
| unknown agent | valid/unknown | grant absent | allow | Deny |
| known agent | wrong agent binding | valid | allow | Deny |
| known agent | wrong workspace | valid | allow | Deny |
| known agent | valid | missing | allow | Deny |
| known agent | valid | expired | allow | Deny |
| known agent | valid | out of scope | allow | Deny |
| known agent | valid | valid | deny | Deny |
| known agent | valid | valid | policy store corrupt | Deny |
| known agent | valid | capability store corrupt | allow | Deny |
| known agent | valid | valid | allow | Allow |
| any | any | any | rollback denied | Deny |
| any denied case | any | any | any | Zero mutation |

Adapt the operator row to the repository's actual trusted-operator contract. Never interpret “no agent ID” as sufficient authority by itself.

---

## 35. Security invariants — must never regress

The implementation is incomplete if any of these are false:

1. No ambiguous caller authorization.
2. No route-based authorization.
3. No request-field identity spoofing.
4. No cross-agent capability use.
5. No cross-workspace capability use.
6. No scope-prefix collision.
7. No expired grant authorization.
8. No malformed grant permissiveness.
9. No policy-deny bypass.
10. No authorization-store-error allow.
11. No mutation before authorization.
12. No partial multi-file authorization bypass.
13. No rollback authorization bypass.
14. No transport-specific security semantics.
15. No sensitive authorization data leakage.
16. No duplicate authorization engine.
17. No weakening of existing MCP trust/authentication.
18. No claim that authorization eliminates filesystem TOCTOU.

---

## 36. Relationship to edit safety

Authorization and edit safety are separate gates.

Authorization answers:

> May this caller perform this operation on this resource?

Edit safety answers:

> Is it still safe to apply this operation to the current filesystem state?

Therefore the implementation must preserve both:

    Identity
       ↓
    Authorization
       ↓
    Expected-state / conflict validation
       ↓
    Preparation
       ↓
    Snapshot/recovery boundary
       ↓
    Atomic mutation
       ↓
    Verification

Do not remove expected-state checks because authorization succeeded.

Do not grant authorization because expected-state checks succeeded.

---

## 37. Relationship to snapshots and rollback

Snapshots are recovery material, not authorization.

A snapshot's existence must never imply that the caller is authorized to mutate or restore it.

Rollback remains consequential mutation and therefore requires authorization.

This prompt may wire authorization into the existing rollback path, but it must not implement the rollback engine itself.

If rollback authorization is denied:

    Deny
    → no restoration
    → no deletion of newly-created targets
    → no filesystem mutation

---

## 38. Relationship to audit

Audit records what happened; authorization decides whether the operation may proceed.

Do not use an audit record as an authorization source.

Do not implement persistent audit storage in this prompt.

If an authorization decision is audited, the event should make the decision explainable without exposing secrets or full file content.

---

## 39. Relationship to MCP trust

MCP trust and edit authorization are separate security layers.

MCP trust answers whether an MCP server/tool boundary is trusted to execute.

Edit authorization answers whether a particular caller may perform a particular consequential filesystem mutation.

Never replace one with the other.

Preserve:

- bearer authentication;
- high-risk default deny;
- TLS requirements;
- session isolation;
- existing MCP security checks.

---

## 40. Relationship to AgentProfile/AgentSession

Agent identity is input to authorization, not authorization itself.

The authorization layer may consume:

- agent ID;
- session ID;
- workspace ID;
- lifecycle state;
- trusted caller context.

It must not redefine:

- AgentProfile;
- AgentRegistry;
- AgentSession;
- identity generation.

If the existing identity subsystem cannot provide a required fact safely, document the missing contract rather than inventing a second identity store.

---

## 41. Duplicate-mechanism audit

Before completion, search for and classify every authorization-like mechanism.

Look for:

    authorize
    authorization
    permission
    capability
    policy
    can_edit
    can_write
    can_modify
    rollback_allowed
    agent_id checks
    session_id checks
    filesystem mutation guards

For every match, decide whether it is:

- canonical authorization;
- transport authentication;
- MCP trust;
- filesystem safety;
- edit conflict validation;
- audit;
- unrelated permission logic.

Do not automatically merge unrelated security layers.

The final architecture must have one authoritative decision for edit/rollback authorization.

---

## 42. Verification gates

Run:

    cargo fmt --all -- --check
    cargo check --all-targets
    cargo test --all-targets
    cargo clippy --all-targets --all-features -- -D warnings
    git diff --check

Also run focused tests for:

- authorization;
- capability grants;
- policy;
- edit mutation denial;
- rollback denial;
- MCP authorization parity;
- path/scope security;
- concurrent authorization where implemented.

If any gate fails, diagnose and fix within this prompt's scope.

Do not weaken tests or lint configuration merely to obtain green CI.

---

## 43. Documentation updates

Update implementation-facing documentation only when necessary to keep the current contract accurate.

Do not modify unrelated implementation prompts.

Do not rewrite the roadmap.

Do not mark future features complete merely because this authorization boundary exists.

If the current documentation contains a stale claim that this work has already been completed, correct only documentation directly required by the implementation and record the change in the final report.

---

## 44. Strict scope boundary

At completion, the changed implementation must remain limited to edit/rollback authorization and its required tests/integration points.

The implementation must not contain:

- a new snapshot subsystem;
- a new rollback subsystem;
- a new audit store;
- a new agent registry;
- a new session store;
- a new MCP server;
- a new policy engine;
- a new capability store;
- a second edit executor;
- a model router;
- a workflow engine;
- a swarm scheduler.

If another subsystem appears necessary, use its existing public contract or report it.

---

## 45. Independence rule

This prompt must be executable against the current rust branch without requiring:

- Prompt 01 PR;
- Prompt 02 PR;
- Prompt 03 PR;
- Prompt 04 PR;
- Prompt 05 PR;
- Prompt 06 PR;
- Prompt 08 PR;
- Prompt 09 PR;
- Prompt 10 PR;
- any historical Trust Wedge PR;
- any issue-resolving-prompts PR.

Other prompts may describe adjacent contracts, but the current repository is authoritative.

If a referenced contract is absent, adapt to the current implementation and keep the authorization boundary safe.

---

## 46. Completion criteria

Do not declare this prompt complete until all of the following are true:

### Architecture

- one canonical edit/rollback authorization boundary exists;
- transports do not duplicate authorization logic;
- identity is consumed from trusted runtime state;
- operator/direct invocation is explicitly distinguished from ambiguous identity;
- capability and policy remain separate authorities.

### Security

- policy deny takes precedence;
- capability checks are exact and scope-aware;
- expiry is fail-closed;
- malformed stores fail closed;
- route/request identity spoofing is rejected;
- cross-agent and cross-workspace authorization is rejected;
- rollback crosses the same authorization boundary;
- no mutation occurs before authorization.

### Multi-file behavior

- every affected resource is authorized;
- any unauthorized resource denies the whole mutation before commit;
- no partial authorization bypass exists.

### Integration

- MCP/CLI/API paths use the canonical decision;
- existing MCP trust/authentication remains intact;
- edit safety remains intact;
- snapshot/rollback/audit boundaries remain intact.

### Testing

- unit tests cover identity/capability/policy;
- integration tests prove zero mutation on denial;
- rollback denial is tested;
- adversarial cases are tested;
- concurrency behavior is tested where applicable;
- no existing security regression tests are weakened.

### Quality

- verification gates pass;
- no duplicate authorization mechanism was introduced;
- no unrelated prompt was modified;
- final diff is limited to required implementation/tests/documentation.

---

## 47. Final implementation report

At the end, report concisely:

1. Canonical authorization boundary
   - exact module/service/function;
   - why it is the single choke point.

2. Caller resolution
   - how agent/session/workspace identity is trusted;
   - how ambiguous callers are handled.

3. Capability enforcement
   - permission;
   - scope;
   - expiry;
   - corruption behavior.

4. Policy enforcement
   - matching semantics;
   - deny precedence.

5. Edit/rollback coverage
   - all covered operations;
   - all entry paths audited.

6. Bypasses removed
   - each bypass found;
   - how it was closed.

7. Security behavior
   - zero-mutation denial;
   - cross-agent/workspace isolation;
   - route spoofing resistance;
   - TOCTOU boundary.

8. Tests
   - focused tests;
   - integration tests;
   - adversarial tests;
   - concurrency/failure tests.

9. Verification
   - fmt;
   - check;
   - test;
   - clippy;
   - diff check.

10. Changed files
    - exact list.

11. Limitations
    - residual race conditions;
    - platform limitations;
    - any unresolved prerequisite.

12. Scope confirmation
    - explicitly confirm that no second identity, policy, capability, edit, snapshot, rollback, audit, or MCP system was introduced.

The final report must distinguish implemented facts from assumptions and must not claim security properties that the tests or code do not establish.
