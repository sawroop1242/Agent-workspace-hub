# Prompt 10 — Structured Persistent Audit (AWE-013 / TW-007)

## Mission

Implement and harden one canonical, durable, structured, correlated audit subsystem for consequential AWH operations.

AWH's product promise includes accountability: after an agent, operator, MCP client, or service performs a consequential operation, the system must be able to explain what happened, when it happened, who/what initiated it, which workspace/resource/operation was affected, what authorization decision applied, and whether the operation succeeded, failed, conflicted, or was denied.

The current rust branch has an in-memory bounded `src/services/audit.rs` ring buffer and additional MCP/Control API audit call sites. That implementation is useful as an existing security-event vocabulary and redaction choke point, but it is not a durable audit history. Prompt 10 must evolve the existing audit boundary into one persistent implementation without creating a second event store.

This prompt is standalone. The current `rust` branch is the source of truth. Do not require another implementation-prompt PR, branch, or merge order.

---

# 1. Product boundary

AWH is an agent-agnostic, local-first workspace runtime for coding agents.

AWH owns:

- workspace/filesystem state;
- controlled edits;
- capabilities and policy;
- snapshots/provenance;
- rollback/recovery;
- agent/session runtime identity;
- tasks/runtime state;
- audit and observability;
- MCP, CLI, TUI, and Control API interfaces.

External agents own:

- reasoning;
- planning;
- model/provider selection;
- agent intelligence;
- agent-specific orchestration.

Audit is an accountability boundary, not an authorization engine, filesystem safety engine, snapshot store, or telemetry analytics platform.

The audit flow is:

    consequential request
          ↓
    trusted service boundary
          ↓
    operation result / authorization decision
          ↓
    canonical structured audit event
          ↓
    security filtering
          ↓
    durable append
          ↓
    query/read boundary

Audit records what the system decided and what actually happened. It must never be used to decide whether a future operation is authorized.

---

# 2. Scope

Prompt 10 owns:

1. one canonical persistent audit event model;
2. one canonical audit writer/store;
3. durable append semantics;
4. deterministic event identity and ordering;
5. schema versioning;
6. corruption detection and fail-safe loading;
7. secret/sensitive-data filtering;
8. workspace/resource isolation in queries;
9. service-boundary integration for consequential operations;
10. successful, denied, failed, conflict, and relevant security outcomes;
11. restart persistence;
12. concurrent writer safety;
13. bounded resource usage;
14. diagnostic query/read behavior;
15. migration/compatibility from the current in-memory audit implementation;
16. tests proving durability, integrity, ordering, correlation, filtering, and isolation.

Prompt 10 does not own:

- authorization decisions;
- capability evaluation;
- policy evaluation;
- agent identity/session creation;
- edit execution;
- rollback execution;
- snapshot storage;
- provenance storage;
- MCP authentication;
- MCP trust;
- Control API authentication;
- generic application logging;
- metrics/telemetry analytics;
- remote SIEM/export infrastructure;
- Git history;
- distributed event streaming;
- a second audit/event store.

---

# 3. Required repository forensics

Before implementation, read the current repository rather than relying on historical prompt text.

## 3.1 Product and roadmap

Read:

- `docs/roadmap/GROWTH_STRATEGY.md`
- `docs/roadmap/PROJECT_ROADMAP.md`
- `docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md`
- `docs/FEATURES.md`
- `docs/PROJECT_CONTEXT.md`
- `docs/architecture.md`
- `docs/security.md`
- `docs/threat-model.md`

Extract:

- AWH's product boundary;
- audit's position in the roadmap;
- Trust Wedge expectations;
- existing security/audit guarantees;
- CLI/MCP/Control API expectations;
- persistence conventions;
- phase-exit and verification rules.

The roadmap explicitly places audit/observability after snapshots/provenance and before later runtime features. Do not move unrelated roadmap scope into this prompt.

## 3.2 Implementation-prompt collection

Read:

- `docs/implementation-prompts/README.md`
- Prompt 01;
- Prompt 02;
- Prompt 03;
- Prompt 04;
- Prompt 05;
- Prompt 06;
- Prompt 07;
- Prompt 08;
- Prompt 09;
- Prompt 10;
- Prompt 11 when present;
- Prompt 12;
- Prompt 13–17 when present.

Pay particular attention to:

- Prompt 06 edit safety;
- Prompt 07 authorization;
- Prompt 08 snapshots/provenance;
- Prompt 09 rollback/recovery.

Those prompts own adjacent security and recovery boundaries. Prompt 10 must consume their structured outcomes rather than duplicate their implementation.

The README maps Prompt 10 to `AWE-013 + TW-007 persistent audit`. Preserve that ownership.

## 3.3 Historical Trust Wedge and issue-resolving material

Where the historical material is available, inspect relevant documents under:

- `docs/trust-wedge/`
- `docs/issue-resolving-prompts/`

Search for:

- AWE-013;
- TW-007;
- persistent audit;
- AuditLog;
- audit event;
- event ID;
- sequence;
- append;
- durability;
- corruption;
- redaction;
- provenance;
- authorization decision;
- rollback;
- conflict;
- query;
- retention.

Historical documents are evidence, not authority. If they conflict with current source or consolidated prompts, the current rust branch wins.

If the historical directories have already been consolidated away, record that fact and use the surviving search/index references plus current source instead of inventing historical content.

## 3.4 Current audit implementation

Inspect at minimum:

- `src/services/audit.rs`;
- `src/services/mod.rs`;
- `src/mcp/audit.rs` when present;
- `src/mcp/dispatcher.rs`;
- MCP security/authentication modules;
- Control API audit call sites;
- edit service;
- authorization service;
- snapshot/provenance service;
- rollback service;
- filesystem service;
- CLI audit/read surfaces;
- any TUI/control-plane audit surfaces;
- tests containing `AuditLog`, `record_allow`, `record_deny`, `audit_allow`, `audit_deny`, `AuditEntry`, `recent`, or audit endpoint/query behavior.

Classify each audit-related implementation as:

- canonical audit storage;
- audit adapter;
- security/transport event producer;
- tracing/logging;
- provenance;
- unrelated telemetry;
- duplicate audit mechanism.

Do not merge conceptually different security layers just because they contain the word “audit”.

---

# 4. Current repository contract to preserve

The current rust branch contains a shared `src/services/audit.rs` implementation with:

- `AuditEntry`;
- `AuditLog`;
- `global()`;
- `record_allow()`;
- `record_deny()`;
- `recent()`;
- `len()`;
- `is_empty()`;
- token-like redaction;
- a bounded 1000-entry in-memory ring.

It also contains MCP/Control API audit adapters/call sites.

Preserve useful established behavior while changing the persistence substrate:

- one shared service-layer audit boundary;
- structured rather than arbitrary free-form events;
- deny-side and allow-side events where security semantics require both;
- redaction at the canonical choke point;
- bounded resource use;
- newest-first diagnostic reads where that is already part of an API contract, unless a stable persistent query contract requires explicit ordering fields.

Do not preserve volatility as the final product guarantee.

The existing ring may become:

- an implementation detail/cache;
- a compatibility adapter;
- or be removed after its responsibilities are moved into the persistent store.

It must not remain a competing source of audit history.

---

# 5. Audit versus logging versus provenance

These three mechanisms must remain distinct.

## Audit

Answers:

> What consequential security/product operation occurred, who/what initiated it, what decision/result occurred, and when?

Audit is durable and queryable.

## Tracing/logging

Answers:

> What diagnostic information is useful while the process is running?

Tracing may remain volatile and high-volume. It is not the authoritative audit history.

## Provenance

Answers:

> How is an edit/snapshot/recovery artifact related to another artifact?

Prompt 08 owns provenance. Prompt 10 may correlate audit events with provenance IDs, but must not replace provenance with audit records.

Required relationship:

    edit transaction
       ├── snapshot/provenance
       └── audit event(s)

Do not store full snapshot bytes in audit.

---

# 6. Audit event ownership

The audit subsystem should record outcomes at authoritative service/security boundaries.

At minimum cover:

- authorization allow/deny for consequential mutations where the existing security contract requires it;
- edit success/failure/conflict;
- rollback success/failure/conflict/denial;
- snapshot/recovery security-relevant failure;
- MCP authentication/security denials;
- high-risk MCP tool invocation outcome where already defined;
- Control API authentication/security outcomes;
- other consequential operations already designated as auditable by current security/roadmap documentation.

Do not mechanically audit every function call.

An event should exist because it is useful for accountability or security investigation, not merely because a function was invoked.

Avoid double-recording the same operation at:

- transport;
- adapter;
- service;
- filesystem primitive.

Prefer the authoritative service/security boundary.

Transport-level authentication events can remain transport-owned because they describe authentication itself. A consequential filesystem mutation should not be represented only by an HTTP/MCP transport event.

---

# 7. Canonical audit event model

Define one versioned structured event.

The exact Rust field names may adapt to current source, but the conceptual schema must include enough information to reconstruct the event without exposing secrets.

Required fields:

- schema/version;
- event ID;
- event timestamp;
- stable ordering metadata;
- event kind/outcome;
- action/operation;
- actor/principal classification;
- agent identity when available;
- session identity when available;
- workspace identity when available;
- resource identifier/path when safe;
- edit/transaction identity when applicable;
- snapshot/provenance identity when applicable;
- task identity when applicable;
- authorization decision/result when applicable;
- stable reason/error code;
- correlation/request ID when available;
- non-secret structured metadata needed for diagnosis.

Do not require every field to be populated for every event.

Use explicit optional fields rather than inventing fake values such as:

- `unknown` agent IDs;
- empty strings that mean different things;
- timestamps reused as IDs;
- filesystem paths standing in for resource IDs.

---

# 8. Event identity

Every persisted event must have a stable unique EventId.

Requirements:

- generated before persistence;
- unique within the durable store;
- collision-resistant;
- serialized explicitly;
- never derived solely from wall-clock milliseconds;
- never reused after restart;
- not secret-bearing.

If the repository already has a canonical ID type suitable for audit events, reuse it. Otherwise introduce the smallest audit-specific ID type.

A duplicate EventId must not silently overwrite an existing event.

If an append encounters an ID collision:

- detect it;
- fail safely;
- preserve the existing event;
- do not replace history.

---

# 9. Deterministic ordering

Audit history needs an ordering model stronger than timestamps alone.

Wall-clock time can move backward, collide, or be unavailable.

Use a monotonic sequence/order value scoped to the persistent audit store, or another repository-consistent monotonic ordering mechanism.

Required properties:

- strictly increasing for accepted events in one store;
- persisted/recoverable across restart;
- no reuse of already-issued sequence numbers;
- deterministic newest/oldest ordering;
- concurrent writers cannot publish two events with the same order value.

If multiple workspaces share one physical audit store, define whether sequence ordering is global to the store or partitioned. Do not leave this ambiguous.

Timestamps remain useful metadata but must not be the sole ordering key.

---

# 10. Persistence boundary

Use the repository's established local durable-storage conventions.

Before implementing, inspect:

- snapshot persistence;
- workspace state storage;
- store locking;
- atomic write helpers;
- directory conventions;
- serialization/schema conventions.

Prefer the smallest storage mechanism that provides the required guarantees.

The audit store must survive normal process restart.

A successful audit append must not exist only in process memory.

Do not introduce a database merely because persistence is required if the repository's existing file-based durable store primitives can provide the needed guarantees.

Conversely, do not force a file-per-event design if it creates unbounded metadata overhead or contradicts established storage conventions.

The implementation must document the selected storage layout and why it is appropriate.

---

# 11. Durable append semantics

Audit history is append-oriented.

Normal operation must not silently rewrite prior events.

A successful append must:

1. construct a complete validated event;
2. apply security filtering;
3. assign event identity/order metadata;
4. serialize using the versioned schema;
5. persist through the canonical durable primitive;
6. make the new state recoverable;
7. return success only according to the actual durability guarantee.

If the selected store uses a manifest/index, update it atomically.

If it uses append-only records, ensure truncation/partial-record recovery is detectable.

Do not report an event as durably recorded before the implementation's stated durability point.

---

# 12. Crash consistency

Design for interruption at every persistence boundary.

Test crashes/faults:

- before event serialization;
- after serialization but before write;
- during write;
- after record write but before index/manifest update;
- after index/manifest update;
- during rotation/compaction if implemented;
- during startup recovery.

After restart:

- previously durable events remain readable;
- incomplete records are detected;
- history is not fabricated;
- valid earlier events remain intact;
- sequence ordering remains valid;
- EventId uniqueness remains valid;
- recovery either repairs safely or fails closed.

Do not claim transactional durability across arbitrary filesystem crashes unless the implementation actually provides it.

---

# 13. Corruption handling

Audit is evidence. Corruption must never silently become a false historical record.

Detect at minimum:

- malformed serialization;
- unknown incompatible schema;
- truncated records;
- invalid EventId;
- duplicate EventId;
- invalid sequence;
- sequence regression;
- impossible timestamp/metadata where validation is required;
- checksum/hash mismatch if the chosen storage format provides integrity metadata;
- invalid required fields;
- path/resource metadata that violates the current safe representation.

On corruption:

- never fabricate an event;
- never silently discard all history;
- preserve valid prior history where the format safely permits;
- expose a structured corruption condition;
- fail closed for operations that require authoritative audit integrity if the product contract says audit is mandatory;
- otherwise distinguish “operation completed but audit persistence failed” from “operation failed”.

The implementation must explicitly document which operations are audit-mandatory and which are best-effort.

---

# 14. Audit write failure semantics

Do not confuse audit failure with operation failure.

For a consequential mutation:

    operation result
          +
    audit append result

These are related but not automatically identical.

The prompt must define, from current product/security requirements, whether an audit write is:

- mandatory before mutation;
- mandatory after mutation;
- best-effort after mutation;
- or required only for specific security events.

A safe default for filesystem mutation is:

1. authorization and safety happen before mutation;
2. required pre-mutation audit decisions are recorded before mutation when the current contract requires them;
3. mutation proceeds only after required preconditions;
4. the final outcome is audited after the result is known;
5. if post-mutation audit persistence fails, do not falsely report that the filesystem mutation failed;
6. surface the audit durability failure separately and preserve enough local diagnostic information for recovery/retry.

If the current architecture has a stricter requirement, preserve it.

Never make the final filesystem state ambiguous merely because the audit sink failed.

---

# 15. Security filtering

The audit subsystem is a security boundary.

Never persist:

- bearer tokens;
- API keys;
- TLS private keys;
- secret values;
- environment secret values;
- raw Authorization headers;
- full file contents;
- full tool arguments when they may contain sensitive data;
- snapshot blobs;
- private credential payloads.

Potentially safe:

- stable actor/agent IDs;
- session IDs where the privacy/security contract permits;
- workspace IDs;
- logical workspace-relative paths;
- operation names;
- edit IDs;
- snapshot/provenance IDs;
- policy/capability result;
- stable error codes;
- bounded non-secret metadata.

Do not rely only on individual call sites to redact secrets.

Preserve the existing centralized redaction choke point from `src/services/audit.rs`, but strengthen it for structured fields.

Structured fields are preferable to free-form string concatenation because the implementation can classify sensitive fields explicitly.

---

# 16. Redaction design

Replace or supplement free-form redaction with field-aware filtering.

Classify event fields as:

### Always secret

- token;
- API key;
- password;
- private key;
- secret value;
- Authorization header;
- credential body.

### Potentially sensitive

- arbitrary tool arguments;
- request bodies;
- environment names/values;
- connector payloads;
- file content;
- command-line arguments.

### Safe by contract

- action slug;
- outcome;
- stable reason code;
- edit ID;
- snapshot ID;
- workspace-relative path where policy allows;
- bounded resource identifier.

When uncertain, omit rather than persist.

Audit should record enough metadata to investigate an event without becoming a data-exfiltration channel.

---

# 17. Resource/path privacy

A logical workspace-relative path can be useful for audit.

Do not persist unnecessary absolute host paths.

Do not leak:

- home-directory prefixes;
- temporary absolute paths;
- environment-specific filesystem layout;
- secret-bearing path components when avoidable.

If a resource is sensitive, use a stable logical resource identifier or redacted representation.

Audit queries must preserve workspace isolation.

---

# 18. Workspace isolation

A user querying audit history for workspace A must not receive workspace B's unrelated records unless the caller has an explicit administrative scope under the existing control-plane contract.

The audit store may physically contain events for multiple workspaces.

Therefore:

- every workspace-scoped event must carry a workspace identity where one exists;
- query APIs must accept/derive the caller's allowed workspace scope;
- filters must be applied before returning results;
- pagination must not leak hidden event counts or unrelated metadata where the current security contract forbids it;
- audit query authorization must reuse the existing Control API/operator policy boundary rather than creating a second policy engine.

A global operator view may be permitted if the existing product contract explicitly defines it.

---

# 19. Query/read contract

The persistent store must support useful diagnostic reads without becoming an analytics platform.

At minimum support the repository's existing audit-read needs:

- recent events;
- bounded limit;
- deterministic ordering;
- optional workspace filter;
- optional actor/agent filter where supported;
- optional action filter;
- optional outcome/kind filter;
- optional edit/event correlation IDs where supported.

Queries must have explicit resource bounds.

Never expose an unbounded “read all audit history into memory” operation.

If the CLI/API already has an `audit` endpoint, preserve its external behavior where compatible and route it through the persistent store.

Do not create separate query implementations for MCP, CLI, TUI, and Control API.

---

# 20. Pagination

If the existing or target query surface can return more than a bounded page, use a stable cursor/order model.

A page must not:

- duplicate events unexpectedly;
- skip events because of timestamp collisions;
- expose events added after the cursor in an ambiguous order;
- become an unbounded memory operation.

Prefer a cursor based on the monotonic audit sequence/order field.

Do not use array offsets against a mutable ring as the durable pagination contract.

---

# 21. Retention and bounded storage

Persistence creates a new resource-management problem.

The current ring is bounded to 1000 entries. A persistent audit history cannot be allowed to grow without a documented policy.

Before implementing cleanup, determine the current product contract.

Possible strategies:

- bounded event count;
- bounded byte size;
- time-based retention;
- explicit administrative archival;
- log rotation.

Do not silently delete audit history that the roadmap or security contract promises to retain.

If a bounded policy is already defined, implement it deterministically and test it.

If no destructive retention policy is currently defined, prefer a conservative implementation that preserves history and enforces resource limits at the ingestion/storage boundary rather than inventing arbitrary deletion semantics.

Any deletion/rotation must itself be auditable and must not rewrite the surviving history silently.

---

# 22. No audit-on-audit recursion

Audit operations themselves can produce audit events.

Avoid infinite recursion.

For example:

    audit.query
        → audit service
        → audit event
        → audit service
        → ...

Define explicitly which audit-management operations are audited and which are not.

A query normally should not recursively generate another persistent audit event unless the product contract requires it.

If audit export/delete/rotation is consequential, record a bounded management event without recursively auditing the audit writer.

---

# 23. Correlation model

Audit must correlate with existing AWH identity and transaction contracts.

Where available, include:

- workspace ID;
- agent ID;
- session ID;
- task ID;
- edit ID;
- snapshot ID;
- provenance ID;
- authorization decision ID;
- request/correlation ID;
- MCP session/request identifiers where safe.

Do not create duplicate identity/session/transaction systems just to make correlation possible.

Use optional fields when a particular event has no applicable value.

Correlation IDs are references, not authorization credentials.

---

# 24. Edit integration

Prompt 04/05/06 own the edit transaction and execution model.

Prompt 10 must record edit outcomes through the authoritative edit service boundary.

At minimum distinguish:

- edit requested where audit policy requires it;
- authorization denied;
- preparation failed;
- conflict;
- commit succeeded;
- verification failed;
- recovery/automatic rollback result;
- final failure.

Do not emit contradictory events from every internal operation.

Prefer one final authoritative outcome plus security decision events where needed.

The audit event must correlate to the exact EditId.

Do not store the full EditTransaction in the audit record.

---

# 25. Authorization integration

Prompt 07 owns authorization.

Prompt 10 records authorization decisions; it does not make them.

When authorization returns:

- Allow;
- Deny;
- InfrastructureUnavailable;
- PolicyDenied;
- CapabilityDenied;
- identity/session denial;

the audit event should contain the stable decision/reason code and safe identity/resource correlation.

Never make an authorization decision from an audit record.

Never call audit history to determine whether a caller has permission.

If audit persistence is unavailable, preserve the authorization fail-closed behavior of Prompt 07. Do not convert an authorization denial into allow because the audit store is unavailable.

---

# 26. Snapshot/provenance integration

Prompt 08 owns snapshots/provenance.

Audit may record:

- SnapshotId;
- provenance ID;
- snapshot capture outcome;
- integrity failure;
- recovery-read failure;

where these are consequential/security-relevant.

Do not persist snapshot bytes in audit.

Do not duplicate provenance records.

Do not use audit as a recovery source.

---

# 27. Rollback integration

Prompt 09 owns rollback/recovery.

Audit must record rollback outcomes such as:

- rollback denied;
- rollback eligible;
- recovery material unavailable;
- conflict;
- restored;
- deleted created file;
- partial/failure;
- already rolled back.

Correlate to:

- EditId;
- SnapshotId/provenance ID where available;
- agent/session/workspace;
- outcome/reason.

Do not implement rollback logic inside the audit service.

---

# 28. MCP security integration

Existing MCP security events may include:

- authentication success/failure;
- session creation/destruction;
- tool trust/security denial;
- secret denial;
- circuit-breaker events;
- high-risk tool invocation.

Preserve existing event semantics where they are established.

However, distinguish:

### Transport/security audit

Example:

    control_auth → deny → invalid_or_missing_token

### Consequential service audit

Example:

    filesystem.replace → deny → capability_denied

The latter must not depend solely on the former.

Do not duplicate every MCP transport event as a service event unless both answer materially different accountability questions.

---

# 29. Control API integration

The Control API currently has audit call sites and an audit route.

The implementation must route audit reads through the canonical persistent store.

Authentication remains the Control API security boundary.

Audit queries must not expose the API key or internal auth state.

Do not create an audit-specific authentication mechanism.

Preserve structured API errors and bounded response sizes.

If the current Control API's `/audit` endpoint is intended to show recent events, it must read durable events rather than only process-local memory.

---

# 30. CLI/TUI integration

If current CLI/TUI surfaces expose audit information:

- use the same application audit service;
- preserve structured outcomes;
- support bounded output;
- avoid printing secrets/full file contents;
- keep machine-readable output stable where already defined.

Do not create a CLI-only audit reader.

Do not make terminal rendering the storage layer.

---

# 31. Serialization and schema versioning

Audit events are durable data and therefore require explicit schema handling.

Define:

- schema version;
- serialization format;
- compatibility policy;
- unknown-version behavior;
- migration strategy.

Rules:

- known compatible versions may be read;
- incompatible/unknown versions must fail safely;
- malformed events must not be silently coerced into valid events;
- adding optional fields should preserve forward/backward compatibility according to the chosen policy;
- changing field meaning requires a schema/version transition.

Do not use Rust struct layout as an implicit storage schema.

---

# 32. Integrity protection

A persistent audit record should be tamper-evident within the guarantees the local storage architecture can actually provide.

If the selected storage design supports per-record checksums/hashes, use them.

For an append chain, a previous-record digest may be used if it does not impose disproportionate complexity and the repository's durability model supports it.

Do not claim cryptographic tamper-proofing merely because a SHA-256 field exists.

At minimum:

- detect malformed/truncated records;
- detect accidental corruption;
- reject invalid hashes where hashes are part of the schema;
- preserve clear limitations against a local attacker who can rewrite the entire store.

Audit integrity is not a replacement for OS filesystem permissions or signed remote logging.

---

# 33. File permissions and storage security

The audit store may contain sensitive metadata.

Inspect existing `.agent` storage permissions and apply the repository's established local-storage protections.

Do not make the audit file world-readable by default.

Do not expose secrets through file names.

If platform-specific permission setting is already centralized, reuse it.

Document residual risk for users who can modify the workspace/store files directly.

---

# 34. Concurrency

Multiple agents, MCP requests, CLI commands, and Control API requests may audit concurrently.

The implementation must guarantee:

- no duplicate sequence number;
- no lost accepted event;
- no interleaved/corrupt serialized record;
- deterministic ordering metadata;
- safe concurrent reads;
- bounded memory;
- no deadlock with edit/snapshot/authorization locks.

Reuse existing `StoreLock` or storage coordination primitives where applicable.

Do not introduce a distributed locking service.

Avoid lock-order inversions such as:

    edit lock → audit lock

in one path and:

    audit lock → edit lock

in another.

Audit should generally observe already-computed outcomes rather than hold mutation locks while performing slow persistence unless the actual durability contract requires it.

---

# 35. Performance discipline

Audit must not become the dominant cost of ordinary agent operations.

Measure/consider:

- serialization cost;
- disk I/O;
- fsync policy;
- lock contention;
- query latency;
- startup recovery time;
- storage size.

Do not:

- scan the entire workspace for each event;
- serialize full file contents;
- load the complete audit history for every query;
- hash unrelated files;
- block unrelated operations on long-running queries.

If synchronous durability is required, make that explicit and benchmark it.

If batching is introduced, define the durability point precisely. Do not claim an event is durable before the batch is actually persisted according to the documented contract.

---

# 36. Resource limits

Define limits for:

- maximum event size;
- maximum detail/metadata size;
- maximum query page size;
- maximum startup recovery work;
- maximum retained storage if a retention policy exists;
- maximum number of malformed records processed during recovery;
- maximum concurrent readers/writers if necessary.

Oversized audit data must fail deterministically.

Never truncate security-critical fields silently.

Prefer bounded omission of optional diagnostic metadata over storing an oversized event.

---

# 37. Error model

Audit errors must be structured and distinguish at least:

- invalid event;
- serialization failure;
- storage unavailable;
- lock/contention failure;
- corruption detected;
- incompatible schema;
- duplicate EventId;
- sequence conflict;
- query limit violation;
- permission denied;
- recovery failure.

Do not leak:

- absolute storage paths;
- file contents;
- secret values;
- tokens;
- raw serialized sensitive events.

Client-facing errors should use stable safe categories.

Internal logs may include bounded diagnostics according to existing logging policy.

---

# 38. Startup/recovery procedure

On application startup:

1. locate the canonical audit store;
2. validate storage metadata/schema;
3. recover durable ordering state;
4. validate event records/indexes;
5. detect corruption;
6. establish the next event sequence;
7. expose the store to services only after initialization reaches a known safe state.

If recovery fails:

- do not silently initialize an empty store over the existing history;
- do not reset sequence numbers;
- do not overwrite the corrupt store;
- surface a structured error;
- preserve the original evidence for diagnostics.

If the product permits degraded operation without audit, explicitly mark that degraded state and ensure security-critical operations follow the current fail-closed contract.

---

# 39. Migration from the current in-memory ring

The current `AuditLog` ring must not become a second source of truth.

Migration sequence:

1. identify all current producers;
2. preserve their event semantics;
3. introduce the persistent store behind the existing service boundary;
4. route `record_allow`/`record_deny` and equivalent calls into the canonical persistent writer;
5. preserve redaction;
6. replace `recent()` reads with bounded persistent queries;
7. remove or repurpose the volatile ring;
8. update tests;
9. search for direct ring access;
10. confirm exactly one persistent writer remains.

Do not create:

- `PersistentAuditLog` alongside `AuditLog` with separate writers;
- `AuditStore` plus another event database;
- per-service audit files;
- MCP-only audit persistence;
- Control-API-only audit persistence.

If a small in-memory cache remains for performance, it must be explicitly a cache of the persistent store and never the authoritative history.

---

# 40. Duplicate-writer audit

Search the entire repository for:

    audit
    record_allow
    record_deny
    audit_allow
    audit_deny
    AuditEntry
    AuditLog
    audit event
    security event
    event store
    append event
    history

Classify every writer.

The final implementation must have one canonical persistent writer.

Adapters may construct/submit events, but they must not independently persist them.

---

# 41. Required audit event categories

Use stable categories appropriate to the existing product.

At minimum support outcome classes:

- allow/success;
- deny;
- conflict;
- failure;
- security_error where needed.

Recommended action families:

- `filesystem.*`;
- `filesystem.rollback`;
- `authorization.*`;
- `mcp.*`;
- `control_auth`;
- `snapshot.*`;
- `audit.*` for management events when applicable.

Do not create arbitrary action strings at every call site without a stable naming convention.

---

# 42. Required audit truth table

| Operation state | Audit event required? | Outcome | Filesystem mutation |
|---|---:|---|---:|
| malformed consequential request | Yes where security contract requires | rejected/validation | No |
| unknown caller | Yes | denied | No |
| authorization denied | Yes | denied | No |
| authorization infrastructure failure | Yes | denied/failure | No |
| edit conflict | Yes | conflict | No |
| edit succeeds | Yes | success | Yes |
| edit verification fails | Yes | failure | Possibly, according to edit contract |
| automatic recovery after failed edit | Yes | recovery outcome | According to edit safety contract |
| rollback denied | Yes | denied | No |
| rollback conflicts | Yes | conflict | No |
| rollback succeeds | Yes | success | Yes |
| rollback partially fails | Yes | failure/partial | Partial |
| MCP auth failure | Yes | denied | No |
| audit query | Only if current policy requires | query | No |

Audit persistence failure must be represented separately from the operation outcome unless the current contract explicitly makes audit persistence transactional.

---

# 43. Query security truth table

| Caller/query | Scope | Expected |
|---|---|---|
| authorized operator | permitted workspace | events returned |
| authorized operator | global scope explicitly allowed | events returned |
| unauthorized caller | any workspace | denied |
| workspace-scoped caller | same workspace | matching events only |
| workspace-scoped caller | another workspace | no disclosure |
| invalid cursor | n/a | structured validation error |
| oversized limit | n/a | bounded/validation error |
| corrupt store | n/a | structured audit integrity error |
| missing store | n/a | documented empty/unavailable behavior, never fabricated history |

Adapt the operator/global row to the actual current Control API policy.

---

# 44. Tests — event schema

Test:

- required fields;
- optional correlation fields;
- schema version;
- EventId validation;
- sequence validation;
- serialization round-trip;
- unknown schema handling;
- malformed field handling;
- deterministic serialization where required.

---

# 45. Tests — durability

Use temporary isolated stores.

Test:

- event survives process restart;
- multiple events survive restart;
- order survives restart;
- EventId survives restart;
- next sequence continues correctly;
- empty store initializes safely;
- missing store initializes according to documented behavior;
- partial record is detected;
- corrupt record does not fabricate history.

---

# 46. Tests — append integrity

Test:

- duplicate EventId;
- duplicate sequence;
- sequence regression;
- truncated record;
- invalid checksum/hash if used;
- malformed JSON/serialization;
- incompatible schema;
- interrupted write;
- concurrent writes;
- concurrent readers.

Prove that accepted prior events remain intact.

---

# 47. Tests — security filtering

Explicitly attempt to persist:

- bearer tokens;
- API keys;
- TLS private key material;
- secret values;
- Authorization headers;
- environment secret values;
- file contents;
- sensitive tool arguments.

Assert that forbidden data is omitted or redacted.

Also verify safe values remain useful:

- action;
- outcome;
- stable error code;
- workspace-relative path;
- edit ID;
- non-secret actor metadata.

Test both structured fields and legacy free-form adapter inputs.

---

# 48. Tests — edit/authorization/rollback correlation

Test actual service paths for:

### Edit

- allowed edit;
- denied edit;
- conflict;
- verification failure;
- successful commit.

### Authorization

- policy deny;
- capability deny;
- expired capability;
- identity/session denial;
- authorization-store failure.

### Rollback

- denied rollback;
- successful rollback;
- conflict;
- partial/failure;
- already rolled back.

Every expected event must contain the correct safe correlation identifiers.

---

# 49. Tests — workspace isolation

Create multiple temporary workspaces.

Prove:

- workspace A query returns A events;
- workspace A query cannot retrieve B events;
- global/admin behavior follows the actual policy;
- pagination does not leak hidden records;
- resource filters cannot escape the allowed workspace.

---

# 50. Tests — retention/resource limits

If a retention policy exists, test:

- boundary at maximum size/count;
- deterministic removal;
- oldest-first or documented policy;
- restart after retention;
- no accidental deletion of required history;
- concurrent append during rotation.

If no destructive retention is implemented, test ingestion limits and explicit storage-capacity behavior instead.

---

# 51. Tests — query behavior

Test:

- newest-first/default ordering where promised;
- sequence ordering;
- bounded limit;
- zero limit;
- maximum limit;
- action filter;
- outcome filter;
- workspace filter;
- edit ID filter;
- cursor pagination if implemented;
- invalid cursor;
- empty results;
- corrupt store;
- concurrent append while querying.

Never allow an unbounded query to allocate the entire history.

---

# 52. Tests — redaction choke point

Keep a regression test proving that a future caller cannot bypass redaction simply by calling the canonical writer with a secret-like subject/detail.

If structured sensitive fields are added, test field-level filtering directly.

The final invariant is:

    caller mistake
        ↓
    canonical audit writer
        ↓
    secret filtered
        ↓
    durable event

not:

    caller remembered to redact
        ↓
    maybe safe

---

# 53. Tests — crash/restart

Use fault injection or controlled temporary-store interruption where practical.

Test interruption:

- before write;
- during write;
- after record append;
- during index update;
- during rotation;
- during startup recovery.

After restart verify:

- no false event;
- no missing durable event;
- no duplicate event;
- sequence remains monotonic;
- valid history remains queryable;
- corrupt tail is handled according to documented policy.

---

# 54. Tests — concurrency

Exercise:

- many concurrent writers;
- many concurrent readers;
- writer + reader;
- concurrent audit from multiple agent sessions;
- concurrent edit/rollback outcomes;
- audit query during append;
- storage lock contention.

Assertions:

- no duplicate sequence;
- no corrupted records;
- no lost accepted events;
- deterministic ordering metadata;
- no deadlock;
- bounded memory.

---

# 55. Tests — API/CLI compatibility

Where current interfaces expose audit:

- Control API `/audit`;
- CLI `audit list/show/search/export` where actually implemented;
- TUI audit view where actually implemented;

test that they consume the same persistent service.

Do not mark unimplemented target commands as complete merely because the roadmap lists them.

---

# 56. Failure and degraded-mode policy

Explicitly document behavior for:

### Audit store unavailable before operation

If audit is mandatory for that operation:

    fail closed before mutation

If audit is not mandatory:

    allow according to existing authorization/security rules
    but surface audit degradation

### Audit store fails after operation

Return the actual operation result plus a distinct audit-persistence failure.

Do not:

- claim a successful edit failed when it did not;
- claim a failed edit succeeded because the audit write succeeded;
- erase the operation outcome;
- overwrite prior audit history.

### Audit store corrupt at startup

Do not silently start with a fresh empty history.

Expose the corruption condition and preserve the original store.

---

# 57. Relationship to security logging

Existing `tracing` output remains useful for diagnostics.

Do not force all tracing events into persistent audit.

Audit should contain security/product accountability events with bounded structured metadata.

Tracing may contain stack traces and debugging information under existing logging controls, but it must still follow the repository's secret-handling rules.

The two systems must not recursively call each other.

---

# 58. Relationship to provenance

Provenance and audit answer different questions.

Provenance:

    edit E
      ↔ snapshot S
      ↔ resource R

Audit:

    actor A
      performed/attempted action X
      on workspace W/resource R
      at time T
      with outcome O

Correlate them with IDs.

Do not replace provenance with audit.

---

# 59. Relationship to authorization

Authorization decides:

> Is this operation permitted?

Audit records:

> What authorization decision and operation outcome occurred?

Audit must not call authorization recursively.

Authorization must not query audit history to determine permission.

---

# 60. Relationship to rollback

Rollback owns restoration semantics.

Audit owns recording the rollback request/decision/result.

Do not make audit responsible for determining which bytes to restore.

---

# 61. Relationship to snapshots

Snapshots own exact recovery bytes.

Audit records only metadata/correlation.

Never place file contents in audit.

---

# 62. Relationship to MCP

MCP remains a transport/security boundary.

Audit should observe consequential MCP-driven service outcomes.

Do not create MCP-specific persistent storage.

---

# 63. Relationship to CLI/Control API/TUI

All interfaces call the same audit application service.

No interface owns audit persistence.

No interface may bypass workspace/query authorization.

---

# 64. No analytics platform

Do not implement:

- dashboards;
- metrics warehouse;
- remote telemetry;
- SIEM connectors;
- data lake;
- full-text analytics engine;
- ML anomaly detection;
- cost/token accounting;
- distributed event streaming.

Those are separate future concerns.

Persistent audit means durable accountability, not an analytics platform.

---

# 65. No event-sourcing rewrite

Do not turn AWH into a full event-sourced architecture.

Audit is an observational record of application/security outcomes.

The authoritative state remains:

- filesystem state;
- edit transaction state;
- snapshot state;
- agent/session state;
- policy/capability state.

Audit does not become the source of truth for those systems.

---

# 66. Canonical implementation sequence

Implement in this exact linear sequence.

### Step 1 — Repository forensics

Read project, roadmap, feature, security, Trust Wedge, issue-resolving, implementation prompts, and current audit/source boundaries.

### Step 2 — Inventory audit producers

Find every current writer and classify it.

### Step 3 — Freeze ownership

Define one canonical persistent audit service/store and distinguish adapters from storage.

### Step 4 — Freeze the event schema

Define version, EventId, ordering, outcome, action, actor, workspace/resource, correlation IDs, reason, and safe metadata.

### Step 5 — Define durability semantics

Document the selected storage layout, atomicity, locking, fsync/durability point, restart behavior, and corruption policy.

### Step 6 — Implement persistent storage

Reuse existing storage primitives and security conventions.

### Step 7 — Implement event validation

Reject malformed IDs, sequences, schema versions, oversized events, and unsafe metadata.

### Step 8 — Implement security filtering

Make redaction/field filtering unavoidable at the canonical writer.

### Step 9 — Implement durable append

Guarantee unique identity, monotonic ordering, safe concurrent writes, and documented durability.

### Step 10 — Implement startup recovery

Recover valid history and ordering state without fabricating or silently replacing corrupt history.

### Step 11 — Replace the volatile ring as authority

Route existing `record_allow`/`record_deny` and equivalent producers into the persistent store.

### Step 12 — Implement bounded queries

Provide deterministic recent/query behavior through one service.

### Step 13 — Enforce query isolation

Apply workspace/operator authorization using existing security boundaries.

### Step 14 — Integrate edit outcomes

Record authoritative edit results without duplicating edit execution.

### Step 15 — Integrate authorization outcomes

Record decisions without making audit part of authorization.

### Step 16 — Integrate snapshot/provenance correlation

Record safe IDs only.

### Step 17 — Integrate rollback outcomes

Record exact EditId/outcome/correlation.

### Step 18 — Integrate MCP/Control API security events

Preserve meaningful existing event semantics and eliminate duplicate persistence.

### Step 19 — Integrate CLI/TUI reads

Where those surfaces already exist, route them through the same query service.

### Step 20 — Add corruption/fault tests

Prove restart and crash behavior.

### Step 21 — Add concurrency tests

Prove append/query safety.

### Step 22 — Add security/privacy tests

Prove secrets and sensitive content cannot enter durable audit.

### Step 23 — Audit retention/resource behavior

Ensure storage growth and query allocation are bounded according to the actual product contract.

### Step 24 — Duplicate-mechanism audit

Search for all alternate audit stores/writers and remove or convert them to adapters.

### Step 25 — Run verification gates

Run all required project checks.

### Step 26 — Final scope/security audit

Confirm audit is durable, correlated, queryable, bounded, privacy-safe, and remains observational rather than authoritative for authorization or application state.

---

# 67. Required architecture after implementation

The target architecture is:

    MCP / CLI / Control API / TUI
                 │
                 ▼
          application services
                 │
       ┌─────────┼──────────┐
       ▼         ▼          ▼
  authorization  edit    rollback
       │         │          │
       └─────────┼──────────┘
                 ▼
        canonical audit writer
                 │
       security filtering
                 │
       durable audit store
                 │
          query/read service

And separately:

    edit → snapshot/provenance

Audit correlates with those IDs but does not replace them.

---

# 68. Security invariants

Before completion, prove:

- [ ] one canonical persistent audit writer exists;
- [ ] no competing volatile audit history is authoritative;
- [ ] EventId is unique and persistent;
- [ ] sequence/order is monotonic and restart-safe;
- [ ] accepted events are not silently overwritten;
- [ ] corrupt records fail safely;
- [ ] unknown incompatible schema fails safely;
- [ ] secrets never reach durable audit;
- [ ] full file contents never reach durable audit;
- [ ] absolute host paths are not stored unnecessarily;
- [ ] workspace-scoped queries cannot disclose another workspace;
- [ ] audit does not authorize operations;
- [ ] audit does not replace provenance;
- [ ] audit does not replace snapshots;
- [ ] audit does not implement rollback;
- [ ] audit does not implement MCP authentication;
- [ ] audit does not create a second policy engine;
- [ ] audit does not become an event-sourcing source of truth;
- [ ] queries are bounded;
- [ ] writes are concurrency-safe;
- [ ] restart preserves valid history;
- [ ] operation outcomes are not falsely changed by audit persistence failure;
- [ ] audit failures follow the documented mandatory/best-effort policy;
- [ ] no unrelated implementation prompt is modified.

---

# 69. Verification gates

Run:

    cargo fmt --all -- --check
    cargo check --all-targets
    cargo test --all-targets
    cargo clippy --all-targets --all-features -- -D warnings
    git diff --check

Also run focused tests for:

- audit event schema;
- EventId uniqueness;
- monotonic ordering;
- durable append;
- restart;
- corruption;
- duplicate IDs;
- sequence collisions;
- concurrent writers;
- concurrent readers;
- redaction;
- secret filtering;
- workspace isolation;
- query limits;
- edit correlation;
- authorization correlation;
- rollback correlation;
- MCP security events;
- Control API audit reads;
- CLI/TUI audit reads where implemented;
- audit-store failure semantics.

If CI has additional required gates, run them.

Never claim a gate passed unless it actually ran successfully.

---

# 70. Completion criteria

Prompt 10 is complete only when:

1. AWH has exactly one canonical persistent audit store/writer.
2. The event schema is versioned.
3. Every persisted event has a stable EventId.
4. Event ordering is deterministic and restart-safe.
5. The store survives normal process restart.
6. Corruption is detected rather than converted into fabricated history.
7. Concurrent writers cannot corrupt or duplicate the sequence.
8. Audit data is bounded according to documented limits.
9. Queries are bounded and deterministic.
10. Workspace/query isolation follows existing authorization/security contracts.
11. Secrets, tokens, credentials, and full file contents are excluded.
12. Edit outcomes are correlated with EditId.
13. Rollback outcomes are correlated with EditId and snapshot/provenance IDs where available.
14. Authorization outcomes are recorded without making audit authoritative for authorization.
15. MCP/Control API security events retain meaningful existing semantics.
16. Existing in-memory audit behavior has been migrated or explicitly converted to a cache/adapter.
17. No second audit store exists.
18. No event-sourcing rewrite was introduced.
19. Audit persistence failure cannot falsely rewrite the actual filesystem outcome.
20. Restart, corruption, concurrency, security, privacy, and query tests pass.
21. All verification gates pass.
22. No unrelated implementation prompt was changed.

---

# 71. Independence rule

This prompt is standalone.

The implementer must not wait for:

- Prompt 07 PR;
- Prompt 08 PR;
- Prompt 09 PR;
- Prompt 11 PR;
- Prompt 12 PR;
- any historical Trust Wedge PR;
- any issue-resolving-prompts PR.

Inspect the current rust branch and consume contracts that actually exist.

If a referenced subsystem is incomplete:

1. reuse its existing public contract;
2. implement only the minimum audit integration needed;
3. do not create a replacement subsystem;
4. document the limitation.

The current rust branch is authoritative.

---

# 72. Final implementation report

At completion report:

1. **Forensics**
   - project/roadmap/security documents inspected;
   - current audit producers and storage identified;
   - historical evidence considered.

2. **Audit architecture**
   - canonical writer/store;
   - storage layout;
   - schema version;
   - EventId and ordering model.

3. **Durability**
   - append semantics;
   - atomicity/durability point;
   - restart recovery;
   - corruption handling.

4. **Security/privacy**
   - redaction/filtering;
   - secret handling;
   - path/resource handling;
   - workspace query isolation.

5. **Correlation**
   - agent/session/workspace;
   - edit;
   - snapshot/provenance;
   - authorization;
   - request/task IDs.

6. **Integration**
   - edit;
   - rollback;
   - authorization;
   - MCP;
   - Control API;
   - CLI/TUI where applicable.

7. **Migration**
   - what happened to the existing in-memory ring;
   - all duplicate writers removed or converted to adapters.

8. **Failure semantics**
   - pre-operation audit failure;
   - post-operation audit failure;
   - degraded mode;
   - corruption behavior.

9. **Tests**
   - schema;
   - persistence;
   - restart;
   - corruption;
   - concurrency;
   - security;
   - isolation;
   - integration.

10. **Verification**
    - exact commands run;
    - actual results.

11. **Changed files**
    - exact implementation/test/documentation files changed.

12. **Limitations**
    - residual local-tampering risk;
    - platform/filesystem limitations;
    - durability guarantees;
    - any unresolved prerequisite.

13. **Scope confirmation**
    - explicitly confirm that no second audit store, policy engine, authorization engine, identity system, snapshot store, rollback engine, MCP server, or event-sourcing architecture was introduced.

The final report must distinguish implemented facts from assumptions and must not claim durability, tamper resistance, security isolation, or completeness beyond what the implementation and tests establish.
