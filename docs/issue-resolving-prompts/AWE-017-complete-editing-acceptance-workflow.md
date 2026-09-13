# AWE-017 / #38 — Complete Agent-Grade Editing Acceptance Workflow

> **Status:** Issue-resolving master prompt  
> **Branch:** `rust`  
> **Primary scope:** build and pass the final real-workspace acceptance workflow proving that AWH's agent-grade editing stack works end-to-end across identity, policy, expected state, snapshots, mutation, verification, provenance, audit, rollback, MCP, and CLI.  
> **Dependencies:** AWE-011 / #32, AWE-012 / #33, AWE-013 / #34, AWE-014 / #35, AWE-015 / #36, AWE-016 / #37. Reuse AWE-001 through AWE-010 where their implementation already exists on `rust`.

---

## 1. Mission

Implement the **canonical final acceptance workflow** for agent-grade editing.

This issue is the release-level integration gate for the editing subsystem. It must prove that the individual capabilities delivered by the preceding AWE milestones compose into one safe, observable, reversible workflow on a **real isolated workspace and real filesystem**.

The target workflow is:

```text
Agent
  ↓
Caller / session / workspace identity
  ↓
Read / locate target
  ↓
Prepare precise edit transaction
  ↓
Capability + policy authorization
  ↓
Expected-state / stale-state validation
  ↓
Pre-edit snapshot
  ↓
Apply mutation atomically
  ↓
Post-edit verification
  ↓
Persist provenance + audit
  ↓
Return correlated result
  ↓
Explicit rollback
  ↓
Post-rollback verification
```

The acceptance suite must prove the workflow behavior, not merely that individual functions exist.

The core product invariant is:

```text
unsafe request → rejected before mutation
stale request → conflict before mutation
authorized request → snapshot → apply → verify → provenance/audit
rollback request → exact original bytes restored → verified
```

Do not mark agent-grade editing implemented if this workflow cannot pass against the actual production services and filesystem.

---

## 2. Authoritative acceptance boundary

Issue #38 is the **final editing acceptance gate** before AWH can claim that agent-grade editing is implemented.

The issue requires proof that:

- the entire workflow executes in a real workspace;
- every consequential step is correlated by stable identifiers;
- policy denial causes zero mutation;
- stale state produces a structured conflict;
- the pre-edit snapshot is linked to the edit;
- provenance identifies caller/session/workspace and before/after state;
- audit survives process restart;
- verification failure triggers safe recovery;
- explicit rollback restores the exact original bytes and verifies them;
- MCP and CLI exercise the same canonical service;
- the acceptance path is not mock-only.

The roadmap independently defines the same architectural requirement: MCP, CLI, TUI, and Control API are interfaces over shared application services, and all editing interfaces must use the same `EditService`. fileciteturn177file0

Do not weaken an acceptance criterion simply because one implementation layer is inconvenient to exercise.

If a dependency is genuinely absent, record the dependency as a blocker rather than manufacturing a fake success path.

---

## 3. Forensic preflight — inspect before implementing

Before changing code, inspect the actual `rust` branch and determine what exists today.

At minimum inspect:

```text
src/services/edit.rs
src/services/
src/mcp/
src/cli/
src/context/
src/*snapshot*
src/*audit*
src/*policy*
src/*capability*
src/*session*
src/*agent*
examples/mcp-interop/
tests/
docs/CLI.md
docs/mcp.md
docs/testing.md
docs/PROJECT_STATUS.md
docs/PROJECT_ROADMAP.md
docs/issue-resolving-prompts/AWE-011-capability-policy-edit-paths.md
docs/issue-resolving-prompts/AWE-012-snapshots-provenance.md
docs/issue-resolving-prompts/AWE-013-structured-persistent-audit.md
docs/issue-resolving-prompts/AWE-014-cli-agent-grade-editing.md
docs/issue-resolving-prompts/AWE-015-agent-grade-editing-test-suite.md
docs/issue-resolving-prompts/AWE-016-real-mcp-client-validation.md
```

Also locate the real implementations for:

- caller/agent identity;
- sessions and workspace identity;
- capability grants;
- PolicyEngine and policy decisions;
- canonical `EditService`;
- `EditTransaction`, `EditId`, `ExpectedState`, `FileState`;
- pre-edit file snapshots;
- provenance records;
- persistent audit;
- rollback;
- post-edit verification;
- MCP edit tools;
- CLI edit commands;
- MCP interoperability harness;
- filesystem security and canonical path resolution.

The current edit model is explicitly transport-independent and shared by MCP, CLI, TUI, and Control API. It defines `EditId`, the core edit operations, expected state, before/after file state, and lifecycle status. fileciteturn176file0

Do not assume that the presence of a type, CLI command, tool name, or documentation entry means the behavior is implemented.

### Required forensic report

Before implementation, determine and document internally:

1. What is the canonical service entry point for an edit?
2. How is the caller/agent identified?
3. How are session and workspace identities obtained?
4. Where are capability checks enforced?
5. Where is the authoritative policy decision made?
6. How does an edit receive its stable `EditId`?
7. How is expected state supplied and validated?
8. What snapshot object is created before mutation?
9. Where are exact prior bytes stored?
10. How is provenance persisted and linked to `EditId`?
11. Where is audit persisted and how is it reopened after restart?
12. How is verification performed against the real filesystem?
13. How is verification failure recovered?
14. How does rollback locate the correct pre-edit snapshot?
15. Which MCP tools expose the workflow?
16. Which CLI commands expose the workflow?
17. How can MCP and CLI be proven to invoke the same service?
18. Which existing AWE-015 and AWE-016 tests can be composed instead of duplicated?
19. Which failure-injection hooks already exist?
20. Which parts are genuinely missing and must be implemented for this issue?

Keep the implementation narrowly scoped to the acceptance workflow. Do not start unrelated roadmap work.

---

## 4. Non-negotiable architecture

### 4.1 One canonical editing service

The required architecture is:

```text
                 MCP
                  │
                 CLI
                  │
                 TUI
                  │
             Control API
                  │
                  ▼
          Shared application layer
                  │
                  ▼
             EditService
                  │
       ┌──────────┼──────────┐
       ▼          ▼          ▼
    Policy     Snapshot     Audit
       │          │          │
       └──────────┼──────────┘
                  ▼
          Secure filesystem
```

MCP and CLI must not contain independent mutation algorithms.

Do not implement:

- a CLI-only editor;
- an MCP-only editor;
- a second snapshot mechanism;
- a second provenance model;
- a second audit store;
- a transport-specific rollback algorithm;
- test-only filesystem semantics that differ from production.

The final acceptance test must expose divergence if any adapter bypasses the canonical service.

### 4.2 Real filesystem is mandatory

The acceptance workflow must operate on actual temporary files/directories.

Mocks may be used to isolate failure injection or external dependencies, but mocks cannot be the sole proof of:

- policy denial;
- stale-state detection;
- snapshot creation;
- mutation;
- verification;
- rollback;
- provenance;
- persistent audit;
- MCP/CLI equivalence.

### 4.3 No whole-file rewrite fallback

If a precise edit operation fails, the implementation must not silently fall back to replacing the entire file.

A failed operation must either:

- fail without mutation, or
- enter the canonical atomic recovery path.

The acceptance workflow must detect unexpected whole-file replacement behavior.

---

## 5. Canonical acceptance fixture

Create or reuse a deterministic isolated workspace.

Example structure:

```text
<temp-workspace>/
├── src/
│   └── example.rs
├── docs/
│   └── note.md
└── .agent/
```

The primary target should contain known bytes, for example:

```text
alpha
beta
charlie
```

Capture before mutation:

- exact bytes;
- SHA-256 hash;
- byte size;
- line count;
- relevant contextual text;
- file path relative to the workspace.

The fixture must be:

- outside the repository working tree unless the test specifically validates repository integration;
- independent of the developer's home directory;
- isolated from concurrent tests;
- cleaned up deterministically;
- safe to run repeatedly;
- suitable for Windows/Linux/macOS path semantics where supported;
- compatible with CI and local development.

Never run the final acceptance workflow against a developer's real project or home directory.

---

## 6. Correlation and identity contract

Every consequential step must be attributable to the same workflow.

At minimum correlate:

```text
workflow/run correlation ID
agent ID
session ID
workspace ID
edit ID
snapshot ID
audit event ID(s)
```

Where the existing architecture has fewer identifiers, use the canonical available identifiers rather than inventing a parallel identity system.

The acceptance assertions must prove that the records belong to the same edit.

### Required correlation chain

```text
agent/session/workspace
        ↓
     edit_id
        ↓
  snapshot_id
        ↓
 provenance record
        ↓
 audit events
        ↓
 rollback(edit_id)
```

A record that merely contains the same file path is not sufficient correlation.

Do not use timestamps as the sole correlation key.

---

## 7. Stage 1 — caller/session/workspace setup

Start from a clean isolated workspace and establish the canonical caller identity.

Verify:

- an agent identity exists;
- the session is valid;
- the workspace identity is valid;
- the caller has the expected capabilities;
- the policy engine can evaluate the requested edit;
- the target path belongs to the authorized workspace.

The test must record the identity values needed to correlate later provenance and audit records.

Do not bypass authorization by calling a lower-level function with privileged internal state unless the test is explicitly testing that internal boundary.

---

## 8. Stage 2 — read and locate

Use the canonical read mechanism available to the workflow.

The acceptance path should:

1. read the target file;
2. establish the current file state;
3. locate the intended edit region;
4. prepare a precise operation;
5. preserve enough expected-state information to detect an intervening modification.

The prepared operation should be minimal.

For example:

```text
beta → BETA
```

rather than replacing the complete file.

Capture the expected state before any mutation.

The acceptance test must verify that the expected state corresponds to the actual bytes on disk at preparation time.

---

## 9. Stage 3 — policy/capability authorization

Before applying the edit, execute the real capability and policy path.

Required positive path:

```text
caller/session/workspace
        ↓
capability check
        ↓
policy decision = allow
        ↓
edit continues
```

Required negative path:

```text
caller/session/workspace
        ↓
capability/policy check
        ↓
policy decision = deny
        ↓
edit stops
        ↓
zero filesystem mutation
```

### Policy-denial acceptance test

Create a test where the caller lacks the required edit capability or the policy explicitly denies the target operation.

Capture the exact original bytes before the request.

Attempt the real edit through the public path under test.

Assert:

- request is rejected;
- rejection is attributable to authorization/policy;
- no snapshot is falsely recorded as a successful applied edit;
- target bytes remain exactly unchanged;
- file metadata remains unchanged unless the implementation explicitly documents harmless access-time behavior;
- no partial multi-file mutation occurs;
- audit records the denial if audit is part of the canonical lifecycle;
- no sensitive policy internals are leaked.

The acceptance test must fail if policy is checked only after mutation.

---

## 10. Stage 4 — expected-state and stale-state validation

The authorized positive path must still validate the target state before mutation.

The test must explicitly create a stale-state scenario:

```text
read target
  ↓
record expected state
  ↓
external process modifies target
  ↓
submit original edit with stale expected state
```

Assert:

- a structured conflict is returned;
- the conflict identifies stale expected state using the canonical error model;
- no requested mutation is applied;
- external bytes remain intact;
- no false successful snapshot/provenance/audit record is created;
- a fresh read observes the external modification.

Where stable fields are exposed, assert:

- expected hash;
- observed hash;
- path;
- edit ID/correlation ID;
- conflict classification.

Do not make the test depend on exact human-readable error text.

---

## 11. Stage 5 — pre-edit snapshot

On the successful authorized path, capture the exact pre-edit state before mutation.

The snapshot must be associated with the current `edit_id`.

Required assertions:

- snapshot exists before mutation is committed;
- snapshot identifies the target file/path;
- snapshot contains or securely references the exact prior bytes;
- pre-edit hash matches the bytes actually on disk;
- snapshot is linked to the edit transaction;
- snapshot does not reuse unrelated context snapshots without an explicit abstraction boundary;
- snapshot storage is inside the canonical persistent substrate;
- snapshot corruption or missing data fails safely.

The acceptance workflow must be able to prove later that rollback used the snapshot belonging to this exact edit.

Do not implement a second snapshot store inside the acceptance test.

---

## 12. Stage 6 — atomic apply

Execute the real edit transaction.

Use the canonical edit operation already implemented by the service.

The operation must:

- operate on the authorized workspace;
- preserve expected-state guarantees;
- use canonical secure path resolution;
- avoid symlink/path traversal escapes;
- use the established atomic/recovery mechanism;
- produce the canonical `edit_id`;
- transition through the appropriate lifecycle states;
- expose accurate before/after state.

For a successful single-file edit, assert exact expected bytes.

For a multi-operation transaction, assert transaction-level atomicity and deterministic behavior.

Do not add acceptance-only mutation code.

---

## 13. Stage 7 — post-edit verification

Verification must inspect the actual filesystem after mutation.

Required oracle:

```text
reported success
AND
actual filesystem bytes correct
AND
actual FileState correct
```

Assert:

- after hash is correct;
- after size is correct;
- after line count is correct;
- intended content changed;
- unrelated content did not change;
- expected operation postcondition holds;
- no unexpected files appeared;
- symlink/path boundaries remain intact.

Do not treat a returned `Applied` status as proof that verification succeeded.

If the service exposes a distinct `Verified` and `Committed` lifecycle, assert the appropriate ordering.

---

## 14. Stage 8 — provenance

After successful verification, query or inspect the canonical provenance record.

It must identify enough information to reconstruct what happened without storing file contents in normal audit/provenance records.

At minimum validate:

```text
edit_id
agent/caller identity
session_id
workspace_id
path
operation
timestamp
before_hash
after_hash
snapshot_id
result/status
```

Where the implementation has additional canonical fields, validate them without making the acceptance test unnecessarily brittle.

Assert:

- before hash equals the original bytes;
- after hash equals the verified edited bytes;
- snapshot ID points to the pre-edit snapshot;
- caller/session/workspace match the initiating workflow;
- the record is persistent according to the AWE-012 contract;
- file contents and secrets are not unnecessarily embedded in provenance.

---

## 15. Stage 9 — persistent audit and restart survival

The acceptance gate must prove that audit is not merely an in-memory success log.

Sequence:

```text
successful edit
  ↓
write/persist audit
  ↓
terminate relevant AWH process/service
  ↓
restart
  ↓
query audit
```

Assert that the post-restart audit history still contains the correlated edit lifecycle.

At minimum verify the relevant events for:

- request;
- authorization;
- validation/conflict outcome;
- snapshot;
- apply;
- verification;
- commit/result;
- rollback where performed.

The exact event vocabulary must follow the canonical audit implementation.

Do not create an acceptance-specific audit file merely to make restart survival pass.

### Audit privacy

The test must verify that normal audit records do not contain:

- complete file contents;
- snapshot bytes;
- credentials;
- access tokens;
- unrelated secrets.

---

## 16. Stage 10 — verification-failure recovery

This is a mandatory failure-injection scenario.

The test must force the post-apply verification stage to fail or detect an intentionally invalid postcondition using the canonical failure-injection mechanism.

Required behavior:

```text
snapshot
→ apply
→ verification failure
→ recovery/rollback
→ filesystem restored
→ restoration verified
```

Assert:

- verification failure is surfaced as a structured failure;
- recovery starts only through the canonical recovery path;
- the original bytes are restored exactly;
- the final hash equals the original hash;
- no partial edit remains;
- rollback/recovery is correlated to the same edit;
- audit records the failure and recovery;
- provenance accurately reflects the unsuccessful outcome;
- the system does not report success merely because the initial write succeeded.

If verification-failure injection does not currently exist, implement the smallest reusable test seam necessary in the canonical service. Do not add test-only production semantics that can be enabled accidentally in normal operation.

---

## 17. Stage 11 — explicit rollback

After a successful edit and verification, exercise explicit rollback through the public canonical interface.

Preferred sequence:

```text
original bytes
  ↓
read
  ↓
authorized edit
  ↓
snapshot
  ↓
apply
  ↓
verify
  ↓
rollback(edit_id)
  ↓
verify restoration
```

Required assertions:

- rollback targets the exact `edit_id`;
- correct snapshot is selected;
- original bytes are restored byte-for-byte;
- restored hash equals the original hash;
- restored size and line count are correct;
- unrelated files remain unchanged;
- provenance links rollback to the original edit;
- audit records rollback;
- final lifecycle status is accurate.

### Rollback conflict safety

Add a scenario where the target is externally modified after the successful edit but before rollback.

Assert that rollback does not silently overwrite unrelated external changes when the canonical conflict policy requires refusal.

The rollback test must prove that rollback is **conflict-aware**, not merely a blind file copy.

---

## 18. MCP and CLI equivalence

AWE-017 must prove that MCP and CLI are two interfaces to the same editing semantics.

Do not merely test that both commands return exit code zero.

Use equivalent isolated fixtures and compare stable semantics:

```text
CLI → canonical EditService
MCP → canonical EditService
```

Compare, where applicable:

- operation result;
- edit identity semantics;
- before/after hashes;
- conflict classification;
- policy-denial behavior;
- snapshot linkage;
- provenance;
- audit correlation;
- rollback behavior;
- exact filesystem result.

The implementation must not fork the editing algorithm based on transport.

### Stronger equivalence test

Where practical, execute the same logical operation once through CLI and once through MCP against independent identical fixtures.

The final filesystem states and stable error classifications should be equivalent.

Do not require identical human-readable output.

---

## 19. MCP real-client acceptance

AWE-016 already establishes real MCP-client interoperability. AWE-017 must compose that capability into the **complete editing workflow** rather than replacing it with direct service calls.

At minimum the MCP path should prove:

```text
real MCP client
→ initialize
→ tools/list
→ read
→ edit
→ verify
→ provenance/audit
→ rollback
→ verify restoration
```

Where the real external client cannot expose a particular internal lifecycle object directly, use the canonical AWH query interface or independent filesystem assertion to validate the underlying result.

Do not claim end-to-end MCP acceptance from a mocked dispatcher call.

---

## 20. Multi-file transaction acceptance

The final acceptance workflow must include at least one multi-file scenario if the canonical edit service supports multi-file transactions.

Use two or more independent files.

Positive path:

```text
prepare all
→ authorize
→ validate all expected states
→ snapshot all
→ apply transaction
→ verify all
→ commit
```

Failure path:

```text
prepare files
→ one file conflicts/fails validation
→ transaction rejected
→ no file partially mutated
```

Rollback path:

```text
successful multi-file transaction
→ rollback(edit_id)
→ every affected file restored byte-for-byte
```

Assert transaction-level correlation and audit rather than treating the files as unrelated edits.

---

## 21. Filesystem and security acceptance matrix

The acceptance suite must include representative security cases.

### Path traversal

Reject targets such as:

```text
../outside
../../outside
```

and any equivalent path-normalization escape.

Assert zero mutation outside the workspace.

### Absolute path

Reject unauthorized absolute paths according to the canonical path policy.

### Symlink escape

Create a symlink inside the workspace pointing outside it where the platform permits.

Attempt an edit through the symlink.

Assert that canonical security policy prevents workspace escape.

### Unauthorized workspace

Attempt an edit using a caller/session not authorized for the workspace.

Assert zero mutation.

### Sensitive output

Ensure error/audit/provenance responses do not disclose secrets or raw file contents unnecessarily.

Do not weaken security checks merely to simplify the acceptance fixture.

---

## 22. Encoding and filesystem edge cases

The acceptance workflow should cover at least:

- empty file;
- zero-byte file;
- one-line file;
- final newline present;
- final newline absent;
- LF line endings;
- CRLF line endings where supported;
- UTF-8 text;
- Devanagari text;
- emoji;
- non-ASCII path components where supported;
- very small file;
- moderately large bounded file;
- missing target;
- target changed between read and apply.

The purpose is to ensure the complete workflow does not accidentally introduce encoding or line-ending corruption.

For exact-byte rollback tests, compare raw bytes rather than normalized strings.

---

## 23. Restart and persistence acceptance

The workflow must survive the process boundaries required by AWE-012/AWE-013.

At minimum test:

```text
edit
→ persist snapshot/provenance/audit
→ terminate service/process
→ restart
→ query state
→ rollback using persisted identity
→ verify original bytes
```

If the architecture intentionally does not support rollback after a particular restart boundary, document that limitation explicitly and do not claim the stronger guarantee.

Persistent state corruption must fail closed.

A corrupted snapshot/audit/provenance record must never cause silent restoration from the wrong record.

---

## 24. Concurrency and TOCTOU acceptance

The acceptance workflow must include at least one realistic concurrent modification scenario.

Example:

```text
client reads expected state
       ↓
external writer modifies file
       ↓
client submits edit
```

Expected result:

```text
conflict
+ zero requested mutation
+ external state preserved
```

Where the filesystem coordination layer provides stronger mutation serialization, test the documented behavior.

Do not claim that a hash captured earlier completely eliminates all TOCTOU races unless the canonical implementation actually provides that guarantee.

Document the remaining boundary between validation and atomic commit.

---

## 25. Lifecycle and state-machine assertions

Use the canonical `EditStatus` vocabulary rather than inventing another lifecycle enum.

The successful path should demonstrate an ordering equivalent to:

```text
Requested
→ Authorized
→ Located
→ Validated
→ Snapshotted
→ Applied
→ Verified
→ Committed
```

Failure paths must terminate in the appropriate existing failure/conflict/recovery status.

Do not allow impossible transitions such as:

```text
Committed → Requested
Verified → Authorized
Rejected → Applied
Conflict → Committed
```

If the existing implementation intentionally omits a public lifecycle state, test observable semantics rather than forcing an artificial state transition solely for this issue.

---

## 26. Structured result and error contract

The acceptance suite must assert stable machine-readable semantics.

For success, validate stable fields such as:

```text
edit_id
status/result
before state
after state
snapshot reference
correlation identifiers
```

For failure, distinguish at minimum where supported:

```text
invalid request
policy denied
not found
stale/conflict
snapshot failure
apply failure
verification failure
rollback conflict
rollback failure
internal/persistence failure
```

Do not make tests depend on complete error prose.

Do not expose internal Rust types directly as an accidental wire contract unless that is the established public API.

---

## 27. Failure-injection matrix

The acceptance suite must exercise realistic failures at critical boundaries.

At minimum consider:

| Failure point | Required outcome |
|---|---|
| authorization | zero mutation |
| expected-state validation | conflict, zero mutation |
| snapshot creation | zero mutation |
| pre-commit write | safe recovery, no false success |
| atomic commit | recovery according to canonical service |
| post-edit verification | rollback/recovery |
| provenance persistence | fail safely according to canonical durability policy |
| audit persistence | deterministic documented behavior; no false durable success |
| rollback lookup | structured failure, no mutation |
| rollback write | safe recovery |
| post-rollback verification | recovery/failure surfaced |

Do not create artificial success responses after injected failures.

---

## 28. Property and invariant testing

In addition to the deterministic acceptance workflow, add bounded property-oriented checks for the core invariants.

### Invariant A — denial is non-mutating

```text
deny(request)
⇒ filesystem_after == filesystem_before
```

### Invariant B — stale state is non-mutating

```text
stale(request)
⇒ filesystem_after == externally_modified_state
```

### Invariant C — successful rollback restores bytes

```text
apply(edit)
→ verify
→ rollback(edit)
→ verify
⇒ bytes_after == original_bytes
```

### Invariant D — multi-file preparation failure is atomic

```text
prepare(transaction with one invalid/conflicting operation)
⇒ every affected file remains unchanged
```

### Invariant E — correlation integrity

```text
edit_id
↔ snapshot
↔ provenance
↔ audit
↔ rollback
```

All identifiers in the chain must refer to the same logical edit.

Keep generators bounded so the suite remains deterministic and CI-friendly.

---

## 29. MCP/CLI adapter divergence detection

Add a test or inspection mechanism that makes it difficult for future adapters to bypass the canonical service.

Acceptable evidence includes:

- shared integration fixtures;
- identical service-level instrumentation;
- correlation records proving both paths reach the same service boundary;
- equivalent filesystem/error results;
- architectural tests preventing duplicate mutation helpers.

Do not rely exclusively on source-code inspection.

The strongest proof remains observable behavioral equivalence against real files.

---

## 30. Test isolation, determinism, and cleanup

The acceptance workflow must be safe to run:

- locally;
- in CI;
- repeatedly;
- concurrently with unrelated tests;
- after interrupted prior runs.

Requirements:

- unique temporary workspace per test run;
- deterministic fixture contents;
- deterministic cleanup;
- bounded timeouts;
- no dependence on developer-specific paths;
- no dependence on pre-existing `.agent` state;
- no network requirement unless testing an actual remote MCP transport;
- no accidental writes to the repository working tree;
- cleanup of child processes and transport listeners.

A failed test must preserve enough diagnostic information to reproduce the failure without leaving uncontrolled processes or files behind.

---

## 31. Android/Termux compatibility

Where the current AWH implementation supports Android/Termux, provide a reproducible validation path.

At minimum document:

- binary/runtime prerequisites;
- temporary workspace creation;
- MCP transport used;
- CLI invocation;
- filesystem behavior;
- any platform-specific symlink or permission limitations;
- which tests are automated versus manual.

Do not weaken filesystem security or persistence guarantees merely to accommodate Android.

Platform-specific limitations must be explicit rather than silently skipped.

---

## 32. CI integration

The final acceptance workflow must be runnable as part of the repository test strategy without making normal unit-test execution depend on unavailable external agent installations.

Separate tests into appropriate classes where necessary:

```text
unit
integration
real-filesystem
MCP reference-client
external-client/manual
platform-specific
```

The deterministic automated acceptance workflow should run in CI whenever its prerequisites are available.

External clients that cannot legally/reliably be installed in CI may remain manual, but their evidence must be clearly labelled.

Required Rust verification remains:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

If repository-specific test commands exist, run them as well.

Do not weaken warnings or skip tests to make the acceptance gate green.

---

## 33. Documentation requirements

Update documentation only where implementation changes genuinely require it.

Documentation must accurately distinguish:

- implemented behavior;
- automated acceptance coverage;
- manual interoperability coverage;
- unsupported/platform-specific behavior;
- known limitations.

Do not advertise planned commands or guarantees as implemented merely because AWE-017 is being worked on.

The roadmap explicitly states that a feature is implemented only when behavior exists and is validated, and that MCP-facing features require real-client validation where practical. fileciteturn177file0

If this issue's implementation requires documentation changes, make only the minimum necessary documentation changes after the acceptance workflow is correct.

---

## 34. Performance and resource safety

The acceptance workflow must not introduce unbounded resource usage.

Validate bounded behavior for:

- temporary workspace size;
- snapshot storage;
- audit persistence;
- process lifetime;
- MCP request timeout;
- large but bounded file fixtures;
- repeated acceptance runs.

Do not load arbitrarily large files into memory merely because the acceptance fixture is small.

Do not add unbounded retry loops.

Do not leave child processes, sockets, temporary files, or locks alive after test completion.

---

## 35. Backward compatibility

Preserve existing public behavior unless the issue explicitly requires a contract correction.

In particular:

- existing edit model serialization must remain compatible where intended;
- existing MCP tools must not silently change semantics;
- existing CLI commands must not acquire transport-specific behavior;
- existing policy decisions must remain fail-closed;
- existing snapshot/provenance/audit records must remain readable according to their versioning contract;
- acceptance tests must not require destructive migration of existing `.agent` state.

If a schema migration is unavoidable, implement explicit versioning and safe migration behavior rather than assuming a clean filesystem.

---

## 36. Definition of Done

AWE-017 is complete only when all applicable items below are true.

### Architecture

- [ ] One canonical EditService owns editing semantics.
- [ ] MCP and CLI do not contain duplicate mutation algorithms.
- [ ] Snapshot/provenance/audit remain canonical shared services.
- [ ] Policy/capability enforcement occurs before mutation.

### End-to-end workflow

- [ ] Real isolated workspace is created.
- [ ] Caller/session/workspace identity is established.
- [ ] Real read/locate operation succeeds.
- [ ] Precise edit transaction is prepared.
- [ ] Authorized edit succeeds.
- [ ] Expected state is validated.
- [ ] Pre-edit snapshot is created and linked to `edit_id`.
- [ ] Real filesystem mutation occurs atomically.
- [ ] Post-edit verification reads the actual filesystem.
- [ ] Provenance contains the required identity and state hashes.
- [ ] Audit is persisted.
- [ ] Audit survives restart.
- [ ] Explicit rollback restores exact original bytes.
- [ ] Post-rollback verification succeeds.

### Safety

- [ ] Policy denial produces zero mutation.
- [ ] Stale state produces a conflict and zero requested mutation.
- [ ] Verification failure triggers safe recovery.
- [ ] Rollback is conflict-aware.
- [ ] Multi-file preparation failure produces no partial mutation.
- [ ] Path traversal is rejected.
- [ ] Symlink/workspace escape is rejected.
- [ ] Sensitive contents/secrets are not exposed through audit/error output.

### Interface equivalence

- [ ] MCP exercises the canonical service.
- [ ] CLI exercises the canonical service.
- [ ] MCP real-client acceptance passes where supported.
- [ ] MCP and CLI produce equivalent stable semantics.

### Testing

- [ ] Real filesystem tests pass.
- [ ] Failure-injection tests pass.
- [ ] Restart/persistence tests pass.
- [ ] Concurrency/stale-read tests pass.
- [ ] Unicode/newline/EOF edge cases pass.
- [ ] Property/invariant checks pass where applicable.
- [ ] Test cleanup is deterministic.
- [ ] Full Rust CI passes.

### Documentation

- [ ] Acceptance evidence is documented accurately.
- [ ] Unsupported/manual-only client coverage is explicitly labelled.
- [ ] No future feature is falsely documented as implemented.

---

## 37. Explicit non-goals

Do **not** use AWE-017 to:

- build a new agent framework;
- implement model routing;
- implement generic workflow/DAG orchestration;
- replace OS/container/VM sandboxing with snapshots;
- create a second editing engine;
- create a second identity system;
- create a second snapshot store;
- create a second audit store;
- redesign unrelated MCP infrastructure;
- redesign Git/worktree architecture unrelated to editing acceptance;
- add unrelated TUI functionality;
- add unrelated Control API features;
- optimize prematurely for distributed execution;
- make external client installations mandatory for all CI environments;
- bypass security because a client is difficult to configure.

AWE-017 is about **proving and completing the coherent agent-grade editing workflow**, not expanding the product surface.

---

## 38. Final implementation report

Before declaring the issue complete, produce a concise implementation report containing:

1. files modified;
2. canonical service path used;
3. acceptance test entry point(s);
4. real filesystem fixture design;
5. identity/correlation model used;
6. policy-denial evidence;
7. stale-state/conflict evidence;
8. snapshot/provenance evidence;
9. audit restart evidence;
10. verification-failure recovery evidence;
11. explicit rollback evidence;
12. MCP evidence;
13. CLI evidence;
14. MCP/CLI equivalence evidence;
15. security/path evidence;
16. platform-specific limitations;
17. exact verification commands executed;
18. test results;
19. remaining blockers or known limitations.

Do not claim success if any mandatory acceptance criterion is unverified.

If an external-client requirement cannot be automated, state exactly what was verified and what remains manual.

---

## 39. Final release gate

The final acceptance rule is:

```text
Real workspace
    ↓
Real caller/session identity
    ↓
Real policy/capability decision
    ↓
Real expected-state validation
    ↓
Real snapshot
    ↓
Real atomic edit
    ↓
Real filesystem verification
    ↓
Real provenance
    ↓
Persistent audit
    ↓
Restart
    ↓
Real rollback
    ↓
Exact-byte restoration
    ↓
Final verification
```

If any stage is mocked when real behavior is required, the acceptance gate is not passed.

If policy denial mutates the filesystem, the acceptance gate is not passed.

If stale state mutates the requested target, the acceptance gate is not passed.

If verification failure leaves a partial edit without safe recovery, the acceptance gate is not passed.

If rollback restores approximate content rather than exact original bytes, the acceptance gate is not passed.

If MCP and CLI implement separate editing semantics, the acceptance gate is not passed.

If audit/provenance cannot correlate the operation to its caller/session/workspace/edit, the acceptance gate is not passed.

If the workflow cannot survive the required persistence/restart boundary, the acceptance gate is not passed.

**Only after the complete real-workspace acceptance workflow passes may AWH claim that agent-grade editing is implemented.**

---

## HARD STOP

After implementing, testing, and documenting **AWE-017 only**, stop.

Do not implement AWE-018 or any later roadmap issue.

Do not modify unrelated repository files.

Do not broaden scope after the acceptance gate passes.

Wait for the next explicit issue instruction.
