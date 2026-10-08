# Master Test Prompt 03 — Agent-Grade Filesystem Editing

## Mission

Create and execute a complete, human-behavior-oriented verification suite for the AWH **Agent-Grade Filesystem Editing** feature family on the current `rust` branch.

This prompt owns filesystem read/write/inspection/search/hash/verification and controlled edit behavior. It must validate the real production editing boundary through actual filesystem state and, where available, the real CLI/MCP/API entry points.

Do not redesign or replace the production edit engine merely to satisfy tests. Do not treat `cargo test` alone as acceptance evidence.

The primary acceptance boundary is:

```text
human / agent request
→ workspace resolution
→ capability/policy boundary where implemented
→ filesystem locate/read
→ expected-state/context validation
→ conflict detection
→ atomic mutation
→ post-edit verification
→ resulting filesystem state
```

The test suite must prove what actually happens at the filesystem boundary, including failure and recovery behavior.

---

## 1. Product scope

The final feature contract includes:

### Basic filesystem operations

```text
awh fs read
awh fs write
awh fs stat
awh fs search
awh fs hash
awh fs verify
```

### Controlled edits

```text
awh fs patch
awh fs replace
awh fs insert
awh fs delete-range
awh fs apply-diff
awh fs history
awh fs rollback
```

Every consequential edit is intended to follow:

```text
request
→ capability/policy check
→ locate/read
→ context validation
→ conflict detection
→ atomic apply
→ verification
→ snapshot/provenance
→ audit
```

For this prompt, test the **filesystem editing behavior** itself. Test downstream snapshot/provenance/audit only at the narrow integration boundary necessary to prove that the editing transaction correctly emits or consumes those records; full snapshot, rollback subsystem, and audit suites belong to their own prompts.

---

## 2. Current branch is authoritative

Before writing or changing tests, inspect the current `rust` branch and determine which filesystem operations are actually implemented.

Do not assume that every command in `docs/FEATURES.md` already exists.

For each operation classify:

- Implemented and tested
- Implemented but insufficiently tested
- Not implemented
- Blocked by environment
- Unproven

Never create fake acceptance tests for an unavailable command.

The current implementation may already contain:

- `EditTransaction`;
- CLI filesystem editing;
- expected-state validation;
- post-edit verification;
- rollback/recovery integration;
- canonical filesystem services.

Reuse those implementations and their existing test helpers.

---

## 3. Required repository forensics

Before implementing tests, inspect at minimum:

- `README.md`
- `AGENTS.md`
- `Cargo.toml`
- `docs/FEATURES.md`
- `docs/testing.md`
- `docs/CLI.md`
- `docs/configuration.md`
- `docs/architecture.md`
- `docs/security.md`
- `docs/threat-model.md`
- `docs/roadmap/PROJECT_ROADMAP.md`
- `docs/roadmap/GROWTH_STRATEGY.md`
- `docs/implementation-prompts/README.md`
- filesystem/editing implementation prompts;
- existing filesystem/editing tests.

Search the source tree for:

```text
EditTransaction
EditOperation
ExpectedState
FileState
fs read
fs write
fs patch
replace
insert
delete-range
apply-diff
history
rollback
verify
hash
search
atomic
conflict
stale
```

Read the actual implementation and tests before deciding what assertions are valid.

---

## 4. Human-first testing requirement

Tests must model what a real user or coding agent does.

Do not stop at:

```text
function returns Ok(())
```

Instead verify:

```text
request
→ command/service execution
→ process exit/result
→ actual filesystem bytes
→ metadata/hash where applicable
→ observable error or success
```

For every mutation, assert the resulting filesystem state.

For every rejected mutation, assert that the filesystem state is unchanged.

For every partial/failure scenario, determine whether atomicity requires all-or-nothing behavior and test that exact contract.

---

## 5. Test isolation

All tests must use disposable temporary workspaces.

Never modify:

- the repository checkout;
- the developer's home directory;
- real project files;
- global configuration;
- real credentials;
- unrelated temporary directories.

Fixtures should create:

- ordinary text files;
- binary files;
- empty files;
- nested directories;
- files with spaces;
- Unicode filenames/content;
- symlink fixtures where the platform and security contract support them.

Each test must begin from a known filesystem state.

Avoid process-global environment mutation when tests can execute in parallel.

---

## 6. Basic filesystem read

Test `fs read` against:

- normal text file;
- empty file;
- binary file where supported;
- Unicode content;
- large but valid file;
- missing file;
- directory instead of file;
- permission-denied file where reproducible.

Verify:

- exact bytes or documented representation;
- correct exit status;
- no silent truncation;
- correct missing/error behavior;
- no panic;
- no unrelated file access.

For binary content, do not force text assumptions. Follow the actual CLI/service contract.

---

## 7. Filesystem write

Test `fs write` with:

- new file;
- existing file;
- empty content;
- multiline content;
- Unicode content;
- binary-safe bytes where supported;
- nested path;
- missing parent;
- directory target;
- unwritable target where reproducible.

Verify exact resulting bytes.

When overwrite is allowed, prove the old bytes are replaced according to the contract.

When overwrite is not allowed without an explicit mode, verify that omission fails safely.

Test repeated writes and verify deterministic final state.

---

## 8. Stat behavior

Test `fs stat` for:

- regular file;
- empty file;
- directory;
- missing path;
- nested path;
- Unicode/spaces;
- changed file after mutation.

Where the contract exposes metadata, verify:

- file/directory type;
- size;
- modification information if documented;
- hash/state fields if documented.

Do not assert unstable timestamps exactly unless the product contract requires them.

---

## 9. Search behavior

Test `fs search` against a realistic project tree:

```text
src/
  main.rs
  parser.rs
tests/
  parser_test.rs
README.md
```

Verify:

- matching files are found;
- nonmatching files are excluded;
- nested directories are searched as documented;
- missing roots fail predictably;
- binary files are handled according to contract;
- Unicode content behaves correctly;
- path boundaries are respected;
- output is deterministic where ordering is contractual.

Include:

- zero matches;
- one match;
- many matches;
- repeated matches in one file;
- overlapping patterns if supported;
- special characters.

Do not assume a search ordering that the public contract does not guarantee.

---

## 10. Hash behavior

Test `fs hash` using known bytes.

Verify:

- identical bytes produce identical hashes;
- one-byte changes produce different hashes;
- empty-file hash is stable;
- binary data is handled correctly;
- repeated hashing is deterministic;
- hash output format matches the documented algorithm/encoding.

Use independent expected values where practical instead of calculating the expected result with the exact same production helper.

This prevents a duplicated implementation from making the test pass incorrectly.

---

## 11. Verify behavior

Test `fs verify` against:

- unchanged file;
- expected state matching actual state;
- changed content;
- deleted file;
- replaced file;
- metadata/state changes where relevant.

Verify that verification distinguishes the appropriate conditions:

```text
verified
≠ stale/conflict
≠ missing
≠ apply failure
```

Where the implementation explicitly records a resulting `FileState`, compare it with independently observed filesystem state.

A verification test must never succeed merely because an internal flag says verification succeeded.

---

## 12. Replace operation

Test controlled `replace` using:

- one exact occurrence;
- multiple occurrences;
- zero occurrences;
- replacement at beginning;
- replacement at end;
- multiline text;
- Unicode;
- replacement containing special characters.

Verify the documented occurrence semantics.

If the operation is expected to reject ambiguous replacements, create an ambiguous file and prove it refuses the mutation rather than choosing arbitrarily.

After every successful replacement:

- read the actual file;
- compare exact expected bytes;
- calculate an independent hash where useful.

After every rejected replacement:

- verify original bytes remain unchanged.

---

## 13. Insert operation

Test insertion at:

- beginning;
- middle;
- end;
- empty file;
- line boundary;
- Unicode boundary;
- valid character/byte boundaries according to the implementation.

Include invalid offsets/locations.

Verify:

- exact resulting bytes;
- no accidental duplication;
- no truncation;
- invalid requests do not mutate the file.

If offsets are byte-based, test valid UTF-8 boundaries and invalid byte boundaries explicitly.

If offsets are line/character based, follow that contract instead.

Never infer offset semantics from test convenience.

---

## 14. Delete-range operation

Test deletion of:

- first range;
- middle range;
- final range;
- entire file where allowed;
- empty range;
- adjacent ranges;
- Unicode content;
- invalid start/end;
- reversed range;
- out-of-bounds range.

Verify exact resulting bytes.

For invalid ranges, prove the file remains byte-for-byte unchanged.

If the implementation defines ranges in bytes, explicitly test UTF-8 boundary safety.

---

## 15. Patch operation

Test patching with realistic source files.

Include:

- one valid patch;
- multiple hunks;
- context lines;
- insertion;
- deletion;
- replacement;
- empty patch if supported;
- malformed patch;
- patch with missing context;
- patch against stale content.

Verify:

- exact final bytes;
- expected hunks are applied;
- unrelated content remains unchanged;
- invalid patches fail;
- failed patches do not leave partial changes.

Where patch application is transactional, test multiple-hunk failure so that a later failing hunk cannot leave an earlier hunk committed.

---

## 16. Apply-diff behavior

If `apply-diff` is distinct from `patch`, test its exact contract separately.

Use realistic unified diffs and include:

- new file;
- deleted file;
- renamed/moved content if supported;
- multiple files;
- context mismatch;
- malformed diff;
- partial failure;
- empty diff.

Verify that multi-file operations obey the documented atomicity boundary.

If the implementation does not guarantee cross-file atomicity, test and report the actual behavior instead of inventing stronger guarantees.

---

## 17. Expected-state and stale-state protection

This is a critical security/correctness suite.

Create a file and capture its expected state.

Then modify the file outside AWH.

Attempt an edit using the stale expected state.

Verify:

- edit is rejected;
- stale content is not overwritten;
- current external bytes remain intact;
- the error identifies the conflict category where documented.

Also test:

- file deleted after expected state;
- file replaced with same size but different content;
- file modified and changed back;
- metadata-only changes where relevant.

Do not rely solely on modification time to prove correctness if the production contract uses content hashes or richer `FileState`.

---

## 18. Atomicity and failure injection

Test mutation failure at realistic boundaries.

Where the implementation exposes injectable/testable failure points, exercise:

- read failure;
- validation failure;
- conflict;
- temporary-file creation failure;
- write failure;
- rename/persist failure;
- verification failure.

For every failure, determine the documented atomicity guarantee.

Where all-or-nothing semantics are promised, verify:

```text
failure before commit
→ original filesystem state
```

and never:

```text
half old + half new
```

For multi-operation transactions, test failure in the middle of the transaction.

Do not weaken production code to make failure injection possible.

---

## 19. Exact-byte correctness

Filesystem editing tests must compare bytes, not merely rendered strings, when byte preservation matters.

Include:

- LF;
- CRLF where supported;
- trailing newline;
- no trailing newline;
- multiple blank lines;
- tabs;
- Unicode;
- non-ASCII bytes where binary-safe operations support them.

Verify that an edit does not silently normalize line endings or encoding unless the contract explicitly says it should.

---

## 20. Path safety

Test paths containing:

- `..`;
- nested traversal;
- absolute paths;
- relative paths;
- redundant separators;
- spaces;
- Unicode;
- long valid names;
- symlinks where relevant;
- paths resolving outside the workspace.

The expected result must follow the current security contract.

Never use a sensitive host path as the traversal target.

A successful negative test must prove that the protected outside file was not changed.

---

## 21. Concurrent editing

Run controlled concurrent operations against:

- the same file;
- different files;
- the same workspace;
- multiple workspaces where applicable.

Test:

- concurrent writes;
- concurrent edits;
- edit versus external modification;
- read while edit occurs.

Verify the documented coordination semantics.

Where conflicts are expected, verify that stale data is not silently overwritten.

Where serialization is promised, verify resulting bytes and transaction outcomes rather than relying only on lock acquisition.

---

## 22. CLI versus service parity

Where both service and CLI boundaries exist, test the same logical operation through each supported interface.

For example:

```text
CLI fs write
service write
→ equivalent filesystem result
```

and:

```text
CLI stale edit
service stale edit
→ equivalent conflict semantics
```

Do not create a second edit engine for a test.

If MCP/API/TUI expose filesystem editing, test only the thin boundary necessary to prove they reach the canonical edit service. Full MCP/API/TUI suites belong to their own prompts.

---

## 23. History boundary

If `fs history` is implemented, verify that a user can inspect completed edit history according to the current contract.

Test:

- no history;
- one successful edit;
- multiple edits;
- failed edit;
- concurrent edits where supported;
- restart and history persistence.

Verify that failed mutations are not falsely reported as completed edits.

If history includes IDs, timestamps, hashes, paths, agents, sessions, or snapshots, assert only fields guaranteed by the current contract.

Do not duplicate the full audit/provenance implementation suite.

---

## 24. Rollback boundary

If `fs rollback` is implemented as part of the canonical editing service, test the editing-to-rollback boundary.

At minimum:

1. create known file;
2. perform successful edit;
3. capture edit identity;
4. request rollback;
5. verify actual bytes return to the documented previous state.

Also test:

- unknown edit ID;
- already rolled-back edit;
- stale/conflicting current file;
- rollback after restart;
- rollback of a multi-file edit where supported.

The separate snapshot/rollback master prompt owns the full recovery subsystem. This prompt owns only the correctness of the editing boundary and its handoff to rollback.

---

## 25. Post-edit verification

After every successful mutation, independently inspect the filesystem.

Verify:

- file exists when expected;
- exact bytes match;
- expected hash matches;
- resulting state can be read by a fresh process;
- verification failure is distinguishable from apply failure if the contract exposes that distinction.

Never mark a mutation successful solely because the write syscall returned success.

AWH's documented post-edit verification is intended to read the actual resulting filesystem state. Tests must exercise that real read-back behavior.

---

## 26. Snapshot/provenance/audit integration boundary

Where the current edit transaction emits snapshot/provenance/audit information, verify only the essential contract:

- successful consequential edits produce the expected downstream event/record when implemented;
- the edit identity can be correlated with the resulting operation;
- failed edits are not falsely represented as successful mutations;
- secrets are not persisted.

Do not rebuild snapshot storage, provenance, or audit logic in this prompt.

Those systems receive separate feature-family test prompts.

---

## 27. Capability/policy boundary

Where authorization is already enforced around filesystem mutations, include boundary tests proving that a denied edit does not mutate the filesystem.

Test at least:

- allowed read;
- denied write;
- denied delete;
- denied edit;
- capability/policy decision before mutation.

Do not implement or redesign PolicyEngine here.

The dedicated capability/policy test prompt owns exhaustive policy semantics.

This prompt only proves the filesystem mutation does not occur when its authorization boundary rejects the request.

---

## 28. Environment variables and limits

Read `docs/configuration.md` and test every documented environment variable that materially affects filesystem editing or resource limits.

At minimum inspect and exercise relevant values for:

- `AWH_MAX_MCP_LINE_BYTES`;
- `AWH_MAX_HTTP_BODY_BYTES`;
- `AWH_MCP_REQUEST_TIMEOUT_SECS`;
- `AWH_HTTP_CLIENT_TIMEOUT_SECS`;
- workspace/persistent-state environment configuration if implemented;
- sandbox-related `AWH_BWRAP` where relevant.

Do not invent environment variables.

Use synthetic values in tests.

For size limits, test:

```below limit
exactly at limit
just above limit
```

where the boundary is relevant.

Never print real environment secrets in test output.

---

## 29. Resource and large-input tests

Use bounded but realistic files.

Test:

- empty;
- small;
- medium;
- near configured limit;
- over configured limit;
- many files;
- deeply nested directories.

The test suite must remain practical for CI.

Do not create unbounded memory or disk stress tests.

A resource-limit test should prove that AWH rejects or bounds the request according to its contract rather than crashing.

---

## 30. Human workflow acceptance tests

Automate complete scenarios.

### Workflow A — Safe edit

```text
create source file
→ read
→ capture expected state
→ replace
→ verify
→ read again
→ compare exact bytes
```

### Workflow B — Stale edit

```text
create file
→ capture expected state
→ external process changes file
→ attempt edit
→ verify rejection
→ verify external bytes remain
```

### Workflow C — Failed multi-step edit

```text
create known project
→ submit operation with an intentionally failing later step
→ inspect every affected file
```

Expected: behavior matches the documented transaction atomicity; no unexplained partial mutation.

### Workflow D — Restart

```text
perform edit
→ terminate process
→ start fresh awh process
→ inspect file/history/state
```

Expected: durable state and actual filesystem bytes remain coherent.

### Workflow E — Concurrent agents/processes

```text
two isolated processes
→ same target
→ competing edits
→ inspect final bytes and conflict outcomes
```

Expected: no silent stale overwrite.

---

## 31. Test implementation requirements

Use the existing repository conventions and test helpers.

Prefer:

- pure unit tests for deterministic edit primitives;
- integration tests for filesystem services;
- subprocess tests for public CLI behavior;
- real temporary files/directories;
- independent byte/hash verification;
- concurrency tests;
- property tests for pure validators/parsers;
- failure-injection tests where the implementation supports safe injection.

Every important user-facing operation must have at least one test through its real boundary when that boundary exists.

Tests must assert observable behavior, not implementation trivia.

---

## 32. Property-based testing

Use `proptest` where it provides meaningful coverage.

Useful properties include:

- valid edits preserve intended unaffected content;
- invalid ranges never mutate files;
- hash is deterministic;
- serialize/deserialize of valid state round-trips;
- stale expected state cannot silently authorize a mutation;
- failed operations leave state unchanged where atomicity is guaranteed;
- path validators reject traversal forms required by the security contract.

Avoid generating unlimited filesystem trees or enormous inputs.

Property tests should complement, not replace, carefully designed human workflow tests.

---

## 33. Security regression requirements

Every filesystem editing security regression must become a permanent test.

At minimum preserve these invariants:

- path traversal cannot escape the workspace;
- encoded/normalized traversal cannot bypass the path boundary;
- stale state cannot silently overwrite current content;
- denied mutations do not change the filesystem;
- malformed edit requests do not cause unintended writes;
- failed transactions do not leave unauthorized partial state where atomicity is promised;
- secrets are not written to logs/history/audit;
- internal callers cannot silently bypass required authorization;
- symlink behavior follows the documented security model.

Never add test-only bypasses for path checks, authorization, conflict detection, or verification.

---

## 34. No test theater

Do not:

- test only private helpers;
- use mocks instead of the real filesystem for filesystem acceptance;
- hard-code developer-specific paths;
- assume the current directory is a valid workspace;
- swallow subprocess errors;
- assert only exit code;
- calculate expected results using the exact same production implementation;
- disable conflict detection under tests;
- skip post-edit filesystem inspection;
- fabricate support for unimplemented commands;
- weaken assertions because output is inconvenient;
- change unrelated production behavior solely to make tests pass.

A green suite without actual filesystem-state verification is insufficient.

---

## 35. Execution gates

Run the applicable project gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
```

Also run:

- filesystem-specific integration tests;
- CLI/executable tests;
- relevant security tests;
- concurrency tests;
- failure-injection tests;
- platform-specific tests where applicable.

Run the real built `awh` binary for black-box CLI tests when possible.

If a platform-specific filesystem behavior cannot be verified in the current environment, report it as **Blocked** or **Unproven**, not Passed.

---

## 36. Evidence report

Report every filesystem feature using:

### Passed

Direct evidence demonstrates the expected behavior.

### Failed

The real test executed and exposed a production defect.

### Blocked

The test could not execute because of environment/platform/dependency limitations.

### Unproven

The feature or contract lacks sufficient executable evidence.

For each failure include:

- test name;
- exact reproduction command where applicable;
- initial filesystem state;
- requested operation;
- expected result;
- actual result;
- resulting filesystem state;
- relevant error;
- likely subsystem;
- production defect versus test-environment issue.

For every unimplemented command, explicitly state **Not implemented**.

---

## 37. Completion criteria

This prompt is complete only when:

- the current filesystem/editing implementation was inspected;
- all currently implemented basic filesystem operations have meaningful coverage;
- all currently implemented controlled edit operations have meaningful coverage;
- real CLI/subprocess behavior is tested where available;
- exact resulting filesystem bytes are verified;
- stale-state/conflict behavior is tested;
- atomicity/failure behavior is tested;
- path traversal and workspace-boundary behavior is tested;
- concurrent editing behavior is tested where applicable;
- post-edit verification is independently exercised;
- relevant environment/limit boundaries are tested;
- authorization-denial behavior is tested at the filesystem boundary where implemented;
- history/rollback integration boundaries are tested where implemented;
- existing project tests remain green or failures are explicitly documented;
- no production security bypass was introduced;
- unimplemented target operations are honestly classified;
- no other test master prompt was modified.

The objective is trustworthy evidence that AWH's filesystem surface can safely read, inspect, mutate, verify, and recover project state without silent stale overwrites, unintended partial writes, workspace escapes, or false success.

---

## 38. Scope boundary

This prompt owns **Agent-Grade Filesystem Editing testing only**.

Do not create full feature suites for:

- Foundation and Distribution;
- Workspace Runtime;
- Git/worktrees;
- Capability/Policy Engine;
- Snapshots/Undo/Provenance;
- Context;
- Memory;
- Skills;
- Agent Profiles/policy-routed MCP;
- MCP infrastructure;
- Sessions/Tasks;
- Audit/Observability;
- Terminal;
- Collaboration;
- Control API;
- TUI;
- Connectors;
- Advanced infrastructure.

Those feature families must have independent master test prompts.

Do not modify:

```text
docs/testing-prompts/01-foundation-distribution.md
docs/testing-prompts/02-workspace-runtime.md
```

Do not modify any other existing test master prompt while executing this prompt.
