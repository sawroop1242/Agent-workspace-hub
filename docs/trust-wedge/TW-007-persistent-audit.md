# TW-007 — Persistent Structured Trust-Wedge Audit

## Master implementation prompt

Extend the existing audit choke point into durable, structured, restart-safe Trust-Wedge provenance. This issue is standalone and must work against the current `rust` branch.

### 1. Repository forensics

Inspect:

- `src/services/audit.rs`;
- `src/mcp/audit.rs`;
- `src/mcp/observability.rs`;
- authorization;
- agent/session lifecycle;
- edit;
- snapshot;
- rollback;
- persistence/storage;
- current tests and documentation.

Search for every audit/event writer and identify the canonical audit boundary. Do not create a second audit abstraction.

### 2. Audit event contract

Use existing event/ID types where possible.

Minimum applicable structured fields:

- event ID;
- timestamp;
- agent ID;
- session ID;
- workspace ID;
- action;
- tool/command;
- authorization decision;
- result/outcome;
- edit ID;
- snapshot ID;
- rollback/correlation ID where applicable;
- path/resource reference;
- before/after hashes;
- duration;
- structured error code.

Fields that do not apply to an event should be absent/null according to the existing serialization convention rather than filled with misleading values.

### 3. Required lifecycle coverage

The canonical audit boundary must support, where applicable:

- agent registration/activation/deactivation/session lifecycle;
- authorization allow/deny;
- MCP/tool execution outcome;
- edit start/success/failure;
- snapshot creation/failure;
- rollback start/success/failure;
- verification success/failure;
- conflict detection/refusal;
- persistence/recovery failures.

Do not duplicate event emission in every transport adapter.

### 4. Durable source of truth

Use a durable append-oriented logical source of truth compatible with the repository's persistence model.

An in-memory ring/cache may remain for fast observability, but it must not be the only source of audit history.

Define:

- atomic/consistent append behavior;
- locking/concurrency behavior;
- restart recovery;
- retention/rotation semantics;
- corruption detection;
- malformed/truncated record handling;
- behavior when audit storage is unavailable.

Do not silently replace corrupt authoritative audit state with an empty history.

### 5. Audit availability policy

Explicitly determine which operations are allowed when durable audit persistence is unavailable.

For security-sensitive consequential operations, do not silently claim an auditable success if the repository's Trust-Wedge contract requires durable audit. Fail closed where required by that contract.

For non-security-critical telemetry, preserve existing availability semantics.

Document the chosen boundary and test it.

### 6. Privacy and security

Audit must be useful without becoming a data-leak channel.

Never log:

- raw file contents;
- secret values;
- capability tokens;
- credentials;
- unnecessary request payloads.

Prefer:

- stable IDs;
- action/resource metadata;
- hashes;
- structured error codes;
- redacted/sanitized metadata.

Audit records themselves must remain inside the intended AWH persistence/security boundary.

### 7. Query and reconstruction

Support query/filter semantics through the existing audit interface for:

- agent;
- session;
- workspace;
- edit;
- snapshot;
- rollback/correlation ID;
- action/outcome.

The records must be sufficient to reconstruct a Trust-Wedge lifecycle without relying on ephemeral process logs.

Do not expose an unrestricted remote audit endpoint merely to make querying convenient.

### 8. Tests

Cover with durable storage:

- event persistence;
- restart and reload;
- identity propagation;
- authorization allow/deny;
- edit success/failure;
- snapshot linkage;
- rollback linkage;
- before/after hashes;
- redaction;
- filters;
- malformed/corrupt data;
- concurrent appends where applicable;
- lifecycle reconstruction;
- audit-unavailable behavior.

Use real temporary persistence for integration tests.

### 9. Security gates

Verify:

- one canonical audit choke point;
- consequential actions cannot bypass required audit;
- sensitive content is redacted;
- event identity cannot be forged by arbitrary transport metadata;
- corrupt history does not silently become an empty valid history;
- retention/rotation cannot accidentally delete required active provenance;
- audit querying respects existing authorization boundaries.

### 10. Definition of done

Complete only when:

- production lifecycle paths emit structured events through the canonical boundary;
- durable history survives restart;
- failure/corruption semantics are explicit;
- query/filter behavior is tested;
- security-sensitive data is not exposed;
- relevant formatting, compilation, tests, and security checks pass;
- documentation matches actual behavior;
- no unrelated audit refactor is included.

Final report must list changed files, canonical audit boundary, event schema decisions, durability/failure policy, tests, commands/results, and remaining limitations.

### Non-goals

Do not create a second audit system, add unrestricted remote telemetry, redesign unrelated logging, or implement external-agent orchestration.