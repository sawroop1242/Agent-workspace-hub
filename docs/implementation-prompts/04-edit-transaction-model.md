# Prompt 04 — Canonical Edit Transaction Model (AWE-001 / #22)

## Mission

Maintain exactly one transport-independent edit transaction vocabulary for AWH.

The transaction model is the canonical contract shared by future MCP, CLI, TUI, and Control API editing interfaces. It represents an edit request, expected preconditions, observed file state, runtime correlation, lifecycle, and structured domain failures.

**Hard boundary:** Prompt 04 defines the edit transaction model. It does not execute edits.

Do not implement file mutation, patch execution, snapshots, rollback, authorization, MCP editing tools, CLI editing commands, audit persistence, Git worktrees, or a second edit model.

---

## 1. Product boundary

AWH is an agent-agnostic, local-first workspace runtime for coding agents.

AWH owns workspace state, controlled editing, capability/policy boundaries, snapshots/provenance/rollback, runtime identity, MCP/CLI/TUI/API interfaces, and audit.

External agents own reasoning, planning, model selection, provider orchestration, and agent intelligence.

The transaction model must remain independent of transport and executor implementations.

---

## 2. Required repository forensics

Before changing code:

~~~text
git status --short --branch
git log --oneline -n 20
cargo metadata --no-deps
cargo check --all-targets
~~~

If cargo cannot run, record that fact and never claim the gate passed.

Inspect:

~~~text
Cargo.toml
README.md
AGENTS.md (if present)
docs/FEATURES.md
docs/PROJECT_CONTEXT.md
docs/architecture.md
docs/security.md
docs/threat-model.md
docs/roadmap/GROWTH_STRATEGY.md
docs/roadmap/PROJECT_ROADMAP.md
docs/roadmap/STATUS.md
docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
docs/implementation-prompts/README.md
docs/implementation-prompts/01-*.md
docs/implementation-prompts/02-*.md
docs/implementation-prompts/03-*.md
docs/implementation-prompts/05-*.md
docs/implementation-prompts/06-*.md
docs/implementation-prompts/07-*.md
docs/implementation-prompts/08-*.md
docs/implementation-prompts/09-*.md
docs/implementation-prompts/10-*.md
docs/implementation-prompts/11-*.md
docs/implementation-prompts/15-*.md
~~~

Inspect:

~~~text
src/services/edit.rs
src/services/files.rs
src/services/mod.rs
src/core/identity.rs
src/core/agents.rs
src/core/errors.rs
src/main.rs
~~~

Search:

~~~text
rg -n "EditTransaction|EditOperation|EditId|ExpectedState|FileState|EditStatus|EditError|StateMatch|EditIdentity|EditRefs|validate_shape|filesystem.patch|apply_diff|replace|insert|delete-range|edit transaction" src tests docs
~~~

Also inspect all callers, fixtures, and tests that use these symbols.

The consolidated implementation-prompts directory, current source, roadmap, feature documentation, and tests are the active contract. Historical issue-resolving/trust-wedge material is reference only.

---

## 3. Current implementation facts

The current repository already contains the canonical edit domain in src/services/edit.rs.

Existing concepts include:

~~~text
EditId
EditOperation
ExpectedState
FileState
ExpectedComponent
StateMatch
MalformedExpectedState
EditIdentity
EditRefs
EditStatus
EditTransaction
EditError
~~~

Reuse these types.

Do not create a parallel EditTransaction, PatchRequest, FileMutation, or transport-specific domain model.

The repository's forensic material identifies this area as scaffold-level: the model exists, but no canonical executor currently consumes it. Preserve that boundary.

---

## 4. Target architecture

The eventual runtime path is:

~~~text
external agent
  ↓
MCP / CLI / TUI / Control API
  ↓
trusted caller context
  ↓
authorization / policy
  ↓
EditService
  ↓
EditTransaction
  ↓
filesystem executor
  ↓
snapshot / provenance / audit
~~~

Prompt 04 owns only the transaction-domain contract.

---

## 5. Canonical transaction requirements

Represent:

1. stable edit identity;
2. one or more operations;
3. workspace-relative target resources;
4. expected pre-edit state;
5. observed before state;
6. observed after state;
7. lifecycle status;
8. optional agent/session/workspace identity;
9. optional snapshot/provenance/audit/policy references;
10. deterministic structured errors;
11. serialization/deserialization.

The model must be deterministic, serializable, transport-independent, and safe to validate without filesystem I/O.

---

## 6. EditId

Maintain exactly one EditId.

Requirements:

- non-empty;
- unique under normal generation;
- serializable/deserializable;
- comparable/hashable;
- displayable;
- independent of transport;
- independent of filesystem paths;
- safe for correlation;
- never contains secrets.

Reuse the current ID generation unless a concrete correctness defect is found.

Do not create separate IDs for MCP, CLI, API, TUI, filesystem, or Git.

---

## 7. EditOperation

The canonical operation vocabulary is:

~~~text
Replace
Insert
DeleteRange
Patch
ApplyDiff
~~~

Reuse the existing enum and serde contract.

### Replace

Represent:

- path;
- old/matching text;
- replacement text;
- optional occurrence.

Rules:

- empty match is invalid;
- invalid occurrence is rejected;
- occurrence semantics are deterministic;
- omitted occurrence follows the existing documented behavior;
- structural validation does not read the file.

### Insert

Represent:

- path;
- line/boundary;
- content.

Preserve the existing line convention. If the current implementation defines zero as beginning-of-file and positive values as line boundaries, retain that contract.

Reject invalid path and empty content according to the current model contract.

### DeleteRange

Represent inclusive start and end lines.

Reject invalid zero values where one-based indexing is required and reject start greater than end.

### Patch

Represent a targeted expected old value and replacement value. The model represents the request; later executor code performs matching and writing.

### ApplyDiff

Represent a unified diff payload. Do not implement diff parsing/application in Prompt 04.

---

## 8. Path contract

Edit operation paths are workspace-relative logical resources.

At model validation, reject unsafe input according to the existing canonical path validator:

- empty path;
- absolute path;
- traversal;
- control characters;
- malformed path.

Do not silently rewrite unsafe input into another identity.

Do not duplicate filesystem containment logic.

Do not resolve a model path into a host path.

A path does not grant permission and is not a workspace identity.

---

## 9. ExpectedState

Maintain one ExpectedState contract containing the applicable:

- exact content hash;
- context;
- expected byte size;
- expected line count.

### Hash

A supplied hash represents exact expected content.

Preserve the existing SHA-256 hexadecimal representation and validation rules.

Malformed hash input is a malformed transaction, not an ordinary mismatch.

### Size

Expected size is the observed file size expected at the transaction boundary.

A mismatch is a conflict.

### Line count

Expected line count uses the repository's canonical line-count semantics.

A mismatch is a conflict.

### Context

Context is location-sensitive.

A context string existing somewhere in a file is not sufficient evidence that the intended edit location is unchanged.

The model may return ContextPending when location-specific live-content validation is required.

Never convert ContextPending to Matched without executor-level evidence.

---

## 10. Expected-state matching

Preserve deterministic evaluation:

~~~text
structural validation
  ↓
hash format validation
  ↓
hash comparison
  ↓
size comparison
  ↓
line-count comparison
  ↓
context pending/location-specific validation
~~~

The exact existing first-failure behavior must remain stable unless a concrete bug requires correction.

Important invariants:

~~~text
malformed hash ≠ ordinary conflict
context pending ≠ matched
mismatch ≠ safe to apply
~~~

Never design the model to silently overwrite stale state.

---

## 11. StateMatch

Preserve the canonical result distinction:

~~~text
Matched
Conflicted
ContextPending
Malformed
~~~

Matched means every model-verifiable supplied precondition holds and no unresolved context remains.

Conflicted means a supplied observable precondition does not match.

ContextPending means structural checks pass but location-specific context still requires live-content resolution.

Malformed means supplied expected state is structurally invalid.

Do not collapse these into one boolean.

---

## 12. FileState

Maintain one immutable observed file state containing:

- logical path;
- exact content hash;
- byte size;
- line count.

FileState is observation data.

It is not:

- a snapshot record;
- a rollback record;
- a Git record;
- an audit event;
- an authorization decision.

Future systems may reference FileState, but they own their own persistence.

---

## 13. Before/after state

A transaction may carry before and after FileState collections.

Rules:

- preserve operation/resource correlation;
- do not infer a snapshot merely from before-state;
- do not infer provenance merely from after-state;
- do not read files merely to construct the model;
- do not persist state from the model;
- do not silently discard multi-file observations.

---

## 14. Multi-operation transactions

One transaction may contain multiple operations.

Requirements:

- preserve input order;
- never reorder implicitly;
- validate every operation;
- reject the whole structural transaction if any operation is malformed;
- preserve order through serialization;
- do not silently deduplicate paths;
- do not silently merge operations targeting one file.

The transaction is the future unit of execution atomicity, but Prompt 04 does not implement atomic execution.

---

## 15. Expected-state cardinality

Preserve the existing contract.

If expected state is supplied as one entry per operation, reject any non-empty vector whose length differs from operations.

Do not silently broadcast one expected state across multiple operations.

An empty expected-state collection must retain its explicit meaning under the existing model.

---

## 16. EditIdentity

Maintain one optional correlation structure for:

~~~text
agent_id
session_id
workspace_id
~~~

Rules:

- identity is correlation, not authorization;
- absent values remain absent;
- supplied empty values fail validation;
- use existing typed identity primitives where compatible;
- do not invent identity;
- do not resolve AgentRegistry or AgentSession from this module.

Never substitute:

- MCP client name;
- URL;
- HTTP header;
- display name;
- PID;
- current working directory

for canonical AWH identity.

Do not duplicate Prompt 02's identity/registry/session subsystem.

---

## 17. EditRefs

Maintain optional foreign references for:

~~~text
snapshot_id
provenance_id
audit_event_id
policy_decision_id
~~~

These are correlation references only.

Prompt 04 does not:

- create snapshots;
- write provenance;
- persist audit;
- evaluate policy;
- create policy decisions.

Validate only the local structural safety of supplied references.

---

## 18. EditStatus

Preserve the canonical lifecycle vocabulary:

~~~text
Requested
Authorized
Located
Validated
Snapshotted
Applied
Verified
Committed
Rejected
Conflict
ValidationFailed
ApplyFailed
VerificationFailed
RolledBack
~~~

Successful flow:

~~~text
Requested
 → Authorized
 → Located
 → Validated
 → Snapshotted
 → Applied
 → Verified
 → Committed
~~~

The current model may also permit Validated → Applied because snapshots belong to a later milestone. Do not implement snapshots merely to force the state machine through Snapshotted.

Failure outcomes include:

~~~text
Requested/Authorized/Located → Rejected
Validated → Conflict
Validated → ValidationFailed
Applied → ApplyFailed
Verified → VerificationFailed
post-snapshot/apply/verification → RolledBack
~~~

Preserve the current legal-transition table unless a concrete defect is demonstrated.

---

## 19. Terminal states

Terminal states include:

~~~text
Committed
Rejected
Conflict
ValidationFailed
ApplyFailed
VerificationFailed
RolledBack
~~~

A terminal state cannot transition again.

Do not permit:

~~~text
Committed → Requested
Rejected → Applied
Conflict → Applied
RolledBack → Committed
~~~

Retry is a new transaction with a new EditId.

---

## 20. Transition validation

The transition predicate must be:

- pure;
- deterministic;
- side-effect free;
- independently testable.

It answers only whether a transition is structurally legal.

It must not:

- authorize;
- read files;
- create snapshots;
- execute edits;
- perform rollback;
- emit audit events.

Representative illegal transitions:

~~~text
Requested → Applied
Requested → Committed
Authorized → Applied
Validated → Committed
Committed → Requested
Rejected → Applied
Conflict → Applied
RolledBack → Committed
~~~

---

## 21. EditError

Reuse the existing structured EditError.

It must distinguish applicable structural conditions such as:

- empty transaction;
- expected-state count mismatch;
- invalid path;
- empty match;
- invalid occurrence;
- invalid line range;
- empty insert content;
- invalid hash;
- empty context;
- invalid identity/reference;
- illegal status transition;
- expected-state conflict.

Do not flatten model errors into generic strings.

Errors must not expose secrets, credentials, full file contents, or unnecessary host paths.

Do not move executor-specific errors into this model merely for convenience.

---

## 22. Error ownership

Prompt 04 owns model-level errors.

Filesystem/edit executor owns:

- file-not-found;
- permission failures;
- workspace containment;
- symlink handling;
- I/O;
- location-specific context matching;
- atomic write failure.

Authorization owns:

- capability denial;
- policy denial;
- trust denial;
- agent/session authorization.

Snapshot layer owns:

- snapshot creation;
- snapshot persistence;
- restore failure.

Audit layer owns:

- audit persistence.

Do not create a single generic EditError for all future subsystems.

---

## 23. Structural validation purity

The canonical structural validation function must never:

- open files;
- read directories;
- canonicalize host paths;
- mutate files;
- invoke Git;
- invoke MCP;
- inspect capability grants;
- evaluate policy;
- create snapshots;
- write audit;
- spawn processes;
- resolve secrets.

It must be safe to call repeatedly.

Same valid input must produce the same result.

---

## 24. Serialization contract

All canonical public/domain types must round-trip:

~~~text
value
 → serialize
 → deserialize
 → equivalent value
~~~

Cover:

- every operation;
- expected state;
- file state;
- identity;
- references;
- every status;
- transaction;
- structured serializable results/errors.

Preserve existing serde names, tags, and field behavior unless a compatibility bug is demonstrated.

Any intentional wire-format change must be documented and tested.

---

## 25. Backward compatibility

Before changing an existing field, enum variant, tag, or validation rule:

1. search callers;
2. inspect fixtures;
3. inspect tests;
4. determine whether external consumers exist;
5. preserve compatibility when practical.

Do not remove an existing operation because a later executor is not ready.

Do not add speculative future fields without a concrete contract need.

---

## 26. Duplicate-model prevention

Search for all edit-like domain types:

~~~text
Edit
EditRequest
PatchRequest
FilePatch
Mutation
FileMutation
EditOperation
EditTransaction
ExpectedState
FileState
~~~

Classify every duplicate as:

- canonical domain type;
- transport DTO;
- compatibility type;
- legacy/dead type.

Where practical, adapt DTOs to the canonical model.

Do not maintain two competing canonical models.

Desired architecture:

~~~text
MCP DTO ─┐
CLI args ├→ canonical EditTransaction
TUI form ┤
API DTO ─┘
~~~

Do not create separate domain semantics for each interface.

---

## 27. Interface independence

The canonical model must not depend on:

- Axum;
- HTTP/SSE;
- MCP protocol types;
- CLI parser types;
- ratatui;
- Control API handlers;
- database clients;
- transport-specific errors;
- Git command wrappers.

It may use stable domain primitives already owned by core/services.

The model should be testable without a network or async runtime.

---

## 28. Workspace identity and path

Keep these concepts separate:

~~~text
workspace_id = AWH runtime identity
path         = resource inside that workspace
~~~

Do not derive workspace_id from a path.

Do not derive authorization from a path.

Do not resolve the path into a host filesystem path in Prompt 04.

---

## 29. Context semantics

Context preconditions exist to prevent stale edits.

A context string must be tied to intended edit location by the later executor.

Potential location evidence may come from:

- operation type;
- old text;
- line boundary;
- occurrence;
- patch hunk.

Prompt 04 represents the precondition but does not implement live-file location resolution.

Tests must ensure that the model never calls context globally sufficient merely because the string exists somewhere.

---

## 30. Hard no-mutation boundary

Do not add any real mutation operation to Prompt 04.

Do not implement:

~~~text
filesystem.patch
filesystem.apply_diff
fs replace
fs insert
fs delete-range
EditService executor
atomic write
Git mutation
MCP tool execution
~~~

The model may describe operations. It must not perform them.

---

## 31. Hard no-snapshot boundary

Do not implement:

- snapshot store;
- snapshot directory;
- snapshot creation;
- restore;
- undo;
- workspace history.

Snapshotted status and snapshot_id may exist as vocabulary because later systems depend on the transaction contract.

---

## 32. Hard no-authorization boundary

Do not implement:

- capability lookup;
- policy evaluation;
- trust checks;
- AgentRegistry resolution;
- AgentSession creation;
- MCP authentication.

Identity and policy decision references are correlation data only.

---

## 33. Hard no-audit/provenance boundary

Do not implement persistent audit or provenance.

The relationship is:

~~~text
EditTransaction ≠ AuditEvent
EditTransaction ≠ ProvenanceRecord
EditTransaction ≠ SnapshotRecord
~~~

Future systems may reference EditId.

---

## 34. Linear implementation sequence

### Step 1 — Baseline

Inspect branch, documentation, current model, callers, duplicate types, and tests.

### Step 2 — Ownership map

Confirm ownership of:

- edit types;
- path validation;
- identity primitives;
- structured errors;
- serialization.

Reuse existing owners.

### Step 3 — Operation vocabulary

Verify Replace, Insert, DeleteRange, Patch, ApplyDiff.

### Step 4 — Expected-state contract

Verify hash, context, size, line count, deterministic matching, malformed input, and conflict classification.

### Step 5 — Observed state

Verify FileState path/hash/size/line-count semantics.

### Step 6 — Identity/references

Verify EditIdentity and EditRefs are correlation-only and structurally validated.

### Step 7 — Lifecycle

Verify legal transitions and terminal behavior.

### Step 8 — Errors

Verify structured EditError categories and safe messages.

### Step 9 — Serialization

Run round trips and inspect fixtures/callers.

### Step 10 — Adversarial model tests

Cover malformed paths, hashes, ranges, occurrences, cardinality, identities, references, conflicts, illegal transitions, and terminal reuse.

### Step 11 — Duplicate audit

Repeat repository-wide search and remove/avoid competing canonical domain models within scope.

### Step 12 — Verification

Run all applicable gates.

### Step 13 — Scope audit

Confirm no executor, snapshot, rollback, authorization, MCP tool, or CLI editing feature was added.

---

## 35. Required tests

### EditId

- generated ID is non-empty;
- repeated generation produces distinct IDs under normal operation;
- serde/display/equality/hash behavior.

Do not assert unstable internal timestamp representation.

### Replace

- valid operation;
- omitted occurrence;
- explicit occurrence;
- zero occurrence rejected where one-based;
- empty match rejected;
- invalid path rejected.

### Insert

- valid boundary;
- beginning-of-file semantics;
- valid content;
- empty content rejected;
- invalid path rejected.

### DeleteRange

- valid range;
- equal boundaries;
- invalid zero boundary where one-based;
- start greater than end;
- invalid path.

### Patch

- valid payload;
- invalid path;
- empty expected match;
- serde round trip.

### ApplyDiff

- valid non-empty payload;
- serde round trip;
- no execution.

---

## 36. Expected-state tests

Test:

- matching hash;
- mismatching hash;
- malformed hash;
- matching size;
- mismatching size;
- matching line count;
- mismatching line count;
- no expected state;
- context pending;
- empty context;
- deterministic first-failure behavior.

Required distinctions:

~~~text
malformed hash ≠ conflict
context pending ≠ matched
~~~

---

## 37. FileState tests

Using in-memory content, test:

- deterministic SHA-256;
- byte-size semantics;
- canonical line-count semantics;
- path preservation;
- serde round trip.

No real filesystem I/O is required.

---

## 38. Identity/reference tests

Test:

- absent identity remains absent where allowed;
- valid identifiers round-trip;
- empty identifiers fail;
- unsafe control characters fail;
- unsafe references fail according to shared identifier rules;
- refs remain opaque;
- no identity/reference automatically authorizes an operation.

---

## 39. Lifecycle tests

Test every legal transition.

Also test representative illegal transitions:

~~~text
Requested → Applied
Requested → Committed
Authorized → Applied
Validated → Committed
Committed → Requested
Rejected → Applied
Conflict → Applied
RolledBack → Committed
~~~

Test terminal detection and terminal immutability.

---

## 40. Multi-operation tests

Test:

- single operation;
- multiple operations;
- order preservation;
- all operations validate;
- one malformed operation rejects the transaction;
- expected-state cardinality;
- serde order preservation.

Do not require a real executor.

---

## 41. Property-style invariants

If the repository already uses proptest or equivalent, add focused properties:

1. valid transaction serde round trip preserves structure;
2. terminal statuses never permit transitions;
3. malformed hashes never produce Matched;
4. hash mismatch never produces Matched;
5. operation order survives serialization;
6. invalid expected-state cardinality never validates;
7. structural validation causes no filesystem mutation;
8. generated EditId is non-empty.

Do not add a large new property-testing framework solely for this prompt.

---

## 42. Security tests

Reject:

- absolute paths;
- traversal;
- control characters;
- empty paths;
- malformed hashes;
- invalid ranges;
- invalid occurrence;
- empty match;
- empty context;
- invalid identity/reference values.

Error output must not expose credentials, secrets, complete file contents, or unnecessary host paths.

The model is not the authorization engine; these are input-hardening tests only.

---

## 43. Purity tests

A test must be able to construct a transaction entirely in memory, then:

1. validate shape;
2. compare expected state;
3. serialize/deserialize;
4. validate status transitions;

without:

- creating/modifying files;
- Git;
- MCP;
- network;
- spawned processes;
- an async runtime.

---

## 44. Compatibility tests

Preserve existing edit tests.

If an inconsistency is found:

1. identify the exact current contract;
2. make the smallest correction;
3. add regression coverage;
4. document compatibility impact.

Do not delete tests merely because execution belongs to later prompts.

---

## 45. Documentation alignment

Only change documentation when necessary to reflect the canonical Prompt 04 contract.

Do not modify other implementation prompts.

Do not duplicate an already-canonical definition in multiple documents.

The final report must identify documentation inspected and any documentation actually changed.

---

## 46. Scope boundary

### Owns

- EditId;
- EditOperation;
- ExpectedState;
- FileState;
- StateMatch;
- EditIdentity;
- EditRefs;
- EditStatus;
- EditTransaction;
- EditError;
- pure structural validation;
- deterministic state comparison;
- lifecycle transition validation;
- serialization compatibility;
- model-focused tests.

### Does not own

- file mutation;
- edit execution;
- diff execution;
- atomic filesystem writes;
- live context matching;
- snapshots;
- rollback;
- provenance persistence;
- persistent audit;
- MCP editing tools;
- CLI/TUI/API editing;
- capability enforcement;
- policy evaluation;
- agent routing/session management;
- worktrees;
- multi-agent coordination.

If an out-of-scope subsystem blocks a small type-level change, use the smallest compatibility boundary and report the limitation. Do not implement the subsystem.

---

## 47. Independence rule

Prompt 04 must work against the current repository regardless of whether other prompts have been merged.

Do not require:

- Prompt 01;
- Prompt 02;
- Prompt 03;
- Prompt 05;
- any future PR;
- a prescribed merge order.

Inspect the current code and reuse what exists.

Do not write hidden dependency instructions such as "after Prompt 03" or "once Prompt 05 lands."

---

## 48. No premature completion claims

Do not claim that any of these exist merely because the model exists:

- executable edits;
- atomic edits;
- snapshots;
- rollback;
- provenance;
- persistent audit;
- MCP patching;
- CLI patching;
- authorization enforcement.

Keep the distinction explicit:

~~~text
transaction model
  ≠ execution
  ≠ snapshot
  ≠ rollback
  ≠ audit
  ≠ authorization
~~~

---

## 49. Verification gates

Run:

~~~text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
~~~

Also run focused edit tests and existing property tests where available.

If a gate cannot run, report NOT VERIFIED with the exact reason.

Never claim a passing result that was not executed.

---

## 50. Completion criteria

Prompt 04 is complete only when:

- exactly one canonical edit-domain vocabulary exists;
- existing canonical types are reused;
- all five operation kinds are represented;
- expected-state semantics are deterministic;
- malformed expected state fails closed;
- context is never falsely reported as matched;
- FileState remains immutable observation data;
- multi-operation order is preserved;
- expected-state cardinality is deterministic;
- identity and refs remain correlation-only;
- lifecycle transitions are explicit;
- terminal states cannot transition;
- structured errors are preserved;
- serialization round trips pass;
- malformed-input tests pass;
- adversarial model tests pass;
- structural validation causes no filesystem mutation;
- no executor was introduced;
- no snapshot/rollback implementation was introduced;
- no authorization implementation was introduced;
- no MCP/CLI/TUI/API editing surface was introduced;
- no unexplained duplicate canonical model remains;
- verification gates pass or are explicitly marked NOT VERIFIED;
- scope remains limited to Prompt 04.

Do not claim the broader AWE editing milestone is complete.

---

## 51. Required final report

Report:

### Prompt 04 — Result

~~~text
Status: COMPLETE / PARTIAL / BLOCKED
Baseline commit:
Final commit:
~~~

### Canonical model

List the canonical types and state which were reused or changed.

### Semantics

Describe:

- operation semantics;
- path rules;
- expected-state matching;
- context behavior;
- multi-operation ordering;
- identity/reference behavior;
- lifecycle transitions.

### Compatibility

State:

- serialization impact;
- affected callers;
- preserved tests;
- duplicate models found.

### Tests

List:

- unit tests;
- expected-state tests;
- lifecycle tests;
- serialization tests;
- adversarial tests;
- property tests if applicable;
- purity/no-mutation tests.

### Verification

Give exact results for:

~~~text
cargo fmt
cargo check
cargo test
cargo clippy
git diff --check
~~~

### Files changed

List every changed file and purpose.

### Limitations

Explicitly state that Prompt 04 does not implement:

- edit execution;
- snapshots;
- rollback;
- persistent audit/provenance;
- MCP/CLI/TUI/API editing;
- authorization.

### Scope confirmation

Explicitly confirm that no executor, snapshot store, rollback engine, authorization engine, duplicate transport-specific edit model, or unrelated Trust Wedge feature was introduced.
