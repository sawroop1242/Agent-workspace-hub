# Prompt 05 — Canonical Edit Operation Engine (AWE-002..005)

## Mission

Implement and maintain one production edit-operation execution path over the canonical EditTransaction model.

This prompt covers AWE-002 through AWE-005:

- contextual replacement;
- line-boundary insertion;
- inclusive line-range deletion;
- canonical multi-operation patch preparation and commit;
- unified-diff parsing and application;
- deterministic conflict handling;
- exact-byte/newline preservation;
- no-partial-preparation for multi-operation edits;
- operation-level tests and verification.

This is not a new edit model. The current rust branch already contains the canonical transaction vocabulary and substantial EditService executor code. The implementation task is therefore forensic-first: preserve correct existing behavior, identify concrete gaps, harden semantics where necessary, and avoid duplicate executors.

This prompt is standalone and must be executable against the current repository without requiring another prompt or PR.

---

## 1. Product boundary

AWH owns workspace/filesystem state, controlled editing, expected-state/conflict detection, snapshots/provenance/rollback/audit at their own service boundaries, authorization, and the MCP/CLI/TUI/API interfaces over shared application services.

External agents own reasoning, planning, model/provider selection, and agent-specific orchestration.

The edit operation engine is an AWH application service. It is not an LLM planner, model router, or orchestration framework.

---

## 2. Required repository forensics

Before changing code, inspect the current repository, not historical assumptions.

Read:

- docs/implementation-prompts/README.md;
- docs/roadmap/GROWTH_STRATEGY.md;
- docs/roadmap/PROJECT_ROADMAP.md;
- docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md;
- docs/FEATURES.md;
- docs/PROJECT_CONTEXT.md;
- docs/architecture.md;
- docs/security.md;
- docs/threat-model.md;
- relevant current Trust Wedge material;
- relevant historical AWE material under docs/issue-resolving-prompts/ when available;
- src/services/edit.rs;
- src/services/files.rs;
- src/services/mod.rs;
- path-security, atomic-write, and error helpers;
- relevant authorization interfaces;
- relevant snapshot/rollback services;
- edit, filesystem, MCP, CLI, TUI, and integration tests.

Historical forensic reports are evidence of an earlier state. Do not recreate code they describe as missing if the current rust branch already contains it.

Search for:

    EditService
    EditTransaction
    EditOperation
    filesystem.patch
    filesystem.replace
    filesystem.insert
    filesystem.delete_range
    filesystem.apply_diff
    Replace
    Insert
    DeleteRange
    ApplyDiff
    parse_unified_diff
    apply_hunks_to_content
    PatchStatus
    RollbackRecord

Classify each hit as canonical implementation, adapter, test, legacy/duplicate, or unrelated.

---

## 3. Baseline

Before editing:

1. Record git status --short.
2. Record branch and HEAD.
3. Inspect recent commits affecting edit.rs, files.rs, and related tests.
4. Run cargo metadata --no-deps or the repository-supported baseline check.
5. Run focused edit tests first.
6. Search for every edit executor and transport entry point.

Do not start by rewriting edit.rs.

---

## 4. Canonical architecture

The intended execution path is:

    External agent
        ↓
    MCP / CLI / TUI / Control API
        ↓
    transport adapter / DTO
        ↓
    authorization boundary
        ↓
    canonical EditTransaction
        ↓
    EditService
        ├── validate transaction shape
        ├── validate affected resources
        ├── read live state
        ├── validate expected state/context
        ├── prepare every operation
        ├── commit prepared results
        └── verify/handoff result
        ↓
    snapshot/provenance/audit services at their own boundaries

Prompt 05 owns the operation engine in the middle.

It does not own caller authentication, capability/policy evaluation, agent registry/session resolution, persistent audit, the canonical snapshot store, whole-workspace rollback, Git reset/revert, MCP routing, or agent orchestration.

---

## 5. Hard ownership boundary

### Own

- execution of Replace, Insert, DeleteRange, Patch, and ApplyDiff;
- operation-specific validation;
- live content loading required by an operation;
- deterministic context/location matching;
- construction of prepared new bytes;
- multi-operation preparation;
- deterministic commit through existing filesystem primitives;
- unified-diff parsing/application;
- operation result/status behavior;
- operation-engine tests.

### Do not own

- a second transaction model;
- a second path resolver;
- authorization;
- policy;
- identity;
- MCP authentication/routing;
- persistent snapshot storage;
- persistent provenance storage;
- persistent audit;
- generic undo/redo;
- Git worktrees;
- distributed locking;
- model calls;
- subagent orchestration.

If another subsystem is required, reuse its existing contract rather than reimplementing it.

---

# AWE-002 — Safe contextual replacement

## 6. Replace contract

Replace contains path, old, new, and optional occurrence.

The service must:

1. validate transaction shape;
2. validate workspace-relative path;
3. read current file state;
4. apply the repository's text/binary contract;
5. locate the requested old text;
6. enforce deterministic occurrence semantics;
7. reject missing or ambiguous matches;
8. construct the complete resulting content before mutation;
9. preserve all unrelated bytes;
10. commit only after preparation succeeds;
11. return a structured result;
12. never fall back to blind whole-file replacement when a structured operation fails.

If occurrence is absent, the safe default is:

- one match: replace it;
- zero matches: structured not-found/conflict;
- multiple matches: structured ambiguity failure.

If occurrence is present:

- use the documented occurrence convention;
- reject invalid occurrence values;
- replace only the requested occurrence.

Do not silently interpret an ambiguous request as replace-all.

If the current repository already defines a different compatible convention, preserve it unless it is demonstrably unsafe; document any change.

### Expected state and context

Honor the canonical ExpectedState mechanism.

A stale request must never overwrite newer content.

A context string is location-sensitive. A global substring match is not proof that the intended edit location is unchanged.

### Replace tests

Cover:

- unique, missing, and ambiguous matches;
- explicit first/last occurrence;
- invalid occurrence;
- stale hash;
- size mismatch;
- line-count mismatch;
- context mismatch;
- empty file where valid;
- Unicode;
- Devanagari;
- emoji;
- LF;
- CRLF;
- no final newline;
- replacement containing newlines.

---

# AWE-003 — Line insertion and deletion

## 7. Insert contract

The current canonical model documents:

- line = 0 means beginning of file;
- positive line is the one-based boundary before which content is inserted.

Preserve this contract unless current behavior proves it inconsistent.

The implementation must:

- validate path and line boundary;
- read current bytes;
- construct a deterministic line map;
- preserve newline semantics;
- preserve final-newline state;
- prepare complete new bytes;
- commit only after preparation succeeds.

Test empty files, beginning, first line, middle, last valid boundary, EOF, invalid boundary, multiline content, LF, CRLF, no-final-newline, Unicode, Devanagari, and emoji.

Do not introduce blank lines or newline normalization accidentally.

## 8. DeleteRange contract

DeleteRange uses an inclusive one-based start/end line range.

Reject:

- zero start;
- zero end;
- start greater than end;
- unresolvable ranges.

Define EOF behavior explicitly.

Preserve all unaffected bytes and newline conventions.

Test single-line, first, middle, last, full-file, EOF, invalid, out-of-range, LF, CRLF, Unicode, Devanagari, and emoji cases.

Do not implement line deletion as an unsafe global string replacement.

---

# AWE-004 — Multi-operation filesystem patch

## 9. One transaction and one preparation boundary

A multi-operation patch is one logical EditTransaction.

All operations must enter the same canonical EditService path. Do not maintain separate single-operation and patch algorithms.

Required phases:

    1. structural validation
    2. path/resource validation
    3. live-state read
    4. expected-state/context validation
    5. preparation of every operation
    6. commit
    7. result/verification handoff

The critical invariant is:

> No target file may be mutated until every operation required by the transaction has successfully prepared.

If any operation fails during preparation, no earlier operation may have been committed.

### Preparation

Preparation must compute complete new bytes/content and the metadata required for commit.

For multi-file transactions:

- validate every operation;
- resolve every path;
- read every required target;
- evaluate expected state;
- compute every resulting file;
- detect all preparation failures;
- only then enter commit.

### Multiple operations on one file

Define and test deterministic semantics.

The default contract should be sequential application in transaction order over a stable prepared representation.

Do not silently reorder, deduplicate, or merge operations.

If the current implementation groups operations by path, prove that observable semantics remain equivalent and document the rule.

### Multiple files

- all files prepare before commit;
- result ordering is deterministic;
- preparation failure leaves all files unchanged.

### Commit failures

Reuse existing atomic filesystem primitives.

Do not create a second atomic-write implementation.

If the repository cannot provide database-style multi-file atomicity, state the exact limitation. Do not claim stronger guarantees than the code provides.

Deeper atomicity/recovery hardening remains the responsibility of the later edit-safety boundary.

---

# AWE-005 — Unified-diff parsing and application

## 10. Unified-diff contract

ApplyDiff must use one parser and one application path.

The implementation must:

1. reject empty diffs;
2. parse file headers;
3. validate target paths;
4. parse and validate hunk headers;
5. parse context/add/delete lines;
6. reject malformed hunk structure;
7. reject unsupported binary patches;
8. validate every hunk against current content;
9. construct resulting content before commit;
10. preserve newline semantics;
11. reject context mismatch rather than guessing;
12. support multiple hunks deterministically;
13. support multiple files when the canonical contract permits it;
14. never invoke an uncontrolled shell patch fallback.

The current rust branch contains parse_unified_diff and apply_hunks_to_content. Reuse and harden those helpers rather than creating a second parser.

### Hunk behavior

Context and deleted lines must match the current file exactly under the documented newline convention.

A mismatch is a structured conflict/application failure.

Never:

- fuzzy-match unrelated content;
- search globally for an alternative location;
- skip failed hunks;
- commit only successful hunks;
- partially commit a multi-file diff.

### Hunk ordering

When multiple hunks affect one file, handle line shifts deterministically.

Use either a correct immutable-original offset model or descending source positions. Do not rely on incidental vector mutation.

### Diff paths

Reject absolute paths, traversal, malformed paths, workspace escapes, and unsupported platform-specific forms.

Diff prefixes such as a/ and b/ are syntax, not authorization.

---

## 11. Exact bytes and newline behavior

Preserve every byte not intentionally changed.

Test:

- LF;
- CRLF;
- no final newline;
- Unicode;
- Devanagari;
- emoji;
- multi-byte UTF-8;
- empty files;
- blank lines.

Do not silently normalize newline endings or Unicode.

If the repository supports UTF-8 text only, reject invalid UTF-8 through a structured error rather than lossy decoding.

---

## 12. Path and workspace safety

Use the existing canonical path/security helpers.

Treat operation paths as workspace-relative logical resources.

Reject, according to repository policy:

- empty paths;
- absolute paths;
- traversal;
- control characters;
- malformed paths;
- workspace escapes.

Follow the existing symlink/containment boundary.

Do not create another path resolver.

Path validation is not authorization.

---

## 13. Expected-state and conflict behavior

Use the canonical ExpectedState and FileState mechanisms.

Support:

- SHA-256 content hash;
- expected byte size;
- expected line count;
- location-sensitive context.

Rules:

1. malformed expected state fails closed;
2. hash mismatch is conflict;
3. size mismatch is conflict;
4. line-count mismatch is conflict;
5. context is resolved against the intended location;
6. ambiguous context is not a match;
7. conflict occurs before mutation;
8. errors do not expose full file contents or secrets.

Reuse FileState::check and the existing matcher where appropriate. Do not create a second expected-state matcher.

---

## 14. Status/result behavior

Inspect and reuse the current EditStatus, PatchStatus, and EditError contracts.

Do not create another lifecycle state machine.

The engine must distinguish successful commit, preparation failure, conflict, invalid input, malformed diff, target-not-found, ambiguous match, and commit failure.

Do not confuse:

- EditStatus: transaction lifecycle;
- operation error: concrete failure;
- snapshot status;
- authorization decision.

If the existing result includes rollback records or recovery information, preserve its meaning without turning Prompt 05 into the rollback subsystem.

---

## 15. Existing recovery hooks

The current rust branch may already contain rollback-oriented records or hooks inside edit.rs. Inspect them before changing anything.

If existing edit execution already captures information required by its own contract, preserve and test it.

Do not create:

- a second SnapshotStore;
- a new .agent/snapshots layout;
- snapshot retention;
- snapshot restore CLI;
- whole-workspace restore;
- generic undo/redo;
- a second rollback engine.

Durable file snapshots/provenance are Prompt 08. Explicit edit-level rollback/recovery is Prompt 09.

Prompt 05 must integrate with existing boundaries without duplicating them.

---

## 16. Authorization boundary

Do not implement authorization here.

Do not inspect capability files directly, infer authority from agent names or MCP routes, or bypass an existing gate.

If a caller can bypass the canonical authorization boundary, document that bypass for the authorization work instead of embedding a second policy engine in EditService.

---

## 17. Transport boundary

Thin MCP/CLI/API/TUI adapters may convert DTOs into EditTransaction when the current repository requires that integration.

Adapters must not contain replacement, line-edit, diff, conflict, or write algorithms.

Every transport must reach the same canonical EditService.

Do not expand this prompt into MCP editing/client interoperability work.

---

## 18. Concurrency and TOCTOU

Be precise about the read/prepare/commit race.

At minimum:

- revalidate expected state as close to mutation as the architecture permits;
- reuse existing coordination primitives;
- minimize the validation-to-write window;
- do not claim race-free behavior without actual synchronization;
- document residual TOCTOU limitations.

Global filesystem coordination belongs to Prompt 14.

---

## 19. Error and information-security contract

Reuse EditError and repository error conventions.

Errors may identify the failure category and logical workspace-relative path, but must not leak:

- full file contents;
- secrets;
- tokens;
- unrelated absolute host paths;
- environment secrets.

Do not flatten structured failures into generic strings when a canonical error exists.

---

# 20. Linear implementation procedure

Follow this exact sequence.

### Step 1 — Baseline

Record branch, HEAD, working tree, Rust toolchain, current tests, and actual edit behavior.

### Step 2 — Ownership map

Map canonical transaction types, FilesService/path helpers, atomic writes, EditService, authorization, snapshot, rollback, adapters, and tests.

### Step 3 — Contract lock

Verify Replace, Insert, DeleteRange, Patch, ApplyDiff, line numbering, occurrence indexing, expected-state cardinality, path semantics, and newline semantics.

### Step 4 — AWE-002

Implement/fix contextual replacement and prove deterministic occurrence behavior, stale-state rejection, ambiguity rejection, and exact-byte preservation.

### Step 5 — AWE-003

Implement/fix line insertion/deletion and prove boundaries, EOF behavior, newline preservation, and invalid-range rejection.

### Step 6 — AWE-004

Implement/fix multi-operation preparation and commit and prove that every operation prepares before any mutation.

### Step 7 — AWE-005

Implement/fix unified-diff parsing/application and prove malformed/context/path failures do not mutate targets.

### Step 8 — Expected-state reconciliation

Ensure all operations use the canonical matcher.

### Step 9 — Filesystem reconciliation

Ensure prepared results use the existing filesystem/atomic-write boundary.

### Step 10 — Recovery reconciliation

Preserve existing rollback hooks without creating snapshot/rollback infrastructure.

### Step 11 — Duplicate audit

Search again for every edit operation and parser. There must be one canonical execution implementation per operation.

### Step 12 — Security audit

Test traversal, absolute paths, symlink escape, malformed input, stale state, ambiguous replacement, malformed diffs, and zero mutation on rejected preparation.

### Step 13 — Verification

Run all repository gates and focused edit tests.

### Step 14 — Final scope audit

Inspect git status, diff stat, diff check, and full diff. Remove unrelated changes.

---

# 21. Required test matrix

Use real temporary workspaces and actual filesystem bytes.

### Replace

- unique/missing/ambiguous;
- explicit occurrence;
- invalid occurrence;
- stale hash;
- size mismatch;
- line-count mismatch;
- context mismatch;
- Unicode/Devanagari/emoji;
- LF/CRLF;
- no final newline;
- multiline replacement.

### Insert

- empty file;
- beginning;
- first line;
- middle;
- final valid boundary;
- EOF;
- invalid line;
- multiline content;
- LF/CRLF;
- no final newline;
- Unicode/Devanagari/emoji.

### DeleteRange

- single line;
- first/middle/last;
- full file;
- EOF;
- zero values;
- start > end;
- out of range;
- LF/CRLF;
- Unicode/Devanagari/emoji.

### Patch

- one operation;
- multiple operations;
- multiple files;
- multiple operations on one file;
- failure on first/middle/final preparation;
- stale expected state;
- path failure;
- zero mutation after preparation failure.

### Unified diff

- replacement;
- insertion;
- deletion;
- multiple hunks;
- multiple files;
- context mismatch;
- malformed hunk;
- invalid ranges;
- binary patch;
- empty diff;
- no-final-newline marker;
- traversal/absolute path;
- stale expected state;
- zero mutation on parse/preparation failure.

### Cross-cutting

- serialization round trip where applicable;
- deterministic errors;
- deterministic result ordering;
- concurrent calls where supported;
- symlink/containment behavior;
- exact bytes after success;
- unchanged bytes after rejected preparation.

---

# 22. Property-style invariants

If the repository already has a property-testing framework, add focused properties without introducing a large new dependency.

Useful invariants:

1. a unique replacement changes only the requested occurrence;
2. rejected replacement leaves original bytes unchanged;
3. rejected multi-operation preparation leaves every target unchanged;
4. valid diff application equals expected content;
5. context mismatch never mutates the target;
6. line insertion/deletion preserves unaffected lines;
7. serialization preserves operation meaning;
8. operation order is preserved;
9. traversal is never accepted;
10. malformed expected state never authorizes mutation.

---

# 23. Security requirements

Fail closed for:

- traversal;
- absolute paths;
- malformed expected hashes;
- empty required match strings;
- invalid occurrence;
- invalid line range;
- malformed diff;
- hunk context mismatch;
- stale expected state;
- ambiguous contextual replacement;
- failed multi-operation preparation.

A rejected validation or preparation must not mutate any target.

Do not log full file contents, secrets, authorization tokens, or unnecessary host paths.

Do not trust diff headers, MCP route names, display names, client metadata, or agent-supplied labels as authorization.

---

# 24. Backward compatibility

Before changing public types, serialized representations, or errors:

1. search callers;
2. inspect fixtures;
3. inspect MCP/CLI/API adapters;
4. inspect tests;
5. determine whether external JSON compatibility exists;
6. preserve compatible serde names and enum tags.

Do not rename serialized operation forms for style alone.

For a required compatibility break, document the old form, new form, reason, migration impact, and affected callers/tests.

---

# 25. Explicit non-goals

No second edit model.

No second editor/executor.

No second path-security system.

No capability/policy engine.

No identity/session redesign.

No snapshot store.

No snapshot retention/restore product.

No generic rollback/undo engine.

No persistent audit subsystem.

No MCP server/routing redesign.

No CLI/TUI framework redesign.

No model routing, LLM calls, planner, scheduler, or subagent system.

No worktree manager or distributed lock service.

If a missing dependency is discovered, document it rather than expanding scope.

---

# 26. Verification gates

Run, where supported:

    cargo fmt --all -- --check
    cargo check --all-targets
    cargo test --all-targets
    cargo clippy --all-targets --all-features -- -D warnings
    git diff --check

Also run focused edit tests for operation execution, unified diff parsing/application, expected-state conflicts, multi-operation preparation, and path/security behavior.

Report the actual result of every gate.

If a gate cannot be run, report:

    NOT VERIFIED — <exact reason>

Never convert unavailable verification into a success claim.

---

# 27. Completion criteria

Prompt 05 is complete only when:

- one canonical EditService execution path exists;
- Replace is deterministic and conflict-aware;
- Insert is deterministic;
- DeleteRange is deterministic;
- Patch uses the canonical transaction model;
- ApplyDiff uses one canonical parser/applier;
- every affected resource is prepared before multi-operation commit;
- preparation failure causes zero mutation;
- stale expected state is rejected;
- ambiguous contextual replacement is rejected;
- malformed diffs are rejected;
- unsafe paths are rejected;
- exact bytes/newline behavior is tested;
- Unicode/Devanagari/emoji behavior is tested;
- multi-file behavior is tested;
- no duplicate edit executor remains;
- existing filesystem primitives are reused;
- snapshot/rollback/authorization boundaries remain separate;
- adapters do not duplicate edit semantics;
- verification passes or unavailable gates are explicitly marked NOT VERIFIED;
- no unrelated changes remain.

Do not claim edit-engine completion means snapshot, rollback, audit, or authorization completion.

---

# 28. Independence rule

This prompt must work against the current repository regardless of whether any other implementation prompt is merged.

It must not require:

- Prompt 01;
- Prompt 02;
- Prompt 03;
- Prompt 04;
- Prompt 06;
- Prompt 07;
- Prompt 08;
- Prompt 09;
- any other prompt's PR;
- any prescribed execution order.

If a referenced contract exists, inspect and reuse it. If it does not exist, implement only the minimum local compatibility boundary required for this prompt.

Never instruct the implementer to wait for another prompt or branch.

---

# 29. Final implementation report

The implementer must report:

### Baseline

- branch;
- starting commit;
- working-tree state;
- relevant pre-existing edit behavior.

### Implementation

- canonical EditService path;
- Replace semantics;
- Insert semantics;
- DeleteRange semantics;
- Patch/multi-operation semantics;
- ApplyDiff semantics;
- preparation/commit boundary;
- expected-state/conflict behavior;
- path/security behavior.

### Compatibility

- changed public/serialized contracts;
- preserved contracts;
- migrations, if any.

### Tests

- focused tests;
- integration tests;
- adversarial/security tests;
- property tests, if used.

### Verification

Exact results for all cargo gates and git diff check. Mark unavailable checks NOT VERIFIED.

### Changed files

List every changed file.

### Limitations

State residual TOCTOU, atomicity, unsupported formats, compatibility constraints, and unverified gates.

### Scope confirmation

Explicitly confirm that no second edit model, executor, path-security system, authorization engine, snapshot store, rollback engine, audit subsystem, or transport-specific mutation implementation was created.

The final report must describe actual implementation, not future work.
