# AWE-005 / #26 — Unified-Diff Application

> **Type:** P0 agent-grade editing foundation  
> **Dependency:** AWE-001 / #22, AWE-002 / #23, AWE-003 / #24, AWE-004 / #25  
> **Primary scope:** production unified-diff parsing, preparation, application, and verification through the canonical EditService  
> **Branch:** `rust`  
> **Status:** Issue-resolution master prompt  

## 1. Mission

Implement safe, deterministic, production-grade **unified-diff application** for AWH through the canonical edit service.

The implementation must safely:

- parse standard text unified diffs;
- validate file headers and target paths;
- parse one or more hunks deterministically;
- validate hunk ranges and syntax;
- validate hunk context against the current file contents;
- support additions, deletions, and replacements represented by standard hunks;
- support multiple hunks for one file;
- support multiple files in one diff;
- integrate caller-supplied expected-state/hash checks;
- prepare every affected file before the first filesystem mutation;
- reuse AWE-004 multi-operation transaction preparation/commit semantics;
- perform canonical atomic writes;
- verify the resulting filesystem state;
- return one canonical `EditId` and per-file before/after state;
- reject unsupported binary patches explicitly rather than interpreting binary data as UTF-8 text.

The central invariant is:

> **Malformed input, invalid paths, stale expected state, hunk-context mismatch, unsupported binary input, or any other preparation failure must cause zero filesystem mutation.**

This issue must extend the existing agent-grade editing foundation. It must **not** become a parallel diff engine, parallel transaction model, parallel filesystem security layer, or transport-specific implementation.

---

## 2. Required pre-flight reading

Before modifying code, inspect the current `rust` branch rather than relying on historical assumptions.

Read:

```text
docs/PROJECT_CONTEXT.md
docs/PROJECT_ROADMAP.md
docs/PROJECT_ROADMAP_STATUS.md
docs/PROJECT_STATUS.md
docs/FEATURES.md
docs/CLI.md
docs/architecture.md
docs/mcp.md
docs/issue-resolving-prompts/AWE-001-canonical-edit-transaction-model.md
docs/issue-resolving-prompts/AWE-002-safe-contextual-replacement.md
docs/issue-resolving-prompts/AWE-003-line-range-insert-delete.md
docs/issue-resolving-prompts/AWE-004-multi-operation-filesystem-patch.md
this file
```

Then inspect the actual source tree, especially:

```text
src/services/edit.rs
src/services/
src/core/
src/models/
src/mcp/
src/cli/
Cargo.toml
```

Search for the actual implementations and callers of:

```text
EditTransaction
EditOperation
ApplyDiff
ExpectedState
FileState
EditStatus
EditError
filesystem.patch
replace
insert
delete_range
atomic write
validate_path
workspace root
```

Also locate:

- the canonical filesystem/path-security service;
- the canonical atomic-write helper;
- AWE-002 replacement execution;
- AWE-003 line parsing/edit execution;
- AWE-004 transaction preparation/commit;
- existing file-state/hash utilities;
- existing structured error conventions;
- current tests and temporary-filesystem test helpers.

Do not create a new helper when an existing canonical primitive can be safely extended.

---

## 3. Forensic baseline — verify before coding

The issue's current forensic baseline is authoritative for scope:

- `EditOperation::ApplyDiff` exists as a model variant.
- There is no complete unified-diff parser/executor in the target editing service yet.
- Unified-diff application must reuse the canonical edit transaction.
- There is no justification for creating a second patch transaction abstraction.
- MCP/CLI/TUI exposure is not part of this issue.

The current `EditOperation::ApplyDiff` shape is:

```rust
ApplyDiff {
    diff: String,
}
```

Preserve the public model unless the current branch demonstrates a necessary, backward-compatible domain clarification. Do not replace the canonical `EditTransaction` model with a diff-specific transaction type.

The roadmap's editing dependency chain is:

```text
EditTransaction
→ replace / insert / delete-range / patch
→ apply-diff
→ conflict detection
→ atomic commit
→ verification
→ history / rollback
```

AWE-005 is the **apply-diff** stage. AWE-006, AWE-007, and AWE-008 remain later transaction-safety milestones.

---

## 4. Hard scope boundary

Implement only what is necessary for AWE-005.

### In scope

- unified-diff grammar needed for standard text diffs;
- file header parsing;
- hunk header parsing;
- hunk body parsing;
- deterministic hunk validation;
- context matching;
- additions/deletions/replacements;
- multiple hunks per file;
- multiple files per transaction;
- path validation;
- expected-state validation where supplied;
- preparation before mutation;
- reuse of AWE-004 commit semantics;
- canonical atomic writes;
- post-write verification;
- structured errors;
- real filesystem tests;
- binary-patch rejection/isolation;
- EOF and newline correctness;
- deterministic result ordering.

### Explicitly out of scope

Do **not** implement in this issue:

- MCP tools/routes or MCP-specific schemas;
- CLI commands or CLI-specific diff parsing;
- TUI functionality;
- Control API endpoints;
- automatic snapshots;
- persistent provenance/audit storage;
- semantic rollback or crash-safe multi-file rollback from AWE-006;
- the AWE-007 stale-state coordinator;
- the AWE-008 generalized post-edit verification framework;
- edit history/undo;
- agent profiles or session authorization;
- worktree isolation;
- Git commit/push/pull orchestration;
- remote/cloud editing;
- connector integrations;
- a generic workflow/DAG engine;
- binary patch application;
- image/archive/binary format mutation;
- fuzzy patching that silently changes the requested target;
- a second filesystem security implementation;
- a second transaction model;
- a second edit result model unless the existing service genuinely requires a private internal preparation type.

If a later milestone needs an interface hook, add only the smallest transport-independent contract necessary for future compatibility.

---

## 5. Canonical architecture

The required high-level pipeline is:

```text
caller
  ↓
EditTransaction::ApplyDiff
  ↓
transaction shape validation
  ↓
parse complete diff
  ↓
validate global diff structure
  ↓
resolve + validate every target path
  ↓
classify/reject unsupported binary content
  ↓
load every affected file
  ↓
capture original FileState for every file
  ↓
validate caller ExpectedState values
  ↓
validate all file/hunk relationships
  ↓
apply all hunks in memory
  ↓
compute final content for every affected file
  ↓
compute final FileState for every affected file
  ↓
ONLY NOW enter filesystem commit phase
  ↓
reuse AWE-004 commit semantics / canonical atomic writes
  ↓
observe actual post-write state
  ↓
verify final state
  ↓
return one transaction result
```

The critical boundary is:

```text
================ PREPARE / VALIDATE ================
No filesystem mutation is allowed above this line.
=====================================================

==================== COMMIT =========================
Filesystem mutation begins only after the entire diff
and every affected file have passed preparation.
=====================================================
```

Do not implement:

```text
parse hunk → write file → parse next hunk → write again
```

or:

```text
file A → write A → file B → discover B is invalid
```

Those patterns violate the AWE-004 transaction invariant.

---

## 6. Unified-diff grammar to support

Support the standard **text unified-diff form** required for normal source-code editing.

The parser must understand file headers such as:

```diff
--- a/src/example.rs
+++ b/src/example.rs
```

and hunk headers such as:

```diff
@@ -10,7 +10,9 @@
```

with optional function/section context after the second range, for example:

```diff
@@ -10,7 +10,9 @@ fn example()
```

The hunk body consists of lines beginning with one of:

```text
' '  context
'+'  addition
'-'  deletion
```

and must be parsed without losing the distinction between these three record types.

The implementation must correctly handle the unified-diff convention for a final empty line / no-newline marker:

```text
\ No newline at end of file
```

Do not treat the marker as file content.

Do not silently accept arbitrary text as a hunk body merely because it resembles a diff.

---

## 7. File-header validation

Parse and validate each file pair before mutation.

### 7.1 Standard path pair

For normal modifications, support:

```diff
--- a/path/to/file
+++ b/path/to/file
```

The implementation must establish a deterministic target path.

The `a/` and `b/` prefixes used by common Git-generated unified diffs are metadata prefixes, not workspace path components. Do not accidentally resolve `a/../...` as a workspace path.

### 7.2 New files

Support standard text new-file diffs where represented by the normal unified-diff metadata, such as a `/dev/null` old side and a real new path.

The exact metadata form emitted by Git must be handled according to the parser's documented contract.

New-file application must still obey:

- workspace containment;
- path security;
- hunk validation;
- atomic creation/write semantics;
- expected-state rules when an expected state is supplied.

If the existing AWH filesystem service does not support creating a new file through the canonical edit boundary, fail explicitly and safely rather than silently falling back to an unsafe direct write.

### 7.3 Deleted files

Support standard text deletion diffs where represented by a real old path and `/dev/null` new side, if the existing canonical edit service supports deletion safely.

If file deletion is not part of the currently supported AWE-005 filesystem mutation contract, reject the diff explicitly during preparation with a structured unsupported-operation error.

Never emulate deletion by writing an arbitrary UTF-8 representation of a binary or sentinel value.

### 7.4 Renames and copies

Do not silently interpret Git extended rename/copy metadata as ordinary content edits.

Unless the current branch already has a canonical safe file-rename/copy service that AWE-005 is explicitly required to use, reject unsupported rename/copy metadata before mutation.

The rejection must be deterministic and must not partially apply the content hunks.

---

## 8. Path-security contract

Every resolved diff target must pass the same canonical workspace path-security boundary used by the existing edit service.

Reject:

- absolute paths;
- Windows drive-letter paths;
- UNC paths;
- `..` traversal;
- encoded traversal if the path representation can contain encoded data;
- root/prefix escapes;
- empty/invalid target paths;
- symlink-based workspace escapes where the existing security layer detects them;
- header tricks that cause one side of a diff to resolve outside the workspace.

Examples that must not become writable targets:

```text
/etc/passwd
C:\Windows\...
\\server\share\...
../../secret
../workspace-other/file
```

Do not merely validate the raw string and then construct a different unsafe path representation later.

Do not create a second `validate_path` algorithm.

Reuse the canonical path-security and workspace-containment primitives.

---

## 9. File-pair consistency

For each text file entry, validate the relationship between the `---` and `+++` headers before applying any hunk.

At minimum detect and handle:

- missing `---` header;
- missing `+++` header;
- empty/invalid header path;
- unsupported `/dev/null` side;
- path mismatch between old and new sides when the current service cannot represent a rename;
- duplicate target entries;
- the same target appearing in conflicting ways;
- file headers with no hunks when the format is not supported;
- hunks targeting a file not represented by the current file entry.

Do not let a malformed second file silently invalidate only itself after the first file has already been committed.

---

## 10. Hunk-header parsing

Parse hunk headers deterministically.

Support the normal forms:

```text
@@ -old_start,old_count +new_start,new_count @@
@@ -old_start +new_start @@
```

where omitted counts follow unified-diff semantics for a single line.

Validate:

- numeric syntax;
- non-negative/valid range values according to the unified-diff grammar;
- old/new range consistency with the hunk body;
- impossible ranges;
- integer overflow or unreasonable values;
- line positions that cannot exist in the target file;
- multiple hunks with deterministic ordering.

Do not allow arithmetic overflow to wrap line positions.

Do not use unchecked integer subtraction/addition when calculating target positions.

The optional trailing hunk function/context label is metadata. It may be retained for diagnostics, but it must not alter patch semantics.

---

## 11. Hunk-body validation

Each hunk body line must be classified as exactly one of:

```text
context
addition
deletion
```

or the recognized no-newline marker.

For every hunk, calculate:

```text
consumed_old_lines = context + deletion
produced_new_lines = context + addition
```

These counts must exactly match the old/new counts declared in the hunk header.

Reject the entire diff on mismatch.

Never silently truncate an overlong hunk.

Never silently pad an underlong hunk.

Never treat a malformed prefix as context.

---

## 12. Hunk context matching

Context matching is a safety boundary, not a best-effort convenience.

For a standard hunk:

```diff
@@ -10,5 +10,6 @@
 context A
-context B
+replacement B
 context C
```

the implementation must verify that the target file's relevant lines exactly match the hunk's expected old-side sequence before applying the change.

### Required behavior

- exact literal context matching;
- exact literal deletion matching;
- no whitespace trimming;
- no case folding;
- no Unicode normalization;
- no regex interpretation;
- no automatic fuzzy matching;
- no silent offset search that changes the requested target.

If the hunk's declared line position does not contain the expected old-side sequence, return a structured conflict/preparation error and perform no mutation anywhere in the transaction.

### Why no fuzzy matching

AWH is an agent-grade editing runtime. A silently relocated patch can modify the wrong function or wrong occurrence while appearing successful. Safety takes priority over patch convenience.

If future work wants explicitly opt-in fuzzy patching, that must be a separate architectural feature with its own conflict semantics. Do not add it here.

---

## 13. Hunk application semantics

For each hunk, transform the current **in-memory** file state from its old-side sequence to its new-side sequence.

A hunk must behave conceptually as:

```text
current file
    ↓
locate old-side hunk region
    ↓
verify context + deletions exactly
    ↓
remove old-side deleted lines
    ↓
insert added lines
    ↓
preserve context lines
    ↓
new in-memory file
```

Do not write the intermediate hunk result to disk.

For multiple hunks in the same file:

```text
original file
   ↓
hunk 1 in memory
   ↓
hunk 2 in memory
   ↓
hunk 3 in memory
   ↓
final content
   ↓
one filesystem commit
```

Do not independently reload the file from disk between hunks.

---

## 14. Multiple hunk ordering

Hunks must be applied deterministically.

For a standard unified diff, hunks are expected to describe the original file in ascending old-line order. Validate that ordering and reject contradictory/overlapping hunks unless the current parser contract explicitly supports another representation.

Do not sort hunks silently if doing so changes the supplied diff semantics.

Do not rely on incidental vector/map ordering.

A malformed, overlapping, contradictory, or impossible hunk set must fail during preparation before any write.

When applying multiple hunks in memory, account for earlier additions/deletions changing subsequent in-memory line positions. Do not accidentally interpret every later hunk against the original disk line numbers without applying the correct offset semantics.

A good implementation should model the relation explicitly rather than scattering ad-hoc offset arithmetic across the parser.

---

## 15. Multiple files in one diff

AWE-005 must reuse AWE-004 transaction preparation semantics for multi-file diffs.

Example:

```text
A: valid diff
B: malformed hunk
C: valid diff
```

Required result:

```text
A unchanged
B unchanged
C unchanged
```

The implementation must:

1. parse the complete diff;
2. validate every file entry;
3. resolve every path;
4. load every current file;
5. validate every expected state;
6. validate every hunk;
7. prepare every resulting file in memory;
8. calculate every final state;
9. only then begin filesystem commit.

Do not process file A through the commit path before file B has finished preparation.

The diff is one logical `EditTransaction` and must receive one stable `EditId`.

---

## 16. Expected-state integration

If the caller supplies `ExpectedState` values for the diff transaction, they must be enforced before mutation.

At minimum integrate the canonical:

- SHA-256 hash;
- byte size;
- line count;
- context/precondition semantics already established by AWE-002/AWE-004.

The current file state must be captured before applying any hunk.

Expected state describes the **original transaction boundary state**, not an intermediate hunk result.

For multiple files:

```text
expected A → original A
expected B → original B
expected C → original C
```

Do not validate a file's expected hash against an already-prepared in-memory result.

If the current transaction model's expected-state indexing cannot directly express per-file expectations for `ApplyDiff`, make the smallest backward-compatible clarification required by the existing architecture. Do not create a second expected-state model.

A stale expected state must produce a conflict and **zero mutation for the whole diff transaction**.

---

## 17. EOF and newline semantics

Unified diffs frequently carry important information about final newlines.

Correctly handle:

- LF files;
- CRLF files;
- files with a final newline;
- files without a final newline;
- empty files;
- one-line files;
- hunks at EOF;
- additions at EOF;
- deletions at EOF;
- replacements of the final line;
- the `\ No newline at end of file` marker.

The marker is metadata describing the preceding file-content line. It is not literal content and must not become part of the file.

Do not normalize the entire file's newline style merely because a diff was applied.

Preserve the established AWE-002/AWE-003 text-edit contract.

Where the diff explicitly changes whether a file ends in a newline, the resulting bytes must reflect that change exactly.

Tests must compare actual bytes, not merely `lines()` output.

---

## 18. UTF-8 and Unicode

Text unified-diff application must safely support valid UTF-8 source files containing:

- ASCII;
- Devanagari/Hindi;
- CJK;
- accented characters;
- emoji;
- combining characters.

Do not split a UTF-8 code point while parsing or applying line content.

Line-based byte offsets are acceptable internally only when all string slicing boundaries are proven valid UTF-8 boundaries.

Do not Unicode-normalize content.

Do not convert a text diff into arbitrary byte replacement logic.

If the diff contains bytes that cannot be represented by the service's UTF-8 text contract, reject it explicitly as unsupported rather than corrupting the target file.

---

## 19. Binary patch handling

Binary data is a hard safety boundary.

AWE-005 must **not** silently interpret binary patches as UTF-8 text.

Explicitly recognize or conservatively reject binary-related diff forms, including Git-style binary patch sections such as:

```text
diff --git ...
GIT binary patch
literal ...
delta ...
```

Also handle binary indicators such as:

```text
Binary files ... differ
```

The required behavior for the current milestone is:

```text
binary patch detected
        ↓
structured unsupported-binary error
        ↓
zero filesystem mutation
```

Do not attempt to implement Git's binary patch encoding/decoding in this issue.

Do not decode arbitrary base85/binary patch data into a UTF-8 `String` and write it.

If the current branch already has a safe, canonical binary-patch service, it may be integrated only if AWE-005 can reuse it without creating a second mutation path. Otherwise reject binary patches explicitly.

---

## 20. Git extended headers

A Git-generated diff may contain metadata such as:

```text
diff --git a/file b/file
index abc123..def456 100644
new file mode 100644
old mode 100644
new mode 100755
similarity index ...
rename from ...
rename to ...
copy from ...
copy to ...
```

Do not assume every line after `diff --git` is a content hunk.

The parser must distinguish supported text-content metadata from unsupported operation metadata.

At this milestone:

- ordinary Git metadata that does not change text-patch semantics may be ignored safely after validation;
- binary patch metadata must trigger explicit unsupported behavior;
- rename/copy/mode-change semantics must not be silently applied unless an existing canonical service explicitly supports them;
- malformed extended metadata must fail safely when it makes the diff ambiguous.

Do not let Git metadata bypass the normal path-security or transaction boundary.

---

## 21. Empty and new-file cases

Test standard edge cases explicitly.

### Empty file creation

A valid new-file text diff may contain:

```diff
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
```

The parser must correctly interpret the old side as an empty/nonexistent file according to the supported filesystem contract.

### Empty file deletion

A deletion diff must either use the canonical safe deletion service or fail explicitly as unsupported. Never invent a fake empty-file representation if the semantic operation is deletion.

### Empty hunks

Reject malformed empty hunks when the declared ranges/content cannot represent a valid standard diff.

Do not accept an apparently successful patch that changes nothing unless the current contract explicitly allows a no-op and reports it deterministically.

---

## 22. Duplicate and conflicting file entries

A single diff must not produce ambiguous multi-entry semantics.

Detect cases such as:

```text
file A appears twice with incompatible hunks
file A appears once as new-file and once as modification
file A appears once as deletion and once as modification
```

Unless the existing canonical parser explicitly supports sequential file sections for the same target, reject them during preparation.

Do not apply the first occurrence and silently discard the second.

Do not rely on map insertion behavior to resolve conflicts.

---

## 23. Diff parser design

Keep parsing separate from filesystem mutation.

A good internal architecture is conceptually:

```text
raw diff
  ↓
DiffDocument
  ├── FilePatch
  │    ├── old_path
  │    ├── new_path
  │    ├── metadata
  │    └── Hunks
  │         ├── old range
  │         ├── new range
  │         └── records
  ↓
validated DiffDocument
  ↓
prepared file transformations
  ↓
AWE-004 transaction commit
```

The exact type names are implementation details.

Requirements:

- parser types remain transport-independent;
- parser must not perform filesystem writes;
- parser errors are deterministic and structured;
- parser can be unit-tested independently from the filesystem;
- application code consumes validated parsed structures rather than reparsing strings.

Do not build a general-purpose patch language framework.

---

## 24. Parser failure taxonomy

Use the repository's typed error conventions.

At minimum distinguish the important categories:

```text
EmptyDiff
MalformedHeader
MissingFileHeader
InvalidFilePath
UnsupportedPathForm
MalformedHunkHeader
InvalidHunkRange
HunkBodyCountMismatch
UnexpectedHunkRecord
ContextMismatch
HunkRangeMismatch
DuplicateFilePatch
UnsupportedBinaryPatch
UnsupportedRename
UnsupportedCopy
UnsupportedModeChange
MissingTargetFile
UnexpectedTargetFileState
ExpectedStateConflict
FileReadFailed
PreparationFailed
ApplyFailed
VerificationFailed
```

Exact enum names may differ to match the existing `EditError` architecture.

Do not turn every parser failure into a generic string error if callers need to distinguish malformed input from stale file state or unsupported binary data.

Errors must not include full private file contents unless the existing diagnostics contract explicitly permits a safe bounded excerpt.

---

## 25. Deterministic behavior

AWH agents need predictable edit semantics.

The implementation must guarantee deterministic:

- parsing;
- file-section ordering;
- hunk ordering;
- target-path resolution;
- context validation;
- error category;
- affected-file result ordering;
- before/after state ordering.

Do not let `HashMap` iteration order define externally observable behavior.

If maps are used internally for grouping, use an explicit deterministic ordering when producing or committing results.

Do not sort user-supplied operations/hunks merely to make implementation easier if sorting changes their semantic meaning.

---

## 26. AWE-004 integration

AWE-005 must **consume AWE-004's transaction preparation/commit semantics**, not reimplement them.

The conceptual composition is:

```text
UnifiedDiffParser
       ↓
validated FilePatch/Hunk structures
       ↓
per-file in-memory transformation
       ↓
AWE-004-style preparation plan
       ↓
complete transaction validation
       ↓
canonical atomic commit
       ↓
post-commit state verification
```

AWE-005 may introduce diff-specific parsing and transformation logic, but the filesystem transaction boundary remains AWE-004's responsibility.

Do not duplicate:

- multi-file preparation;
- canonical atomic write handling;
- transaction identity;
- expected-state mapping;
- result aggregation;
- workspace path-security semantics;
- commit-phase orchestration.

If AWE-004's current service API cannot express the diff transformation cleanly, extend that existing service minimally rather than building an independent `DiffService` that writes files itself.

---

## 27. AWE-002 and AWE-003 integration

Reuse the established semantics from earlier editing milestones.

AWE-002 owns/reuses:

- expected-state conflict semantics;
- exact text safety principles;
- canonical file-state calculation;
- atomic edit boundary;
- path-security integration.

AWE-003 owns/reuses:

- canonical line semantics;
- line-ending handling;
- UTF-8-safe line boundaries;
- deterministic line transformation behavior.

AWE-005 must not copy those algorithms into a new implementation merely because unified diffs are line-oriented.

Where a shared helper is missing, extract or extend the smallest canonical helper in the existing edit service.

---

## 28. Transaction identity and result contract

One `ApplyDiff` request represents one logical edit transaction.

Requirements:

- generate/use exactly one canonical `EditId`;
- do not generate one ID per file;
- do not generate one ID per hunk;
- associate all affected files with that ID;
- return per-file before state;
- return per-file after state when the transaction commits and verification succeeds;
- report deterministic status and structured failure information.

The result should make it possible for future MCP/CLI/TUI/API layers to expose the operation without reinterpreting its semantics.

Do not create a transport-specific result shape inside the service layer.

---

## 29. Lifecycle/status semantics

Use the existing AWH edit lifecycle vocabulary.

The implementation should conceptually progress through:

```text
Requested
→ Located
→ Validated
→ Applied
→ Verified
→ Committed
```

or the repository's exact established equivalent.

Failures must use appropriate existing states such as:

```text
Rejected
Conflict
ValidationFailed
ApplyFailed
VerificationFailed
```

Do not report `Committed` merely because the in-memory patch succeeded.

Do not report `Verified` before observing/validating the resulting filesystem state.

Do not add rollback states or snapshot states as part of AWE-005's implementation.

---

## 30. Atomic commit and failure semantics

AWE-005 inherits AWE-004's distinction between **preparation safety** and **rollback safety**.

Before commit:

> Any validation, parsing, path, context, expected-state, or preparation failure must leave all files unchanged.

During commit:

- use the canonical atomic write primitive;
- avoid intermediate hunk writes;
- prefer one final write per affected file;
- report partial commit information honestly if a later filesystem write fails.

Do not implement AWE-006 rollback merely to make this failure mode disappear.

If file A commits and file B fails at the filesystem commit boundary, do not falsely claim the entire transaction was atomically rolled back.

Return a structured apply/commit failure that identifies what is known about committed versus uncommitted files.

---

## 31. Post-commit verification

After commit, calculate/observe actual `FileState` for every affected file.

At minimum verify:

```text
actual hash
actual byte size
actual line count
```

against the prepared final state.

For each file, verify that the actual bytes correspond to the hunk transformation that was prepared.

Do not trust only an in-memory string to claim successful persistence.

If verification fails:

- return `VerificationFailed` or the canonical equivalent;
- identify the affected path when possible;
- do not silently retry;
- do not silently overwrite again;
- do not add rollback behavior from AWE-006.

---

## 32. No-mutation-on-failure invariant

This must be demonstrated by real filesystem tests.

For every failure that occurs before the commit boundary:

```text
all affected files after == all affected files before
```

Mandatory cases include:

- empty diff;
- malformed file header;
- malformed hunk header;
- hunk count mismatch;
- invalid path;
- absolute path;
- traversal path;
- symlink escape;
- unsupported binary patch;
- unsupported rename/copy where applicable;
- missing target file;
- expected hash mismatch;
- expected size mismatch;
- expected line-count mismatch;
- expected context mismatch;
- hunk context mismatch;
- invalid EOF/no-newline representation;
- duplicate/conflicting file section;
- invalid second file after a valid first file;
- invalid later hunk after valid earlier hunks;
- preparation read failure;
- unsupported file operation.

The strongest required test is:

```text
A valid
B invalid
C valid
→ A unchanged
→ B unchanged
→ C unchanged
```

---

## 33. Newline and EOF correctness tests

Use real temporary files and compare bytes.

Required scenarios:

1. LF single-hunk replacement;
2. CRLF single-hunk replacement;
3. final line with newline;
4. final line without newline;
5. `\ No newline at end of file` on old side;
6. no-newline marker on new side;
7. addition at EOF;
8. deletion at EOF;
9. replacement of final line;
10. empty file/new-file diff;
11. one-line file;
12. multiple hunks separated by unchanged lines;
13. Unicode/Devanagari/emoji content;
14. preservation of unrelated newline bytes.

Do not use only high-level line iterators for these assertions; verify the actual serialized bytes.

---

## 34. Path and security tests

Real filesystem tests must cover:

- absolute POSIX path;
- Windows-style drive path even when tests run on Unix, if parser semantics allow the string;
- UNC-like path;
- `../` traversal;
- nested traversal;
- target path containing `..` after a prefix;
- symlink escape where supported;
- valid nested workspace path;
- header `a/` and `b/` prefix handling;
- `/dev/null` handling;
- malformed path header;
- two file headers resolving to the same target unexpectedly.

The tests must prove rejected paths cause no mutation.

---

## 35. Parser unit-test matrix

Build focused parser tests independent of filesystem mutation.

### Valid

- one file / one hunk;
- one file / multiple hunks;
- multiple files;
- single-line omitted hunk counts;
- optional hunk section labels;
- additions;
- deletions;
- replacements;
- final no-newline marker;
- Git-style `diff --git` metadata followed by valid text hunks;
- new-file text diff if supported;
- deleted-file text diff if supported.

### Invalid

- empty diff;
- incomplete file header;
- missing `+++`;
- missing `---`;
- malformed hunk marker;
- invalid numbers;
- integer overflow;
- old-count mismatch;
- new-count mismatch;
- unexpected hunk record prefix;
- misplaced no-newline marker;
- binary patch marker;
- malformed Git metadata that changes semantics;
- unsupported rename/copy metadata;
- duplicate file sections;
- contradictory paths.

Parser tests must assert structured error categories, not only that an error occurred.

---

## 36. Real filesystem integration-test matrix

At minimum implement:

### Basic success

1. one-file one-hunk patch;
2. one-file multi-hunk patch;
3. multi-file patch;
4. pure addition;
5. pure deletion;
6. replacement;
7. patch touching first line;
8. patch touching last line;
9. patch touching EOF.

### Conflict/failure

10. context mismatch;
11. deletion mismatch;
12. stale expected hash;
13. stale expected size;
14. stale expected line count;
15. stale expected context;
16. malformed later hunk;
17. malformed second file;
18. missing file;
19. invalid path;
20. traversal;
21. binary patch;
22. unsupported rename/copy;
23. duplicate target entry;
24. commit failure where injectable;
25. verification failure where injectable.

### Text correctness

26. LF;
27. CRLF;
28. no final newline;
29. final newline;
30. empty file;
31. one-line file;
32. Unicode;
33. Devanagari;
34. emoji;
35. large-but-supported file;
36. multiple hunks with shifted line positions.

For every preparation failure, compare all affected file bytes before and after.

---

## 37. Property-oriented tests

If the repository already uses `proptest` or another property-testing facility, add focused properties where useful.

Good candidates:

```text
valid diff application is deterministic
invalid diff preparation causes zero mutation
reported after hash equals actual resulting bytes
hunk old/new counts always agree with parsed records
context/deletion records consumed by a hunk equal its declared old count
```

Do not introduce a large new fuzzing framework solely for AWE-005.

If fuzzing already exists, include malformed-diff inputs that must never panic or mutate files.

---

## 38. Panic-safety and malformed input

Unified diff is agent-controlled input and must be treated as untrusted.

The parser/application path must not panic on:

- empty strings;
- very long headers;
- huge numeric values;
- truncated hunks;
- invalid UTF-8 under the chosen API boundary;
- unexpected line prefixes;
- missing headers;
- inconsistent counts;
- duplicate sections;
- malicious traversal paths;
- binary patch content.

Avoid unchecked indexing, unchecked arithmetic, `unwrap()`/`expect()` on untrusted parse state, and assumptions that a split operation always returns the expected number of fields.

Errors must be returned through the repository's typed error architecture.

---

## 39. Resource and size limits

Respect existing workspace/filesystem limits.

Do not permit an attacker-controlled diff to create unbounded memory usage merely by declaring enormous hunk counts or supplying enormous lines.

At minimum:

- reject integer overflow;
- reject impossible ranges before allocation;
- avoid allocating vectors based solely on an untrusted count without a bounded/validated relationship to actual input;
- reuse existing file/diff size limits if present;
- fail before filesystem mutation when limits are exceeded.

Do not silently truncate the diff or target file.

If the repository does not yet have a dedicated diff-size limit, use the smallest reasonable guard consistent with existing AWH resource-limit conventions and avoid creating an unrelated global configuration system in this issue.

---

## 40. Encoding policy

AWE-005 is a text unified-diff feature.

The implementation must explicitly define its input encoding contract.

If the service accepts a Rust `String`, invalid UTF-8 cannot reach the parser through that API; however, target files may still contain bytes that cannot be decoded as UTF-8.

For non-UTF-8 target files:

- do not reinterpret arbitrary bytes as UTF-8;
- do not partially overwrite them;
- return a structured unsupported/binary/text-encoding error unless an existing canonical service explicitly supports another representation.

For UTF-8 files, preserve bytes outside the requested hunk changes.

---

## 41. No duplicate editing algorithms

AWE-005 must not become a second implementation of AWE-002/AWE-003/AWE-004.

Do not duplicate:

- path validation;
- workspace containment;
- SHA-256 calculation;
- file-state construction;
- expected-state validation;
- atomic write logic;
- transaction ID generation;
- line-boundary logic where an existing reusable helper is available;
- transaction result aggregation.

The diff-specific code should primarily own:

```text
diff parsing
hunk validation
hunk context matching
in-memory diff transformation
```

The canonical edit service should continue to own:

```text
transaction identity
filesystem security
state validation
preparation/commit boundary
atomic writes
final state verification
```

---

## 42. Shared-service / transport independence

The implementation must live below interface layers.

Future interfaces must be able to call:

```text
MCP
CLI
TUI
Control API
    ↓
shared EditService
```

Do not place parser/application logic inside:

```text
src/mcp/*
src/cli/*
src/tui/*
src/api/*
```

unless the current repository architecture has an existing shared-service location that makes another placement necessary.

The CLI contract already defines `fs apply-diff` as a target command, but AWE-005 must implement the underlying reusable service rather than the command itself.

Likewise, the MCP architecture requires shared application services rather than transport-specific filesystem semantics.

---

## 43. Backward compatibility

Do not break AWE-001/AWE-002/AWE-003/AWE-004 behavior.

After implementation, existing tests for:

- `EditTransaction`;
- `EditOperation`;
- replacement;
- line insert/delete;
- multi-operation patch;
- path security;
- file state/hash;
- atomic writes

must continue to pass.

If a shared helper must change, preserve existing semantics and add regression tests.

Do not opportunistically refactor unrelated editing or MCP code.

---

## 44. Code quality requirements

Follow existing Rust conventions.

Required:

- `cargo fmt --all -- --check` clean;
- `cargo check --all-targets` clean;
- `cargo test --all-targets` clean;
- `cargo clippy --all-targets --all-features -- -D warnings` clean;
- public APIs documented where applicable;
- no avoidable `unwrap()`/`expect()` in untrusted diff paths;
- typed errors for meaningful parser/application categories;
- bounded allocations;
- no dead code introduced merely to prepare for speculative future features.

Keep parser and application code readable. Diff parsing is security-sensitive because malformed agent-controlled input crosses into filesystem mutation.

---

## 45. Verification workflow

After implementation, run the full verification sequence:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Then perform focused validation of AWE-005:

```text
1. parser unit tests
2. hunk-count tests
3. context mismatch tests
4. stale-state tests
5. path-security tests
6. binary-patch rejection tests
7. single-file real filesystem tests
8. multi-hunk tests
9. multi-file real filesystem tests
10. CRLF/EOF/no-newline tests
11. Unicode/Devanagari/emoji tests
12. zero-mutation-on-preparation-failure tests
13. deterministic before/after state tests
14. commit/verification failure tests where injectable
```

Do not treat compilation alone as completion.

---

## 46. Diff/scope audit

Before committing, inspect the final Git diff.

Confirm that:

- the AWE-005 implementation is actually connected to the canonical edit service;
- no duplicate transaction model was created;
- no duplicate path-security implementation was created;
- no transport-specific implementation was added unnecessarily;
- no MCP/CLI/TUI/API feature was implemented prematurely;
- no AWE-006 rollback logic was added;
- no AWE-007 stale-state coordinator was added;
- no AWE-008 generalized verification framework was added;
- no unrelated files were modified.

If an unrelated modification appears, revert it before declaring AWE-005 complete.

---

## 47. Definition of Done

AWE-005 is complete only when all of the following are true:

- [ ] `ApplyDiff` is implemented through the canonical edit service.
- [ ] Standard text unified-diff file headers are parsed deterministically.
- [ ] Standard hunk headers are parsed and validated.
- [ ] Hunk body counts exactly match declared ranges.
- [ ] Context and deletion lines are checked exactly against the target file.
- [ ] Additions/deletions/replacements apply correctly in memory.
- [ ] Multiple hunks for one file work deterministically.
- [ ] Multiple files share one transaction and one `EditId`.
- [ ] Every affected file is prepared before the first mutation.
- [ ] Expected-state/hash checks are enforced where supplied.
- [ ] Absolute/traversal/symlink-escape paths are rejected through canonical security helpers.
- [ ] Binary patches are explicitly rejected or routed through an already-existing safe binary service; they are never treated as UTF-8 text.
- [ ] Unsupported rename/copy/mode semantics are explicit rather than silently applied.
- [ ] LF/CRLF/final-newline/EOF semantics are tested.
- [ ] UTF-8/Devanagari/emoji cases are tested.
- [ ] Malformed input never panics.
- [ ] Preparation failures cause zero mutation across all affected files.
- [ ] Atomic filesystem writes use the existing canonical boundary.
- [ ] Post-write `FileState` is verified.
- [ ] Result ordering is deterministic.
- [ ] Structured errors distinguish malformed diff, conflict, unsupported binary, apply, and verification failures where appropriate.
- [ ] AWE-001 through AWE-004 regression tests remain green.
- [ ] `cargo fmt` passes.
- [ ] `cargo check --all-targets` passes.
- [ ] `cargo test --all-targets` passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [ ] Final Git diff contains only intended AWE-005 implementation/test changes.

---

## 48. Required final implementation report

When the implementation is complete, report:

1. **AWE-005 implementation summary** — what was actually added/changed.
2. **Parser contract** — supported unified-diff forms and intentionally unsupported forms.
3. **Transaction integration** — how AWE-004 preparation/commit was reused.
4. **Path/security behavior** — how all target paths are validated.
5. **Binary behavior** — exactly how binary patches are detected/rejected.
6. **Hunk semantics** — context, offsets, counts, EOF, and newline behavior.
7. **Expected-state behavior** — hash/context/size/line-count handling.
8. **Verification behavior** — how final states are confirmed.
9. **Tests added** — unit, integration, property/fuzz tests if any.
10. **Verification commands/results** — fmt/check/test/clippy.
11. **Files changed** — exact list, with unrelated changes explicitly excluded.
12. **Known limitations** — only genuine limitations that remain after implementation.
13. **Commit/PR information** — if applicable.

Do not claim binary support, rollback, race-free filesystem mutation, or transport exposure unless those features were actually implemented and tested.

---

## 49. Hard STOP rules

After AWE-005 is correctly implemented and verified:

**STOP.**

Do not continue automatically into:

- AWE-006 atomic rollback-safe edits;
- AWE-007 stale-state coordination;
- AWE-008 post-edit verification framework;
- AWE-009 MCP agent-grade editing;
- AWE-010 edit-level rollback;
- snapshots/provenance/audit persistence;
- CLI/TUI/API exposure;
- agent profiles/worktrees/collaboration;
- remote/cloud editing;
- unrelated refactors.

If a later milestone is required to complete an architectural dependency, make only the smallest compatibility change needed for AWE-005 and clearly report it. Do not implement the later milestone.

---

## 50. Final implementation instruction

Resolve **AWE-005 / GitHub issue #26 — Add unified-diff application** on the `rust` branch.

Treat this document as the complete issue-resolution contract.

Implement a safe, deterministic, text-only unified-diff application path that:

```text
parses
  ↓
validates
  ↓
context-checks
  ↓
prepares every file
  ↓
reuses AWE-004 transaction semantics
  ↓
atomically commits
  ↓
verifies
  ↓
returns one EditId + per-file before/after state
```

The non-negotiable invariant is:

> **No malformed, stale, conflicting, unsupported, or otherwise unprepared unified diff may mutate any affected file.**

Use the existing AWH editing/security architecture. Do not build parallel infrastructure. Do not silently broaden scope. Verify the real filesystem behavior, not merely parser output.

When the Definition of Done and verification gates pass, report the implementation and **STOP at AWE-005**.