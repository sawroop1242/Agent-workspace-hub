# Master Test Prompt 07 — Rollback & Recovery

## Mission

Create and execute a complete, human-behavior-oriented verification suite for AWH's **edit-level Rollback & Recovery** feature on the current `rust` branch.

This prompt owns explicit rollback of one completed/eligible edit transaction: exact edit identity, eligibility, durable recovery-material consumption, authorization, produced-state conflict detection, safe restoration/removal, multi-file preflight, post-rollback verification, lifecycle/result semantics, idempotency, crash/restart behavior, and security boundaries.

Do not implement missing production behavior merely to make tests pass. Inspect the current branch first, test the real canonical service path, and classify gaps honestly.

**Do not treat `cargo test` alone as sufficient evidence.**

---

## 1. Feature boundary

The final AWH contract includes edit-level recovery/rollback through the controlled filesystem-edit lifecycle.

A rollback request should conceptually follow:

```text
exact EditId
→ eligibility check
→ canonical snapshot/provenance resolution
→ rollback authorization
→ complete target preflight
→ current-state vs produced-state conflict check
→ exact restoration/removal
→ post-rollback verification
→ provenance/audit correlation
→ authoritative result
```

This prompt must prove:

- only the exact requested edit can be rolled back;
- rollback is explicitly authorized;
- possession of an EditId or SnapshotId is not authority;
- canonical durable recovery material is used;
- corrupt or mismatched recovery material fails closed;
- every affected path is preflighted before mutation;
- newer external changes are never overwritten;
- existing files are restored byte-for-byte;
- files created by the edit are removed only when safely attributable to that edit;
- multi-file rollback does not discover conflicts after earlier files were already restored;
- actual filesystem state is verified after mutation;
- partial outcomes are reported honestly;
- repeated rollback is safe;
- restart/crash recovery cannot fabricate a successful rollback;
- path, symlink, authorization, provenance, and audit boundaries remain intact.

Do not duplicate complete suites for editing, snapshots/provenance, capability/policy, Git, filesystem containment, or audit. Test only the rollback/recovery boundary and its necessary integrations.

---

## 2. Current implementation versus target

Before writing tests, inspect the current `rust` branch.

The repository's current rollback implementation may contain concepts such as:

- `RollbackRecord`;
- `RollbackOutcome`;
- `RollbackResult`;
- `EditRollbackStatus`;
- `capture_rollback_records`;
- `rollback_edits`;
- `EditAction::Rollback`;
- `filesystem.rollback`;
- `SnapshotStore::recovery_view(edit_id)`.

These names are not assumptions that every feature is complete.

For every target behavior classify:

- Implemented and tested;
- Implemented but insufficiently tested;
- Not implemented;
- Blocked;
- Unproven.

Current source and current tests take precedence over roadmap/history.

---

## 3. Required repository forensics

Before modifying tests, inspect at minimum:

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
- `docs/implementation-prompts/04-*.md` through the current rollback implementation prompt
- `docs/testing-prompts/05-capability-and-policy.md`
- `docs/testing-prompts/06-snapshots-and-provenance.md`

Inspect current source and tests for:

- `src/services/edit.rs`;
- `src/services/files.rs`;
- `src/services/snapshot.rs`;
- `src/services/authorization.rs`;
- service composition/modules;
- CLI/MCP adapters exposing rollback;
- audit/provenance integration;
- rollback-related tests.

Search for:

```text
rollback
recovery
RollbackRecord
RollbackOutcome
RollbackResult
EditRollbackStatus
rollback_edits
capture_rollback_records
EditAction::Rollback
filesystem.rollback
SnapshotStore
recovery_view
after_hash
before_bytes
conflict
TOCTOU
```

Classify each relevant implementation as canonical, adapter, test, legacy/compatibility, duplicate, or unrelated.

---

## 4. Human-first test model

Every important test must represent a realistic user/agent workflow.

The core acceptance scenario is:

```text
agent performs controlled edit
→ edit completes
→ AWH records recovery material
→ user/agent requests rollback of exact edit
→ AWH authorizes rollback
→ AWH checks current filesystem state
→ AWH restores/removes affected resources
→ AWH verifies actual filesystem
```

Do not replace this with a test that only constructs `RollbackRecord` and calls a pure helper.

For every successful rollback independently verify:

1. correct edit was selected;
2. authorization succeeded;
3. original bytes/state were restored;
4. edit-created resources were removed when appropriate;
5. unrelated resources were unchanged;
6. result status accurately describes the outcome.

---

## 5. Isolated disposable environment

Every rollback test must use disposable isolated state.

Use unique:

- workspace root;
- AWH persistent state directory;
- edit IDs;
- agent/session identities;
- fixture files;
- repositories where needed.

Never use:

- developer-global AWH state;
- production repositories;
- personal credentials;
- real secrets;
- a shared mutable workspace between unrelated tests.

Restart tests must preserve only the disposable fixture state.

Avoid concurrent mutation of process-global environment variables.

---

## 6. Exact edit identity and eligibility

Rollback must consume an exact edit identity.

Test:

- valid EditId;
- unknown EditId;
- malformed EditId;
- empty EditId;
- another valid EditId;
- edit from another workspace;
- edit with missing recovery material;
- edit with invalid/corrupt recovery material;
- edit with no rollback eligibility;
- edit whose lifecycle does not permit explicit rollback.

Verify invalid/ineligible requests:

- fail deterministically;
- do not mutate any filesystem resource;
- do not fall back to latest edit;
- do not select by path;
- do not select by timestamp;
- do not select an arbitrary matching snapshot.

For two edits:

```text
edit A → snapshot A
edit B → snapshot B
rollback A
```

must restore only A's recovery state.

---

## 7. Authorization boundary

Rollback is consequential filesystem mutation.

Use the current authoritative authorization path, expected where implemented to include:

```text
EditAction::Rollback
→ filesystem.rollback
```

Test:

- authorized principal;
- missing capability;
- revoked capability;
- expired capability where implemented;
- out-of-scope capability;
- policy denial;
- wrong agent identity;
- wrong session identity;
- wrong workspace binding;
- malformed/unavailable authorization state.

For every denial verify:

- rollback does not mutate files;
- no created file is deleted;
- no existing file is restored;
- no Git state is changed merely by rollback;
- no permissive fallback occurs.

Do not use possession of EditId, SnapshotId, route name, workspace path, or provenance record as authorization.

Do not duplicate the full Capability/Policy suite from Master Test Prompt #05.

---

## 8. Canonical recovery material

Rollback must consume verified recovery material from the canonical snapshot/provenance boundary.

Where `SnapshotStore::recovery_view(edit_id)` or equivalent exists, use it.

Test:

- valid snapshot;
- missing snapshot;
- missing blob;
- corrupted blob;
- wrong content hash;
- wrong byte length;
- malformed manifest;
- unsupported schema;
- mismatched snapshot ID;
- snapshot belonging to another edit;
- snapshot belonging to another workspace;
- mismatched path entry;
- incomplete multi-file snapshot;
- malformed provenance;
- provenance pointing to the wrong snapshot.

Every recovery-material failure must cause:

```text
rollback denied/failed
→ zero filesystem mutation
```

Do not fall back to current filesystem contents, Git history, guessed snapshots, or lossy text conversion.

Master Test Prompt #06 owns the full snapshot/provenance suite; this prompt verifies only the rollback consumption boundary.

---

## 9. Existing versus created resources

Explicitly distinguish:

### Existing before edit

```text
original bytes
→ edit changes file
→ rollback
→ exact original bytes
```

### Missing before edit

```text
missing
→ edit creates file
→ rollback
→ file absent
```

### Empty existing file

```text
empty file exists
→ edit modifies it
→ rollback
→ empty file still exists
```

Test all three.

Also test:

- file changed from empty to non-empty;
- file changed from non-empty to empty;
- file renamed/moved where the current edit model supports it;
- multiple created files;
- multiple pre-existing files.

Do not treat “missing” and “zero-byte existing” as equivalent.

---

## 10. Exact-byte restoration

Rollback restoration must preserve raw bytes.

Use fixture content containing:

- ASCII;
- UTF-8;
- Devanagari;
- emoji;
- tabs;
- repeated spaces;
- trailing whitespace;
- LF;
- CRLF;
- no final newline;
- final newline;
- binary bytes;
- embedded NUL bytes where the current filesystem/edit model supports them.

Verify with raw byte reads.

Do not normalize:

- line endings;
- Unicode;
- whitespace;
- final newline;
- encoding.

If the current edit operation is text-only, rollback must still use the canonical exact recovery bytes rather than reconstructing text from a lossy representation.

---

## 11. Produced-state conflict detection

Rollback must never overwrite a newer external change.

For each affected path establish the transaction-produced state.

Where current implementation uses `after_hash`, compare current bytes against that expected produced state. Where a canonical `FileState` comparison exists, use it.

Test:

- unchanged produced state → rollback permitted;
- content changed after edit → conflict;
- same-size different content → conflict;
- file deleted after edit → conflict;
- file replaced after edit → conflict;
- metadata/state change relevant to current contract → conflict;
- created file changed externally → do not delete it;
- external change in one of several files → complete preflight must fail before mutation.

For conflict cases verify:

- conflicting resource remains untouched;
- unrelated resources remain untouched;
- no best-effort overwrite occurs;
- result explicitly identifies conflict.

---

## 12. Multi-file preflight

For an edit touching multiple files, rollback must inspect all targets before beginning restoration.

Test:

```text
file A = safe
file B = safe
file C = externally changed
→ rollback request
```

Expected:

```text
preflight detects C
→ no A/B/C mutation
→ conflict result
```

Also test:

- conflict on first resource;
- conflict on middle resource;
- conflict on final resource;
- missing target;
- invalid target;
- corrupt recovery entry;
- duplicate path;
- duplicate resource identity.

Do not accept a rollback that restores earlier files and then discovers a later conflict unless the current implementation explicitly documents and safely represents partial rollback.

---

## 13. Restoration planning

Where the current implementation builds a rollback plan, test that it is derived only from verified canonical data.

For each target assert the plan correctly distinguishes:

- restore existing file;
- delete edit-created file;
- already absent created file;
- conflict;
- invalid/corrupt recovery state.

Do not recreate a second rollback planning algorithm in tests.

Tests should inspect final observable behavior, using plan-level assertions only where the plan is an actual public/domain contract.

---

## 14. Canonical filesystem mutation

Rollback must reuse the canonical FilesService/filesystem mutation primitives.

At the real boundary verify:

- workspace containment remains enforced;
- relative-path validation remains enforced;
- symlink protections remain enforced;
- atomic write semantics are preserved;
- delete operations are guarded;
- resource limits remain respected.

Do not test a rollback-specific raw `std::fs::write` implementation.

Where practical, instrument or independently observe the canonical service path to prove rollback does not bypass it.

---

## 15. Path traversal and symlink attacks

Treat persisted rollback paths as untrusted recovery data.

Test:

- absolute path;
- `..`;
- repeated separators;
- root/prefix path;
- traversal-shaped persisted path;
- encoded traversal where the interface decodes input;
- symlinked directory;
- symlink replacement;
- sibling-prefix path such as `project-a` versus `project-ab`;
- platform-specific separators.

Verify:

- no resource outside workspace changes;
- malicious recovery material is rejected;
- legitimate in-workspace paths continue to work.

Do not duplicate the complete filesystem security suite; verify rollback's use of the canonical path-safety boundary.

---

## 16. Post-rollback verification

A successful filesystem write/delete is not proof of successful rollback.

After rollback:

- read every affected resource through the canonical filesystem boundary;
- compare against exact pre-edit state;
- verify absence for edit-created resources;
- verify no unrelated resource changed;
- verify the rollback result matches observed state.

Test verification failure where fault injection is possible.

If verification fails, the result must not claim full successful restoration.

---

## 17. Result and lifecycle semantics

Inspect current `RollbackOutcome`, `RollbackResult`, and `EditRollbackStatus` vocabulary.

Test every implemented outcome.

Typical states may include:

- Restored;
- AlreadyRolledBack;
- Conflict;
- Failed.

Do not invent enum values.

Verify:

- successful rollback reports successful restoration;
- conflict is distinguishable from generic failure;
- partial restoration is represented honestly;
- repeated rollback has deterministic semantics;
- an unknown/ineligible edit does not report success;
- result data identifies the exact edit.

---

## 18. Idempotent repeated rollback

Test:

### First rollback succeeds

```text
rollback A
→ exact original state
rollback A again
```

The second request must not overwrite unrelated newer changes.

### First rollback conflicts

```text
external modification
→ rollback A = conflict
→ rollback A again
```

The external modification must remain untouched.

### Partial rollback

If partial rollback is supported:

```text
partial result
→ fresh rollback request
→ inspect actual state again
```

Do not trust a previous result as current filesystem truth.

---

## 19. Restart and persistence

Rollback behavior must survive process boundaries.

Test:

1. create edit and durable recovery state;
2. terminate AWH;
3. start a fresh process;
4. request rollback;
5. verify exact restoration.

Also test restart after:

- snapshot lookup;
- authorization;
- conflict detection;
- partial rollback;
- successful rollback.

Where interruption can be injected, verify a subsequent process:

- reads durable truth;
- does not fabricate a successful result;
- rechecks current filesystem state;
- preserves external changes;
- handles partially restored state honestly.

Do not rely on an in-memory `RollbackRecord` surviving within one process as persistence evidence.

---

## 20. Crash/fault injection

Use disposable fixtures to induce failures at rollback stages:

- before authorization;
- after authorization but before mutation;
- during snapshot resolution;
- during preflight;
- during first restoration;
- between multi-file restorations;
- during created-file deletion;
- during post-rollback verification;
- during provenance/audit recording.

After each injected failure:

1. restart;
2. inspect actual filesystem;
3. inspect durable rollback/provenance state;
4. retry where supported;
5. verify no external change is overwritten;
6. verify the result reflects actual residual state.

Do not call a partially restored workspace “successful” without verification.

---

## 21. Concurrency and TOCTOU

Test controlled races between rollback and external modification.

Where the implementation provides synchronization hooks, use them.

At minimum test:

- writer changes target after preflight;
- writer changes target before commit;
- two rollback requests for the same edit;
- rollback of independent edits;
- snapshot read concurrent with rollback;
- lock contention.

Expected security property:

```text
newer external state
→ rollback must not overwrite it
```

A race that cannot be deterministically reproduced should still have a documented test strategy; do not rely on arbitrary sleeps.

---

## 22. Workspace and resource isolation

Create:

- workspace A;
- workspace B;
- edit A;
- edit B.

Verify:

- rollback A affects only workspace A;
- rollback B affects only workspace B;
- same relative paths across workspaces remain isolated;
- an EditId from A cannot select B's recovery material;
- a SnapshotId from B cannot be substituted for A.

Also test unrelated files inside the same workspace remain unchanged.

---

## 23. Agent/session identity

Where the current rollback boundary carries agent/session identity, verify it is preserved and enforced.

Test:

- correct agent/session;
- wrong agent;
- wrong session;
- stale session;
- disabled/inactive agent where applicable;
- mismatched workspace identity.

Do not duplicate full AgentProfile/AgentSession tests. The purpose here is to prove rollback does not lose identity context or use a caller-supplied identity as authority.

---

## 24. Provenance and audit integration

Rollback must correlate with existing provenance/audit systems without creating replacements.

Where implemented, verify successful rollback records:

- exact EditId;
- snapshot/provenance correlation;
- workspace;
- agent/session where permitted;
- rollback outcome;
- affected resources/correlation data;
- timestamp where supported.

For denial/conflict/failure verify the appropriate outcome is observable.

Audit/provenance records must not contain:

- file contents;
- API keys;
- authentication tokens;
- synthetic secret values except where the product explicitly permits safe test labels.

Do not make filesystem rollback depend on a second audit implementation.

---

## 25. Sensitive-data handling

Use synthetic sensitive-looking content.

Verify user-visible errors and rollback result output do not expose:

- complete file contents;
- credentials;
- API keys;
- unrelated absolute host paths;
- internal stack traces.

For exact-byte tests, assert bytes internally without printing complete fixture contents in failure messages.

---

## 26. Limits and pathological inputs

Use actual limits from `docs/configuration.md` and current source.

Test:

- empty file;
- one-byte file;
- maximum allowed recovery content;
- just-over-limit content;
- many affected files;
- deeply nested paths;
- long valid path components;
- oversized recovery metadata where applicable.

Verify rejected rollback state cannot cause:

- partial unsafe mutation;
- resource-limit bypass;
- memory exhaustion from unbounded recovery loading;
- false success.

Do not invent limits.

---

## 27. Independent oracles

Do not derive expected results only by invoking the same production rollback helper.

Use independent observations:

- direct raw-byte filesystem reads;
- existence checks;
- independently calculated SHA-256;
- independent Git `rev-parse`/status when relevant;
- durable-state inspection;
- audit/provenance readback.

For every successful rollback, the strongest evidence is:

```text
exact EditId selected
+
authorization allowed
+
all targets preflight-safe
+
actual filesystem == exact pre-edit state
```

For every denial/conflict:

```text
decision denied/conflicted
+
affected resources unchanged
```

---

## 28. Human workflow acceptance scenarios

Automate complete end-to-end scenarios.

### Workflow A — Existing file

```text
create original bytes
→ perform controlled edit
→ rollback exact edit
→ read file
```

Expected: original bytes restored exactly.

### Workflow B — Created file

```text
file absent
→ controlled edit creates file
→ rollback exact edit
```

Expected: file absent again.

### Workflow C — Empty existing file

```text
create empty file
→ edit it
→ rollback
```

Expected: empty file exists.

### Workflow D — External change

```text
perform edit
→ modify result externally
→ rollback
```

Expected: conflict; external change preserved.

### Workflow E — Multi-file conflict

```text
edit A/B/C
→ externally modify C
→ rollback
```

Expected: complete preflight detects C before restoring A/B.

### Workflow F — Revoked authorization

```text
perform edit
→ revoke rollback capability
→ request rollback
```

Expected: denial; filesystem unchanged.

### Workflow G — Corrupt recovery material

```text
perform edit
→ corrupt snapshot/provenance
→ request rollback
```

Expected: recovery failure; filesystem unchanged.

### Workflow H — Restart

```text
perform edit
→ terminate AWH
→ start fresh AWH
→ rollback
```

Expected: exact recovery succeeds from durable state.

### Workflow I — Repeat

```text
rollback edit
→ make unrelated newer change
→ rollback same edit again
```

Expected: no overwrite of the newer change.

---

## 29. Property and boundary testing

Use `proptest` where meaningful.

Useful properties:

- unknown EditIds never resolve a valid edit;
- malformed IDs never select a different edit;
- corrupted produced-state hashes never authorize restoration;
- random path traversal forms never escape workspace;
- arbitrary non-matching current bytes cause conflict;
- exact snapshot bytes round-trip through rollback unchanged;
- existing-empty and missing states remain distinct;
- repeated rollback never overwrites unrelated current state.

Do not use generated tests merely to inflate counts.

---

## 30. Security regression suite

Every rollback bypass or data-loss defect becomes a permanent regression test.

At minimum preserve:

- exact edit identity;
- authorization before mutation;
- snapshot/edit/workspace binding;
- corruption fail closed;
- path containment;
- symlink protection;
- complete multi-file preflight;
- produced-state conflict detection;
- external-change preservation;
- exact-byte restoration;
- safe created-file deletion;
- post-rollback verification;
- honest partial outcomes;
- repeated rollback safety;
- restart/crash safety;
- no secret leakage;
- provenance/audit correlation without authorization bypass.

---

## 31. No test theater

Do not:

- test only `RollbackRecord` construction;
- test only a boolean result;
- mock the entire filesystem when real temporary files are available;
- restore files using test code and call that production rollback;
- compare normalized text instead of exact bytes;
- select snapshots by timestamp/path/name;
- use EditId possession as authorization;
- bypass the real authorization boundary;
- skip the produced-state conflict check;
- restore file A before proving file B is safe;
- trust an in-memory record as persistence;
- swallow crash/failure errors;
- convert partial restoration into success;
- mark blocked tests passed;
- fabricate unimplemented CLI/MCP commands;
- modify any other master test prompt.

A rollback test suite is insufficient if it proves only that a helper can write bytes back; it must prove the real safety boundary prevents data loss.

---

## 32. Test implementation requirements

Use existing test conventions and helpers.

Prefer:

- unit tests for deterministic rollback-result/state logic;
- integration tests through the canonical EditService;
- real temporary files;
- real persistent SnapshotStore;
- real authorization boundary;
- subprocess/restart tests;
- exact-byte independent oracles;
- corruption fixtures;
- concurrency/fault-injection tests;
- CLI/MCP integration only where those interfaces currently expose rollback.

Do not create:

- a second rollback engine;
- a second snapshot store;
- a second policy evaluator;
- a second path resolver;
- a second audit log.

Every consequential rollback test should assert:

1. authoritative result;
2. actual filesystem state;
3. relevant durable recovery/provenance state;
4. authorization/audit evidence where implemented.

---

## 33. Execution gates

Run current project gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Then run focused rollback/recovery tests for:

- exact EditId;
- eligibility;
- authorization;
- snapshot/provenance binding;
- exact bytes;
- missing-vs-empty;
- created-file removal;
- produced-state conflict;
- multi-file preflight;
- post-rollback verification;
- repeated rollback;
- path/symlink safety;
- persistence/restart;
- crash/fault injection;
- concurrency/TOCTOU;
- security regressions.

Run additional repository-required CI gates where applicable.

Report commands only as passed if they actually completed successfully.

---

## 34. Evidence reporting

Classify every rollback behavior as:

### Passed
Real rollback behavior was executed and independently verified.

### Failed
The real test executed and exposed a production defect.

### Blocked
Execution could not occur because of an external/environmental prerequisite.

### Unproven
Available implementation/evidence is insufficient.

For each failure report:

- exact EditId;
- workspace;
- agent/session context where relevant;
- affected resources;
- authorization state;
- snapshot/provenance state;
- expected result;
- actual result;
- filesystem state before/after;
- rollback lifecycle state;
- relevant error;
- likely subsystem.

Never include secret values or complete sensitive file contents.

For absent final-target behavior explicitly report **Not implemented**.

---

## 35. Completion criteria

This prompt is complete only when:

- current rollback implementation and ownership were inspected;
- exact EditId selection was tested;
- rollback eligibility was tested;
- authorization was proven before mutation;
- canonical snapshot/provenance recovery material was consumed;
- corrupt/mismatched recovery material fails closed;
- existing files restore byte-for-byte;
- missing-vs-empty semantics are preserved;
- created files are removed only when safely attributable;
- produced-state conflict detection is tested;
- external changes are never overwritten;
- all multi-file targets are preflighted before mutation;
- canonical filesystem safety is reused;
- post-rollback filesystem state is independently verified;
- result/lifecycle semantics are tested;
- repeated rollback is safe;
- persistence/restart behavior is proven;
- crash/fault behavior is tested;
- concurrency/TOCTOU behavior is tested where practical;
- workspace and identity isolation is tested;
- provenance/audit integration is checked without duplication;
- sensitive data is protected;
- relevant limits are tested;
- permanent security regressions exist;
- final-target gaps are honestly classified;
- no unrelated master test prompt was modified.

The objective is trustworthy evidence that AWH explicit rollback is a **single canonical, authorized, conflict-aware, exact-byte, verified recovery operation** that restores only the requested completed edit and never silently destroys newer external work.

---

## 36. Scope boundary

This prompt owns **Rollback & Recovery testing only**.

Do not create full feature suites for:

- Foundation and Distribution;
- Workspace Runtime;
- Agent-Grade Filesystem Editing;
- Git/worktrees;
- Capability and Policy Engine;
- Snapshots/Provenance;
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

Snapshot/provenance, authorization, filesystem safety, and audit may be exercised only at their rollback integration boundaries. Their complete suites belong to their own master test prompts.

Do not modify:

```text
docs/testing-prompts/01-foundation-distribution.md
docs/testing-prompts/02-workspace-runtime.md
docs/testing-prompts/03-agent-grade-filesystem-editing.md
docs/testing-prompts/04-git-and-worktrees.md
docs/testing-prompts/05-capability-and-policy.md
docs/testing-prompts/06-snapshots-and-provenance.md
```

Do not modify any other existing test master prompt while executing this prompt.
