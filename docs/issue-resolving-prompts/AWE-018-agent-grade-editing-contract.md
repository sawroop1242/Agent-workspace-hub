# AWE-018 / #39 — Agent-Grade Editing Contract

> **Status:** Issue-resolving master prompt  
> **Branch:** `rust`  
> **Primary scope:** establish the authoritative, implementation-backed documentation contract for agent-grade filesystem editing after AWE-017 acceptance.  
> **Dependencies:** AWE-009 / #30, AWE-014 / #35, AWE-017 / #38. Reuse AWE-001 through AWE-016 where their implemented behavior is part of the contract.  
> **Hard boundary:** this issue is documentation-contract work. Do not invent, silently enable, or redesign editing behavior merely to make documentation examples pass.

---

## 1. Mission

Produce the **canonical Agent-Grade Editing contract** for AWH.

The documentation must tell an agent, developer, operator, reviewer, and future implementer exactly what the editing subsystem **actually guarantees**, how it is invoked, what inputs are accepted, what results and errors mean, how identity/policy/security participate, how recovery works, and where guarantees end.

The central rule is:

```text
source + tests + real acceptance evidence
                ↓
        documented contract
```

Never reverse this relationship:

```text
target documentation
        ↓
pretend implementation exists
```

The final documentation must distinguish three states wherever relevant:

1. **Implemented and acceptance-proven** — source and tests/evidence demonstrate the behavior.
2. **Implemented but not fully acceptance-proven** — source exists, but required acceptance evidence is incomplete.
3. **Planned/target architecture** — roadmap or design documentation describes intended behavior, but implementation evidence is absent.

A roadmap entry, CLI command tree, MCP tool name, type definition, or issue requirement is **not evidence that a feature ships**.

AWE-018 is complete only when the published contract is an honest projection of the implementation and AWE-017 acceptance evidence.

---

## 2. Forensic baseline — inspect before documenting

Before editing documentation, inspect the actual `rust` branch.

At minimum inspect:

```text
src/services/edit.rs
src/services/
src/mcp/
src/cli/
src/context/
docs/CLI.md
docs/PROJECT_STATUS.md
docs/PROJECT_ROADMAP.md
docs/mcp.md
docs/testing.md
docs/issue-resolving-prompts/AWE-009-mcp-agent-grade-editing.md
docs/issue-resolving-prompts/AWE-014-cli-agent-grade-editing.md
docs/issue-resolving-prompts/AWE-017-complete-editing-acceptance-workflow.md
```

Also locate the real implementations/evidence for:

- `EditTransaction`;
- `EditId`;
- `EditOperation`;
- `ExpectedState`;
- `FileState`;
- `EditStatus`;
- `EditError`;
- canonical `EditService` mutation behavior;
- replace/insert/delete-range/patch/apply-diff;
- post-edit verification;
- conflict detection;
- atomic apply and recovery;
- edit-level rollback;
- file snapshots;
- provenance;
- persistent audit;
- caller/agent identity;
- session identity;
- workspace identity;
- capabilities;
- policy enforcement;
- MCP tool registry and schemas;
- real MCP transport/client tests;
- CLI commands and actual `--help` output;
- MCP/CLI adapter tests;
- path canonicalization and symlink protections;
- AWE-017 acceptance workflow and its pass/fail evidence.

### Mandatory forensic questions

Answer these before writing the final contract:

1. Which edit operations are actually executable today?
2. What is the canonical service entry point?
3. Which interfaces invoke that service?
4. What exact MCP tool names are registered?
5. What exact argument schemas are exposed?
6. What exact result schemas are exposed?
7. Which CLI commands are actually implemented?
8. What are their real argument/option names?
9. What does `--help` actually show?
10. Which operations require an `ExpectedState`?
11. How are hash, size, line count, and context interpreted?
12. What constitutes a conflict?
13. What happens before any mutation on validation failure?
14. What atomicity guarantee exists for one file?
15. What atomicity guarantee exists across multiple files?
16. What failure boundaries remain inherently non-atomic?
17. How are snapshots created and identified?
18. How do file snapshots differ from context-engine snapshots?
19. What provenance is persisted?
20. What audit is persisted and what survives restart?
21. How does rollback identify the edit and snapshot?
22. What state must the target have before rollback is allowed?
23. Which capability/policy checks apply to editing and rollback?
24. How are caller, session, and workspace identities represented?
25. Which TOCTOU guarantees are real, and which are explicitly limitations?
26. Which path/symlink protections are enforced?
27. What errors are stable machine-readable contract versus implementation detail?
28. Which claims were proven by AWE-017?
29. Which claims remain unproven or unsupported?
30. Which existing documentation is stale or dangerously overclaims implementation?

Do not write from memory when source/tests can answer the question.

---

## 3. Documentation authority hierarchy

When sources disagree, resolve them in this order:

1. **Actual production implementation** on `rust`.
2. **Executable tests and real acceptance evidence**, especially AWE-017.
3. **Real CLI help / generated schema / MCP discovery output**.
4. **Current architecture documentation describing implemented behavior**.
5. **Roadmap and issue requirements**.
6. **Historical design notes and target command trees**.

A roadmap may describe the intended final system, but it must never override contrary implementation evidence.

If implementation and documentation disagree:

- determine the implementation-backed behavior;
- update documentation to match it;
- if the implementation itself is incorrect, do not silently change implementation under AWE-018 unless the requested correction is explicitly necessary for documentation acceptance and remains within scope;
- otherwise record the discrepancy and stop rather than inventing semantics.

---

## 4. One canonical editing contract

The contract must make the architecture explicit:

```text
MCP ─┐
CLI ─┤
TUI ─┼──> shared application services ──> EditService
API ─┘                                      │
                         ┌──────────────────┼─────────────────┐
                         ▼                  ▼                 ▼
                      Policy            Snapshot           Audit
                         │                  │                 │
                         └──────────────────┼─────────────────┘
                                            ▼
                                   secure filesystem
```

Document that:

- MCP must not have an independent editing engine;
- CLI must not have an independent editing engine;
- TUI/API must not create divergent edit semantics;
- policy/capability semantics are shared;
- snapshot/provenance semantics are shared;
- rollback semantics are shared;
- verification semantics are shared;
- transport changes must not silently change filesystem behavior.

If the actual implementation differs, document the actual state and identify the architectural gap rather than claiming the desired architecture is already shipped.

---

## 5. Contract state labels

Every public editing capability documented as available must have evidence.

Use a consistent status vocabulary such as:

```text
Implemented + acceptance-proven
Implemented + partially validated
Planned / target
Unavailable
```

Do not use ambiguous language such as:

- “supported” when only a type exists;
- “available” when a command is merely listed in a roadmap;
- “atomic” without specifying its boundary;
- “safe” without describing the actual security checks;
- “rollback” when only an inverse operation exists;
- “snapshot” when referring to context-engine state rather than file recovery state.

If a feature is not implemented, clearly mark it as such instead of deleting the roadmap requirement merely to make documentation look complete.

---

## 6. Edit transaction model

Document the canonical transaction vocabulary using the actual source contract.

At minimum explain:

```text
EditId
EditTransaction
EditOperation
ExpectedState
FileState
EditStatus
EditError
```

Document the supported operation variants **only when actually implemented**:

```text
Replace
Insert
DeleteRange
Patch
ApplyDiff
```

For each operation document:

- purpose;
- required inputs;
- path semantics;
- content semantics;
- line numbering semantics where applicable;
- expected-state requirements;
- validation behavior;
- success result;
- conflict behavior;
- verification behavior;
- rollback relationship;
- known limitations.

### Line semantics

Where implemented, document exact semantics rather than vague phrases such as “insert at line”: 

- whether line numbers are one-based;
- whether insertion occurs before or after the specified line;
- what line `0` means if supported;
- whether delete ranges are inclusive;
- behavior at beginning/end of file;
- empty-file behavior;
- behavior with and without a final newline.

Do not invent semantics from the roadmap if the implementation differs.

---

## 7. Expected-state and conflict contract

Document expected-state validation as a safety mechanism, not as an optional informational field if the implementation treats it as authoritative.

Explain the actual fields and their meaning, such as:

```text
hash
context
size
line_count
```

Precisely state:

- which fields are required;
- which are optional;
- whether all supplied fields must match;
- how context is interpreted;
- what happens when state is stale;
- whether validation occurs before snapshot/mutation;
- whether a conflict guarantees zero mutation;
- what machine-readable conflict information is returned.

Use an explicit example:

```text
read file
  ↓
record expected state
  ↓
another process edits file
  ↓
submit stale edit
  ↓
Conflict
  ↓
original external bytes remain unchanged
```

Do not document “hash matching” as the entire conflict model if context/metadata are also authoritative.

If there is an unavoidable TOCTOU window, document the exact boundary rather than claiming perfect concurrency safety.

---

## 8. Atomicity contract and limits

This section must be unusually precise.

Never write simply:

> “Edits are atomic.”

Instead specify the actual guarantee boundary:

- preparation/validation atomicity;
- single-file write atomicity;
- multi-file transaction atomicity;
- verification-aware recovery;
- rollback behavior;
- crash/interruption guarantees;
- what happens if the process terminates at each mutation boundary;
- what guarantees depend on the underlying filesystem;
- which platform-specific limitations exist.

Document the difference between:

```text
no mutation on validation failure
```

and:

```text
full crash-proof multi-file commit
```

They are not equivalent.

If AWE-006 provides recovery rather than a true filesystem transaction, say so explicitly.

If a guarantee cannot be made across an external filesystem boundary, state that limitation.

---

## 9. Snapshot contract

Document **file snapshots** as the recovery substrate only if the implementation exists and AWE-017 proves the required behavior.

Explicitly distinguish:

### File snapshot

Used to preserve exact pre-edit file state for edit recovery/rollback.

### Context-engine snapshot

Used for context/session/application state and **not automatically a file recovery snapshot**.

Do not use “snapshot” generically when the distinction affects safety.

Document:

- snapshot identity;
- association with `edit_id`;
- exact pre-edit bytes or canonical secure reference;
- persistence location if public and stable;
- corruption behavior;
- lifecycle/retention semantics if implemented;
- access controls;
- whether normal audit records contain file contents.

Never claim that context snapshots can restore files unless source and tests explicitly prove that behavior.

---

## 10. Provenance contract

Document the canonical provenance record and its correlation model.

Where implemented, explain fields such as:

```text
edit_id
agent_id / caller identity
session_id
workspace_id
path
operation
timestamp
before_hash
after_hash
snapshot_id
result/status
```

Explain that provenance should permit reconstruction of **what happened** without unnecessarily storing raw file contents in ordinary metadata.

Document:

- persistence guarantees;
- restart behavior;
- queryability by edit ID;
- relationship to snapshots;
- relationship to audit;
- sensitive-data boundaries.

Do not create a second provenance vocabulary merely because an adapter wants a simpler response.

---

## 11. Rollback contract

Document rollback as a distinct operation with explicit preconditions.

Where implemented, state that rollback is associated with the original `edit_id` and uses the canonical pre-edit snapshot rather than reconstructing an inverse edit from assumptions.

Document the actual sequence:

```text
rollback(edit_id)
      ↓
locate edit/provenance/snapshot
      ↓
check current state against verified post-edit state
      ↓
reject if stale/conflicting
      ↓
restore exact original bytes
      ↓
verify actual filesystem
      ↓
record result
```

Document:

- repeated rollback behavior;
- rollback conflict behavior;
- authorization requirements;
- missing/corrupt snapshot behavior;
- newly created/deleted file behavior if supported;
- multi-file rollback semantics;
- exact-byte restoration guarantee;
- post-rollback verification.

Never claim “undo” is safe merely because an inverse textual operation can be constructed.

---

## 12. Capability and policy contract

Document the authorization chain precisely:

```text
caller
  ↓
session
  ↓
workspace
  ↓
capability
  ↓
policy
  ↓
tool/command
  ↓
EditService
  ↓
filesystem
```

Clarify that an MCP route or CLI command is **not itself authorization**.

Document:

- capability required for each mutation class;
- path/resource scope;
- session/workspace restrictions;
- expiry semantics if implemented;
- policy denial behavior;
- zero-mutation requirement on denial;
- rollback authorization;
- whether internal service calls are subject to the same policy boundary.

Never document “authenticated” as equivalent to “authorized to mutate files.”

---

## 13. Caller/session/workspace identity

Define the identity model using actual implementation terminology.

Document the relationship:

```text
Agent / caller
      ↓
Session
      ↓
Workspace
      ↓
Edit
```

Explain how identities appear in:

- edit results;
- provenance;
- audit;
- policy decisions;
- MCP requests where exposed;
- CLI invocation where exposed.

Do not invent a second identity model for documentation or adapters.

If a field is internal-only, do not present it as a stable public API unless source guarantees it.

---

## 14. MCP contract

Document MCP from the **real registered implementation**, not from the desired roadmap.

For every shipped editing tool document:

- exact tool name;
- description;
- required parameters;
- optional parameters;
- JSON types;
- enum values where applicable;
- path semantics;
- expected-state schema;
- transaction semantics;
- result schema;
- error behavior;
- authorization requirements;
- correlation identifiers;
- rollback semantics if exposed.

Examples must correspond to actual `tools/list` and `tools/call` behavior.

### MCP discovery rule

Where practical, generate or validate documentation against real discovery output:

```text
initialize
→ tools/list
→ inspect schema
→ tools/call
```

Do not hand-document a parameter name that is not present in the actual schema.

### MCP errors

Distinguish:

```text
protocol/transport error
schema/invalid-params error
tool-not-found error
policy/authorization error
validation error
conflict error
apply error
verification error
rollback error
internal service error
```

Use the implementation's actual machine-readable structure and stable fields.

Do not promise that human-readable error strings are stable unless explicitly guaranteed.

---

## 15. CLI contract

The existing `docs/CLI.md` is explicitly a **target CLI contract**, not proof that every listed command is implemented. fileciteturn182file0

AWE-018 must therefore distinguish:

```text
canonical target command tree
        ≠
currently shipped CLI
```

For every agent-grade edit command documented as shipped, verify actual source and real `--help` output.

Potential editing commands include:

```text
awh fs replace
awh fs insert
awh fs delete-range
awh fs patch
awh fs apply-diff
awh fs rollback
```

Only document a command as available if implementation evidence supports it.

For each shipped command document:

- exact invocation;
- required arguments;
- optional flags;
- expected-state input;
- output format;
- exit codes;
- errors;
- policy requirements;
- machine-readable mode if actually implemented;
- rollback behavior.

If the final CLI supports a stable JSON output mode, document the real schema. Do not invent one because agents would benefit from it.

---

## 16. MCP ↔ CLI contract equivalence

Where both interfaces expose the same operation, documentation must demonstrate that their **domain semantics** agree.

For example:

```text
MCP filesystem.replace
        │
        └──> EditService::replace

CLI awh fs replace
        │
        └──> EditService::replace
```

The documentation must not imply that equivalent operations have different conflict, authorization, snapshot, verification, or rollback rules unless that difference is intentional and implemented.

Use AWE-015/AWE-016/AWE-017 evidence where available.

---

## 17. Minimal-patch recovery examples

All examples must teach agents to make the smallest safe mutation.

Prefer:

```text
replace one exact occurrence
```

or:

```text
insert a small line range
```

or:

```text
apply a bounded patch/diff
```

Avoid examples that instruct agents to:

```text
read entire file
→ regenerate entire file
→ overwrite entire file
```

unless the actual operation is explicitly a full replacement and the documentation clearly identifies the risk.

Every recovery example should demonstrate:

```text
read
→ expected state
→ minimal edit
→ verify
→ inspect result
→ rollback if required
```

Include at least one example of stale-state recovery:

```text
conflict detected
→ re-read file
→ recompute expected state
→ prepare a fresh minimal patch
→ retry
```

Do not recommend blindly retrying the same stale transaction.

---

## 18. Error taxonomy

Create one authoritative error taxonomy based on actual implementation.

At minimum consider:

```text
InvalidRequest
InvalidPath
NotFound
ExpectedStateMismatch / Conflict
PolicyDenied
CapabilityDenied
SnapshotFailure
ApplyFailure
VerificationFailure
RollbackConflict
RollbackFailure
AuditFailure
ProvenanceFailure
TransportFailure
ToolNotFound
InvalidToolArguments
InternalError
```

Do not add names that do not exist in the actual public contract merely for completeness.

For every documented error explain:

- when it occurs;
- whether mutation has occurred;
- whether the caller should retry;
- whether the caller must re-read state first;
- whether rollback is appropriate;
- what machine-readable information is stable;
- what information must not be exposed.

### Retry guidance

Clearly separate:

```text
safe retry after re-read
```

from:

```text
unsafe blind retry
```

Examples:

- stale-state conflict → re-read and prepare a new transaction;
- policy denial → do not retry without authorization change;
- malformed parameters → correct request;
- verification failure → do not blindly repeat mutation;
- transport timeout → inspect operation state before retrying if mutation may already have occurred.

---

## 19. Security contract and exact limits

Documentation must state security guarantees precisely.

Cover:

- workspace containment;
- canonical path validation;
- absolute path handling;
- `..` traversal;
- symlink escape prevention;
- authorization before mutation;
- secret-safe errors/logs;
- audit/provenance content boundaries;
- snapshot access controls;
- resource limits;
- malformed input handling;
- TOCTOU limitations;
- concurrency semantics;
- platform-specific filesystem behavior.

Never claim:

- “symlink-proof” without defining the exact enforcement boundary;
- “race-free” unless proven;
- “transactional filesystem semantics” when only application-level recovery exists;
- “sandboxed” when the workspace guard is not a sandbox;
- “zero data exposure” if metadata can still reveal paths/hashes.

Security documentation must describe what the system prevents, not merely what it intends to prevent.

---

## 20. TOCTOU and concurrency documentation

Explicitly document the distinction between:

```text
state checked
```

and:

```text
state guaranteed unchanged until commit
```

If the implementation uses expected-state validation plus atomic writes but cannot eliminate every external concurrent modification race, state that limitation.

Document:

- stale-read detection;
- concurrent edit behavior;
- lock scope if locks exist;
- transaction serialization if implemented;
- external-process races;
- post-write verification;
- rollback conflict checks.

Do not describe optimistic conflict detection as a universal concurrency lock.

---

## 21. AWE-017 acceptance evidence

AWE-017 is the final behavioral evidence source for this contract.

Link the relevant acceptance results and summarize what was actually proven.

The documentation must distinguish:

```text
AWE-017 passed
```

from:

```text
AWE-017 planned
```

Do not state that an acceptance criterion passed unless the implementation and test evidence exists.

Where AWE-017 identifies a limitation, preserve that limitation in the public contract.

Do not “document around” a failing acceptance test.

---

## 22. Documentation generation and drift prevention

Where practical, make public documentation derive from implementation artifacts rather than manually duplicating schemas.

Preferred sources include:

- Rust type definitions;
- serde schemas;
- MCP `tools/list` output;
- CLI `--help` output;
- generated command metadata;
- executable integration fixtures.

If automatic generation is not currently available, create tests/checks that detect meaningful drift where feasible.

At minimum, validate manually or automatically that:

```text
documented MCP tool name == registered MCP tool name
```

```text
documented parameter == actual schema parameter
```

```text
documented CLI command == actual command/help output
```

```text
documented error field == actual machine-readable field
```

Do not introduce a documentation-generation framework as unrelated scope unless the existing architecture already supports it.

---

## 23. Documentation structure

Produce a clear user-facing contract, preferably organized into:

1. Overview
2. Implementation status
3. Architecture
4. Edit transaction model
5. Operations
6. Expected state and conflicts
7. Atomicity and limitations
8. Snapshots
9. Provenance
10. Rollback
11. Identity
12. Capabilities and policy
13. MCP API
14. CLI API
15. MCP/CLI equivalence
16. Error taxonomy
17. Security model
18. Concurrency/TOCTOU
19. Minimal-patch examples
20. Recovery guidance
21. Acceptance evidence
22. Compatibility/platform notes
23. Troubleshooting

Use tables for stable contracts where that improves scanability.

Do not duplicate conflicting descriptions across multiple documents.

If an existing canonical document already owns a section, update that document only when necessary and only if the issue scope permits it. Do not spread contradictory copies of the same contract across the repository.

---

## 24. Example quality requirements

Every executable example must be checked against the real implementation.

Examples must:

- use real command/tool names;
- use real argument names;
- use valid JSON where JSON is shown;
- use valid paths;
- use valid expected-state shapes;
- show realistic result/error structures;
- avoid secrets;
- avoid pretending that target-only features are available.

For MCP examples, validate the conceptual sequence:

```text
initialize
→ tools/list
→ choose tool
→ tools/call
→ inspect structured result
```

For CLI examples, validate against real help output.

For recovery examples, use minimal edits and fresh state after conflicts.

---

## 25. Contract tests / documentation tests

Where the repository already has documentation or schema test infrastructure, extend it to validate the editing contract.

At minimum consider tests that detect:

- stale MCP tool names;
- missing required parameters;
- CLI command drift;
- incorrect operation names;
- incorrect error identifiers;
- examples that cannot be parsed;
- claims of implemented behavior unsupported by the current build.

Tests must remain behavior-focused.

Do not create tests whose only purpose is to increase documentation test count.

If a documentation claim cannot be automatically validated, anchor it to a specific source/test/evidence reference.

---

## 26. Backward compatibility

Document compatibility deliberately.

For any existing editing API or command that changed during AWE work, identify:

- old behavior;
- new behavior;
- compatibility status;
- migration guidance;
- whether a breaking change is intentional.

Do not silently document a new contract that contradicts clients still supported by the implementation.

If no compatibility guarantee exists, say so explicitly.

---

## 27. Platform-specific behavior

Document only verified platform differences.

Relevant areas may include:

- path separators;
- symlink support;
- atomic rename semantics;
- file locking;
- newline preservation;
- UTF-8 behavior;
- filesystem permissions;
- Android/Termux behavior if supported by the project.

Do not claim universal filesystem semantics from a Linux-only test.

Mark platform-specific acceptance results clearly.

---

## 28. Sensitive information boundary

Documentation examples must never contain:

- real secrets;
- access tokens;
- API keys;
- private filesystem contents;
- personal credentials;
- production workspace paths that expose sensitive information.

Use deterministic synthetic fixtures.

Do not publish snapshot bytes or raw audit contents merely to demonstrate correlation unless those bytes are intentionally public test fixtures.

---

## 29. Scope discipline

AWE-018 is a contract/documentation issue.

Do not use it to:

- redesign EditService;
- create a new editing engine;
- create a second snapshot implementation;
- create a second audit store;
- replace the MCP transport;
- redesign CLI architecture;
- add unrelated agent features;
- change policy semantics solely to simplify documentation;
- claim unsupported functionality;
- rewrite the roadmap to hide incomplete implementation.

If documentation reveals a real implementation defect, record it accurately and route it to the appropriate implementation issue unless fixing it is explicitly part of AWE-018 acceptance.

---

## 30. Required verification workflow

Before declaring AWE-018 complete:

### Source verification

- inspect all referenced implementation paths;
- verify every public command/tool name;
- verify schemas and field names;
- verify error taxonomy;
- verify lifecycle semantics;
- verify security claims.

### Runtime verification

Where the environment supports it:

```text
MCP initialize
→ tools/list
→ representative editing tools/call
```

and:

```text
awh <real command> --help
```

Use AWE-016/AWE-017 evidence rather than inventing additional runtime claims.

### Documentation verification

Search for contradictory phrases such as:

```text
currently implemented
fully supported
atomic
sandboxed
secure
rollback supported
snapshot supported
```

and verify each claim against evidence.

Search for roadmap-only command names and ensure they are clearly marked target/planned where not implemented.

Search for “snapshot” and ensure context-engine snapshots are not conflated with file snapshots.

### Full Rust verification

Run the repository's canonical checks:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

If documentation-specific checks exist, run them as well.

Do not weaken code quality gates merely because this issue is documentation-focused.

---

## 31. Definition of Done

AWE-018 is complete only when all applicable criteria below are satisfied:

- [ ] The canonical agent-grade editing contract is documented.
- [ ] Documentation is based on actual `rust` implementation evidence.
- [ ] AWE-017 acceptance results are linked or explicitly reported as unavailable/incomplete.
- [ ] Implemented, partially validated, planned, and unavailable states are clearly distinguished.
- [ ] EditTransaction and operation semantics are documented accurately.
- [ ] Expected-state and conflict semantics are precise.
- [ ] Atomicity guarantees and limitations are explicit.
- [ ] File snapshots are clearly distinguished from context-engine snapshots.
- [ ] Provenance and rollback semantics are documented accurately.
- [ ] Caller/session/workspace identity is documented.
- [ ] Capability/policy requirements are documented.
- [ ] MCP tool names and schemas match actual implementation evidence.
- [ ] CLI commands and examples match actual implementation/help output.
- [ ] MCP and CLI semantics do not contradict one another.
- [ ] Error taxonomy is machine-readable where the implementation exposes stable fields.
- [ ] Security claims include exact limits and TOCTOU boundaries.
- [ ] Recovery examples use minimal precise patches rather than unsafe whole-file rewrites.
- [ ] No documentation example contains secrets or sensitive data.
- [ ] No roadmap-only feature is presented as shipped.
- [ ] No second identity, snapshot, provenance, audit, or editing model is introduced.
- [ ] Documentation does not conceal known implementation gaps.
- [ ] Repository formatting/build/test/clippy checks pass.
- [ ] Only files necessary for the documentation contract are modified.

---

## 32. Explicit non-goals

This issue does **not** authorize:

- implementing missing editing functionality merely to satisfy prose;
- creating a new edit executor;
- redesigning `EditTransaction`;
- changing MCP schemas without an implementation requirement;
- changing CLI behavior without an implementation requirement;
- changing policy/capability semantics;
- changing snapshot storage semantics;
- changing audit persistence semantics;
- replacing context-engine snapshots;
- adding a generic documentation platform;
- claiming completion based on roadmap intent;
- suppressing or deleting evidence of incomplete functionality.

---

## 33. Final implementation/reporting requirements

At completion, report:

```text
AWE-018 implementation status

Documentation files changed:
- <exact path(s)>

Evidence reviewed:
- source paths
- tests
- MCP discovery/client evidence
- CLI help evidence
- AWE-017 acceptance evidence

Implemented contract:
- <summary>

Known limitations:
- <summary>

Planned/unimplemented items explicitly marked:
- <summary>

Validation:
- cargo fmt --all -- --check
- cargo check --all-targets
- cargo test --all-targets
- cargo clippy --all-targets --all-features -- -D warnings
```

Do not report a green status merely because Markdown renders successfully.

The meaningful success condition is:

```text
implementation evidence
        +
acceptance evidence
        +
accurate documentation
        =
trusted agent-grade editing contract
```

---

## 34. HARD STOP

When AWE-018 is complete:

1. Do not begin AWE-019 or any later issue.
2. Do not modify unrelated repository files.
3. Do not expand the editing architecture beyond the documented contract.
4. Do not silently convert planned features into claimed implementation.
5. Do not weaken security/atomicity language to make acceptance easier.
6. Report exactly what was documented, what evidence supports it, and what remains unimplemented.

**STOP after AWE-018.**