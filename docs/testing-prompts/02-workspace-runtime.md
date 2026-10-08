# Master Test Prompt 02 — Workspace Runtime

## Mission

Create and execute a complete, human-behavior-oriented verification suite for the AWH Workspace Runtime feature family on the current `rust` branch.

This prompt owns **Workspace Runtime testing only**. Do not implement missing production features merely to make tests pass. Test the behavior that the current repository actually implements, and explicitly classify planned or unavailable workspace commands as unproven rather than pretending they exist.

The primary acceptance boundary is:

```text
human
→ real awh CLI / real service boundary
→ isolated workspace directory
→ durable workspace identity/state
→ observable result
```

Do not treat `cargo test` alone as sufficient evidence.

---

## 1. Product scope

The Workspace Runtime feature contract is:

```text
awh workspace create|list|open|info|remove
```

A workspace is the primary scope for:

- filesystem;
- Git;
- agents;
- sessions;
- context;
- memory;
- skills;
- snapshots;
- policy.

For this prompt, test the **workspace lifecycle and identity boundary itself**:

- workspace creation;
- workspace discovery/listing;
- workspace opening/resolution;
- workspace information;
- workspace removal;
- workspace root validation;
- durable workspace manifest/state;
- workspace identity;
- canonical workspace-root binding;
- idempotent initialization where it is part of the current workspace contract;
- restart/process-boundary behavior;
- isolation between separate workspaces;
- malformed/foreign/corrupt workspace state;
- path handling specific to workspace lifecycle;
- concurrent lifecycle operations where the implementation makes a guarantee.

Do not duplicate the full test suites for filesystem editing, Git/worktrees, agents, sessions, policy, snapshots, context, memory, skills, MCP, audit, terminal, collaboration, API, TUI, or connectors.

Those feature families get independent master test prompts.

---

## 2. Current implementation versus final target

Before testing, determine which Workspace Runtime commands and service APIs actually exist on the current `rust` branch.

The final feature document lists:

```text
awh workspace create
awh workspace list
awh workspace open
awh workspace info
awh workspace remove
```

The current branch may implement only part of this target.

In particular, inspect the existing workspace initialization/runtime implementation before writing tests. The current branch documentation may describe `awh init [--path DIR]`, a durable `.agent/workspace.json` manifest, `WorkspaceId`, and workspace-root validation. Treat those as current behavior only after verifying the source and tests on the branch being tested.

Never write a test that assumes a planned command exists.

For every target command, classify evidence as:

- **Implemented and tested**
- **Implemented but insufficiently tested**
- **Not implemented**
- **Blocked by environment**
- **Unproven**

---

## 3. Required repository forensics

Before changing tests, inspect the current `rust` branch enough to establish the real Workspace Runtime contract.

At minimum read:

- `README.md`
- `AGENTS.md`
- `Cargo.toml`
- `docs/FEATURES.md`
- `docs/testing.md`
- `docs/CLI.md`
- `docs/configuration.md`
- `docs/architecture.md`
- `docs/security.md`
- `docs/roadmap/PROJECT_ROADMAP.md`
- `docs/roadmap/GROWTH_STRATEGY.md`
- `docs/implementation-prompts/README.md`
- the current workspace/runtime implementation documentation;
- all existing implementation prompts that mention workspace identity or `awh init`;
- existing workspace-related tests.

Inspect the actual source for:

- workspace command definitions;
- workspace service/runtime modules;
- manifest/state structures;
- `WorkspaceId` and related identity types;
- path resolution/canonicalization;
- persistent storage;
- locking;
- CLI-to-service wiring;
- error types;
- serialization/deserialization;
- existing test helpers.

Search the repository for:

```text
workspace
WorkspaceId
workspace_root
workspace.json
initialize_workspace
load_workspace_manifest
awh workspace
```

Current source and current tests take precedence over stale/archive documentation.

---

## 4. Human-first testing model

Every public workspace lifecycle feature must be tested from the perspective of a real user.

A strong test should answer:

1. Can a user create or initialize a workspace in a fresh directory?
2. Can they discover it afterward?
3. Can a fresh process open it?
4. Can they inspect its identity and root?
5. Does the workspace remain the same after restart?
6. Can two workspaces coexist without sharing identity or state?
7. Does removing a workspace affect only the intended workspace?
8. What happens if the target path is a file?
9. What happens if the workspace manifest is malformed?
10. What happens if a manifest belongs to another root?
11. What happens when two lifecycle operations race?
12. Does AWH fail closed instead of silently resetting or adopting foreign state?

Prefer real subprocess tests for CLI behavior.

Use direct service/unit tests for deterministic internals, but never use them as the only proof of a user-facing workflow.

---

## 5. Test isolation

Every test must use isolated temporary directories.

Never use:

- the developer's real project;
- the real `HOME`;
- a shared persistent AWH directory;
- a user's actual repository;
- real credentials;
- global Git state.

For every workspace fixture:

- create a unique temporary root;
- make the root exist before spawning `awh`;
- use explicit process environment;
- avoid process-global environment mutation in parallel tests;
- clean up automatically.

If a test intentionally checks persistence, preserve the temporary directory for the duration of the scenario and start a fresh process rather than relying on an in-process singleton.

---

## 6. Workspace identity tests

If the current implementation uses a durable workspace manifest, test its identity contract.

For a newly initialized workspace, verify:

- a workspace identifier exists;
- the identifier has the documented representation/prefix;
- the workspace root recorded in state corresponds to the canonical workspace root;
- the manifest version is supported;
- required metadata is present;
- state is durable.

Do not assert a particular random ID.

Instead assert properties such as:

```text
workspace_id exists
workspace_id is valid
workspace_id is stable for the same workspace
different workspaces do not silently share an ID
```

If IDs are typed, test both valid serialization and invalid identifier input at the appropriate boundary.

Do not create duplicate identity models in tests when the production type already exists.

---

## 7. Create / initialize behavior

Test the real supported creation/initialization entry point.

### Fresh directory

Use:

```text
mkdir project
awh init
```

or the current workspace-create command if implemented.

Verify:

- successful exit;
- expected workspace state is created;
- manifest/state is readable;
- workspace identity is observable where documented;
- the workspace root is correct;
- unrelated files are preserved.

### Existing unrelated files

Create:

```text
project/
  README.md
  src/
```

then initialize.

Verify the workspace operation does not delete or rewrite unrelated files.

### Repeated initialization

Run the operation twice.

Verify:

- it is idempotent if the contract says so;
- the workspace ID remains unchanged;
- manifest bytes remain unchanged when the contract guarantees byte preservation;
- existing agents/policy/state are not silently reset;
- the second invocation reports the documented result.

Do not weaken assertions simply because the command succeeds twice.

---

## 8. Root validation and canonicalization

Workspace identity must be bound to the intended workspace root.

Test:

- existing directory;
- nonexistent root where creation is allowed;
- file used as root;
- nested directory;
- relative path where the public command allows it;
- absolute path;
- path containing spaces;
- Unicode path;
- normalized path;
- equivalent lexical paths that resolve to the same canonical directory;
- copied workspace metadata placed under a different root.

For a manifest that records the canonical root, explicitly test the foreign-manifest case:

1. initialize workspace A;
2. copy its workspace metadata into workspace B;
3. run the workspace operation against B;
4. verify A's manifest is **not adopted as B's identity**;
5. verify A's identity is not silently rewritten;
6. verify the operation fails with the documented deterministic error.

Do not canonicalize paths in the test merely to hide a production resolution bug. Exercise the same public input a human would provide.

---

## 9. Open / resolve behavior

If `awh workspace open` or an equivalent current command exists, test it as a separate process.

Verify:

- valid workspace opens;
- opening by supported identifier/path resolves the intended workspace;
- fresh process can open previously initialized state;
- unknown workspace fails;
- malformed workspace state fails;
- foreign workspace state fails;
- file-as-root fails;
- missing required state fails according to the contract.

Opening one workspace must never silently select another workspace merely because one is available elsewhere.

If the current branch does not implement `workspace open`, record it as unimplemented rather than substituting a private service call.

---

## 10. List behavior

If `awh workspace list` exists, create multiple isolated workspaces and verify:

- all eligible workspaces are discoverable;
- each workspace is represented once;
- identities are distinct;
- paths map to the correct workspace;
- unrelated directories are not falsely reported as workspaces;
- malformed state is handled according to the documented contract;
- list output remains deterministic where the contract requires ordering;
- a workspace removed from the registry/state is no longer reported.

If the product contract does not define ordering, do not invent a sorting requirement.

Do not rely on a single-workspace test as proof of list isolation.

---

## 11. Info behavior

If `awh workspace info` exists, verify that it reports the documented workspace metadata.

At minimum, where the contract exposes them, check:

- workspace ID;
- workspace root;
- manifest/state version;
- creation metadata if public;
- current workspace identity.

Test the command against:

- valid workspace;
- missing workspace;
- malformed manifest;
- foreign manifest;
- wrong path;
- multiple workspaces.

Do not expose secrets merely because they happen to exist elsewhere in the workspace.

---

## 12. Remove behavior

If workspace removal is implemented, treat it as destructive and test it conservatively.

First determine the documented semantics:

- remove registry entry only;
- remove AWH workspace metadata;
- remove workspace directory;
- preserve project files;
- or another explicit contract.

Then test exactly that behavior.

Always create a disposable temporary workspace.

Verify:

- correct workspace is selected;
- unrelated workspace remains intact;
- unrelated files are not deleted;
- repeated removal is deterministic;
- removing one workspace does not change another workspace's ID/state;
- stale references fail safely after removal.

Never test removal against the developer's actual project.

If confirmation or force flags exist, test both safe and explicit-destructive paths.

---

## 13. Persistence across process boundaries

Workspace state must not depend on one process staying alive.

Test:

1. create/initialize workspace;
2. record observable identity/state;
3. terminate process;
4. start a fresh `awh` process;
5. open/info/status the same workspace;
6. compare the durable identity and root.

Repeat the same test after a fresh process with a clean in-memory state.

If the implementation uses a manifest, read the actual persisted manifest as supporting evidence, but do not treat direct file inspection as a replacement for the user-facing CLI test.

---

## 14. Corruption and fail-closed behavior

Deliberately construct safe disposable failures:

- empty manifest;
- malformed JSON;
- truncated manifest;
- unsupported manifest version;
- missing required field;
- invalid workspace ID;
- wrong root;
- manifest pointing to another workspace;
- permissions preventing state access, where reproducible.

For each case verify:

- the operation fails;
- no new identity is silently generated over existing corrupted state;
- existing state is not silently reset;
- error is deterministic enough for automation;
- no panic occurs;
- no unrelated workspace is selected.

If the implementation has a documented recovery command, test recovery separately and verify that it is explicit rather than an automatic fallback.

---

## 15. Workspace isolation

Create at least two independent workspaces:

```text
workspace-A
workspace-B
```

Verify:

- IDs differ;
- roots differ;
- operations against A do not mutate B;
- operations against B do not mutate A;
- listing contains both exactly once;
- opening A never resolves B;
- corrupting A does not corrupt B;
- removing A does not remove B.

Where the current workspace service has persistent registries, inspect the registry behavior as well as the user-facing result.

Do not import tests from filesystem, Git, or agent feature families merely to prove those systems work. Only test the workspace boundary required to establish isolation.

---

## 16. Concurrency and locking

If the implementation uses a workspace manifest lock, state lock, or equivalent concurrency mechanism, test the observable contract.

Run controlled concurrent operations such as:

- two initializations of the same workspace;
- initialization and read;
- two reads;
- two independent workspace initializations.

Verify:

- no manifest corruption;
- no duplicate conflicting workspace identities;
- no partial JSON;
- no lost state;
- deterministic conflict behavior where conflicts are expected.

Do not require a specific lock implementation.

The acceptance criterion is safe, coherent state under the concurrency contract.

---

## 17. Filesystem safety

Workspace lifecycle tests must verify that paths stay within their intended scope.

Test:

- `..` traversal;
- nested traversal;
- encoded or normalized traversal if the input boundary accepts such values;
- symlink behavior where supported and security-relevant;
- workspace root that resolves outside the lexical input;
- copied metadata;
- paths containing spaces/Unicode.

The exact expected behavior must follow the current security contract.

Never allow a test fixture to target sensitive host files.

Use temporary directories outside the repository where possible.

---

## 18. Error and CLI behavior

For every public workspace command, verify:

- exit status;
- stdout;
- stderr;
- error category/message where contractually stable;
- no panic;
- no unexpected backtrace;
- no success output after failure;
- machine-readable output where supported.

For successful commands, assert meaningful state changes rather than merely `exit 0`.

For failures, assert that the failure corresponds to the intended invalid condition.

Do not write brittle tests against timestamps, random IDs, absolute temporary paths, or formatting that the public contract does not guarantee.

---

## 19. Human workflow acceptance tests

Automate complete workflows, not isolated commands.

### Workflow A — New workspace

```text
create temporary project
→ awh init / awh workspace create
→ awh status or workspace info
→ terminate
→ start fresh process
→ inspect workspace
```

Expected: the same workspace identity and root remain valid.

### Workflow B — Two workspaces

```text
create A
create B
list
open A
info A
open B
info B
```

Expected: A and B remain distinct.

### Workflow C — Reinitialize

```text
initialize
capture manifest/state
initialize again
compare identity/state
```

Expected: no silent reset.

### Workflow D — Foreign state

```text
initialize A
copy workspace metadata to B
attempt B initialization/open
```

Expected: foreign identity is rejected according to the current contract.

### Workflow E — Corrupt state

```text
initialize
corrupt disposable workspace metadata
start fresh awh process
inspect/open
```

Expected: deterministic failure, no automatic identity replacement.

### Workflow F — Removal

Where implemented:

```text
create A
create B
remove A
list
inspect B
```

Expected: only the documented A state is removed; B remains usable.

---

## 20. Test implementation requirements

Use the repository's existing conventions.

Prefer:

- unit tests for identity/path/manifest invariants;
- integration tests for workspace services;
- subprocess tests for real CLI behavior;
- temporary filesystem fixtures;
- restart/process-boundary tests;
- concurrency tests for lifecycle locking;
- property tests for pure parsers/validators.

A user-facing behavior must have at least one test at the real boundary when the binary/CLI is available.

Every important test must assert both:

1. the operation succeeded or failed as expected;
2. the resulting observable workspace state is correct.

Do not replace production workspace state with an in-memory fake merely to make tests easier.

---

## 21. Security regression requirements

Workspace tests must preserve these project invariants:

- workspace identity is not silently transferable to another root;
- path traversal and encoded-path bypasses are rejected where the security contract requires;
- corrupt state fails closed;
- invalid state does not trigger silent reset;
- unknown workspace references fail safely;
- one workspace cannot accidentally operate on another;
- secrets are not written to workspace logs/test output;
- internal helper calls do not bypass required workspace validation.

Never add `cfg(test)` shortcuts that bypass production path or identity checks.

Every discovered workspace-identity or path-isolation security regression must become a permanent test before the fix is considered complete.

---

## 22. Boundary and property tests

For pure workspace validators, use `proptest` where useful.

Cover:

- empty workspace names/IDs where relevant;
- invalid ID prefixes;
- invalid characters;
- very long values;
- whitespace;
- malformed serialized state;
- unsupported versions;
- missing required fields;
- unusual valid paths.

Properties should be meaningful, for example:

```text
valid serialized workspace state round-trips
invalid identity cannot become valid through normalization
same canonical root does not unexpectedly produce multiple identities
foreign root cannot validate against another workspace's manifest
```

Do not use property testing merely to increase test counts.

---

## 23. No test theater

Do not:

- test only private functions when the CLI is available;
- hard-code developer-specific paths;
- depend on an already initialized workspace;
- use global state shared by unrelated tests;
- swallow subprocess failures;
- assert only that a command did not panic;
- fabricate support for unimplemented workspace commands;
- change production behavior solely to satisfy tests;
- disable locks/path validation under tests;
- overwrite corrupt state during a validation test;
- claim persistence based only on an in-memory object;
- claim isolation based only on two different variables in one process.

A green test suite is not sufficient evidence if the real user workflow was never exercised.

---

## 24. Test execution gates

Run the repository's applicable gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
```

Also run:

- workspace-specific integration tests;
- real executable/CLI tests;
- relevant security tests;
- concurrency tests;
- platform-specific workspace tests where available.

If a test requires a platform-specific feature, run it on the actual supported platform or report it as unverified.

Do not alter unrelated test prompts or unrelated feature suites as part of this work.

---

## 25. Evidence report

Report workspace coverage using four explicit categories:

### Passed

Direct evidence from a successful test.

### Failed

The test executed and exposed a production defect.

### Blocked

The test could not run because of an environment/platform/dependency prerequisite.

### Unproven

The feature or contract lacks sufficient implementation or executable evidence.

For every failure record:

- test name;
- reproduction command;
- expected result;
- actual result;
- relevant error;
- likely subsystem;
- production defect versus test-environment issue.

For every unimplemented final-target command, state that it is unimplemented rather than writing a fake test that pretends it passed.

---

## 26. Completion criteria

This prompt is complete only when:

- the current Workspace Runtime implementation and contracts were inspected;
- every currently implemented workspace lifecycle operation has appropriate unit/integration coverage;
- real CLI/subprocess behavior is tested where available;
- workspace identity is tested;
- canonical-root binding is tested;
- restart/persistence is tested;
- multiple-workspace isolation is tested;
- malformed and foreign workspace state is tested;
- failure paths are tested;
- path-safety behavior is tested;
- concurrency/locking behavior is tested where applicable;
- existing tests remain green or failures are documented;
- no production security bypass was introduced;
- unimplemented target commands are honestly classified;
- no unrelated test master prompt was modified.

The objective is trustworthy evidence that AWH can safely create, identify, resolve, inspect, isolate, persist, and remove workspaces according to the **actual current branch contract**.

---

## 27. Scope boundary

This prompt owns **Workspace Runtime testing only**.

Do not create full feature suites for:

- Foundation and Distribution;
- filesystem editing;
- Git/worktrees;
- capability/policy;
- snapshots/rollback;
- context;
- memory;
- skills;
- Agent Profiles;
- sessions/tasks;
- MCP infrastructure;
- audit/observability;
- terminal;
- collaboration;
- Control API;
- TUI;
- connectors;
- advanced infrastructure.

Those feature families must have independent master test prompts.

Do not modify `docs/testing-prompts/01-foundation-distribution.md`.

Do not modify any other existing test master prompt while executing this prompt.
