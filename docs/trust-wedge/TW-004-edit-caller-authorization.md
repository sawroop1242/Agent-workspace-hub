# TW-004 — EditService Caller Identity + Authorization

## Master implementation prompt

Integrate caller identity and pre-mutation authorization into the existing canonical EditService. This is a standalone Trust-Wedge issue and must not require another unmerged TW issue or PR.

### 1. Repository forensics

Read the current `src/services/edit.rs` completely before editing. Then inspect:

- authorization/capability/policy services;
- MCP execution gate and tool broker;
- CLI edit adapters;
- filesystem/path security;
- workspace/project boundaries;
- snapshot/provenance hooks;
- current edit models and IDs;
- existing tests and error conventions.

Search for every edit entry point. Identify the canonical mutation service and preserve it. Do not create a second editor.

### 2. Canonical edit flow

All consequential edits must follow one security-critical sequence:

`caller context -> authorization -> prepare/resolve target -> expected-state validation -> snapshot/provenance hook -> mutation -> verification -> result/audit hook`

Authorization must occur before mutation.

The exact existing service architecture may differ; adapt to it without weakening the invariant.

### 3. Caller identity contract

Reuse existing identity types where possible. An edit should carry, where available and applicable:

- Edit ID;
- agent ID;
- session ID;
- workspace ID;
- task/correlation ID;
- target path/resource;
- operation type.

Identity is provenance, not authorization. Authorization must still be evaluated against the caller's actual capability/policy context.

MCP and CLI adapters must converge on the same EditService and authorization semantics.

### 4. Authorization invariants

A consequential edit must be rejected before mutation when:

- caller is unknown/invalid;
- session is inactive or does not belong to the agent/workspace;
- capability is missing, expired, or out of scope;
- policy denies the operation;
- target is outside the authorized workspace/scope;
- expected state is stale or otherwise unsafe.

For every denied case, prove that protected file bytes and relevant metadata remain unchanged.

Do not treat successful path parsing or a route name as permission.

### 5. Safe mutation semantics

Before writing:

- resolve the target under the authorized workspace;
- reject unsafe traversal/escape;
- validate expected state where the edit contract requires it;
- detect conflicts/stale state;
- integrate with the canonical snapshot/provenance mechanism when required;
- use the existing safe-write/atomic mutation behavior.

Never fall back to an unconditional full-file overwrite merely because a structured edit cannot be applied.

After mutation, verify the expected resulting state before reporting success.

A failed verification must not be reported as a successful edit.

### 6. Provenance boundary

The edit service should expose enough structured information for the snapshot/audit subsystem:

- stable edit ID;
- caller identity;
- workspace;
- target path;
- operation;
- before/after hashes when available;
- snapshot linkage when created;
- result/error/conflict state.

Do not implement a second audit or snapshot store inside EditService.

### 7. Concurrency and conflict behavior

Account for:

- another writer changing the file between prepare and apply;
- expected-state mismatch;
- file disappearance/replacement;
- permission/read-only errors;
- path changes or workspace changes;
- concurrent edits where the existing service supports locking/versioning.

Unsafe concurrent edits must fail closed rather than silently overwrite newer state.

### 8. Tests

Use real temporary files and the production edit service for:

- authorized edit success;
- missing capability;
- expired capability;
- policy denial;
- wrong workspace/scope;
- inactive/invalid session;
- stale expected state;
- concurrent/external modification;
- successful Edit ID/caller propagation;
- post-write verification;
- failed edit proving no unintended mutation;
- MCP/CLI adapter equivalence.

For every denial/conflict test, assert actual file bytes and relevant metadata, not only returned errors.

### 9. Security gates

Verify:

- authorization occurs before mutation;
- one canonical EditService remains;
- no duplicate authorization gate with divergent rules;
- workspace containment is enforced;
- caller/agent/session/workspace identities cannot be substituted;
- no unrestricted default capability is introduced;
- logs/errors do not expose secrets or file contents;
- external/newer state is not silently overwritten.

### 10. Definition of done

Complete only when:

- production edit entry points use the canonical authorized flow;
- identity reaches edit results/provenance;
- positive and negative filesystem tests pass;
- stale/conflict behavior is demonstrated;
- relevant repository checks pass;
- documentation matches actual behavior;
- no unrelated refactor is included.

Final report must list changed files, canonical edit flow, authorization behavior, conflict guarantees, tests, commands/results, and remaining limitations.

### Non-goals

Do not redesign EditService, create another editor, implement durable snapshots/rollback/audit from scratch, add remote execution, or perform unrelated filesystem refactoring.