# Master Test Prompt 08 — Audit & Observability

## Mission

Create and execute a complete, human-behavior-oriented verification suite for AWH's **Audit & Observability** feature on the current `rust` branch.

This prompt owns the accountability boundary: consequential operations must produce trustworthy, structured, correlated audit evidence; audit history must be safely persisted and queryable; logs and audit must remain distinct; secrets and sensitive payloads must never become audit data; and evidence must remain correctly associated with workspace, agent, session, operation, authorization, edit, snapshot, rollback, Git, MCP, terminal, and connector activity where those identities exist.

Do not implement missing production behavior merely to make tests pass. Inspect the current branch first, exercise the real canonical service/application boundaries, and classify gaps honestly.

**Do not treat `cargo test` alone as sufficient evidence.**

---

## 1. Feature boundary

The final AWH contract provides:

```text
awh audit list
awh audit show
awh audit search
awh audit export

awh logs show
awh logs follow
awh logs clear
```

Audit is the durable accountability record. Observability/logging is diagnostic runtime information.

The audit boundary should conceptually be:

```text
consequential operation
→ authoritative service/security boundary
→ structured outcome/decision
→ canonical audit event
→ secret/sensitive-data filtering
→ durable persistence
→ query/read/export
```

This prompt must prove, using real workflows where available:

- consequential operations generate the appropriate audit evidence;
- allow, deny, failure, conflict, and security outcomes are distinguishable where required;
- event identity is unique and stable;
- event ordering is deterministic and survives restart;
- workspace/agent/session/resource correlation is preserved;
- audit history survives normal process restart;
- corruption and partial persistence fail safely;
- audit queries do not cross workspace/security boundaries;
- secrets, credentials, file contents, and sensitive arguments are not persisted;
- audit append failures have explicit and truthful semantics;
- logs are not silently treated as authoritative audit history;
- CLI/MCP/API/TUI surfaces use the same canonical audit state where implemented;
- export/read operations cannot become an information-disclosure bypass;
- bounded limits prevent unbounded memory/disk/query behavior;
- concurrent writers and readers do not corrupt history;
- the system provides enough evidence to reconstruct a consequential operation without duplicating provenance.

Do not duplicate complete suites for editing, snapshots/provenance, rollback, policy, Git, MCP transport, terminal, connectors, sessions, or other feature families. Test only their audit/observability integration boundaries here.

---

## 2. Current implementation versus target

Before writing tests, inspect the current `rust` branch.

Do not assume the historical audit design is still authoritative. Identify the actual implementation and classify each relevant component as:

- implemented and sufficiently tested;
- implemented but insufficiently tested;
- partial;
- legacy/compatibility;
- duplicate/non-canonical;
- not implemented;
- blocked;
- unproven.

At minimum investigate:

- `src/services/audit.rs`;
- `src/services/mod.rs`;
- MCP audit/event modules and dispatcher;
- Control API audit/log surfaces;
- CLI audit/log commands;
- TUI log/audit views where present;
- tracing subscriber setup;
- persistent state/storage abstractions;
- edit, snapshot, rollback, Git, policy/capability, session/agent, terminal, connector and MCP security call sites that emit audit evidence;
- all audit-related tests.

Search for concepts including:

```text
AuditEntry
AuditLog
audit_allow
audit_deny
record_allow
record_deny
recent
audit
observability
logs
event_id
sequence
correlation
redact
persistence
export
search
workspace_id
agent_id
session_id
edit_id
snapshot_id
rollback
authorization
```

Current source and current tests are the source of truth.

---

## 3. Required repository forensics

Before modifying tests, read at minimum:

- `README.md`
- `AGENTS.md`
- `Cargo.toml`
- `docs/FEATURES.md`
- `docs/testing.md`
- `docs/security.md`
- `docs/threat-model.md`
- `docs/architecture.md`
- `docs/CLI.md`
- `docs/roadmap/PROJECT_ROADMAP.md`
- `docs/roadmap/GROWTH_STRATEGY.md`
- `docs/implementation-prompts/README.md`
- the current implementation prompts covering capability/policy, snapshots, rollback and persistent audit.

Also inspect relevant historical Trust Wedge / issue material when it exists, especially AWE-013 and TW-007. Historical material is evidence only; current `rust` source wins.

Record the actual storage, event, redaction, logging, query, and interface architecture before designing tests.

---

## 4. Human-first test model

Every important integration test must represent something an actual AWH user, agent, operator, MCP client, or service would do.

At minimum create workflows resembling:

### Workflow A — successful consequential operation

```text
initialize isolated workspace
→ establish agent/session identity if implemented
→ perform a consequential operation
→ obtain operation result
→ query audit
→ identify the exact event
→ verify actor/context/action/outcome/correlation
```

### Workflow B — denied operation

```text
attempt policy/capability-protected mutation
→ receive denial
→ query audit
→ verify denial is recorded
→ verify stable reason/category
→ verify no secret/payload leakage
```

### Workflow C — failed/conflicted operation

Trigger a real operation failure or stale-state conflict where the current implementation supports it.

Verify that:

- the audit outcome describes failure/conflict rather than success;
- the operation's real state agrees with the audit result;
- no fabricated success event exists.

### Workflow D — restart

```text
write audit event
→ terminate process normally
→ start again using the same disposable state
→ list/search/show prior event
```

### Workflow E — security investigation

```text
perform several correlated operations
→ query by workspace/agent/session/action/time/correlation where supported
→ reconstruct the relevant sequence
→ verify unrelated workspace history is excluded
```

Tests must exercise real boundaries rather than merely constructing `AuditEntry` values and calling serialization helpers.

---

## 5. Disposable isolated environment

Every integration test must use isolated disposable state.

Use unique:

- workspace roots;
- AWH persistent state directories;
- workspace IDs;
- agent/session identities;
- repositories where required;
- audit fixtures;
- ports/sockets for real-interface tests.

Never use:

- developer-global AWH state;
- production repositories;
- personal credentials;
- real API keys;
- real TLS private keys;
- shared mutable audit history between unrelated tests.

Environment variables must be explicit and test-controlled. Restore or isolate process environment changes.

Where the application supports an explicit state/root environment variable, use it rather than relying on the developer's home directory.

---

## 6. Event identity and uniqueness

Test the real event-ID mechanism.

Verify:

- every persisted event has an explicit valid ID;
- IDs are unique across many events;
- IDs remain unique across restart;
- IDs are not solely timestamps;
- an ID collision cannot silently replace an existing event;
- malformed IDs are rejected if the schema validates them;
- event IDs do not contain secret material.

Use an independent set/map of observed IDs as the oracle.

If IDs are UUID-like, do not assert a particular implementation merely for its own sake; assert the documented uniqueness and persistence properties.

---

## 7. Ordering and sequence integrity

Audit history must have deterministic ordering stronger than wall-clock timestamps alone.

Exercise:

- multiple sequential events;
- events with equal or near-equal timestamps;
- concurrent writers;
- restart followed by additional events.

Verify:

- accepted events have deterministic ordering;
- ordering survives restart;
- sequence/order metadata does not regress or repeat;
- concurrent writers do not publish duplicate order values;
- querying newest/oldest returns the documented order;
- timestamps remain metadata rather than the sole ordering oracle.

If the implementation intentionally uses a different monotonic mechanism, test that mechanism's actual contract rather than requiring a particular field name.

---

## 8. Canonical event schema

For every applicable consequential event, verify the structured record contains enough information to answer:

- what happened;
- when;
- who/what initiated it;
- which operation/action occurred;
- which workspace was affected;
- which agent/session was involved when identity exists;
- which resource was affected when safe;
- what authorization/capability decision applied when relevant;
- whether it succeeded, failed, conflicted, or was denied;
- stable reason/error information;
- correlation/request identity when available;
- edit/snapshot/rollback/task identifiers when applicable.

Optional values must remain genuinely optional.

Reject or flag:

- fake identity values used to hide missing identity;
- ambiguous empty strings;
- timestamps used as identity;
- raw free-form payloads where structured safe fields are required.

Do not require fields that the current product contract does not define.

---

## 9. Correlation across AWH subsystems

Use real cross-feature workflows where those features are implemented.

At minimum test the audit integration boundary for:

### Editing

A controlled edit should produce auditable evidence that can be related to the edit identity and relevant resource without persisting file contents.

### Snapshots/provenance

Where snapshot/provenance IDs exist, verify audit correlation references them rather than copying snapshot bytes into audit.

### Rollback

A rollback outcome should be distinguishable and correlatable to the relevant edit/recovery identity where implemented.

### Capability/policy

Allow/deny decisions for consequential operations should be represented at the authoritative security boundary where required.

### MCP

MCP authentication/security/tool outcomes should be auditable where the contract requires them. Do not count generic transport logs as a substitute for consequential service audit.

### Git

Guarded Git mutations should produce appropriate audit evidence without attempting to duplicate Git history.

### Agent/session

When AWH-native agent/session identity exists, verify it survives into audit records and cannot be substituted by an untrusted route namespace.

### Terminal/connectors

If implemented, verify high-risk terminal/connector outcomes are auditable without persisting credentials, command secrets, or connector payloads.

Do not fail the entire feature suite merely because a future subsystem is not yet implemented; classify that integration as **Not implemented** or **Unproven**.

---

## 10. Audit versus logs

Prove that audit and diagnostic logging are distinct.

Test:

- an audit event is queryable through the audit boundary;
- diagnostic tracing may contain additional runtime details;
- absence/rotation/clearing of volatile logs does not silently erase durable audit history;
- log formatting cannot mutate the authoritative audit record;
- audit records do not require parsing human-readable log strings;
- secret redaction is enforced independently at the canonical audit boundary.

If the current implementation intentionally shares an internal helper, verify there is still one authoritative audit record rather than two competing stores.

Do not use the tracing subscriber as the sole oracle for durable audit behavior.

---

## 11. Persistence and restart

Use the real durable audit store.

Test:

1. append an event;
2. confirm the documented durability point;
3. stop the process;
4. start a new process against the same isolated state;
5. list/search/show the event;
6. append another event;
7. verify both remain valid and correctly ordered.

Also test:

- empty store;
- many events;
- restart immediately after append;
- repeated restart;
- persistence across separate process instances;
- state-directory relocation where supported.

Do not accept an in-memory-only pass as persistence evidence.

---

## 12. Crash consistency and partial writes

Where fault injection or controllable storage boundaries exist, test interruption at persistence stages such as:

- before serialization;
- after serialization but before write;
- during record write;
- after record write but before index/manifest publication;
- during startup recovery.

After restart verify:

- previously durable events remain readable;
- incomplete records are detected;
- valid prior events are preserved where safe;
- ordering remains valid;
- no event is fabricated;
- recovery either repairs safely or reports a structured failure.

Do not claim arbitrary filesystem crash atomicity unless the implementation actually provides it.

---

## 13. Corruption handling

Construct disposable corrupted audit state using the real storage format.

Test at minimum:

- malformed serialization;
- truncated record;
- unsupported/incompatible schema;
- invalid event ID;
- duplicate event ID;
- sequence regression;
- invalid required field;
- integrity/checksum mismatch when the format supports one;
- malformed metadata.

Verify corruption does not become a false historical record.

Where safe recovery is supported:

- valid earlier events remain available;
- corruption is surfaced explicitly;
- later operations do not silently overwrite the damaged history;
- sequence state is not fabricated.

Distinguish clearly between:

- audit history corruption;
- inability to query;
- operation failure;
- audit-write failure.

---

## 14. Secret and sensitive-data protection

This is a mandatory security test area.

Generate unique canary values for:

- bearer tokens;
- API keys;
- passwords;
- private-key-like material;
- environment secret values;
- Authorization headers;
- connector credentials;
- sensitive command arguments;
- sensitive tool arguments;
- representative file-content secrets.

Cause these values to pass through relevant application paths.

Then inspect:

- persisted audit files;
- audit query results;
- audit exports;
- audit CLI output;
- MCP audit responses;
- Control API audit responses;
- TUI audit/log displays where applicable;
- diagnostic logs where the contract requires redaction.

The exact canary must not appear in prohibited outputs.

Verify redaction occurs at the canonical boundary, not only at one caller.

Prefer omission over unsafe guessing when a field is uncertain.

Do not store:

- complete file contents;
- snapshot blobs;
- raw Authorization headers;
- private keys;
- credentials;
- unbounded tool/request payloads.

---

## 15. Path and resource privacy

Where resource paths are audited, test:

- logical workspace-relative paths;
- absolute host paths;
- home-directory prefixes;
- temporary directory names;
- secret-like path components;
- traversal-looking paths.

Verify the audit representation follows the documented privacy contract.

A query must not expose unnecessary host filesystem layout.

Audit must never become a path-traversal or file-disclosure interface.

---

## 16. Workspace and identity isolation

Create at least two independent workspaces with separate identities.

Generate audit activity in both.

Verify:

- workspace A queries return only A-authorized events;
- workspace B queries return only B-authorized events;
- a missing/invalid workspace scope does not broaden visibility;
- agent/session filters cannot cross workspace boundaries;
- a route name or user-supplied identifier cannot manufacture authorization;
- an event from workspace A cannot be reassigned to B merely by changing a query parameter;
- persisted records retain their canonical workspace association.

If the product intentionally has an operator/global audit view, test that explicit authorization path separately and do not treat it as a workspace-isolation failure.

---

## 17. Query, filtering, and search semantics

Exercise every implemented audit read operation:

- list;
- show;
- search;
- export.

Test filters supported by the current interface, such as:

- event ID;
- workspace;
- agent;
- session;
- action;
- outcome;
- time range;
- correlation ID;
- edit/snapshot/rollback ID.

Verify:

- exact matching behaves deterministically;
- empty results are explicit;
- malformed filters fail safely;
- unknown event IDs do not reveal unrelated records;
- pagination/limits, if implemented, are bounded;
- ordering is documented and stable;
- filters compose without accidentally broadening scope;
- query results do not mutate audit history.

Search input must not be interpreted as a filesystem path, shell command, SQL fragment, or arbitrary code.

---

## 18. Show/export security

Test audit `show` and `export` as data-exposure surfaces.

Verify:

- only authorized records are returned;
- export does not bypass normal filtering/isolation;
- export contains the same redaction guarantees as normal reads;
- output size is bounded where the contract requires limits;
- malformed export destination/path is rejected safely;
- export does not write outside the permitted destination;
- export errors do not leak secret payloads;
- exported schema remains versioned and machine-readable where applicable.

If export is not implemented, classify it honestly rather than fabricating a test.

---

## 19. Append failure semantics

Inject or simulate audit-storage failure at the real persistence boundary.

For consequential operations, determine from current source/product contract whether the audit event is:

- required before mutation;
- required after mutation;
- best-effort;
- required only for security decisions.

Then test the actual rule.

Never allow a test to call an operation “failed” merely because a post-operation audit append failed if the filesystem/Git/etc. mutation actually succeeded.

Conversely, never accept a successful security-sensitive operation when the contract requires a mandatory audit decision/event and that evidence could not be persisted.

The result must distinguish:

```text
operation result
+
audit persistence result
```

when both are independently relevant.

---

## 20. Concurrent writers and readers

Use multiple threads/processes against the same disposable audit store where supported.

Exercise:

- concurrent event appends;
- concurrent reads during writes;
- restart while readers exist;
- concurrent export/search;
- simultaneous writes from different workspaces.

Verify:

- no duplicate sequence numbers;
- no duplicate event IDs;
- no malformed published records;
- no lost events beyond explicitly documented failure semantics;
- no torn query results that violate the store contract;
- no cross-workspace leakage;
- no deadlock that prevents bounded recovery;
- writer failures do not corrupt prior valid history.

Use independent event counts and IDs as the oracle.

---

## 21. Resource and abuse limits

Audit itself must not become a denial-of-service vector.

Test:

- very large event metadata;
- long action names;
- long paths;
- many events;
- repeated denied requests;
- repeated searches;
- concurrent readers;
- large exports;
- malformed/corrupt stores.

Verify documented limits are enforced before dangerous allocation where practical.

A single event or query must not cause unbounded memory growth.

Repeated audit activity must not silently disable core security enforcement.

---

## 22. CLI black-box testing

Where commands are implemented, test them through the real compiled `awh` binary.

Exercise:

```bash
awh audit list
awh audit show <event-id>
awh audit search ...
awh audit export ...
awh logs show
awh logs follow
awh logs clear
```

Use the exact current CLI syntax discovered from source/help.

Verify:

- exit codes;
- stdout/stderr separation;
- machine-readable output where documented;
- human-readable output where documented;
- no secret leakage;
- correct workspace scope;
- deterministic error behavior;
- persistence across a new process.

Do not merely call internal Rust functions and claim CLI coverage.

---

## 23. MCP/Control API integration

Where audit is exposed through MCP or the Control API, use the real interface boundary.

Test:

- successful audit reads;
- denied/unauthenticated reads;
- workspace scoping;
- malformed parameters;
- unknown event IDs;
- bounded result sizes;
- secret redaction;
- stable structured errors;
- correlation with events created through actual operations.

The MCP/HTTP/Control API transport suite remains responsible for its complete protocol/authentication semantics. This prompt only verifies the audit/observability integration.

---

## 24. TUI/log surface integration

If the TUI exposes audit/log information, exercise it through the real TUI/backend boundary where practical.

Verify:

- it consumes canonical state;
- it does not maintain a second audit database;
- displayed records obey redaction;
- workspace/session filtering is preserved;
- clearing volatile logs does not silently erase durable audit;
- malformed audit state is surfaced safely.

Do not require visual snapshot testing when deterministic backend behavior is the meaningful oracle.

---

## 25. Independent oracles

Never use the audit implementation itself as the only oracle for whether audit is correct.

Cross-check with:

- actual filesystem state;
- actual Git state;
- actual operation result;
- independent event-ID sets;
- independent sequence checks;
- process restart;
- raw persisted bytes where appropriate;
- separate workspace fixtures;
- known secret canaries;
- expected authorization decisions;
- independent counts.

For example, after an edit:

```text
operation says success
+ filesystem actually changed
+ audit contains matching outcome
+ audit does not contain file contents
```

All four facts matter.

---

## 26. Security regression suite

Create permanent regression tests for any discovered issue involving:

- missing audit event;
- false success audit;
- false failure audit;
- event-ID collision;
- sequence regression;
- duplicate publication;
- workspace leakage;
- agent/session identity spoofing;
- route-name authorization confusion;
- secret leakage;
- file-content leakage;
- path privacy leakage;
- audit export bypass;
- corrupted-store acceptance;
- partial-write corruption;
- concurrent-writer corruption;
- audit persistence loss;
- audit/log conflation;
- audit append failure causing incorrect operation state;
- unbounded query/export behavior.

Every security bug discovered during this work must become a reproducible regression test before the fix is considered complete.

---

## 27. Property and boundary testing

Where deterministic properties exist, add property/boundary tests for:

- event-ID uniqueness;
- sequence monotonicity;
- serialization round-trip;
- schema validation;
- redaction invariants;
- filter composition;
- workspace isolation;
- bounded event size;
- bounded query size.

Useful properties include:

```text
redact(redact(event)) == redact(event)

persist(load(events)) preserves valid event identity/order

query(workspace=A) never returns event.workspace=B

accepted sequence numbers are strictly monotonic

a rejected append cannot replace an existing EventId
```

Do not generate only arbitrary data with no assertion about the security/product contract.

---

## 28. Failure-injection requirements

Where practical, inject failures at real boundaries rather than mocking the entire audit subsystem.

Examples:

- unwritable audit directory;
- unavailable state path;
- corrupted record;
- truncated record;
- storage lock contention;
- concurrent writer;
- interrupted process;
- invalid query;
- export destination failure;
- oversized event;
- invalid schema version.

For every failure verify:

1. exact error category;
2. operation result;
3. audit result;
4. persisted state;
5. absence of secret leakage;
6. whether retry is safe;
7. whether prior history remains intact.

Never convert a failure into a pass merely because the process did not panic.

---

## 29. Cross-platform verification

Run the audit/observability tests on the repository's supported CI platforms where applicable:

- Linux x86_64;
- Linux ARM64 where available;
- macOS x86_64/ARM64 where available;
- Windows x86_64;
- Android/Termux ARM64 where practical.

Pay particular attention to:

- filesystem locking;
- path representation;
- atomic persistence;
- newline/encoding behavior;
- process restart;
- file permissions;
- concurrent access.

Do not mark a platform as verified unless it actually executed the relevant test path.

---

## 30. Test implementation requirements

Prefer:

- unit tests for pure schema/redaction/order invariants;
- integration tests through the canonical audit service;
- real temporary persistent stores;
- real subprocesses for restart/CLI behavior;
- real filesystem and Git workflows for audit integration;
- real MCP/HTTP/Control API boundaries where exposed;
- fault injection at storage/service boundaries;
- independent security canaries.

Do not create:

- a second audit store;
- a second audit event model solely for tests;
- a fake persistence layer that proves only the fake;
- test-only authorization that bypasses PolicyEngine;
- test-only redaction that bypasses production filtering;
- tests that restore or mutate state manually and attribute the result to AWH;
- tests that inspect only logs when the requirement is durable audit.

Use existing fixtures/helpers when they exercise the canonical implementation.

---

## 31. Execution gates

Run the repository's actual gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Then run focused audit/observability tests for:

- event identity;
- ordering;
- schema;
- redaction;
- persistence/restart;
- corruption;
- append failures;
- workspace isolation;
- query/search;
- export;
- concurrency;
- resource limits;
- editing integration;
- snapshot/rollback correlation;
- policy/capability decisions;
- MCP/Control API integration;
- CLI/TUI/log behavior;
- security regressions.

Run repository-specific external interop or CI checks where applicable.

Report a command as passed only if it actually completed successfully.

---

## 32. Evidence reporting

Classify each target behavior as:

### Passed

The real behavior executed and was independently verified.

### Failed

The real test executed and exposed a production defect.

### Blocked

Execution could not occur because of an external/environmental prerequisite.

### Unproven

The implementation/evidence is insufficient to establish the requirement.

### Not implemented

The documented final-target behavior does not exist on the current branch.

For every failure include:

- operation;
- event ID where applicable;
- workspace;
- agent/session identity where applicable;
- expected result;
- actual result;
- operation result;
- audit result;
- persisted-state observation;
- relevant error;
- security impact;
- likely subsystem.

Never include secret values or complete sensitive file contents.

---

## 33. Completion criteria

This prompt is complete only when:

- the current audit/observability architecture was inspected;
- the canonical audit store and event model were identified;
- successful consequential operations were audited;
- denied operations were audited where required;
- failed/conflicted operations were distinguishable;
- event identity uniqueness was independently verified;
- ordering/sequence integrity was independently verified;
- schema/version behavior was tested;
- agent/session/workspace correlation was tested where implemented;
- edit/snapshot/rollback correlation was tested at integration boundaries;
- capability/policy decisions were tested at their audit boundary;
- MCP/Control API/CLI/TUI integration was tested where implemented;
- audit and diagnostic logging were proven distinct;
- persistence across restart was proven;
- crash/partial-write behavior was tested where practical;
- corruption is detected and handled safely;
- secret and sensitive-data filtering was independently verified;
- path/resource privacy was tested;
- workspace isolation was proven;
- query/search/show/export behavior was tested;
- export cannot bypass authorization/redaction;
- append failure semantics are explicit and truthful;
- concurrent writers/readers were exercised;
- resource limits were tested;
- permanent security regressions were added;
- final-target gaps were honestly classified;
- no duplicate audit store or test-only substitute was introduced;
- no other master test prompt was modified.

The objective is trustworthy evidence that AWH's audit and observability layer can answer **what happened, who/what initiated it, what decision/result occurred, and how the event relates to the affected runtime state**, while remaining durable, correlated, bounded, isolated, and safe against secret disclosure.

---

## 34. Scope boundary

This prompt owns **Audit & Observability testing only**.

Do not create full feature suites for:

- Foundation and Distribution;
- Workspace Runtime;
- Agent-Grade Filesystem Editing;
- Git/worktrees;
- Capability and Policy Engine;
- Snapshots/Provenance;
- Rollback & Recovery;
- Context;
- Memory;
- Skills;
- Agent Profiles/policy-routed MCP;
- MCP protocol/transport infrastructure;
- Sessions/Tasks;
- Terminal;
- Collaboration;
- Control API;
- TUI;
- Connectors;
- Advanced infrastructure.

Those features may be exercised only where necessary to prove their audit/observability integration boundary.

Do not modify:

```text
docs/testing-prompts/01-foundation-distribution.md
docs/testing-prompts/02-workspace-runtime.md
docs/testing-prompts/03-agent-grade-filesystem-editing.md
docs/testing-prompts/04-git-and-worktrees.md
docs/testing-prompts/05-capability-and-policy.md
docs/testing-prompts/06-snapshots-and-provenance.md
docs/testing-prompts/07-rollback-and-recovery.md
```

Do not modify any other existing master test prompt while executing this prompt.
