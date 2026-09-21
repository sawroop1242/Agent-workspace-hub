# TW-006 — Conflict-Aware Rollback + Recovery

## Master implementation prompt

Implement a first-class, conflict-aware rollback operation using the repository's canonical edit and file-snapshot state. This issue is standalone and must not require another unmerged TW issue or PR.

### 1. Repository forensics

Inspect before editing:

- EditService and edit state;
- file-snapshot/provenance service;
- workspace/path security;
- authorization/capability/policy checks;
- persistence/locking;
- audit/observability;
- CLI and MCP rollback entry points;
- current tests and error types.

Identify the authoritative edit/snapshot representations and reuse them. Do not create a second recovery subsystem.

### 2. Canonical rollback flow

Rollback must follow the safety sequence:

`resolve caller -> authorize rollback -> resolve edit -> resolve snapshot -> validate workspace/path -> validate current state -> detect conflict -> restore exact bytes when safe -> verify -> persist result/provenance -> audit`

Authorization must happen before consequential restoration.

The implementation may adapt the sequence to existing services, but it must preserve the safety properties.

### 3. Identity and correlation

Rollback needs its own stable correlation identity using existing ID types where available.

The result should retain linkage to:

- original Edit ID;
- Snapshot ID;
- agent ID;
- session ID;
- workspace ID;
- rollback/correlation ID;
- target path;
- outcome/conflict state.

Rollback is a new operation; it must not pretend that the original edit never occurred.

### 4. Recovery state validation

Before restoring bytes:

- resolve the requested edit;
- require the expected snapshot;
- verify snapshot integrity;
- verify the current file is still in the state for which rollback is safe;
- verify workspace/path containment;
- verify caller authority;
- acquire appropriate locks/version checks if supported.

If current state differs from the expected post-edit state, treat it as a conflict.

**Never blindly overwrite an externally modified/newer file.**

### 5. Conflict behavior

On conflict:

- return a structured conflict result/error;
- do not restore snapshot bytes;
- do not delete or modify the external changes;
- preserve the original snapshot;
- record the conflict through the canonical provenance/audit boundary.

A rejected rollback must be non-destructive.

### 6. Successful recovery

When all checks pass:

1. restore the exact snapshot bytes;
2. preserve required file metadata according to the snapshot contract;
3. verify the resulting bytes/hash;
4. report success only after verification;
5. persist rollback provenance/audit linkage.

If restoration or verification fails, return failure and do not claim recovery success.

Handle partial-write/IO failures using the repository's safe-write/recovery conventions.

### 7. CLI/runtime integration

Reconcile the current interface, for example:

`awh fs rollback <edit-id>`

Do not force this exact syntax if the repository already has an established command contract.

Keep rollback algorithms in services, not `main.rs`. MCP, CLI, and future control-plane adapters should converge on the same recovery service.

### 8. Failure and restart behavior

Define behavior for:

- missing edit;
- missing snapshot;
- corrupt snapshot;
- unauthorized caller;
- wrong workspace;
- stale/current-state mismatch;
- external modification;
- read-only/permission failure;
- lock contention;
- process restart;
- repeated rollback;
- already-recovered state.

Repeated calls must be deterministic and must not silently destroy newer state.

### 9. Tests

Use real temporary files and production services.

Mandatory tests:

- successful byte-identical restore;
- post-restore hash verification;
- missing edit;
- missing snapshot;
- corrupt snapshot;
- unauthorized rollback;
- wrong workspace/scope;
- external modification before rollback;
- non-destructive conflict handling;
- restart persistence;
- repeated/deterministic calls;
- failure injection where supported;
- audit/provenance linkage.

For conflicts, assert the externally modified bytes remain unchanged.

### 10. Security gates

Verify:

- rollback is authorized independently of route/command name;
- workspace containment is enforced;
- snapshot integrity is checked;
- current-state conflict is checked before write;
- denied/conflicted rollback causes zero protected mutation;
- no blind file-copy restore exists;
- secrets/file contents are not leaked in errors or logs;
- only the canonical snapshot/edit services are used.

### 11. Definition of done

Complete only when:

- the real runtime exposes the canonical rollback service;
- safe restoration and conflict refusal are executable;
- restart/persistence behavior is tested;
- positive and negative filesystem tests pass;
- audit/provenance linkage exists through the canonical boundary;
- repository checks pass;
- docs match actual behavior;
- no unrelated TW work is included.

Final report must list changed files, rollback flow, conflict invariant, tests, commands/results, and remaining limitations.

### Non-goals

Do not redesign EditService, replace the snapshot system, create a second audit system, add remote execution, or implement unrelated filesystem features.