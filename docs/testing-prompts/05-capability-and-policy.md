# Master Test Prompt 05 — Capability & Policy Engine

## Mission

Create and execute a complete, human-behavior-oriented verification suite for AWH's Capability and Policy Engine on the current rust branch.

This prompt owns authorization testing for capabilities, grants, revocation, policy evaluation, policy precedence, scope, expiry, fail-closed behavior, and enforcement at consequential execution boundaries.

Do not implement missing production features merely to make tests pass. Establish what the current branch actually implements, test that behavior through real boundaries, and classify planned or unavailable behavior honestly.

Primary acceptance boundary:

human or agent request
→ trusted identity
→ capability resolution
→ PolicyEngine / policy rules
→ specialized execution gate where applicable
→ canonical AWH service
→ observable result + audit

Do not treat cargo test alone as sufficient evidence.

---

## 1. Product scope

The final feature contract includes:

awh capability list|show|grant|revoke|check
awh policy list|show|check|validate|explain

Capabilities include resources/actions such as:

- filesystem.read
- filesystem.write
- filesystem.delete
- git.read
- git.write
- process.execute
- network.request
- mcp.invoke
- secrets.read

The PolicyEngine is authoritative.

This prompt must prove that:

- a capability grant is not merely stored metadata;
- a capability cannot be used outside its permitted principal/scope;
- revocation takes effect at the enforcement boundary;
- expiry is enforced if expiry is part of the current implementation;
- workspace policy can narrow authority where the contract says it can;
- denial happens before the consequential operation;
- denied operations have zero unintended side effects;
- internal/direct callers cannot silently bypass authorization;
- MCP route names, tool discovery, URL namespaces, or possession of identifiers never become authority;
- malformed or unavailable authorization state fails closed where the security contract requires it;
- allow/deny decisions are auditable without secrets.

Do not duplicate complete MCP protocol, Git, filesystem editing, terminal, connector, snapshot, session, audit, or agent-profile suites. Test only the authorization boundary needed to prove those services cannot bypass policy.

---

## 2. Current implementation versus final target

Before writing tests, determine exactly what the current rust branch implements.

Distinguish these authorization layers:

1. Capability grants associated with agent identity.
2. Workspace-local policy rules, currently documented as DENY-only for selected tools.
3. Built-in tool trust, including reserved awh.builtin authorization for Medium/High-risk built-in MCP tools.
4. Specialized execution gates around particular tool classes.
5. Session/agent identity binding used to determine the principal.

Inspect current source and tests before asserting semantics.

For every final-target capability/policy command or API classify evidence as:

- Implemented and tested
- Implemented but insufficiently tested
- Not implemented
- Blocked by environment
- Unproven

Never turn a roadmap item into a passing test merely because it appears in docs/FEATURES.md.

If the current branch has narrower semantics than the final target, test the real narrower contract and explicitly record the gap.

---

## 3. Required repository forensics

Before changing tests, inspect enough of the current branch to establish the actual authorization contract.

At minimum read:

- README.md
- AGENTS.md
- Cargo.toml
- docs/FEATURES.md
- docs/testing.md
- docs/CLI.md
- docs/configuration.md
- docs/security.md
- docs/mcp.md
- docs/roadmap/PROJECT_ROADMAP.md
- docs/roadmap/GROWTH_STRATEGY.md
- docs/implementation-prompts/README.md
- current implementation prompts concerning agent identity, MCP routing/security, edit authorization, and testing
- current authorization-related tests

Inspect source for:

- CapabilityGrant;
- CapabilityGrantStore;
- PolicyRule;
- PolicyStore;
- PolicyEngine or its current equivalent;
- authorize_caller_capability;
- authorize_builtin_tool;
- specialized authorization/execution gates;
- SessionIdentity;
- principal/agent/session resolution;
- capability matching;
- policy matching;
- scope handling;
- expiry handling;
- persistence and locking;
- audit emission;
- CLI handlers.

Search the repository for:

CapabilityGrant
CapabilityGrantStore
PolicyRule
PolicyStore
PolicyEngine
authorize
authorize_caller_capability
authorize_builtin
capability
permission
scope
expires_at
CAPABILITY_DENIED_CODE
POLICY_DENIED_CODE
BUILTIN_TOOL_TRUST_ID
awh capability
awh policy

Current source and current tests take precedence over historical forensic reports.

---

## 4. Human-first testing model

Every important authorization behavior must be expressed as a realistic user or agent action.

Answer questions such as:

1. Can an authorized principal perform the operation?
2. Does the same operation fail after the grant is revoked?
3. Does another principal remain denied?
4. Does a grant for one capability avoid granting unrelated capabilities?
5. Does a scoped grant stay inside its scope?
6. Does an expired grant stop authorizing access?
7. Does a policy deny override an otherwise valid capability allow where documented?
8. Does denial happen before the underlying service executes?
9. Can a direct/internal service call bypass the same enforcement?
10. Does changing an MCP route name or URL namespace fail to create authority?
11. Does corrupt authorization state fail closed?
12. Does the audit record identify the decision without storing credentials?

Prefer real subprocess/CLI and service-boundary tests.

Use direct unit tests for deterministic matching/parsing, but use real authorization plus real service execution to prove enforcement.

---

## 5. Isolated test environment

Every test must use disposable isolated state.

Never use:

- the developer's real HOME;
- real AWH trust/capability/policy state;
- personal credentials;
- production repositories;
- real external services when a local fixture can prove behavior;
- shared mutable authorization state between unrelated tests.

Use unique temporary:

- workspace roots;
- persistent state directories;
- agent identities;
- session identities;
- files;
- Git repositories where Git is the consequential resource.

Where authorization state derives from workspace root, bind the test to that root explicitly.

Process-restart tests must preserve only disposable fixtures and start a fresh process.

Do not mutate process-global environment concurrently.

---

## 6. Principal and identity binding

Authorization must apply to the actual caller, not merely to a string supplied by the caller.

Create at least:

- principal A;
- principal B;
- valid session for A;
- valid session for B;
- workspace bound to the test.

Verify:

- A's grant authorizes A;
- B cannot use A's grant;
- changing a user-supplied agent/session/workspace identifier does not impersonate another principal;
- unknown agent identity is rejected;
- disabled/inactive identity cannot acquire authority;
- mismatched workspace identity is rejected where enforced;
- stale/invalid session cannot inherit another principal's capabilities.

If the branch uses SessionIdentity, use it rather than inventing a parallel test identity model.

Do not treat possession of an agent name, grant ID, WorktreeId, workspace ID, route name, URL, or filesystem path as proof of authorization.

---

## 7. Capability vocabulary and least privilege

Test every capability/permission represented by the current implementation.

At minimum inspect whether the branch supports:

- filesystem;
- process;
- network;
- environment;
- secrets;
- Git/resource permissions;
- MCP invocation.

For each supported permission test:

1. no grant;
2. exact grant;
3. unrelated grant;
4. multiple grants;
5. revocation;
6. malformed grant;
7. expired grant where implemented;
8. scoped grant where implemented.

Example:

process grant
→ process operation allowed
→ network operation still denied

filesystem grant
→ filesystem operation allowed
→ terminal/process operation remains denied

Do not assume permission implication unless the production contract explicitly defines it.

---

## 8. Grant lifecycle

Where the current branch exposes capability grants, test the complete lifecycle.

### Create / grant

Verify:

- valid grant is persisted;
- grant belongs to the intended principal;
- capability/permission is correct;
- scope/expiry are preserved when supported;
- duplicate IDs are deterministic;
- invalid principal/capability/scope input is rejected;
- no secret is persisted.

### List / show

Verify:

- relevant grants are reported;
- identity and capability data are correct;
- malformed entries do not produce fabricated authority;
- output does not leak secrets.

### Check

Where capability check exists, compare its decision with actual enforcement.

The check command/API must not report allowed while real execution denies, or vice versa, unless the contract explicitly distinguishes advisory from enforcement checks.

### Revoke

Test:

grant
→ authorized operation
→ revoke
→ fresh authorization attempt
→ denied operation
→ no side effect

Test multiple grants so revoking one cannot remove unrelated authority.

---

## 9. Scope enforcement

If grants have a scope field, test its semantics at the real enforcement boundary.

Determine scope meaning from source; do not invent a grammar.

For the implemented scope model test:

- exact in-scope resource;
- nested in-scope resource where allowed;
- sibling resource;
- parent resource;
- path-prefix boundary such as project-a versus project-ab;
- unrelated workspace;
- malformed scope;
- empty scope where meaningful;
- traversal attempts;
- canonicalized equivalent paths;
- symlink escape where resource paths are scoped.

A scope must not become an unsafe string-prefix authorization bug.

Use real temporary resources and independently inspect resulting state.

---

## 10. Expiry and temporal behavior

If capability grants support expires_at, test actual enforcement semantics.

Use deterministic timestamps or injectable clocks if available.

Test:

- no expiry;
- future expiry;
- exactly at expiry;
- just after expiry;
- malformed timestamp;
- timezone/offset forms accepted by the contract;
- expired grant after restart;
- expired grant with another still-valid grant;
- revocation of an expired grant.

Key property:

expired authority ≠ active authority

Do not use long sleeps merely to wait for expiry.

If the current branch stores expiry but does not enforce it, classify that gap explicitly rather than writing a false passing acceptance test.

---

## 11. Policy rule lifecycle

For the current workspace policy implementation, test every supported operation.

If the current contract is DENY-only, test:

- add deny rule;
- list rules;
- inspect/show where implemented;
- check;
- validate;
- explain where implemented;
- remove rule.

Determine matching semantics from source.

For every policy rule test:

- exact tool match;
- exact command/program match where supported;
- exact resource/path match where supported;
- non-matching tool;
- non-matching pattern;
- malformed pattern;
- empty values;
- traversal-shaped values;
- sibling resource names.

A deny rule must not accidentally deny unrelated resources because of unsafe prefix or substring matching.

---

## 12. Authorization precedence and composition

This is a critical suite.

When multiple authorization layers exist, prove their documented order.

At minimum construct:

no capability + no policy deny → denied

capability allow + no policy deny → allowed

capability allow + matching policy deny → denied

capability allow + non-matching policy deny → allowed

revoked capability + any policy state → denied

expired capability + absent policy → denied

Where built-in trust is involved:

built-in trust absent → High-risk tool denied

built-in trust present + required capability present + policy allows → allowed

built-in trust present + required capability absent → denied

built-in trust present + policy denies → denied

Do not infer precedence solely from function names. Establish it from the actual implementation and observable contract.

---

## 13. Default-deny and fail-closed behavior

Test absence and failure of authorization state.

At minimum:

- no grant;
- unknown capability;
- unknown principal;
- unknown grant ID;
- missing capability store;
- malformed capability store;
- unreadable capability store;
- missing policy store;
- malformed policy store;
- unreadable policy store;
- malformed policy rule;
- unavailable trust store where built-in trust is required.

For each case determine whether the contract requires denial.

When fail-closed is required verify:

- operation is denied;
- underlying service does not execute;
- no partial mutation occurs;
- no permissive state is generated;
- no fallback to allow occurs.

Do not automatically replace corrupt state unless the product explicitly defines that recovery.

---

## 14. Zero-side-effect denial tests

A denied authorization decision is not enough.

For every consequential resource class implemented by the branch, create a deliberately denied operation and inspect the resource afterward.

### Filesystem

Deny write/delete/edit.

Assert exact file bytes and relevant metadata remain unchanged.

### Process / terminal

Use a harmless observable local command that would create a marker file.

Deny it.

Assert the marker was never created.

### Git

Use a disposable repository.

Deny a mutation such as commit or another currently gated mutation.

Assert HEAD, refs, and prepared working-tree state remain as expected.

### Network

Where network permission is enforced, use a local test server.

Deny the request.

Assert no request reached the server.

### MCP

Deny a tool invocation.

Assert the underlying handler/service was not executed.

### Secrets

Use synthetic secret-shaped values.

Deny secret access.

Assert the value does not appear in output, logs, audit, or error text.

Do not use production credentials.

---

## 15. Internal/direct-call bypass resistance

The PolicyEngine is authoritative, so public adapters are not the only boundary to test.

Identify canonical service methods that can perform consequential operations.

Where the implementation exposes an authorization-aware service API, test:

authorized entry point → decision → service

Then attempt the nearest plausible direct/internal bypass.

Verify an internal caller cannot silently obtain extra authority merely by avoiding CLI/MCP adapters.

Do not require private Rust functions to be externally callable if intentionally inaccessible. Test the strongest public/service boundary available.

If a lower-level service intentionally assumes authorization already happened, document that architectural boundary instead of falsely calling it a bypass.

---

## 16. MCP route and discovery independence

Where MCP is implemented, authorization must not depend on route naming.

Test the same logical tool through:

- correct agent route;
- foreign/incorrect agent route;
- unknown agent route;
- supported route/name manipulations.

Verify:

- route identity does not grant capability;
- changing /{agent} does not impersonate another agent;
- tool discovery does not grant authority;
- a listed tool is not automatically executable;
- denied invocation is denied at execution time;
- disabled/inactive agents have no active authorized route where required.

Do not duplicate the complete MCP protocol suite here.

---

## 17. Tool discovery versus authority

Where the canonical Tool Registry exposes risk or required permissions, test that descriptive metadata is not itself authority.

Verify:

- risk metadata does not grant permission;
- requiredPermissions describes gate requirements rather than granting them;
- unknown tools cannot inherit authority by name prefix;
- fake metadata cannot obtain permission;
- discovery and execution decisions remain separate.

Test names such as:

workspace.*
filesystem.*
git.*
terminal.*
connector.*
github.*

Do not authorize by prefix unless production policy explicitly defines a prefix rule.

---

## 18. Persistence and restart

Authorization state must remain coherent across process boundaries.

For grants:

create workspace
→ create agent
→ grant capability
→ perform authorized operation
→ terminate
→ fresh process
→ perform operation again
→ revoke
→ fresh process
→ verify denial

For policy:

add deny rule
→ terminate
→ fresh process
→ verify denial
→ remove rule
→ fresh process
→ verify allowed behavior when capability/trust permits it

For corruption:

write malformed disposable state
→ fresh process
→ consequential operation
→ verify fail closed

Do not rely on in-memory caches as persistence evidence.

---

## 19. Concurrency

Test authorization state under controlled concurrency.

Where supported test:

- two simultaneous capability checks;
- grant versus check;
- revoke versus check;
- two grants;
- revoke one grant while another remains;
- policy add versus check;
- policy remove versus check;
- concurrent authorization of independent resources.

Verify:

- no partial persisted grant;
- no malformed policy file;
- no lost unrelated grants/rules;
- no stale allow after revocation where live enforcement is promised;
- no cross-principal authority;
- deterministic final state.

Do not require a particular locking algorithm.

Avoid arbitrary sleeps; use barriers, process completion, polling, or deterministic synchronization.

---

## 20. Audit and observability boundary

Every consequential authorization decision that the current contract audits must be checked for:

- allow/deny outcome;
- principal identity;
- workspace identity where applicable;
- capability/policy reason;
- tool/action identity;
- correlation information where supported;
- timestamp where supported;
- no secret values.

Test both successful and denied authorization.

For denied operations, verify the audit evidence indicates denial occurred before the consequential mutation.

Do not duplicate the complete Audit/Observability suite.

---

## 21. Error behavior

Test invalid authorization requests including:

- unknown capability;
- malformed capability;
- unknown principal;
- invalid agent ID;
- invalid grant ID;
- duplicate grant;
- malformed scope;
- malformed expiry;
- invalid policy rule;
- unknown policy rule;
- malformed authorization store;
- unauthorized revoke;
- unauthorized policy modification.

Verify:

- non-zero CLI exit or structured service error;
- deterministic category where stable;
- no panic;
- no secret leakage;
- no unauthorized state mutation.

---

## 22. CLI black-box coverage

Where current CLI commands exist, execute the real awh binary.

At minimum inspect and test the actual current command set for:

awh capability list
awh capability show ...
awh capability grant ...
awh capability revoke ...
awh capability check ...

awh policy list
awh policy show ...
awh policy check ...
awh policy validate
awh policy explain ...

Also test currently supported trust-related commands that are part of the authorization contract, without duplicating their entire MCP suite:

awh mcp trust ...
awh mcp block ...
awh mcp revoke ...
awh mcp status ...
awh mcp permissions ...

If a command does not exist on the current branch, classify it as Not implemented.

Do not substitute an internal API call and call the CLI feature tested.

---

## 23. Environment variables and configuration

Read docs/configuration.md and test every authorization-related environment/configuration input actually used by the current branch.

Relevant examples may include:

- AWH_API_KEY for remote transport authentication;
- GITHUB_TOKEN for GitHub tool exposure;
- GITHUB_API_URL;
- GITHUB_DEFAULT_OWNER;
- GITHUB_DEFAULT_REPO;
- AWH_BWRAP;
- resource limits affecting the gated operation.

Do not invent authorization environment variables.

For each relevant variable test:

- absent;
- valid synthetic value;
- invalid value;
- interaction with authorization state;
- secret-safe logging.

A configuration value may enable discovery or authentication without granting a capability. Test that distinction.

Never put real tokens into fixtures, source, command-line arguments, snapshots, or logs.

---

## 24. Secrets and sensitive values

Use synthetic secrets only.

Test that secret values are absent from:

- stdout;
- stderr;
- structured errors;
- logs;
- audit;
- persisted capability/policy state;
- test snapshots;
- Git diffs generated by tests.

If authorization metadata identifies a secret permission, assert the permission label rather than the secret value.

---

## 25. Boundary and property testing

Use proptest where it provides meaningful value.

Useful properties include:

- unknown capability names never become valid through normalization;
- unrelated permissions do not imply one another;
- safe resource scopes do not match unrelated sibling prefixes;
- malformed grants never evaluate to allow;
- malformed policy rules never broaden authority;
- revoked grants do not authorize future checks;
- expired grants do not authorize future checks;
- valid authorization state round-trips without changing meaning.

Do not use property testing merely to increase test counts.

---

## 26. Security regression suite

Every discovered authorization bypass becomes a permanent regression test.

At minimum preserve:

- default deny where required;
- least privilege;
- capability isolation;
- principal isolation;
- workspace isolation;
- scope containment;
- expiry enforcement where implemented;
- live revocation;
- policy deny precedence;
- built-in high-risk default deny;
- malformed-state fail closed;
- direct/internal bypass resistance;
- route namespace is not authority;
- tool discovery is not authority;
- no unauthorized filesystem/process/Git/network/MCP mutation;
- no secret leakage;
- authorization decisions are auditable.

A denied operation must have zero unintended consequential side effects.

Never add a test-only authorization bypass.

---

## 27. Failure injection

Deliberately induce authorization-layer failures in disposable state.

Test:

- unreadable grant store;
- malformed grant store;
- truncated grant store;
- duplicate/colliding grant records;
- unreadable policy store;
- malformed policy store;
- invalid policy rule;
- invalid trust state;
- unavailable underlying authorization dependency where injectable;
- authorization immediately before an underlying service failure.

For each failure verify:

1. authorization fails safely;
2. the consequential operation does not run unless authorization already succeeded;
3. no permissive fallback is created;
4. persisted authorization state is not silently destroyed;
5. errors are observable and useful.

If failure occurs after authorization but before service completion, classify it as an execution failure rather than an authorization failure.

---

## 28. Independent oracle requirement

Where practical, verify authorization effects using independent observation.

Examples:

- filesystem bytes read directly from disk;
- marker-file existence;
- Git rev-parse HEAD, status --porcelain, and refs;
- local HTTP server request counter;
- MCP handler/service invocation evidence;
- persisted state read independently;
- audit records read through the canonical audit reader.

Do not calculate expected authorization by calling the same production authorization helper under test.

For denial, strongest oracle is:

authorization denied
+
underlying resource unchanged

---

## 29. Human workflow acceptance tests

Automate complete scenarios.

### Workflow A — Least privilege

create agent A
→ grant process only
→ process operation succeeds
→ network operation fails
→ filesystem mutation fails

Expected: only granted capability is usable.

### Workflow B — Revocation

grant capability
→ authorized operation
→ revoke capability
→ fresh operation

Expected: second operation is denied with no side effect.

### Workflow C — Two principals

grant A
→ A succeeds
→ B attempts same operation

Expected: B remains denied.

### Workflow D — Policy narrowing

grant required capability
→ operation succeeds
→ add matching policy deny
→ same operation

Expected: policy denial wins where the current contract defines that composition.

### Workflow E — Corrupt authorization state

create valid authorization state
→ corrupt disposable state
→ fresh awh process
→ consequential operation

Expected: fail closed; no resource mutation.

### Workflow F — Scope

grant scoped authority
→ operate inside scope
→ operate outside scope

Expected: in-scope follows grant; out-of-scope is denied.

### Workflow G — Expiry

Where implemented:

grant with deterministic expiry
→ before expiry: allowed
→ after expiry: denied

Expected: expired authority is not accepted.

### Workflow H — MCP route independence

authorized agent route
→ allowed operation

foreign/renamed route
→ authorization still follows trusted identity

Expected: route naming cannot impersonate another principal.

---

## 30. Cross-platform matrix

Run authorization tests on supported platforms where practical:

- Linux x86_64;
- Linux ARM64;
- macOS x86_64;
- macOS ARM64;
- Windows x86_64;
- Android/Termux ARM64 where the current feature contract supports the tested boundary.

Pay attention to:

- persistent-state paths;
- file locking;
- path canonicalization;
- case sensitivity;
- process execution;
- local networking;
- permission/resource naming.

Do not mark a platform Passed from compilation alone.

Classify unavailable runtime evidence as Blocked or Unproven.

---

## 31. No test theater

Do not:

- test only CapabilityGrantStore serialization and claim enforcement is verified;
- test only PolicyStore matching and claim consequential authorization is verified;
- mock PolicyEngine when a real enforcement boundary is available;
- assert only an allowed boolean without performing the operation;
- assert only a denied boolean without checking side effects;
- use a route name or user-supplied identity as a fake trust source;
- calculate expected decisions with the same production helper;
- depend on developer-global authorization state;
- use real credentials;
- swallow subprocess/service failures;
- turn blocked tests into passed tests;
- fabricate final-target commands that are not implemented;
- weaken fail-closed behavior to simplify tests;
- modify unrelated test master prompts.

A green unit suite without proof that denied consequential operations remain unchanged is insufficient.

---

## 32. Test implementation requirements

Use existing repository test conventions and helpers.

Prefer:

- unit tests for capability/policy parsing and deterministic matching;
- integration tests for persistent authorization stores;
- subprocess tests for real CLI behavior;
- real session/agent identities;
- real service boundaries;
- disposable filesystem/Git/network fixtures;
- MCP integration only where needed to prove authorization;
- concurrency tests;
- failure-injection tests;
- property tests for pure authorization logic.

Do not build a second authorization engine in test code.

Tests may provide independent oracles and fixture builders, but expected decisions must come from the documented contract and independently observable outcomes.

Every consequential allow/deny test should assert:

1. authorization decision;
2. resulting resource state;
3. relevant audit/security evidence when implemented.

---

## 33. Execution gates

Run the project's current gates:

cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets

Then run:

- capability-specific integration tests;
- policy-specific integration tests;
- real CLI tests;
- relevant MCP authorization tests;
- zero-side-effect denial tests;
- persistence/restart tests;
- concurrency tests;
- security regression tests.

If the repository has additional required CI gates, use those current gates.

Record environment/platform prerequisites and failures honestly.

---

## 34. Evidence report

Report every authorization feature as:

### Passed

Direct evidence demonstrates intended authorization behavior and, for consequential actions, resulting resource state.

### Failed

The real test executed and exposed a production defect.

### Blocked

The test could not execute because of unavailable platform, dependency, environment variable, credential-free prerequisite, or other external condition.

### Unproven

The current implementation or environment does not provide enough evidence.

For every failure include:

- test name;
- principal/session;
- workspace/resource;
- authorization state;
- exact AWH invocation where applicable;
- expected decision;
- actual decision;
- resource state before/after;
- relevant audit evidence;
- error;
- likely subsystem;
- production defect versus test-environment issue.

Never include secret values.

For every unimplemented final-target command, explicitly state Not implemented.

---

## 35. Completion criteria

This prompt is complete only when:

- current capability/policy implementation was inspected;
- current authorization layers were distinguished;
- capability vocabulary was enumerated from source;
- principal identity binding was tested;
- least-privilege behavior was tested;
- grant lifecycle was tested where implemented;
- revocation was tested;
- scope was tested where implemented;
- expiry was tested where implemented;
- policy lifecycle and matching were tested where implemented;
- authorization precedence was tested;
- default-deny/fail-closed behavior was tested;
- denied consequential operations were independently shown to have no unintended side effects;
- internal/direct bypass resistance was tested at the strongest available boundary;
- MCP route/discovery independence was tested where applicable;
- persistence across restart was tested;
- concurrent state changes were tested where applicable;
- audit behavior was checked at the authorization boundary;
- relevant environment variables were tested safely;
- security regressions have permanent coverage;
- final-target gaps are honestly classified;
- no real secrets were used;
- no unrelated test master prompt was modified.

The objective is trustworthy evidence that PolicyEngine/capability authorization is an authoritative security boundary: authority is explicit, least-privilege, principal-bound, scope/expiry-aware where implemented, revocable, fail-closed, auditable, and enforced before consequential AWH operations.

---

## 36. Scope boundary

This prompt owns Capability and Policy Engine testing only.

Do not create full feature suites for:

- Foundation and Distribution;
- Workspace Runtime;
- Agent-Grade Filesystem Editing;
- Git/worktrees;
- Snapshots/Undo/Provenance;
- Context;
- Memory;
- Skills;
- Agent Profiles/policy-routed MCP;
- MCP protocol/transport infrastructure;
- Sessions/Tasks;
- Audit/Observability;
- Terminal;
- Collaboration;
- Control API;
- TUI;
- Connectors;
- Advanced infrastructure.

Those feature families must have independent master test prompts.

Do not modify:

docs/testing-prompts/01-foundation-distribution.md
docs/testing-prompts/02-workspace-runtime.md
docs/testing-prompts/03-agent-grade-filesystem-editing.md
docs/testing-prompts/04-git-and-worktrees.md

Do not modify any other existing test master prompt while executing this prompt.
