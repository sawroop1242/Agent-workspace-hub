# Prompt 12 — CLI Agent-Grade Editing (AWE-014)

## Mission

Implement and harden the canonical AWH filesystem editing and rollback interface exposed through the awh CLI.

This prompt owns the CLI adapter, terminal contract, output contract, exit behavior, and real-binary validation for agent-grade editing. It must expose existing AWH application services without creating CLI-specific mutation semantics, a second editor, a second authorization engine, a second snapshot store, a second rollback engine, or a separate audit system.

Target architecture:

~~~text
real terminal
    |
    v
awh fs command
    |
    v
CLI argument validation / workspace resolution
    |
    v
existing authorization boundary
    |
    v
canonical EditService / EditTransaction
    |
    +--> canonical snapshot / provenance
    +--> canonical rollback / recovery
    +--> canonical persistent audit
    |
    v
workspace filesystem
    |
    v
post-edit verification
    |
    v
stable CLI result / exit status
~~~

The current rust branch is the source of truth. Historical Trust-Wedge and issue-resolving material is requirements evidence only. Current code, tests, and actual behavior win when historical material conflicts.

---

## 1. Product boundary

AWH is an agent-agnostic, local-first workspace runtime for coding agents.

AWH owns workspace/filesystem state, controlled editing, capabilities/policy, snapshots, provenance, rollback, audit, agent profiles/sessions, and MCP/CLI/TUI/Control API interfaces.

External agents own reasoning, planning, model selection, and agent-specific orchestration.

Prompt 12 owns only the CLI interface to editing and recovery.

The CLI must not own:

- edit semantics;
- expected-state matching;
- path-security semantics;
- transaction preparation;
- atomic filesystem commit;
- verification;
- snapshot persistence;
- rollback eligibility;
- rollback conflict detection;
- capability/policy decisions;
- durable audit persistence.

All of those remain canonical service responsibilities.

---

## 2. Scope

The editing/recovery command family is:

~~~text
awh fs replace
awh fs insert
awh fs delete-range
awh fs patch
awh fs apply-diff
awh fs verify
awh fs history
awh fs rollback
~~~

Inspect related filesystem commands because they establish existing CLI conventions:

~~~text
awh fs read
awh fs write
awh fs stat
awh fs search
awh fs hash
~~~

A command listed in docs/CLI.md is a target contract, not proof that its implementation exists. Never fake an unsupported command merely to satisfy documentation.

---

## 3. Required documentation forensics

Read all relevant current product and roadmap material before implementation:

~~~text
docs/roadmap/GROWTH_STRATEGY.md
docs/roadmap/PROJECT_ROADMAP.md
docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
docs/FEATURES.md
docs/PROJECT_CONTEXT.md
docs/architecture.md
docs/security.md
docs/threat-model.md
docs/CLI.md
docs/implementation-prompts/README.md
~~~

Read the adjacent implementation contracts:

~~~text
docs/implementation-prompts/01-init-runtime-contracts.md
docs/implementation-prompts/02-agent-runtime-identity.md
docs/implementation-prompts/03-mcp-routing-and-security.md
docs/implementation-prompts/04-edit-transaction-model.md
docs/implementation-prompts/05-edit-operation-engine.md
docs/implementation-prompts/06-edit-safety.md
docs/implementation-prompts/07-edit-authorization.md
docs/implementation-prompts/08-snapshots-provenance.md
docs/implementation-prompts/09-rollback-recovery.md
docs/implementation-prompts/10-persistent-audit.md
docs/implementation-prompts/11-mcp-editing-validation.md
docs/implementation-prompts/16-testing-and-acceptance.md
docs/implementation-prompts/17-contract-status-and-roadmap.md
~~~

Where present, inspect:

~~~text
docs/trust-wedge/
docs/issue-resolving-prompts/
~~~

Search historical material for AWE-014, CLI editing, fs replace, fs insert, delete-range, patch, apply-diff, history, rollback, terminal validation, output, exit codes, expected state, conflict, authorization, snapshots, audit, and workspace isolation.

Historical material must not override current implementation.

---

## 4. Required source forensics

Inspect the complete current implementations:

~~~text
src/main.rs
src/lib.rs
src/services/mod.rs
src/services/edit.rs
src/services/files.rs
src/services/authorization.rs
src/services/snapshot.rs
src/services/provenance.rs
src/services/audit.rs
src/core/errors.rs
~~~

Also inspect the current workspace, agent, session, capability, policy, configuration, and CLI modules actually used by main.rs.

Determine:

- CLI command declaration and parsing;
- global options;
- workspace-root resolution;
- configuration precedence;
- caller/agent/session context;
- authorization context;
- EditAction mapping;
- canonical edit service construction;
- actually executable edit operations;
- rollback/recovery API;
- snapshot/provenance/audit APIs;
- structured domain errors;
- existing output conventions;
- existing exit-code conventions;
- JSON/machine-readable output, if any;
- stdout/stderr conventions;
- signal handling;
- binary integration-test conventions.

Do not infer implementation from type declarations alone.

---

## 5. Current implementation inventory

Before changing code, produce an internal table with:

~~~text
command
arguments
canonical service
authorization action
workspace context
success result
failure mapping
exit behavior
existing tests
missing tests
~~~

For every command, classify it as:

~~~text
implemented and reusable
partially implemented
service exists but CLI is missing
unsupported
~~~

Only implemented/service-backed commands may receive a success path.

---

## 6. Canonical service rule

All CLI editing must converge on the existing application services.

Required conceptual mapping:

| CLI | Canonical domain operation |
|---|---|
| fs replace | canonical Replace operation |
| fs insert | canonical Insert operation |
| fs delete-range | canonical DeleteRange operation |
| fs patch | canonical multi-operation EditTransaction |
| fs apply-diff | canonical ApplyDiff operation |
| fs rollback | canonical rollback/recovery service |
| fs verify | canonical verification/state service |
| fs history | canonical provenance/audit/recovery read boundary |

Do not create:

~~~text
CliEditService
CliPatchEngine
CliRollbackService
CliSnapshotStore
CliAuditStore
~~~

or equivalent.

If existing CLI code mutates files directly, redirect it to the canonical service instead of adding another abstraction.

---

## 7. CLI argument contract

Every supported command must have explicit deterministic argument semantics.

Cover:

- required paths;
- old/new replacement values;
- occurrence selection;
- line numbers;
- start/end ranges;
- expected hash;
- expected size;
- expected line count;
- expected context;
- patch representation;
- unified-diff input;
- edit ID for rollback;
- workspace selection;
- output mode;
- verbosity;
- existing configuration/profile options.

Use existing CLI naming conventions where possible.

If stdin/file input already exists, preserve it. For large patch/diff input, use bounded file/stdin handling where the current architecture supports it.

Reject ambiguous simultaneous input sources unless the current CLI explicitly defines precedence.

Never put secrets in process arguments when an existing secure configuration mechanism exists.

---

## 8. Workspace and path resolution

All edit paths must pass through the canonical workspace/filesystem security boundary.

Test:

~~~text
relative paths
nested paths
absolute paths
..
../ traversal
dot segments
workspace root
symlink escape
different workspace
empty path
long paths
control characters
platform-specific separators
~~~

Do not implement a weaker CLI normalization step.

CLI current directory, --path, workspace names, or arbitrary user-supplied absolute paths must never become authorization substitutes.

The invariant is:

~~~text
CLI workspace selection
    -> canonical workspace resolution
    -> canonical EditService
    -> same workspace for snapshot/provenance/audit/rollback
~~~

---

## 9. Authorization ordering

For a consequential mutation:

~~~text
CLI syntax validation
    -> workspace/caller/session resolution
    -> capability/policy authorization
    -> canonical edit validation
    -> snapshot/recovery preparation
    -> canonical edit execution
    -> post-edit verification
    -> provenance/audit integration
    -> CLI result
~~~

Malformed CLI syntax may fail before authorization because no mutation is possible.

Once a valid mutation reaches application services, authorization must happen before mutation.

Do not implement capability/policy checks inside CLI command handlers. Use the existing EditAction and authorization boundary.

---

## 10. Default-deny behavior

Verify current authorization requirements for:

~~~text
replace
insert
delete-range
patch
apply-diff
rollback
~~~

Missing, malformed, expired, unavailable, or out-of-scope authorization must fail closed according to the current contract.

A denial must produce:

- no mutation;
- no partial edit;
- no rollback;
- no false success;
- non-zero exit;
- safe error information;
- audit behavior according to the canonical audit contract.

Do not add a CLI --force escape hatch that bypasses policy.

---

## 11. Edit transaction integration

Reuse the canonical edit vocabulary already defined by the repository, including where present:

~~~text
EditId
EditOperation
ExpectedState
FileState
EditStatus
EditError
EditRefs
EditIdentity
~~~

The CLI may translate arguments into these types.

It must not create a second transaction model or perform a lossy conversion through a CLI-specific representation.

Successful mutations must expose the canonical edit ID when the service provides one so history, provenance, audit, and rollback can correlate the operation.

---

## 12. fs replace

Implement/harden:

~~~text
awh fs replace <path> <old> <new>
~~~

against the canonical replace operation.

Verify:

- secure workspace-relative path;
- exact old/new text semantics;
- occurrence semantics;
- ambiguity/conflict handling;
- expected-state checks;
- no blind overwrite;
- exact Unicode behavior;
- newline preservation;
- canonical edit ID/status;
- non-zero failure.

Do not use ad-hoc String::replace in the CLI as the mutation engine.

---

## 13. fs insert

Implement/harden:

~~~text
awh fs insert <path> <line> <content>
~~~

against the canonical insertion operation.

Verify:

- line numbering matches the service;
- beginning/end boundaries;
- newline style;
- no accidental extra newline;
- no-final-newline behavior;
- expected-state behavior;
- Unicode preservation;
- canonical result reporting.

The CLI must never manipulate file bytes directly.

---

## 14. fs delete-range

Implement/harden:

~~~text
awh fs delete-range <path> <start-line> <end-line>
~~~

against the canonical delete-range operation.

Verify:

- unambiguous line numbering;
- start <= end;
- range validation;
- empty/single-line behavior;
- newline preservation;
- expected-state checks;
- canonical atomicity;
- non-zero failure.

---

## 15. fs patch

Implement/harden:

~~~text
awh fs patch ...
~~~

as a thin adapter to the canonical multi-operation transaction.

The CLI must not become the transaction engine.

Verify:

- operation parsing;
- ordering;
- operation count limits;
- malformed/ambiguous operations;
- path safety;
- expected-state validation;
- prepare-before-commit;
- canonical partial/atomic behavior;
- verification;
- edit ID propagation;
- structured errors.

---

## 16. fs apply-diff

Implement/harden:

~~~text
awh fs apply-diff ...
~~~

against the canonical unified-diff operation.

Verify:

- supported input sources;
- malformed diff rejection before mutation;
- workspace-safe diff paths;
- context mismatch conflicts;
- multi-file semantics;
- no blind whole-file fallback;
- no false partial success;
- exact-byte/newline semantics.

Do not implement a second diff parser in the CLI.

---

## 17. Expected-state options

Expose expected-state preconditions only through the canonical model.

Where the current CLI contract supports them, provide equivalent semantics for:

~~~text
expected hash
expected byte size
expected line count
expected context
~~~

Use existing option names if present; otherwise choose stable names and document them.

Required invariant:

~~~text
expected state
    -> canonical validation
    -> conflict on mismatch
    -> no stale overwrite
~~~

A conflict must produce non-zero exit and leave the filesystem unchanged.

Never print the complete conflicting file merely to explain a mismatch.

---

## 18. fs rollback

Implement/harden:

~~~text
awh fs rollback <edit-id>
~~~

against the canonical rollback/recovery service.

Verify:

- strict edit-ID validation;
- correct workspace binding;
- authorization;
- canonical snapshot/provenance lookup;
- produced-state conflict protection;
- exact-byte restoration;
- created-file safety;
- concurrent modification protection;
- post-rollback verification;
- repeated rollback behavior;
- audit/provenance correlation.

Do not manually copy snapshot files or delete files from the CLI as a rollback implementation.

---

## 19. fs history

First determine the authoritative current history contract.

If history is backed by canonical provenance/audit/recovery records, expose bounded metadata such as:

~~~text
edit ID
timestamp
workspace ID
agent/session correlation when available
operation
status
resource metadata
rollback availability/status
~~~

Never expose by default:

- complete file contents;
- secrets;
- bearer tokens;
- API keys;
- giant diffs;
- unnecessary absolute host paths.

If no authoritative history service exists for the requested view, return an explicit unsupported/not-yet-implemented result rather than inventing a database.

Do not create a CLI-local history store.

---

## 20. fs verify

Determine the actual current verification contract.

The command must use canonical verification/state primitives.

It must not mutate files merely to verify state.

Where applicable, distinguish:

~~~text
verified
conflict
missing
mismatch
unsupported
internal failure
~~~

Exit status must reflect the result.

---

## 21. Output contract

Follow existing CLI conventions, but enforce these invariants.

### Success

- exit 0;
- concise result;
- canonical edit/rollback identifier when available;
- no secrets.

### Input failure

- non-zero;
- actionable message;
- no panic/backtrace by default;
- no mutation.

### Authorization failure

- non-zero;
- stable denial classification;
- no mutation;
- no sensitive details.

### Conflict

- non-zero;
- stable conflict classification;
- retry-relevant metadata;
- no stale overwrite.

### Service/recovery failure

- non-zero;
- sanitized classification;
- actual filesystem outcome not fabricated.

Human-readable output and diagnostics must follow repository conventions.

---

## 22. Machine-readable output

Inspect whether current CLI already has JSON or another machine-readable output mode.

If it exists, all Prompt-12 commands must use it consistently.

If it does not exist, do not redesign the entire CLI solely for Prompt 12.

If a narrowly scoped machine-readable mode is already part of the target contract, define bounded stable fields such as:

~~~text
command
status
edit_id / rollback_id
workspace_id
resource metadata
verification state
conflict information
error code
~~~

Never serialize raw Rust errors, backtraces, complete files, secrets, or arbitrary request bodies.

---

## 23. Exit-code contract

Reuse existing repository exit codes if they exist.

Otherwise define only the minimum stable categories required by the CLI contract, such as:

~~~text
0 success
input/usage failure
authorization failure
conflict
verification/recovery failure
internal/service failure
~~~

Do not create a unique exit code for every internal error.

Do not silently change established exit behavior without a compatibility reason.

---

## 24. stdout and stderr

Use conventional CLI stream semantics:

- successful command data on stdout;
- diagnostics/errors on stderr;
- machine-readable stdout remains parseable;
- debug/tracing output must not pollute machine-readable stdout;
- secrets never appear on either stream.

Test redirection and pipeline behavior.

---

## 25. Error mapping

Map canonical errors to stable CLI categories.

At minimum distinguish:

| Condition | CLI behavior |
|---|---|
| malformed CLI syntax | usage/input failure |
| invalid path | path/input failure |
| workspace unavailable | workspace failure |
| identity/session failure | identity failure |
| capability/policy denial | authorization failure |
| stale expected state | conflict |
| malformed patch/diff | input/edit failure |
| snapshot preparation failure | recovery failure; no mutation |
| edit failure | service/edit failure |
| verification failure | verification/recovery failure |
| rollback conflict | rollback conflict |
| missing edit ID | rollback input failure |
| missing/corrupt snapshot | recovery failure |
| audit/provenance observer failure | follow canonical contract; never invent filesystem state |
| unexpected internal failure | sanitized internal failure |

Never expose Rust backtraces, internal storage paths, environment variables, credentials, or raw request payloads.

---

## 26. Configuration

Use the same configuration precedence as the existing CLI.

Inspect and preserve current:

~~~text
defaults
-> configuration
-> environment
-> explicit CLI options
~~~

Do not create a separate editing configuration loader.

Configuration may select behavior but cannot bypass authorization.

---

## 27. Agent/session context

If current CLI editing supports agent/session context, preserve it.

Do not create an MCP-style route identity for CLI commands.

Where full AgentSession is available, use it.

Where it is not yet available, use the strongest existing canonical context without fabricating an authorization identity from username, shell PID, current directory, or an arbitrary --agent string.

---

## 28. Audit integration

All consequential CLI operations must use the canonical persistent audit subsystem.

Correlate where available:

- edit ID;
- agent ID;
- session ID;
- workspace ID;
- operation;
- bounded resource metadata;
- authorization result;
- final status;
- rollback status.

Never record complete file contents, credentials, giant diffs, or sensitive argument arrays.

Prompt 10's audit subsystem is authoritative.

---

## 29. Snapshot and provenance integration

The CLI must never manually create snapshots.

Required conceptual chain:

~~~text
CLI request
    -> canonical edit service
    -> required snapshot capture
    -> mutation
    -> verification
    -> provenance
    -> audit
~~~

If snapshot preparation fails, follow the canonical service contract and do not bypass it with a CLI flag.

---

## 30. Rollback safety

Rollback must preserve produced-state safety:

~~~text
rollback
    -> authorization
    -> resolve canonical snapshot
    -> verify current state is transaction-produced state
    -> restore exact prior bytes
    -> verify
    -> provenance/audit
    -> CLI result
~~~

If another actor changed the file, do not overwrite that change blindly. Return the canonical conflict and non-zero exit.

---

## 31. Concurrency and TOCTOU

Test concurrent CLI invocations.

At minimum cover:

- two edits to one file;
- two edits to different files;
- edit while another process changes the target;
- rollback while another process changes the target;
- repeated rollback;
- two commands in the same workspace;
- commands against different workspace roots.

Rely on canonical service locking and atomicity.

Do not introduce a CLI-specific lock that conflicts with service locking.

---

## 32. Interactive and non-interactive behavior

Commands must work in:

- interactive terminals;
- CI;
- redirected stdin/stdout;
- shell pipelines.

Do not require interactive confirmation unless the current security contract requires it.

If confirmation is required, non-interactive execution must fail deterministically rather than hang.

Interactive confirmation is never a substitute for authorization.

---

## 33. Resource limits

Respect existing AWH limits.

Test:

- large replacement;
- large insertion;
- large patch;
- large diff;
- many operations;
- long paths;
- large expected context;
- repeated invocations.

Reject excessive input before mutation.

Do not introduce unbounded buffering where the current architecture provides bounded input.

CLI limits complement, not replace, service-level limits.

---

## 34. Interruption behavior

Inspect current signal handling.

On interruption:

- never print false success;
- never claim rollback unless it happened;
- preserve service atomicity;
- return non-zero when appropriate;
- keep stdout parseable;
- diagnostics go to stderr.

Never implement Ctrl-C rollback by manually reversing files.

---

## 35. Real terminal validation

AWE-014 requires evidence from the actual executable, not only direct Rust function tests.

Build the real binary and run supported commands against isolated temporary workspaces.

At minimum exercise:

~~~text
awh init
awh fs replace
awh fs insert
awh fs delete-range
awh fs patch
awh fs apply-diff
awh fs verify
awh fs history
awh fs rollback
~~~

Adapt arguments to the actual final CLI contract.

Only claim a command is implemented when its real service-backed executable path works.

For unsupported commands, test and report the explicit unsupported behavior.

Never test destructive editing against the repository itself.

---

## 36. Real terminal acceptance matrix

Record actual before/after filesystem bytes.

| Scenario | Expected evidence |
|---|---|
| replace | exact bytes |
| insert | exact line/newline behavior |
| delete-range | exact remaining bytes |
| patch | all operations correct |
| apply-diff | expected files changed |
| malformed arguments | non-zero, no mutation |
| invalid path | non-zero, no escape |
| policy denial | non-zero, no mutation |
| stale state | conflict, no overwrite |
| snapshot failure | mutation blocked |
| verification failure | canonical recovery behavior |
| rollback success | exact original bytes |
| rollback conflict | changed live state preserved |
| repeated rollback | canonical behavior |
| workspace mismatch | no cross-workspace mutation |
| concurrent edit | canonical isolation/conflict |
| oversized input | bounded rejection |
| machine output | parseable when supported |
| stdout/stderr | no diagnostic pollution |

---

## 37. Workspace isolation

Use at least two temporary workspaces.

Verify:

~~~text
workspace A edit != workspace B edit
~~~

Also verify:

- A paths cannot mutate B;
- A rollback IDs cannot restore B;
- A history view cannot expose B;
- authorization remains workspace-scoped;
- --path or equivalent cannot bypass the canonical boundary.

Inspect actual filesystem state.

---

## 38. Unicode and byte preservation

Test:

~~~text
ASCII
UTF-8
Devanagari
emoji
mixed Unicode
LF
CRLF
mixed newline files
no-final-newline
empty files
zero-byte files where supported
~~~

Inspect exact bytes rather than only rendered text.

The CLI must not introduce encoding conversion.

---

## 39. Shell quoting

Test command arguments containing:

~~~text
spaces
quotes
apostrophes
backslashes
dollar signs
Unicode
newlines where supported
JSON characters
diff markers
~~~

The CLI parser must receive exactly what the shell supplies.

For complex content, prefer existing file/stdin forms.

Do not implement a second shell parser.

---

## 40. CLI/MCP/service parity

The CLI and MCP interfaces must converge on the same application services:

~~~text
MCP ---CLI ----+--> canonical AWH services
TUI ---/
API ---/
~~~

Where practical, test equivalent replace, insert, delete, patch, diff, and rollback operations through both interfaces and compare domain outcomes.

Protocol/output envelopes may differ. Filesystem, authorization, conflict, recovery, and audit semantics must not.

---

## 41. Testing architecture

Use multiple boundaries.

### Unit tests

Cover:

- argument conversion;
- output formatting;
- exit classification;
- error mapping;
- expected-state parsing;
- workspace option parsing.

### Service integration tests

Use canonical edit/rollback services for domain behavior.

### Binary integration tests

Spawn the real awh binary and inspect:

- exit status;
- stdout;
- stderr;
- filesystem bytes;
- persistent state.

### Security tests

Prove:

- denial does not mutate;
- traversal fails;
- workspace isolation holds;
- secrets are absent;
- stale edits cannot overwrite.

Do not use output snapshots alone as proof of mutation correctness.

---

## 42. Failure injection

Where current services expose test seams, inject failures into:

~~~text
authorization
snapshot capture
edit preparation
atomic write
verification
provenance
audit
workspace resolution
~~~

For each case record separately:

~~~text
filesystem outcome
CLI outcome
audit outcome
provenance outcome
~~~

An observer failure must not cause the CLI to invent a filesystem result.

---

## 43. History/read security

If history is available, enforce bounded reads and workspace isolation.

Test:

- limits;
- pagination if supported;
- workspace filters;
- invalid IDs;
- unknown IDs;
- malformed records;
- corrupt persisted records;
- restart persistence;
- secret filtering;
- no full file-content disclosure;
- no arbitrary path access.

Use canonical audit/provenance/snapshot query boundaries.

Never create a CLI-local history database.

---

## 44. Documentation consistency

Use docs/CLI.md as the target contract.

Do not rewrite it wholesale.

If implementation changes make a statement materially false, update only the necessary implementation-facing documentation as part of the smallest justified change.

Do not claim:

~~~text
appears in help = implemented
~~~

Implementation requires:

~~~text
real service
-> authorization
-> mutation/recovery
-> verification
-> tests
-> real terminal evidence
~~~

Do not modify any other implementation prompt.

---

## 45. Duplicate-mechanism audit

Before finalizing, search CLI code for:

~~~text
std::fs::write
std::fs::remove_file
rename
copy
String::replace
diff parser
snapshot restore
rollback
EditTransaction
filesystem.replace
filesystem.patch
~~~

Classify each occurrence as:

- read-only behavior;
- thin adapter;
- canonical service;
- duplicate domain implementation.

Remove or redirect duplicate mutation logic introduced by this task.

Do not remove unrelated legitimate filesystem functionality.

---

## 46. Non-goals

Do not implement:

- MCP protocol or transport;
- TUI redesign;
- model routing;
- agent reasoning;
- agent spawning;
- orchestration;
- new policy engine;
- new capability engine;
- new snapshot store;
- new rollback engine;
- new audit store;
- Git worktrees;
- Git reset/revert;
- terminal execution;
- connectors;
- Control API redesign;
- shell completion redesign;
- generic CLI framework rewrite.

Only make adjacent changes when they are the smallest canonical fix required to expose editing safely.

---

## 47. Relationship to Prompt 03

Prompt 03 owns MCP routing/security.

Prompt 12 must not import MCP route semantics into CLI code.

CLI and MCP may share the same underlying authorization/application services, but CLI is not an MCP session and must not invent MCP identity behavior.

---

## 48. Relationship to Prompts 04–10

Consume the canonical contracts from:

~~~text
Prompt 04 — edit transaction model
Prompt 05 — edit operation engine
Prompt 06 — edit safety
Prompt 07 — edit authorization
Prompt 08 — snapshots/provenance
Prompt 09 — rollback/recovery
Prompt 10 — persistent audit
~~~

Do not reimplement any of them.

If an integration seam is missing, make the smallest canonical service fix required. Never solve a service gap with a CLI-only workaround.

---

## 49. Relationship to Prompt 11

Prompt 11 owns MCP editing/client validation.

Prompt 12 must converge on the same application services:

~~~text
MCP ---CLI ----+--> EditService / Recovery / Audit / Provenance
TUI ---/
API ---/
~~~

Do not duplicate MCP or CLI domain behavior.

---

## 50. Relationship to Prompt 16

Prompt 16 owns broad Trust-Wedge acceptance.

Prompt 12 must still contain focused executable CLI tests and real-terminal evidence required for AWE-014.

Do not defer essential CLI correctness to Prompt 16.

---

## 51. Independence rule

This prompt must be executable against current rust as a standalone implementation task.

Do not require:

- another prompt PR;
- historical branch;
- old commit;
- unavailable issue;
- external agent;
- MCP client.

If an adjacent contract is incomplete, integrate with the available canonical interface and report the limitation.

Current source wins over historical assumptions.

---

## 52. Required implementation sequence

Execute linearly.

### Step 1 — Forensics

Read all required documents and current source.

### Step 2 — Inventory

Determine exactly which editing/recovery commands are backed by real services.

### Step 3 — Command/service mapping

Map every supported command to one canonical service and one authorization action.

### Step 4 — CLI wiring

Implement thin adapters only.

### Step 5 — Security

Verify workspace, caller/session, capability, and policy enforcement.

### Step 6 — Output/error contract

Implement stable human and existing machine-readable output.

### Step 7 — Recovery/observability

Verify canonical snapshot, provenance, rollback, and audit integration.

### Step 8 — Binary tests

Run the actual awh executable against temporary workspaces.

### Step 9 — Adversarial tests

Run denial, traversal, stale-state, malformed input, resource, Unicode, concurrency, and rollback-conflict tests.

### Step 10 — Documentation consistency

Correct only statements made false by the implementation.

### Step 11 — Final verification

Run all repository checks and inspect the final diff for duplicate mechanisms.

---

## 53. Required parsing tests

For every supported command cover:

- missing arguments;
- extra arguments;
- wrong types;
- invalid line numbers;
- invalid ranges;
- empty values;
- malformed patch/diff;
- missing input file;
- unreadable input;
- unsupported output mode;
- conflicting input sources.

No parsing failure may mutate the workspace.

---

## 54. Required authorization tests

For every mutation cover:

- authorized;
- missing capability;
- policy denial;
- expired grant where supported;
- out-of-scope grant;
- wrong workspace;
- wrong agent/session where supported;
- unavailable authorization state.

Every denial must prove unchanged filesystem state.

---

## 55. Required edit correctness tests

Through the real CLI where practical:

- replacement;
- occurrence semantics;
- insertion;
- deletion;
- multi-operation patch;
- unified diff;
- expected hash;
- expected size;
- expected line count;
- expected context;
- stale state;
- Unicode;
- Devanagari;
- emoji;
- LF/CRLF;
- no-final-newline;
- empty/zero-byte files;
- multiple files.

---

## 56. Required rollback tests

Through the real CLI:

- successful rollback;
- unknown ID;
- malformed ID;
- unauthorized rollback;
- wrong workspace;
- produced-state mismatch;
- concurrent modification;
- missing snapshot;
- corrupt snapshot;
- repeated rollback;
- post-rollback verification.

Inspect actual filesystem bytes after every case.

---

## 57. Required output/exit tests

For every major outcome verify:

~~~text
exit code
stdout
stderr
filesystem state
audit/provenance state where applicable
~~~

At minimum:

- success;
- usage failure;
- authorization denial;
- conflict;
- service failure;
- verification failure;
- rollback conflict.

Prefer stable error codes/fields over brittle prose assertions.

---

## 58. Required persistence/restart tests

For persistent history/recovery:

1. execute an edit;
2. terminate the CLI process;
3. start a new CLI process;
4. query history or rollback;
5. verify persisted state remains available;
6. corrupt persisted state in a controlled test;
7. verify failure is fail-closed and does not fabricate history or recovery state.

---

## 59. Required real-terminal acceptance

Use isolated temporary directories.

Representative flow:

~~~bash
cargo build --release
awh init
awh fs replace ...
awh fs insert ...
awh fs delete-range ...
awh fs patch ...
awh fs apply-diff ...
awh fs verify ...
awh fs history ...
awh fs rollback <edit-id>
~~~

Adapt arguments to the actual implementation.

Do not test destructive commands against the repository itself.

If a command remains unsupported because its canonical service is absent, test its explicit unsupported behavior and report that limitation.

---

## 60. Security invariants

The completed implementation must satisfy:

1. CLI is never an authorization bypass.
2. CLI arguments cannot override canonical caller identity.
3. CLI workspace selection cannot escape the authorized workspace.
4. Every consequential edit passes the existing authorization boundary.
5. Editing semantics exist in exactly one canonical service.
6. Snapshot semantics exist in exactly one canonical store.
7. Rollback semantics exist in exactly one canonical service.
8. Audit persistence exists in exactly one canonical subsystem.
9. Invalid arguments cause no mutation.
10. Authorization denial causes no mutation.
11. Stale expected state cannot overwrite newer state.
12. Rollback cannot overwrite unrelated post-edit changes.
13. Secrets do not appear in output or audit metadata.
14. Full file contents are not emitted by default.
15. Machine-readable output is not polluted by diagnostics.
16. Exit status accurately reflects the domain outcome.
17. Real binary behavior matches tested service semantics.
18. Concurrent CLI invocations remain isolated.
19. Required persistent recovery/history state survives restart.
20. Unsupported commands are never falsely reported as implemented.

---

## 61. Performance discipline

Avoid:

- duplicate file reads solely for formatting;
- duplicate patch parsing;
- giant result serialization;
- unnecessary cloning;
- unbounded history loading;
- heavyweight service construction when avoidable.

Do not optimize by bypassing validation, authorization, snapshots, or verification.

---

## 62. Verification gates

Run:

~~~bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
~~~

Also run focused CLI tests, binary integration tests, filesystem/edit tests, authorization tests, snapshot/recovery tests, rollback tests, audit tests, and workspace/path security tests.

Execute the real-terminal acceptance matrix.

Record exact commands and outcomes.

If a required test cannot run because of an environment limitation, state the exact limitation and do not mark it passed.

---

## 63. Completion criteria

Prompt 12 is complete only when:

- supported editing commands use canonical services;
- parsing is deterministic;
- workspace resolution is safe;
- authorization is enforced;
- expected-state semantics are preserved;
- replace/insert/delete/patch/apply-diff use canonical behavior;
- rollback uses canonical recovery;
- history/verify use authoritative current services;
- snapshot/provenance/audit boundaries are preserved;
- output is stable and sanitized;
- exit status is meaningful;
- stdout/stderr separation is correct;
- machine-readable output is valid where supported;
- Unicode and exact-byte semantics are preserved;
- stale edits cannot overwrite newer state;
- rollback cannot overwrite unrelated post-edit changes;
- concurrent invocations are safe;
- persistence/restart behavior works where required;
- real binary tests pass;
- adversarial security tests pass;
- no duplicate editor/rollback/snapshot/audit/authorization subsystem exists;
- documentation does not falsely claim unsupported commands;
- verification gates pass.

---

## 64. Final implementation report

The executor must report:

1. source/document forensics;
2. current CLI architecture;
3. command inventory;
4. command-to-service mapping;
5. workspace/path resolution;
6. authorization integration;
7. edit transaction integration;
8. snapshot/provenance integration;
9. rollback integration;
10. audit integration;
11. output/error contract;
12. exit-code behavior;
13. machine-readable output behavior;
14. real-binary evidence;
15. security/adversarial tests;
16. concurrency/isolation tests;
17. persistence/restart tests;
18. resource/performance considerations;
19. exact verification commands and outcomes;
20. changed files;
21. environment limitations;
22. explicit confirmation that no duplicate domain subsystem was introduced.

Classify significant items as:

~~~text
implemented and tested
implemented but environment-limited
existing and reused
unsupported/not implemented
not in scope
~~~

Never claim a command is implemented solely because its parser or help entry exists.

---

## 65. Strict scope

Implementation work is limited to the smallest source/test/example changes required for AWE-014.

Do not modify any other implementation prompt.

Do not modify Prompt 11 or any prompt outside Prompt 12.

Do not perform unrelated CLI refactors or redesign the CLI framework.

If a canonical service defect blocks safe CLI exposure, make the smallest necessary canonical fix and document why it is required for AWE-014.

The final result must make the CLI a thin, safe, testable interface over AWH's existing agent-grade editing and recovery architecture.
