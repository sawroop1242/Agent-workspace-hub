# AWE-015 / #36 — Agent-Grade Editing Test Suite

> **Status:** Issue-resolving master prompt  
> **Branch:** `rust`  
> **Primary scope:** prove the production agent-grade editing safety contract with behavior-focused tests, real filesystem boundaries, failure injection, concurrency/stale-state scenarios, and transport-adapter validation.  
> **Dependencies:** AWE-004 / #25, AWE-006 / #27, AWE-007 / #28, AWE-008 / #29, AWE-010 / #31; integrate with AWE-011 / #32, AWE-012 / #33, AWE-013 / #34, and AWE-014 / #35 only where those already exist on the target branch.

## 1. Mission

Build the authoritative agent-grade editing test suite for AWH.

This issue is **not** a request to increase test count. It is a request to prove the safety properties on which autonomous filesystem editing depends.

The test suite must demonstrate, against the real implementation:

```text
invalid/conflict/policy-denied → zero mutation

prepare failure → every affected file unchanged

apply → verify → rollback → original bytes/state

stale concurrent read → conflict, not silent overwrite

MCP/CLI request → canonical EditService semantics
```

Tests must be strong enough that a future refactor which accidentally bypasses validation, weakens conflict detection, performs partial mutation, restores the wrong bytes, or creates transport-specific edit behavior fails deterministically in CI.

Do not “fix” a failing test by weakening its assertion. If a required invariant cannot currently be proven, identify the production defect or missing seam and implement only the minimum production change necessary to make the invariant testable and correct.

---

## 2. Forensic baseline — establish reality before editing

Before writing tests, inspect the current `rust` branch and document the actual implementation rather than relying on roadmap text.

At minimum inspect:

```text
src/services/edit.rs
src/services/
src/mcp/
src/cli/
src/tui/
docs/CLI.md
docs/PROJECT_ROADMAP.md
```

Search for every current caller and implementation of:

- `EditTransaction`
- `EditOperation`
- `ExpectedState`
- `FileState`
- `EditStatus`
- `EditError`
- `EditService`
- `workspace.write_file`
- `filesystem.patch`
- `filesystem.read`
- replace/insert/delete-range/patch/apply-diff/rollback
- snapshot/provenance APIs
- policy/capability authorization
- audit recording
- CLI `fs` editing commands
- MCP editing tools

The current canonical model in `src/services/edit.rs` contains `EditTransaction`, `EditOperation`, `ExpectedState`, `FileState`, lifecycle statuses, structured shape-validation errors, and path validation. The existing model-level tests are not sufficient proof of filesystem mutation safety. fileciteturn162file0

The issue body explicitly states that the repository has a large general test suite but lacks production edit-executor tests proving these safety invariants. Treat the absence of executor behavior tests as the central forensic gap. fileciteturn158file0

Do not assume that a documented CLI command or roadmap entry proves that its implementation exists. The project roadmap explicitly sequences implementation, unit tests, integration tests, terminal validation, failure/recovery validation, and documentation as separate stages. fileciteturn159file1

### Required forensic output before implementation

Identify:

1. the canonical mutation service,
2. the actual filesystem abstraction used by it,
3. the existing test organization,
4. current test helpers/fixtures,
5. how temporary workspaces are created,
6. how MCP requests are tested,
7. how CLI commands are tested,
8. how policy/capability checks are invoked,
9. how snapshots/rollback are persisted and restored,
10. how audit records are observed,
11. whether failure injection already exists,
12. whether property-based testing infrastructure already exists,
13. whether platform-specific tests already have conventions.

Do not create duplicate test infrastructure when an existing reusable harness can safely be extended.

---

## 3. Non-negotiable architecture

### 3.1 Test the canonical service, not duplicate implementations

All production edit behavior must be tested through the same canonical service used by MCP, CLI, TUI, and Control API.

The suite must reject architecture where:

- MCP has its own editing algorithm,
- CLI has its own editing algorithm,
- tests directly mutate files in a way production does not,
- one adapter bypasses conflict detection,
- rollback is reconstructed independently in a transport layer.

The project architecture treats the edit model as transport-independent shared vocabulary. fileciteturn162file0

### 3.2 Real filesystem proof is mandatory

For filesystem safety guarantees, use real temporary directories and real files.

Mocks may be used for narrow unit seams such as injected failures, policy decisions, or deterministic service dependencies, but a mock-only test is never sufficient to prove:

- atomic file replacement,
- path containment,
- symlink behavior,
- byte restoration,
- multi-file mutation boundaries,
- stale-state detection against actual file changes,
- newline/encoding behavior,
- persistence/restart behavior.

This directly follows the acceptance requirement that filesystem guarantees cannot be proven solely by mocks. fileciteturn158file0

### 3.3 No test-only alternate semantics

Do not introduce a simplified “test editor” whose semantics differ from production.

Test helpers may:

- create isolated workspaces,
- write known fixture bytes,
- capture file state,
- compare directory trees,
- inject deterministic failures,
- invoke production services.

They must not silently implement a second edit engine.

---

## 4. Define the test oracle before writing cases

Create a precise invariant vocabulary.

### Invariant A — invalid/conflict/policy denial is mutation-free

For every operation capable of mutation:

```text
request
→ validation / authorization / state check
→ failure
→ exact filesystem tree equals pre-request tree
```

Verify exact bytes, not merely hashes, whenever the test concerns mutation safety.

Cover:

- malformed transaction,
- empty transaction,
- invalid path,
- traversal path,
- absolute path,
- invalid line range,
- empty replacement match,
- malformed unified diff,
- missing target,
- wrong expected hash,
- wrong expected size,
- wrong expected line count,
- missing/incorrect contextual match,
- stale file changed between read and apply,
- policy denial,
- capability denial,
- expired capability where applicable,
- unauthorized rollback,
- verification failure before commit where the architecture exposes that boundary.

### Invariant B — successful edit has a verified post-state

For every successful operation:

```text
before state
→ prepare
→ apply
→ verify
→ committed result
```

The test must assert the actual resulting bytes and the resulting `FileState` rather than merely asserting `Ok(...)`.

### Invariant C — rollback restores the exact prior state

For an edit that successfully applies and is then rolled back:

```text
original bytes
→ apply
→ verified after bytes
→ rollback(edit_id)
→ exact original bytes
```

The final state must be byte-identical to the original state.

Do not accept semantic equivalence as sufficient when exact restoration is the contract.

### Invariant D — multi-file preparation failure causes no partial mutation

For a transaction affecting multiple files:

```text
prepare all
→ one preparation/validation/conflict failure
→ commit nothing
→ every affected file remains byte-identical
```

This must be proven with at least two files where the failure occurs after another file would otherwise be writable.

### Invariant E — stale concurrent state produces a conflict

At minimum prove:

```text
read expected state
→ external process/thread modifies file
→ edit submitted with stale expected state
→ conflict
→ external bytes preserved
```

The test must prove the implementation does not silently overwrite the newer state.

---

## 5. Unit-test matrix for every edit primitive

Build focused unit tests for each supported operation.

### 5.1 Replace

Test:

- exact single occurrence,
- multiple occurrences,
- explicit occurrence selection,
- zero matching occurrences,
- ambiguous/multiple matches when uniqueness is required,
- empty `old` rejection,
- replacement with empty `new`,
- replacement that changes file size,
- replacement that changes line count,
- replacement containing Unicode,
- replacement containing newline sequences,
- expected hash mismatch,
- expected size mismatch,
- expected line-count mismatch,
- context mismatch,
- successful post-state verification.

### 5.2 Insert

Respect the canonical semantics:

- `line = 0` inserts at beginning,
- one-based boundary semantics,
- insertion before a requested line,
- insertion at EOF,
- insertion into empty file,
- insertion with/without final newline,
- multiline content,
- CRLF input,
- LF input,
- Unicode/Devanagari/emoji.

Explicitly test that line numbering is not accidentally interpreted as zero-based indexing.

### 5.3 DeleteRange

Test:

- single-line deletion,
- first-line deletion,
- last-line deletion,
- full-file deletion,
- deletion from a one-line file,
- invalid zero start/end,
- reversed range,
- range beyond EOF,
- empty file,
- CRLF/LF,
- final-newline preservation semantics,
- Unicode lines.

### 5.4 Patch

Test patch as the canonical precomputed replacement path, not as an independent editor.

Verify:

- expected state is checked,
- old content is checked,
- successful replacement,
- stale state rejection,
- zero mutation on preparation failure,
- post-edit verification,
- transaction identity and lifecycle.

### 5.5 ApplyDiff

Test through the canonical unified-diff implementation.

Cover:

- one hunk,
- multiple hunks,
- multiple files,
- context mismatch,
- malformed diff,
- invalid target path,
- path traversal headers,
- unsupported/binary content behavior,
- CRLF/LF,
- no-final-newline cases,
- zero mutation if any hunk cannot be prepared,
- successful verification.

Do not reimplement a diff parser in tests.

---

## 6. ExpectedState and conflict test suite

`ExpectedState` currently supports hash, context, size, and line-count preconditions. The tests must prove that these fields have meaningful enforcement semantics where the service claims to support them. fileciteturn162file0

Create independent tests for:

1. correct hash → edit proceeds,
2. wrong hash → conflict and zero mutation,
3. correct size + wrong hash → conflict,
4. wrong size → conflict,
5. wrong line count → conflict,
6. correct global state but incorrect local context → conflict where contextual protection applies,
7. multiple expected states → correct operation-to-state association,
8. missing expected state → behavior matches the authoritative contract rather than silently bypassing required protection,
9. stale state on one file in a multi-file transaction → no files mutated,
10. external file modification between state observation and mutation → conflict or safe failure according to the canonical service boundary.

Do not assert only an error variant. Also capture the file tree before the operation and prove it is unchanged after the failure.

Where structured conflict metadata exists, assert:

- edit ID,
- path,
- expected state,
- observed/current state,
- operation identity,
- machine-readable conflict classification.

Avoid asserting unstable human-readable error prose unless it is explicitly part of the public contract.

---

## 7. Atomicity and rollback suite

### 7.1 Single-file atomicity

Inject or induce failures at each meaningful stage:

```text
validation
snapshot
prepare
write
rename/replace
verification
commit/finalization
```

Where the architecture supports deterministic failure injection, use it instead of timing-based filesystem races.

For every pre-commit failure, assert:

- original bytes remain intact,
- no temporary file becomes the visible target,
- no partial content is observable,
- lifecycle/result identifies the failure correctly.

### 7.2 Multi-file transaction atomicity

Use at least three files for stronger ordering coverage.

Test failures:

- before first mutation,
- after first prepared file,
- after one file has been applied,
- after multiple files have been applied,
- during verification,
- during recovery/rollback.

The suite must distinguish:

- preparation failure,
- commit failure,
- recovery/rollback failure,
- post-commit verification failure.

Do not collapse all failures into one generic assertion.

### 7.3 Exact rollback

For every rollback test:

1. create original bytes,
2. capture the authoritative pre-edit state,
3. apply edit,
4. verify the resulting bytes,
5. invoke rollback by the authoritative `EditId`,
6. verify the final bytes are exactly the original bytes,
7. verify final metadata/state where applicable.

Test rollback after:

- replace,
- insert,
- delete-range,
- patch,
- multi-file patch,
- newly created target where supported,
- target deletion/replacement between edit and rollback,
- stale target before rollback.

The rollback test must prove conflict-aware behavior: if the target changed after the edit, rollback must not overwrite unrelated newer bytes merely to restore an old snapshot.

---

## 8. Snapshot/provenance and audit verification

Where AWE-012 and AWE-013 are implemented on the branch, tests must verify their integration without duplicating their implementations.

For a successful edit assert that the appropriate provenance chain can be correlated:

```text
edit_id
→ pre-edit snapshot/reference
→ operation
→ before state
→ after state
→ rollback/audit history
```

Test:

- snapshot is associated with the correct edit ID,
- restart does not destroy required rollback state,
- snapshot bytes are not accidentally replaced by post-edit bytes,
- corrupted snapshot data fails safely,
- audit contains correlation identifiers but not file contents or secrets,
- repeated rollback has deterministic behavior.

Do not create an alternative snapshot store or audit store inside the test suite.

---

## 9. Filesystem boundary and security suite

Use real temporary directories.

### Paths

Test:

- valid relative path,
- empty path,
- absolute path,
- `../escape`,
- nested traversal,
- encoded-looking traversal strings where relevant,
- workspace root escape,
- invalid path components.

### Symlinks

Where the platform supports symlinks:

- symlink to a file outside workspace,
- symlinked directory outside workspace,
- symlink substitution between validation and mutation where safely reproducible,
- symlink to an in-workspace target,
- broken symlink.

Assert fail-closed behavior for escape attempts.

Mark platform-specific cases explicitly rather than silently skipping them.

### TOCTOU

Tests must acknowledge that a user-space precheck cannot alone eliminate all races. Prove the strongest atomic boundary the implementation actually provides.

At minimum test a concurrent mutation scenario and verify that the service does not silently overwrite an externally changed file when its state precondition has become stale.

---

## 10. Encoding, newline, and boundary matrix

The suite must use real byte fixtures, not only Rust string literals normalized through helpers.

Cover:

| Case | Required |
|---|---|
| empty file | yes |
| one line, no final newline | yes |
| one line, final newline | yes |
| multiple LF lines | yes |
| multiple CRLF lines | yes |
| mixed newline input | where supported/defined |
| UTF-8 ASCII + Unicode | yes |
| Devanagari | yes |
| emoji / multi-byte scalar values | yes |
| zero-byte file | yes |
| large-but-bounded fixture | yes |
| very long line | yes |

Never compute expected byte offsets by assuming one Unicode character equals one byte.

Verify both content and `FileState.size`/`line_count` semantics.

---

## 11. Property-based testing

Introduce property-based tests only if they fit the repository's existing test dependencies and conventions. Do not add a heavyweight dependency merely for cosmetic coverage.

If a property-testing framework is already present, use it for invariants such as:

### Operation composition

For valid non-overlapping operations:

```text
apply(ops) == deterministic sequential semantics
```

where the canonical service defines the expected ordering.

### Invalid transaction preservation

For generated invalid transactions:

```text
execute(tx) == failure
filesystem_after == filesystem_before
```

### Rollback invariant

For generated supported edits:

```text
rollback(apply(original, edit)) == original
```

subject to documented conflict/recovery boundaries.

### Determinism

Equivalent prepared inputs must produce equivalent resulting bytes and states.

### No accidental path escape

Generated relative paths must never cause writes outside the workspace.

Do not generate unbounded strings/files that can make CI unreliable. Bound sizes and operation counts deliberately.

---

## 12. Failure injection design

Failure injection must be deterministic and reproducible.

Prefer explicit seams such as:

```text
FailAt::Prepare
FailAt::Snapshot
FailAt::Apply { file_index }
FailAt::Verify { file_index }
FailAt::Commit
FailAt::Rollback
```

Use whatever abstraction already exists in the implementation; do not invent a production API solely to satisfy a test if an existing dependency seam is sufficient.

If a production failure-injection seam is genuinely required, keep it:

- deterministic,
- test-only or explicitly controlled,
- impossible to activate accidentally in normal production execution,
- independent of wall-clock sleeps.

The tests must prove recovery behavior rather than merely prove that an injected error is returned.

---

## 13. Concurrent/stale-read tests

At least one test must model two actors:

```text
Actor A: read file + capture ExpectedState
Actor B: modify file
Actor A: submit edit with stale ExpectedState
```

Use threads/processes according to the actual service boundary.

Prefer a deterministic synchronization primitive over arbitrary `sleep()` calls.

Assert:

- Actor A receives a conflict/safe failure,
- Actor B's bytes remain intact,
- no stale write occurs,
- audit/provenance identifies the conflict where those facilities exist.

Add a stronger race test if the filesystem abstraction permits deterministic coordination between validation and commit.

---

## 14. MCP adapter tests

Once the MCP editing surface exists, test it as a thin transport adapter.

At minimum cover:

- tool discovery/listing,
- schema validation,
- valid replace/insert/delete-range/patch/apply-diff requests,
- invalid arguments,
- stale expected state,
- policy denial,
- rollback,
- structured error mapping,
- edit ID propagation,
- before/after state propagation,
- no mutation on failed MCP request.

The MCP test must prove that an MCP call reaches the same canonical edit semantics tested directly at the service layer.

Do not duplicate all low-level filesystem tests in MCP tests. Use representative end-to-end cases plus equivalence assertions.

---

## 15. CLI adapter tests

Once the AWE-014 CLI surface exists, test actual command invocation rather than only internal functions.

Cover:

```text
awh fs replace
awh fs insert
awh fs delete-range
awh fs patch
awh fs apply-diff
awh fs rollback
```

Verify:

- successful exit status,
- non-zero exit on conflict,
- non-zero exit on validation failure,
- non-zero exit on policy denial,
- non-zero exit on verification failure,
- machine-readable output where supported,
- stable edit ID propagation,
- human-readable diagnostic safety,
- no secret/file-content leakage,
- exact filesystem behavior equivalent to direct EditService execution.

Run these tests through real process boundaries when practical. The issue explicitly requires MCP and CLI adapter coverage once those surfaces exist. fileciteturn158file0

---

## 16. Cross-layer equivalence tests

Create a small set of canonical scenarios and execute them through:

1. direct EditService,
2. MCP adapter,
3. CLI adapter,
4. other editing adapters that actually exist on the branch.

Compare:

- success/failure class,
- resulting bytes,
- conflict behavior,
- `EditId` presence,
- before/after state,
- rollback result,
- policy enforcement.

The goal is not identical textual output. The goal is identical domain semantics.

---

## 17. Test filesystem-tree oracle

Build or reuse a helper that captures an isolated workspace before and after a transaction.

The oracle should capture, as appropriate:

- relative paths,
- file bytes,
- file existence,
- relevant metadata,
- symlink identity/target where supported.

For mutation-free assertions, compare the entire relevant tree rather than only the target file.

This is especially important for proving that failed multi-file transactions did not mutate another file and that failed operations did not leave temporary artifacts.

Avoid including timestamps that make otherwise identical trees fail comparison unless timestamps are themselves part of the contract.

---

## 18. Test cleanup and isolation

Every integration test must use an isolated temporary workspace.

Guarantee:

- no test depends on repository working-tree state,
- no test writes into the source checkout,
- no shared mutable fixture across parallel tests,
- cleanup occurs after success and failure,
- temporary artifacts do not leak between tests,
- tests can run in parallel unless a specific global resource requires serialization.

Never make a test pass by deleting the repository's real files after an operation.

---

## 19. Test naming and diagnostics

Test names must describe behavior and invariant, not implementation details.

Prefer names such as:

```text
stale_hash_rejects_edit_without_mutating_file
multi_file_prepare_failure_leaves_every_file_unchanged
rollback_restores_exact_original_bytes
symlink_escape_is_rejected_without_mutation
cli_conflict_returns_nonzero_and_preserves_file
mcp_patch_uses_canonical_edit_service_semantics
```

Failure diagnostics should include:

- test scenario,
- operation,
- path relative to isolated workspace,
- expected state summary,
- observed state summary,
- before/after byte hashes,
- transaction/edit ID where available.

Do not print secrets or complete sensitive file contents in normal test failures.

---

## 20. Platform-specific behavior

Run the suite on the project's supported CI platforms.

Explicitly identify behavior that differs across:

- Linux,
- macOS,
- Windows,
- filesystems with different symlink permissions.

Do not silently mark platform-specific security tests as passing when they were skipped.

Use clear test attributes and CI reporting so maintainers can distinguish:

```text
passed
failed
unsupported/skipped with documented reason
```

A platform-specific skip must not hide a portable safety invariant.

---

## 21. Resource and performance safety

The test suite must not introduce an accidental denial-of-service against CI.

Bound:

- generated file sizes,
- property-test case counts,
- diff sizes,
- operation counts,
- transaction file counts,
- retry loops,
- concurrency loops.

Add at least one moderately sized real-file test to expose accidental quadratic behavior without making the default test suite impractically slow.

Avoid arbitrary long sleeps.

---

## 22. Backward compatibility

Do not break existing unrelated tests or public APIs merely to construct the new suite.

When test helpers need a new interface:

- prefer existing production abstractions,
- keep helper APIs private to tests when possible,
- avoid exposing test-only concepts through public services,
- preserve serialization compatibility.

Do not rename stable edit fields solely to make tests easier.

---

## 23. Documentation and test inventory

Update documentation only if required to accurately describe the test contract and only as part of the implementation work for this issue.

Document:

- where agent-grade editing tests live,
- which invariants they prove,
- which tests are platform-specific,
- how failure injection works,
- how to run focused edit tests,
- how to run the complete suite.

Do not create a second roadmap or duplicate issue specification.

---

## 24. Required test layers

The completed suite must contain an intentional pyramid:

### Layer 1 — pure/unit

Fast tests for:

- operation shape,
- line semantics,
- diff preparation/parsing,
- state comparisons,
- deterministic error classification.

### Layer 2 — service + real filesystem

Real temporary files for:

- mutation,
- conflict detection,
- verification,
- atomicity,
- rollback,
- security boundaries.

### Layer 3 — failure/recovery

Injected and induced failures for:

- apply,
- verify,
- commit,
- rollback/recovery.

### Layer 4 — concurrency

Deterministic stale-read and external modification scenarios.

### Layer 5 — transport integration

Real MCP and CLI adapter/process tests where implemented.

No single layer may be presented as proof for a property that belongs to another layer.

---

## 25. Required negative tests

The suite must deliberately try to break the editor.

At minimum include:

- traversal,
- absolute path,
- symlink escape,
- malformed transaction,
- wrong expected hash,
- wrong expected size,
- wrong expected line count,
- context mismatch,
- missing target,
- invalid line range,
- malformed diff,
- conflicting multi-file transaction,
- policy denial,
- rollback against changed target,
- injected write failure,
- injected verification failure,
- interrupted/recovery boundary where the implementation can model it,
- stale concurrent write.

The expected outcome for every safety failure must be explicit.

---

## 26. Required positive tests

At minimum prove successful:

- replace,
- insert at beginning,
- insert at line boundary,
- insert at EOF,
- delete single line,
- delete range,
- patch,
- unified diff,
- multi-operation transaction,
- multi-file transaction,
- rollback by edit ID,
- Unicode content,
- CRLF content,
- LF content,
- no-final-newline content,
- empty-file transition where supported.

For every positive mutation test, assert actual final bytes and not just return status.

---

## 27. Interaction with prior AWE issues

### AWE-004 — multi-operation filesystem patch

Prove that the transaction is prepared as a coherent unit and that preparation failure cannot leave earlier files mutated.

### AWE-006 — atomic rollback-safe edits

Exercise real recovery paths and exact-byte restoration.

### AWE-007 — stale-state conflict detection

Prove hash/context/metadata conflicts against real external changes.

### AWE-008 — post-edit verification

Assert that successful edits are actually verified against the filesystem.

### AWE-010 — edit-level rollback

Prove authoritative `EditId` rollback and conflict-aware exact restoration.

### AWE-011 — capability/policy edit paths

Where present, prove authorization is enforced consistently and denial is mutation-free.

### AWE-012 — snapshots/provenance

Where present, prove rollback references the canonical snapshot/provenance chain rather than a test-local copy.

### AWE-013 — structured persistent audit

Where present, prove lifecycle correlation and sensitive-content boundaries.

### AWE-014 — CLI agent-grade editing

Where present, prove CLI semantics are equivalent to the canonical service.

Do not reimplement any of these systems in AWE-015.

---

## 28. Avoid false confidence

The following are explicitly insufficient as sole evidence:

- asserting only `Result::is_ok()`;
- checking only a returned hash without comparing actual bytes where required;
- testing only in-memory strings for filesystem guarantees;
- mocking the filesystem for all integration scenarios;
- testing only the happy path;
- testing only one file in a multi-file transaction;
- using `sleep()` as the only concurrency mechanism;
- checking only error text;
- checking only test count;
- snapshotting expected output without verifying real files;
- duplicating production logic inside test helpers.

A test suite that passes these weak checks but cannot prove the core invariants is not complete.

---

## 29. CI verification

Before declaring AWE-015 complete, run the repository's complete required Rust gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also run focused tests for the agent-grade editing suite and, where applicable:

```bash
cargo test <focused-edit-test-filter>
```

If integration tests require a platform capability, record the exact command and reason rather than silently omitting them.

No unrelated CI regression may be accepted as “out of scope” without investigation.

---

## 30. Definition of Done

AWE-015 is complete only when all applicable conditions are true:

- [ ] Existing edit model tests remain green.
- [ ] Replace behavior is covered at unit and real-filesystem levels.
- [ ] Insert behavior is covered at unit and real-filesystem levels.
- [ ] Delete-range behavior is covered at unit and real-filesystem levels.
- [ ] Patch behavior is covered at unit and real-filesystem levels.
- [ ] Unified-diff behavior is covered at unit and real-filesystem levels.
- [ ] Expected hash/context/size/line-count conflicts are covered.
- [ ] Invalid/conflicting operations prove zero mutation.
- [ ] Multi-file preparation failure proves zero partial mutation.
- [ ] Atomic apply/recovery behavior is covered.
- [ ] Exact edit-level rollback is covered.
- [ ] Rollback conflict behavior is covered.
- [ ] UTF-8/Devanagari/emoji behavior is covered.
- [ ] LF/CRLF/EOF/empty-file boundaries are covered.
- [ ] Traversal and symlink security is covered.
- [ ] At least one deterministic stale concurrent-read scenario is covered.
- [ ] Failure injection covers apply and verification failures where the architecture permits injection.
- [ ] Property-based invariants are implemented when compatible with the existing test infrastructure.
- [ ] MCP adapter tests exist when the MCP edit surface exists.
- [ ] CLI adapter/process tests exist when the CLI edit surface exists.
- [ ] Transport adapters are proven to use canonical edit semantics.
- [ ] Test helpers do not contain a second edit implementation.
- [ ] Tests use real temporary filesystem boundaries for filesystem guarantees.
- [ ] Platform-specific behavior is explicit and documented.
- [ ] Test output is secret-safe.
- [ ] Full Rust CI passes.
- [ ] Focused AWE-015 tests pass.
- [ ] No unrelated production behavior was changed merely to satisfy a test.

---

## 31. Explicit non-goals

Do **not** use AWE-015 to:

- redesign `EditTransaction`,
- create a second editing engine,
- create a second snapshot store,
- create a second audit system,
- redesign MCP,
- redesign the CLI,
- add a generic workflow engine,
- add unrelated test coverage,
- inflate test count without proving an invariant,
- weaken production safety checks to make tests pass,
- replace real filesystem tests with mocks,
- introduce a new property-testing framework without architectural justification,
- rewrite unrelated modules.

If another issue is discovered, record it as a finding or follow-up rather than silently expanding AWE-015.

---

## 32. Final implementation report

At completion, report:

1. **Forensic findings** — what existed before the test work.
2. **Test files added/changed** — exact paths.
3. **Invariant coverage** — map each core invariant to concrete tests.
4. **Failure coverage** — apply/verify/rollback/concurrency failures tested.
5. **Filesystem coverage** — path, symlink, newline, Unicode, EOF, empty-file cases.
6. **Transport coverage** — MCP/CLI and other adapters actually tested.
7. **Property testing** — framework and bounded properties used, if any.
8. **Platform behavior** — passed/skipped/unsupported cases.
9. **Production changes** — exact minimum changes required to make the tests valid, if any.
10. **Commands executed** — focused and full CI commands.
11. **Results** — exact pass/fail status.
12. **Known limitations** — only genuine implementation/platform limitations.
13. **Follow-up issues** — only issues discovered outside AWE-015 scope.

Do not claim coverage that was not actually executed.

---

## 33. Hard stop / issue boundary

After AWE-015 is fully implemented, tested, and verified:

**STOP.**

Do not automatically implement AWE-016 or any later issue.

Do not modify unrelated files merely because they are nearby in the roadmap.

Do not expand the issue into architecture redesign.

The success criterion for AWE-015 is simple but strict:

> **The AWH agent-grade editing implementation must have a real, maintainable, CI-enforced test suite that proves its safety invariants against real filesystem behavior, not merely a large number of passing tests.**
