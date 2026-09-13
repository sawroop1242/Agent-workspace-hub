# AWE-014 / #35 — CLI Agent-Grade Editing

## Master Issue-Resolving Prompt

### Mission

Implement and verify the production-grade **CLI surface for agent-grade filesystem editing**.

The CLI must expose the canonical editing service through the final `awh fs` contract while remaining a thin transport adapter. It must provide the same editing semantics, authorization behavior, conflict handling, verification, rollback guarantees, and audit/provenance correlation as the MCP editing surface.

The core rule is:

```text
CLI ───────────────┐
MCP ───────────────┤
TUI ───────────────┼→ canonical AWH services → filesystem
Control API ───────┘
```

There must be **one editing implementation**. The CLI must never contain a second replacement engine, line editor, patcher, diff applier, rollback algorithm, policy implementation, or filesystem mutation path.

The issue body explicitly establishes that the final CLI contract lists `awh fs patch/replace/insert/delete-range/apply-diff/hash/verify/history/rollback`, but the forensic audit confirms that the agent-grade edit commands are not yet implemented. fileciteturn154file0

The target command contract is documented as a product surface, not proof of implementation, so only commands backed by real working services may be exposed as completed functionality. fileciteturn153file0

---

## 1. Authoritative forensic baseline

Before modifying implementation code, inspect the actual `rust` branch.

Do not assume that documentation means code already exists.

Verify:

- current CLI framework and command registration
- current `main.rs` responsibilities
- existing CLI modules/subcommands
- canonical `EditService`
- `EditTransaction`
- `EditId`
- `EditOperation`
- `ExpectedState`
- `FileState`
- edit lifecycle/status model
- capability and policy services
- caller/agent/session/workspace identity resolution
- snapshot/provenance service
- audit service
- MCP edit tools and their schemas
- existing filesystem path validation helpers
- existing CLI output/error conventions
- existing integration-test structure

The current canonical CLI reference explicitly requires the CLI to delegate to shared application services and forbids independent implementations of filesystem, editing, policy, snapshots, sessions, and other domain behavior. fileciteturn153file0

The CLI reference also places the edit commands in the agent-grade editing phase after `EditTransaction`, with conflict detection, atomic apply/rollback, post-edit verification, history, and rollback as the surrounding service capabilities. fileciteturn153file0

If implementation has advanced beyond the documented forensic state, preserve the newer correct behavior and close only the remaining gap.

Do not rewrite working architecture merely to match this prompt.

---

## 2. Non-negotiable architecture

### 2.1 CLI is a thin adapter

CLI responsibilities are limited to:

1. parse arguments/options
2. resolve the current invocation context
3. construct the canonical request/transaction model
4. invoke the shared service
5. render human-readable or machine-readable results
6. map canonical errors to stable process exit codes

The CLI must not:

- read and rewrite files directly for editing
- implement string replacement itself
- implement line insertion/deletion itself
- parse unified diffs itself if the canonical service owns that logic
- implement rollback itself
- calculate an alternative edit hash/state model
- bypass capability checks
- bypass policy checks
- bypass snapshots
- bypass verification
- invent a CLI-only `edit_id`
- create a CLI-only audit model

### 2.2 Same EditService as MCP

Every CLI edit command must reach the **same canonical EditService** used by MCP.

Do not create:

```text
CliEditService
CliPatchService
CliRollbackService
```

or any equivalent parallel implementation.

The issue explicitly requires the exact same EditService semantics as MCP. fileciteturn154file0

### 2.3 Shared security pipeline

The effective flow must remain:

```text
CLI arguments
  ↓
caller / agent identity
  ↓
session
  ↓
workspace
  ↓
capability
  ↓
policy
  ↓
EditTransaction
  ↓
expected-state/conflict validation
  ↓
snapshot
  ↓
mutation
  ↓
post-edit verification
  ↓
commit
  ↓
provenance + audit
```

The CLI must not reorder this sequence in a way that weakens security or recovery guarantees.

---

## 3. Command contract

Implement the following agent-grade editing commands:

```text
awh fs replace
awh fs insert
awh fs delete-range
awh fs patch
awh fs apply-diff
awh fs rollback
```

Each command must be discoverable through:

```bash
awh fs --help
```

and its own help output:

```bash
awh fs replace --help
awh fs insert --help
awh fs delete-range --help
awh fs patch --help
awh fs apply-diff --help
awh fs rollback --help
```

Do not expose a command merely because it appears in `docs/CLI.md`. The implementation must be backed by the canonical service and pass real command-level validation. The CLI reference itself warns that its command tree is the final target surface rather than proof that every command currently exists. fileciteturn153file0

---

## 4. `awh fs replace`

Expose canonical contextual replacement.

The command must accept enough information to construct the canonical `Replace` operation, including as applicable:

- target path
- exact search/match text
- replacement text
- expected state
- expected occurrence semantics
- agent/session/workspace context

Do not implement replacement logic inside the CLI.

The service must remain responsible for:

- exact matching
- occurrence validation
- stale-state detection
- contextual validation
- path security
- authorization
- snapshotting
- mutation
- verification
- audit/provenance

Support explicit expected-state inputs where the service contract allows them, rather than silently replacing the user's expected-state protections with a weaker CLI convenience mode.

If the canonical service requires explicit expected-state data, the CLI must require or derive it through an authoritative read operation; it must never silently use a stale or invented value.

---

## 5. `awh fs insert`

Expose canonical line-boundary insertion.

Preserve the exact service semantics established by AWE-003:

- one-based line semantics
- insertion before the specified line
- `0` as the beginning-of-file boundary where supported by the service
- deterministic EOF behavior
- correct handling of empty files
- correct final-newline behavior
- UTF-8 and Unicode safety

The CLI must pass the requested line/content into the canonical transaction model.

Do not split or reinterpret Unicode text at the CLI layer.

Do not implement line parsing independently from the canonical service.

---

## 6. `awh fs delete-range`

Expose canonical inclusive line-range deletion.

Preserve the canonical semantics:

```text
start_line … end_line
```

with one-based inclusive boundaries.

The CLI must reject invalid ranges through the canonical validation path rather than performing partial local mutation.

Support:

- deleting one line
- deleting multiple lines
- deleting all content
- empty files
- final newline preservation rules
- LF/CRLF handling
- Unicode/Devanagari/emoji content

No direct file rewriting may occur in the command handler.

---

## 7. `awh fs patch`

Expose the canonical multi-operation patch transaction.

This command must support the AWE-004 transaction model rather than sequentially performing independent CLI edits.

Requirements:

- construct one `EditTransaction`
- preserve the transaction's `edit_id`
- validate all operations before mutation
- preserve expected-state checks
- preserve deterministic operation ordering
- preserve multi-file transaction semantics
- preserve atomic/recovery semantics
- preserve post-edit verification
- preserve rollback behavior
- emit one correlated lifecycle result

The CLI must not implement “patch” as:

```text
for each operation:
    run another CLI edit
```

unless the canonical service explicitly models that composition as one transaction.

A multi-operation failure must not leave the CLI reporting success for partially applied work.

---

## 8. `awh fs apply-diff`

Expose canonical unified-diff application from AWE-005.

The CLI must treat the diff as input to the canonical service.

Do not implement a second diff parser in the CLI.

The service remains responsible for:

- diff parsing
- hunk validation
- context matching
- path validation
- multi-file semantics
- expected-state/conflict handling
- binary-content rejection where applicable
- newline/EOF behavior
- Unicode behavior
- transaction preparation
- atomic mutation
- verification
- rollback

Support reading the diff from the CLI according to existing CLI conventions, such as an input file or standard input, if the service/application architecture supports it.

Do not silently convert an invalid diff into a whole-file overwrite.

This directly enforces the issue requirement that the CLI must **never silently fall back to whole-file overwrite**. fileciteturn154file0

---

## 9. `awh fs rollback`

Expose canonical edit-level rollback from AWE-010.

Rollback must accept the canonical `EditId`.

The CLI must not reconstruct inverse operations itself.

The service must remain responsible for:

- locating the edit
- resolving its snapshot/provenance
- authorization
- capability validation
- conflict detection
- exact-byte restoration
- atomic/recovery-aware mutation
- post-rollback verification
- audit/provenance

Preserve conflict-aware rollback semantics.

If the target file has changed since the original edit's verified post-edit state, rollback must be rejected rather than overwriting the newer state.

The CLI must surface the structured rollback conflict/failure rather than hiding it behind a generic error.

Repeated rollback must use the canonical service semantics such as an already-rolled-back state where supported.

---

## 10. Edit ID and lifecycle output

Every mutation command must expose the authoritative `edit_id` when an edit transaction exists.

Human-readable output should make the lifecycle understandable without requiring users to inspect raw logs.

For example:

```text
Edit: <edit-id>
Operation: replace
Path: src/example.rs
Status: committed
Before: <hash>
After:  <hash>
```

The exact presentation may follow existing CLI conventions.

Do not fabricate status information in the CLI.

Only display:

- status returned by the service
- hashes returned by authoritative state
- conflict information returned by the canonical service
- snapshot/provenance identifiers returned by the canonical service

For conflicts, expose enough structured information to explain:

- that the state was stale/conflicting
- expected state summary
- actual state summary
- affected path/operation
- edit ID where one exists

Do not print file contents merely to make an error more verbose.

---

## 11. Human-readable output

Human-readable output is the default.

Output must be:

- concise
- deterministic
- useful to developers/agents
- free of secrets
- free of misleading success messages
- stable enough for terminal use

Examples of conceptual states:

```text
Edit requested
Edit authorized
Edit applied
Edit verified
Edit committed
```

or a concise final status:

```text
Edit <id> committed successfully.
```

Do not make output depend on debug logging.

Do not expose stack traces by default.

---

## 12. Machine-readable output

Provide stable machine-readable output where the existing CLI framework supports it.

Prefer an explicit format option such as:

```bash
--output json
```

only if consistent with the existing CLI architecture.

Do not invent a second JSON schema separate from MCP/service responses.

The machine-readable representation should preserve canonical fields such as:

```text
edit_id
operation
status
path(s)
before_state
after_state
snapshot_id
conflict/error information
rollback information
```

Use stable enum/string values.

Do not include secret material or full file contents.

JSON output must remain parseable even when the command fails.

Do not mix human progress text into stdout when stdout is explicitly requested as machine-readable data. Use stderr for diagnostics if the CLI architecture permits this.

---

## 13. Exit-code contract

Process exit status is part of the CLI API.

At minimum, return non-zero for:

- invalid CLI arguments
- path validation failure
- authorization denial
- capability denial
- stale-state conflict
- validation failure
- snapshot failure
- apply failure
- verification failure
- commit failure where applicable
- rollback conflict
- rollback failure
- internal service failure

The issue explicitly requires non-zero status for conflict, policy denial, verification failure, and rollback failure. fileciteturn154file0

Do not return exit code `0` merely because the CLI process successfully invoked the service.

If the repository already has a canonical exit-code taxonomy, reuse it.

Otherwise define a small stable taxonomy that distinguishes at least:

```text
usage/input error
authorization/policy failure
conflict
operation failure
internal failure
```

Do not create dozens of unstable one-off exit codes without a real need.

---

## 14. Error mapping

Map canonical service errors into CLI output and exit status without destroying structured information.

Examples:

```text
EditError::Conflict
→ conflict message + edit/path/state details + non-zero exit

PolicyDenied
→ authorization denied + policy reason + non-zero exit

VerificationFailed
→ verification failure + edit ID + non-zero exit

RollbackConflict
→ rollback rejected because target changed + non-zero exit

SnapshotFailure
→ snapshot failure + non-zero exit
```

Do not stringify every structured error into one opaque message before the output layer.

Preserve error categories for JSON output.

Never convert a policy denial into “file not found” or another misleading filesystem error.

---

## 15. Policy and capability enforcement

The CLI must use exactly the same AWE-011 authorization path as MCP.

The command handler must not directly decide:

```text
allowed = true
```

or equivalent.

Authorization must resolve through canonical:

```text
agent
→ session
→ workspace
→ capability
→ policy
```

The route `awh fs ...` is not itself an authorization boundary.

A user invoking the CLI directly must still receive the same policy result as an equivalent MCP call under the same identity/session/workspace context.

If CLI execution has a local/default identity mode, verify that it cannot silently escalate privileges or bypass the configured policy.

Denied mutation must produce:

- no filesystem mutation
- no false snapshot event
- no false commit event
- appropriate audit event
- non-zero CLI exit

---

## 16. Snapshot, provenance, and audit integration

The CLI must inherit the canonical services rather than implementing local history.

A successful edit should produce the same underlying:

```text
EditId
SnapshotId
before state
after state
provenance
Audit events
```

as an equivalent MCP edit.

The CLI must not create a second `.agent` history directory or CLI-specific snapshot store.

`fs rollback` must use the same persistent snapshot/provenance substrate established by AWE-012 and the same edit-level rollback semantics established by AWE-010.

Audit events must flow through the canonical AWE-013 service.

Do not expose snapshot bytes in normal CLI audit/history output.

---

## 17. Conflict semantics

CLI conflict behavior must be identical to MCP conflict behavior.

Test at least:

1. read/prepare expected state
2. external modification
3. invoke CLI edit with stale state
4. canonical service rejects mutation
5. CLI returns non-zero
6. CLI reports edit/conflict information
7. file remains unchanged by the rejected operation

For rollback:

1. perform edit
2. verify commit
3. externally modify file
4. invoke `awh fs rollback <edit-id>`
5. canonical rollback detects mismatch
6. no rollback mutation occurs
7. CLI returns non-zero

Do not implement CLI-specific conflict heuristics.

---

## 18. No whole-file overwrite fallback

This is a hard security and correctness requirement.

Never implement a fallback such as:

```text
edit fails
→ read file
→ generate new complete file
→ overwrite target
```

for the purpose of making an edit command “work.”

If the canonical operation cannot be safely applied:

```text
reject
→ report structured failure
→ leave filesystem unchanged
```

This requirement applies especially to:

- stale state
- failed contextual replacement
- invalid line ranges
- diff context mismatch
- policy denial
- verification failure
- snapshot failure
- rollback conflict

---

## 19. Path and filesystem security

The CLI must rely on canonical filesystem security helpers.

Do not normalize paths in a way that bypasses service-level validation.

Preserve:

- workspace containment
- canonical path validation
- traversal protection
- symlink escape protection
- platform-appropriate path handling
- deterministic invalid-path errors

Do not let a CLI option bypass canonical path validation through:

- `..`
- absolute paths
- symlinked parents
- alternate path spellings
- platform-specific separators
- encoded path tricks

The service remains authoritative.

---

## 20. TOCTOU and concurrency

The CLI cannot eliminate filesystem races merely by parsing arguments carefully.

Preserve the canonical EditService TOCTOU protections and expected-state checks.

Do not introduce a CLI sequence such as:

```text
CLI reads file
CLI validates state
CLI waits
CLI writes file directly
```

when the canonical service can perform the operation safely as one transaction.

For CLI commands that need a read-derived expected state, make the race boundary explicit and rely on the canonical service to reject stale state.

Do not claim stronger concurrency guarantees than the underlying service provides.

---

## 21. Identity and non-interactive operation

The CLI must work correctly in automated/agent environments.

Do not require interactive prompts for security-critical mutation decisions unless the existing architecture explicitly requires them.

Where the CLI needs agent/session/workspace context, resolve it through the canonical context mechanism.

Support non-interactive execution without weakening authorization.

Do not add a hidden “force” flag that bypasses:

- policy
- capability
- expected-state checks
- snapshotting
- verification
- rollback safety

If a `--force` option exists elsewhere, ensure it cannot become a policy bypass.

---

## 22. Help and discoverability

The following must work:

```bash
awh fs --help
awh fs patch --help
awh fs replace --help
awh fs insert --help
awh fs delete-range --help
awh fs apply-diff --help
awh fs rollback --help
```

Help must accurately describe:

- required arguments
- optional expected-state/context inputs
- input/output modes
- relevant path semantics
- edit ID requirements for rollback
- exit/error behavior where appropriate

Do not advertise unsupported flags or functionality.

The command help must not claim stronger guarantees than the canonical service actually provides.

---

## 23. CLI implementation boundaries

Keep command registration and argument parsing separated from service logic.

Prefer a structure conceptually similar to:

```text
CLI command definition
        ↓
request/argument conversion
        ↓
application service invocation
        ↓
canonical result/error
        ↓
CLI renderer + exit code
```

Avoid putting substantial logic into `main.rs`.

If the current CLI framework supports dedicated command modules, use them.

Do not perform unrelated CLI refactoring in this issue.

Do not redesign the entire CLI framework merely to expose these commands.

---

## 24. MCP equivalence tests

For each operation, compare equivalent MCP and CLI execution paths.

At minimum compare:

- Replace
- Insert
- DeleteRange
- Patch
- ApplyDiff
- Rollback

Verify that equivalent requests produce equivalent service semantics for:

- authorization
- validation
- expected-state behavior
- conflict handling
- snapshot creation
- mutation
- verification
- rollback
- audit/provenance
- final status

The output formatting may differ because CLI and MCP are different transports.

The underlying behavior must not.

---

## 25. Real terminal tests

Tests must execute the actual CLI binary rather than only calling command functions directly.

Cover:

### Discovery

```text
awh fs --help
```

### Success

- replace
- insert
- delete-range
- patch
- apply-diff
- rollback

### Failure

- invalid arguments
- missing target
- invalid range
- replacement not found
- stale expected state
- policy denial
- capability denial
- verification failure
- rollback conflict
- rollback failure

### Output

Verify:

- human output
- JSON/machine output where supported
- edit ID visibility
- structured conflict information
- structured error category
- no secret leakage

### Exit codes

Assert actual process exit statuses.

### Filesystem invariants

After every rejected operation, assert that the target filesystem state is unchanged.

---

## 26. Edge-case coverage

Exercise at least:

- empty file
- zero-byte file
- one-line file
- multi-line file
- file with final newline
- file without final newline
- LF
- CRLF
- Unicode
- Devanagari
- emoji
- spaces in paths
- nested workspace paths
- path traversal attempts
- symlink escape attempts
- multiple files in one patch
- large but bounded input
- malformed diff
- multiple diff hunks
- rollback after a successful edit
- rollback after external modification

Do not duplicate the underlying algorithm merely to test it; tests should exercise the canonical service through the CLI boundary.

---

## 27. Security-sensitive output rules

CLI output must not leak:

- API keys
- bearer tokens
- passwords
- OAuth credentials
- private keys
- snapshot bytes
- full file contents in errors
- environment secrets

Audit redaction remains centralized in AWE-013.

CLI output must not attempt to replace the audit redaction layer with its own incomplete secret scanner.

For diagnostics that include paths, use the same path privacy conventions as the service/audit architecture.

---

## 28. Performance and automation safety

The CLI should remain suitable for agent invocation.

Avoid:

- unnecessary full-file reads in the CLI
- duplicate hashing outside the service
- loading huge diffs into multiple redundant buffers
- unbounded error output
- interactive prompts in automated paths

Enforce reasonable CLI input limits consistent with service-level resource limits.

Do not introduce a CLI-specific limit that contradicts the canonical service unless the CLI explicitly documents it as a transport limit.

---

## 29. Compatibility

Preserve existing working commands and CLI conventions.

Do not break:

- root command parsing
- existing `fs read/stat/search/hash` commands
- existing global options
- existing configuration loading
- workspace selection
- logging behavior
- completion generation

Only add the agent-grade editing surface required by AWE-014 and minimal supporting code required to integrate it correctly.

Do not modify unrelated command families.

---

## 30. Documentation discipline

Update CLI documentation only if necessary to reflect the actually implemented AWE-014 commands.

Documentation must not be used to mark a command complete before real implementation and tests exist.

If an existing canonical CLI reference intentionally describes the final target surface, preserve its distinction between target contract and implementation status.

Do not document unsupported behavior.

---

## 31. Testing hierarchy

Follow the repository validation sequence:

```text
compile
→ unit tests
→ integration tests
→ real terminal tests
→ failure/recovery tests
→ documentation check
```

At minimum run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Additionally run targeted CLI tests and real binary invocations for every AWE-014 command.

If a test requires a real workspace, use a temporary isolated workspace and clean it deterministically.

---

## 32. Definition of Done

AWE-014 is complete only when all are true:

- [ ] `awh fs replace` exists and is discoverable.
- [ ] `awh fs insert` exists and is discoverable.
- [ ] `awh fs delete-range` exists and is discoverable.
- [ ] `awh fs patch` exists and is discoverable.
- [ ] `awh fs apply-diff` exists and is discoverable.
- [ ] `awh fs rollback` exists and is discoverable.
- [ ] Every command uses the canonical EditService.
- [ ] No editing algorithm is duplicated in `main.rs` or CLI handlers.
- [ ] CLI and MCP share the same edit semantics.
- [ ] Edit IDs are authoritative and visible in mutation results.
- [ ] Human-readable output is the default.
- [ ] Stable machine-readable output is available where supported.
- [ ] Exit codes correctly distinguish failure from success.
- [ ] Policy/capability denial returns non-zero.
- [ ] Stale/conflicting edits return non-zero.
- [ ] Verification failure returns non-zero.
- [ ] Rollback failure/conflict returns non-zero.
- [ ] Rejected operations leave the filesystem unchanged.
- [ ] No whole-file overwrite fallback exists.
- [ ] Rollback uses canonical conflict-aware edit-level rollback.
- [ ] Snapshot/provenance integration is shared with MCP.
- [ ] Audit integration is shared with MCP.
- [ ] Path and symlink security remains centralized.
- [ ] Real terminal/integration tests exist.
- [ ] Help/discovery tests exist.
- [ ] Success/failure/recovery tests exist.
- [ ] MCP-equivalence tests exist where practical.
- [ ] Unicode/newline/EOF/path edge cases are covered.
- [ ] Full Rust CI gates pass.
- [ ] No unrelated CLI architecture was modified.

---

## 33. Explicit non-goals

Do **not** use AWE-014 to implement:

- a new editing engine
- a new filesystem service
- a new policy engine
- a new capability system
- a new identity model
- a new snapshot store
- a new provenance store
- a new audit store
- a second MCP editing implementation
- a generic workflow engine
- a generic CLI framework rewrite
- unrelated CLI command families
- AWE-015's complete editing test-suite redesign
- AWE-016 real MCP-client validation as a separate milestone
- AWE-017 complete acceptance workflow

AWE-014 is specifically the **CLI transport surface for the already-canonical agent-grade editing services**.

---

## 34. Required final implementation report

When implementation is complete, report:

1. exact files changed
2. CLI commands implemented
3. canonical service entry points used
4. argument/request mapping
5. human-readable output behavior
6. machine-readable output behavior
7. exit-code mapping
8. policy/capability integration
9. conflict semantics
10. snapshot/provenance integration
11. audit integration
12. rollback behavior
13. real terminal tests executed
14. MCP-equivalence tests executed
15. full Rust CI results
16. any known limitations

Do not claim a command is production-ready unless the actual binary invocation has been tested.

---

## HARD STOP

After AWE-014 is implemented and verified, **STOP**.

Do not begin AWE-015, the complete editing test-suite redesign, AWE-016 real MCP-client validation, AWE-017 acceptance workflow, or unrelated CLI work in the same implementation task.

Do not modify unrelated repository files merely because they could benefit from CLI editing.

The objective is a thin, secure, production-grade `awh fs` CLI surface backed entirely by the canonical AWH editing architecture.