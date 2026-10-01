# Master Test Prompt 06 — Snapshots & Provenance

## Mission

Create and execute a complete, human-behavior-oriented verification suite for AWH's **Snapshots & Provenance** feature on the current `rust` branch.

This prompt owns testing of durable exact-byte snapshots, snapshot manifests/blobs, integrity validation, edit linkage, provenance records, recovery views, persistence, corruption handling, and snapshot/provenance security boundaries.

Do not implement missing production features merely to make tests pass. Inspect the current branch, establish the implemented contract, test it through real boundaries, and classify gaps honestly.

**Do not treat `cargo test` alone as sufficient evidence.**

---

## 1. Product boundary

AWH snapshots provide durable recovery material for completed edit transactions. Provenance binds that recovery material to the exact edit/workspace and records lifecycle/outcome information.

This prompt must prove:

- snapshot identity is exact and stable;
- snapshot content is byte-for-byte faithful;
- manifests correctly identify entries and content;
- blob integrity is verified;
- snapshot data is durably persisted;
- provenance binds the correct edit to the correct snapshot;
- unrelated edits/workspaces cannot consume another snapshot;
- missing, truncated, malformed, or tampered data fails closed;
- recovery views expose only verified canonical recovery material;
- snapshot existence is not authorization;
- secrets and sensitive content are not unnecessarily exposed by metadata/errors;
- restart does not silently change snapshot meaning;
- concurrent publication/read operations cannot expose torn snapshots.

Do not duplicate full edit execution, rollback execution, authorization, filesystem, Git, or audit suites. Test their snapshot/provenance boundary only.

---

## 2. Current implementation versus target

Before writing tests, inspect the current `rust` branch and classify each implemented capability.

The final feature area includes concepts such as:

- `SnapshotId`
- `SnapshotEntryId`
- `FileSnapshot`
- `FileSnapshotEntry`
- `SnapshotStore`
- `ProvenanceRecord`
- `ProvenanceOutcome`
- `recovery_view(edit_id)`

Current source and tests are authoritative.

For each target behavior classify:

- Implemented and tested
- Implemented but insufficiently tested
- Not implemented
- Blocked
- Unproven

Never convert a roadmap statement into evidence of implementation.

---

## 3. Required repository forensics

Before changing tests, inspect at minimum:

- `README.md`
- `AGENTS.md`
- `Cargo.toml`
- `docs/FEATURES.md`
- `docs/testing.md`
- `docs/security.md`
- `docs/threat-model.md`
- `docs/architecture.md`
- `docs/roadmap/PROJECT_ROADMAP.md`
- `docs/roadmap/GROWTH_STRATEGY.md`
- `docs/implementation-prompts/README.md`
- current snapshot/provenance implementation prompt(s)
- current edit, filesystem, authorization, rollback, and audit tests

Inspect at minimum:

- `src/services/snapshot.rs`
- `src/services/edit.rs`
- `src/services/files.rs`
- `src/services/authorization.rs`
- relevant service modules
- CLI/MCP adapters exposing snapshot/provenance behavior
- current snapshot/provenance tests

Search for:

```text
SnapshotStore
SnapshotId
SnapshotEntryId
FileSnapshot
FileSnapshotEntry
ProvenanceRecord
ProvenanceOutcome
recovery_view
snapshot
provenance
before_bytes
after_hash
sha256
content_hash
manifest
blob
schema
integrity
```

Classify hits as canonical implementation, adapter, test, compatibility code, duplicate, or unrelated.

---

## 4. Human-first acceptance model

Every important test must correspond to a realistic lifecycle:

```text
human/agent performs controlled edit
→ AWH captures exact pre-edit state
→ snapshot is published durably
→ provenance binds edit + workspace + snapshot
→ process restarts or later recovery reads occur
→ AWH reconstructs verified recovery material
```

Use real temporary workspaces and real files.

For every successful snapshot, independently verify:

1. the snapshot exists;
2. the expected paths are represented;
3. exact original bytes are recoverable;
4. hashes and lengths match;
5. the snapshot survives restart;
6. provenance points to the exact edit/snapshot relationship.

---

## 5. Isolated test environment

Every test must use disposable isolated state.

Never use:

- real user AWH state;
- production repositories;
- personal credentials;
- shared snapshot stores between unrelated tests.

Use unique temporary:

- HOME/state roots;
- workspace roots;
- files;
- edit IDs;
- snapshot IDs;
- provenance records.

When testing restart, preserve only the disposable fixture.

Do not mutate process-global environment concurrently.

---

## 6. Exact-byte snapshot fidelity

This is the primary content invariant.

Create files containing deliberately difficult byte sequences, including where supported:

- empty files;
- UTF-8;
- Devanagari;
- emoji;
- tabs;
- multiple spaces;
- CRLF;
- LF;
- no final newline;
- trailing newline;
- embedded NUL bytes;
- arbitrary binary bytes;
- very large files near configured limits.

For each file:

```text
original bytes
→ snapshot
→ recovery read
```

Assert exact byte equality.

Do not compare decoded strings when raw bytes are available.

Do not normalize:

- line endings;
- Unicode;
- whitespace;
- encoding;
- final newline.

An existing empty file must remain distinguishable from a missing file.

---

## 7. Snapshot entry semantics

For every `FileSnapshotEntry` or equivalent, verify:

- stable entry ID;
- workspace-relative path;
- existence-before-edit state;
- byte length;
- content hash;
- snapshot relationship;
- exact content reference/blob;
- required schema fields.

Test:

- one file;
- multiple files;
- nested directories;
- similarly named paths;
- path ordering if deterministic ordering is promised;
- duplicate path input;
- duplicate entry IDs;
- empty path;
- absolute path;
- traversal-shaped path;
- malformed path.

A snapshot must never silently contain two ambiguous representations of one logical path.

---

## 8. Manifest and blob integrity

Treat the snapshot manifest and content blobs as a security boundary.

Create a valid snapshot, then independently inspect its durable representation.

Verify:

- manifest identifies the intended snapshot;
- each entry references the intended blob;
- byte lengths match;
- SHA-256/content hashes match;
- all referenced blobs exist;
- no required entry is missing;
- malformed references fail closed.

Then deliberately corrupt disposable state:

- change blob bytes;
- truncate blob;
- delete blob;
- change recorded length;
- change recorded hash;
- change entry ID;
- change path;
- remove entry;
- add unexpected entry;
- modify snapshot ID;
- modify edit ID;
- modify schema version;
- truncate manifest;
- replace manifest with invalid serialization.

Every integrity failure must prevent the corrupted data from being accepted as valid recovery material.

Do not silently repair corruption by treating the modified bytes as authoritative.

---

## 9. Content-addressed storage behavior

If the current implementation uses content-addressed blobs, test the actual contract.

Verify:

- identical content can reuse the same blob where deduplication is intended;
- different content never resolves to the same valid content identity;
- blob hash is independently reproducible;
- content length is checked;
- a blob collision/tamper scenario fails integrity validation;
- deleting one unrelated snapshot does not invalidate another snapshot that legitimately references shared content.

Do not assume deduplication exists if the current branch does not implement it.

---

## 10. Snapshot identity

Test exact `SnapshotId` handling.

Verify:

- newly created snapshot has an unambiguous ID;
- repeated IDs are rejected or handled according to the current contract;
- unknown ID returns a deterministic not-found/error result;
- malformed ID cannot resolve another snapshot;
- an ID from workspace A cannot resolve workspace B data;
- caller-supplied IDs cannot redirect a lookup to unrelated content.

Never select a snapshot using:

- “latest” semantics unless explicitly implemented;
- timestamp proximity;
- path alone;
- arbitrary matching entry;
- filename alone.

Where an edit references a snapshot, resolution must be exact.

---

## 11. Edit-to-snapshot binding

Provenance is the authoritative relationship between an edit and its recovery snapshot.

Test:

```edit A → snapshot A
edit B → snapshot B
```

Then attempt:

- edit A + snapshot B;
- edit B + snapshot A;
- edit A + unrelated snapshot;
- unknown edit + valid snapshot;
- valid edit + missing snapshot;
- valid edit + snapshot from another workspace.

Every mismatch must fail closed.

A valid snapshot must not become usable merely because its bytes look plausible.

---

## 12. Workspace isolation

Create two disposable workspaces with similar paths and content.

Verify:

- workspace A's snapshot resolves only for A;
- workspace B's snapshot resolves only for B;
- identical file paths across workspaces do not collide;
- a provenance record from A cannot authorize recovery material in B;
- changing a workspace identifier cannot redirect recovery to another workspace.

Do not use global snapshot lookup that ignores workspace binding unless the actual architecture explicitly defines a safe equivalent.

---

## 13. Provenance lifecycle

Test the complete lifecycle implemented by the branch.

For a successful edit, verify provenance records:

- exact edit identity;
- exact snapshot identity;
- workspace identity;
- affected resource set where supported;
- agent/session correlation where supported;
- lifecycle/outcome;
- schema/version fields;
- timestamp/correlation data where supported.

Test negative cases:

- missing edit identity;
- missing snapshot identity;
- mismatched edit;
- mismatched workspace;
- unsupported outcome;
- malformed record;
- duplicate provenance identity;
- truncated persisted record.

Invalid provenance must not manufacture valid recovery authority.

---

## 14. Provenance outcomes

Inspect the current `ProvenanceOutcome` vocabulary and test every implemented outcome.

Do not invent enum variants.

For each outcome verify:

- it serializes correctly;
- it deserializes correctly;
- it remains distinguishable from other outcomes;
- unsupported values fail safely;
- restart preserves the outcome;
- an outcome cannot be changed merely by changing unrelated snapshot metadata.

If outcome changes are lifecycle-controlled, verify illegal transitions are rejected.

---

## 15. Recovery view

If `SnapshotStore::recovery_view(edit_id)` or an equivalent API exists, test it as the canonical recovery-read boundary.

Verify:

- exact edit ID resolves the correct recovery view;
- returned entries correspond only to that edit;
- pre-edit bytes are exact;
- missing/corrupt snapshot fails;
- mismatched provenance fails;
- workspace mismatch fails;
- unbound snapshot fails;
- recovery view never returns unverified content.

Do not reimplement snapshot validation in the test suite.

Use an independent oracle to verify recovered bytes.

---

## 16. Persistence and restart

Snapshot state must survive process boundaries.

Workflow:

```text
create workspace
→ perform/capture edit state
→ publish snapshot + provenance
→ terminate AWH
→ start fresh AWH process
→ load snapshot/provenance
→ recover exact bytes
```

Verify:

- IDs are stable;
- manifest remains valid;
- blobs remain readable;
- provenance relationship remains intact;
- recovery bytes are unchanged.

Also test:

- restart after snapshot publication;
- restart before a later provenance read;
- repeated reads after restart;
- multiple snapshots for one workspace;
- multiple workspaces.

Do not treat an in-memory object surviving within one test process as persistence evidence.

---

## 17. Publication atomicity

Determine the current snapshot publication protocol from source.

Test for reader-visible consistency.

A reader must never observe a state where:

- manifest is visible but required blob is missing due only to publication ordering;
- blob exists but its final content is incomplete;
- provenance points to a snapshot that cannot yet be validated;
- manifest contains a partially written record;
- snapshot appears valid while publication is incomplete.

Use controlled staging/failure injection where the implementation supports it.

If orphaned blobs can remain after a crash, verify they cannot be interpreted as valid snapshots without a committed manifest/provenance relationship.

Do not require a garbage collector unless the current contract requires one.

---

## 18. Crash and failure injection

Use disposable state to simulate:

- interrupted manifest write;
- truncated blob;
- missing blob;
- interrupted provenance write;
- malformed manifest;
- malformed provenance;
- unsupported schema;
- partial publication;
- filesystem write failure;
- permission failure;
- disk/resource limit where practical.

After each injected failure:

1. restart AWH;
2. inspect snapshot/provenance state;
3. attempt recovery;
4. verify invalid state fails closed;
5. verify unrelated valid snapshots remain usable;
6. verify no corrupted record is silently promoted to valid state.

Do not delete unrelated valid state as a recovery shortcut.

---

## 19. Concurrency

Test controlled concurrent operations for:

- snapshots of different edits;
- snapshots of the same workspace;
- reads during publication;
- simultaneous recovery reads;
- provenance writes for different edits;
- repeated reads of one snapshot.

Verify:

- no cross-snapshot content mix;
- no manifest points to the wrong blob;
- no torn reader-visible snapshot;
- no duplicate committed IDs;
- no corrupted provenance record;
- no deadlock;
- deterministic final state where ordering is defined.

Do not require global serialization unless the architecture promises it.

Use barriers/process synchronization rather than arbitrary sleeps.

---

## 20. Limits and resource boundaries

Read `docs/configuration.md` and test the actual snapshot/resource limits implemented by the branch.

At minimum consider:

- empty content;
- one-byte content;
- content at configured maximum;
- content just over maximum;
- many entries;
- deeply nested paths;
- unusually long path components;
- oversized manifest;
- oversized blob/reference data.

Verify rejected input:

- does not create misleading valid snapshot metadata;
- does not leave a partially committed snapshot that recovery treats as valid;
- does not bypass configured limits.

Do not invent limit values if the current branch uses different values.

---

## 21. Path safety

Snapshot paths are workspace-relative resource identities.

Test:

- absolute paths;
- `..`;
- `.`;
- repeated separators;
- encoded traversal-shaped input where relevant;
- symlinked directories;
- symlink replacement/race where practical;
- sibling-prefix names;
- Windows-style paths where relevant;
- platform-specific separators.

Verify snapshot creation and recovery never escape the workspace.

Do not duplicate the complete filesystem containment suite; assert the snapshot boundary uses the canonical path-safety semantics.

---

## 22. Snapshot existence is not authorization

A snapshot is recovery material, not permission.

At the appropriate integration boundary verify:

- knowing a SnapshotId does not grant authorization;
- knowing an EditId does not grant authorization;
- possessing snapshot bytes does not grant authorization;
- provenance visibility does not bypass the capability/policy boundary;
- rollback still uses the authoritative authorization path.

Keep the authorization decision itself covered by Master Test Prompt #05; this prompt tests only the snapshot/provenance interaction.

---

## 23. Sensitive data handling

Use synthetic secrets and sensitive-looking file contents.

Verify metadata/errors do not unnecessarily expose:

- full file contents;
- credentials;
- API keys;
- environment secrets;
- unrelated host paths.

When exact content must be recovered for correctness, keep assertions inside isolated test fixtures and ensure test output does not print the entire secret.

Test malformed-state diagnostics for secret-safe behavior.

---

## 24. Serialization compatibility

Snapshot and provenance are durable contracts.

Test:

- current schema serialization;
- deserialize → serialize round trip;
- stable field names;
- required fields;
- unsupported schema version;
- missing required field;
- unknown field behavior according to the current serde contract;
- malformed type;
- truncated record.

Do not silently default a missing recovery-critical field into valid state.

If migration exists, test the repository's actual migration mechanism.

---

## 25. Independent integrity oracle

Do not calculate expected integrity solely by calling the same production helper.

For content, independently calculate SHA-256 and byte length from the fixture bytes.

For filesystem recovery, independently read the original fixture bytes.

For persisted state, independently inspect the durable representation.

For edit binding, construct two separate edits/snapshots and verify cross-selection fails.

The strongest acceptance condition is:

```text
snapshot accepted
+
independent bytes/hash/length match
+
exact edit/workspace binding
+
restart still succeeds
```

---

## 26. Human workflow acceptance tests

Automate complete scenarios.

### Workflow A — Single-file recovery material

```text
create original file
→ capture snapshot
→ modify file
→ resolve recovery material for exact edit
→ compare recovered bytes to original
```

### Workflow B — Multi-file snapshot

```text
create several files
→ capture one edit snapshot
→ resolve recovery material
→ verify every original byte sequence
```

### Workflow C — Two edits

```text
edit A → snapshot A
edit B → snapshot B
→ resolve A
→ verify only A's recovery material
```

### Workflow D — Corruption

```text
create valid snapshot
→ corrupt blob/manifest
→ restart
→ request recovery
```

Expected: fail closed.

### Workflow E — Restart

```text
publish snapshot + provenance
→ terminate process
→ start fresh process
→ recover exact bytes
```

Expected: identical recovery material.

### Workflow F — Workspace isolation

```text
workspace A → snapshot A
workspace B → snapshot B
→ attempt cross-workspace resolution
```

Expected: mismatch is rejected.

### Workflow G — Crash residue

```text
simulate incomplete publication
→ restart
→ enumerate/read snapshots
```

Expected: incomplete state is not treated as valid recovery material.

---

## 27. Property and fuzz testing

Use `proptest` or equivalent where it provides meaningful value.

Useful properties include:

- valid bytes round-trip exactly;
- hash/length metadata remains consistent;
- malformed manifests never validate as valid snapshots;
- malformed provenance never binds an unrelated edit;
- random unknown snapshot IDs never resolve valid records;
- snapshot IDs remain exact rather than fuzzy;
- serialization round trips preserve recovery semantics;
- corruption of any integrity-critical byte is detected.

Do not use fuzz/property tests merely to increase coverage counts.

---

## 28. Security regression suite

Every discovered snapshot/provenance integrity bug becomes a permanent regression test.

At minimum preserve:

- exact-byte recovery;
- manifest integrity;
- blob integrity;
- length/hash verification;
- edit-to-snapshot binding;
- workspace isolation;
- unknown-ID rejection;
- malformed-state fail closed;
- unsupported-schema rejection;
- publication consistency;
- crash residue cannot become valid;
- snapshot possession does not grant authorization;
- no sensitive data leakage;
- no cross-snapshot content mix.

---

## 29. No test theater

Do not:

- test only struct serialization and claim durable recovery is verified;
- mock SnapshotStore when real durable storage is available;
- compare normalized strings instead of exact bytes;
- trust the same production hash helper as the only integrity oracle;
- use a snapshot ID as proof of authorization;
- ignore persisted state;
- ignore restart;
- treat an orphan blob as a valid snapshot;
- swallow corruption errors;
- delete invalid state and call recovery successful;
- fabricate unimplemented CLI/API behavior;
- mark blocked runtime evidence as passed;
- modify unrelated master test prompts.

A snapshot suite is insufficient if it proves metadata exists but does not prove that exact recovery material survives persistence and corruption tests.

---

## 30. Test implementation requirements

Use existing repository conventions and helpers.

Prefer:

- unit tests for deterministic serialization/integrity primitives;
- integration tests against real SnapshotStore;
- real temporary files;
- subprocess/restart tests;
- independent SHA-256/byte-length oracles;
- corruption fixtures;
- concurrency tests;
- failure-injection tests;
- real edit/provenance integration where available.

Do not create a second snapshot store or integrity implementation in test code.

Every consequential snapshot test should assert:

1. operation/result;
2. durable state;
3. independently verified bytes/integrity;
4. relevant provenance relationship.

---

## 31. Execution gates

Run current project gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
```

Then execute the focused snapshot/provenance suite, including:

- exact-byte tests;
- manifest/blob integrity tests;
- edit binding tests;
- workspace isolation;
- persistence/restart;
- corruption/failure injection;
- publication consistency;
- concurrency;
- limit/boundary tests;
- relevant rollback integration boundary;
- relevant audit/authorization boundary without duplicating their complete suites.

Use current CI gates in addition to these where required.

---

## 32. Evidence reporting

Classify each feature:

### Passed
Direct evidence proves the behavior through the real snapshot/provenance boundary.

### Failed
The real test executed and exposed a production defect.

### Blocked
Execution was prevented by an external/environmental limitation.

### Unproven
The current implementation does not provide enough evidence.

For every failure record:

- test name;
- edit ID;
- snapshot ID;
- workspace;
- fixture paths;
- expected bytes/integrity;
- actual result;
- persisted state;
- restart state;
- relevant error;
- likely subsystem.

Never print real secrets or complete sensitive file contents.

For final-target behavior absent from the branch, explicitly report **Not implemented**.

---

## 33. Completion criteria

This prompt is complete only when:

- current SnapshotStore/provenance implementation was inspected;
- exact-byte snapshot fidelity was tested;
- entry identity/path semantics were tested;
- manifest/blob integrity was tested;
- edit-to-snapshot binding was tested;
- workspace isolation was tested;
- provenance lifecycle/outcomes were tested where implemented;
- recovery_view or equivalent canonical recovery read was tested;
- persistence across restart was proven;
- malformed/truncated/tampered state fails closed;
- publication/crash consistency was tested;
- concurrency was tested where applicable;
- resource limits were tested;
- path containment was tested at the snapshot boundary;
- snapshot existence was proven not to be authorization;
- sensitive data handling was checked;
- serialization compatibility was checked;
- independent integrity oracles were used;
- security regressions have permanent coverage;
- final-target gaps are honestly classified;
- no unrelated test master prompt was modified.

The objective is trustworthy evidence that AWH snapshot/provenance storage is a **durable, exact, integrity-verified, edit-bound recovery source** that cannot be confused with authorization and cannot silently accept corrupted, unrelated, or partially published state.

---

## 34. Scope boundary

This prompt owns **Snapshots & Provenance testing only**.

Do not create full feature suites for:

- Foundation and Distribution;
- Workspace Runtime;
- Agent-Grade Filesystem Editing;
- Git/worktrees;
- Capability and Policy Engine;
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

The rollback feature may be exercised only to prove the snapshot/provenance integration boundary. Full rollback behavior belongs to its own feature prompt.

Do not modify:

```text
docs/testing-prompts/01-foundation-distribution.md
docs/testing-prompts/02-workspace-runtime.md
docs/testing-prompts/03-agent-grade-filesystem-editing.md
docs/testing-prompts/04-git-and-worktrees.md
docs/testing-prompts/05-capability-and-policy.md
```

Do not modify any other existing test master prompt while executing this prompt.
