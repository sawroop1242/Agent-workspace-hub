# Master Test Prompt 04 — Git and First-Class Worktrees

## Mission

Create and execute a complete, human-behavior-oriented verification suite for AWH's Git and First-Class Worktrees feature family on the current rust branch.

This prompt owns Git repository operations exposed by AWH and the managed Git worktree isolation boundary. Testing must use real disposable Git repositories, the real awh executable/service boundary where available, and the real Git executable.

Do not treat cargo test alone as acceptance evidence. Do not redesign Git or create fake Git behavior merely to make tests pass.

Primary acceptance boundary:

human / agent request
→ workspace/session/worktree resolution
→ authorization boundary where implemented
→ canonical GitService / WorktreeStore
→ real git process + real repository
→ observable Git/worktree state

---

## 1. Product scope

The final AWH Git contract includes:

status
diff
staged-diff
log
branch
branches
worktree
stage
unstage
commit
push
pull
reset
clean
validate

The final worktree contract includes:

create
list
inspect
remove
merge
status

The current branch may implement only part of this contract.

Before testing, inspect the current source and classify every operation as:

- Implemented and tested
- Implemented but insufficiently tested
- Not implemented
- Blocked by environment
- Unproven

Never represent a roadmap command as currently available.

This prompt covers the Git/worktree feature family only. It does not own the complete filesystem-editing, capability/policy, snapshot, audit, agent identity, session, MCP, API, TUI, or collaboration suites.

---

## 2. Required repository forensics

Before writing or changing tests, inspect the current rust branch.

At minimum inspect:

- README.md
- AGENTS.md
- Cargo.toml
- docs/FEATURES.md
- docs/testing.md
- docs/CLI.md
- docs/configuration.md
- docs/architecture.md
- docs/security.md
- docs/threat-model.md
- docs/roadmap/PROJECT_ROADMAP.md
- docs/roadmap/GROWTH_STRATEGY.md
- docs/implementation-prompts/README.md
- Git implementation prompts
- worktree implementation prompts
- existing Git/worktree tests
- relevant CI workflows

Inspect actual source for:

- GitService;
- worktree lifecycle service/store;
- CLI Git commands;
- CLI worktree commands;
- repository/path validation;
- structured Git process invocation;
- Git timeout handling;
- worktree persistence;
- ownership/session binding;
- locking;
- reconciliation;
- audit integration.

Search for:

GitService
git status
git diff
git commit
git push
git pull
git reset
git clean
git worktree
WorktreeStore
WorktreeRecord
WorktreeId
reconcile
BranchCheckedOut
Dirty
RecoveryRequired

Current source and current tests take precedence over archived roadmaps.

---

## 3. Real Git test environment

Every Git acceptance test must use a disposable real Git repository.

Fixture setup should:

1. create a temporary directory;
2. initialize a real Git repository;
3. configure local test identity only;
4. create deterministic commits/files;
5. invoke the real AWH binary/service;
6. inspect state with both AWH and real Git;
7. clean up automatically.

Use synthetic values such as:

name: AWH Test User
email: awh-test@example.invalid

Never use the user's global Git configuration.

Do not modify the AWH repository under test.

Before running a test, verify that the required git executable exists and record its version.

---

## 4. Git command safety

Acceptance tests must prove that AWH invokes Git through structured argument boundaries rather than shell interpolation.

Use hostile-but-safe fixture values for:

- valid unusual branch names;
- file names;
- paths containing spaces;
- values beginning with a dash where the contract permits;
- strings containing shell metacharacters;
- commit messages containing quotes/newlines;
- repository paths containing spaces.

Verify that these values are treated as data rather than executable shell syntax.

Tests must never rely on shell expansion to invoke AWH or Git.

Where source inspection is needed to verify argv handling, combine it with black-box tests that demonstrate the behavior.

---

## 5. Git repository validation

Test commands against:

- valid Git repository;
- non-Git directory;
- missing directory;
- file instead of directory;
- nested repository where supported;
- repository with no commits;
- repository with detached HEAD.

Verify:

- valid repositories operate normally;
- invalid repositories fail predictably;
- AWH does not silently operate on an unrelated parent repository;
- failure does not create unintended files or state.

If repository discovery follows an explicit AWH workspace root, test that root boundary directly.

---

## 6. Git status

Test the real AWH Git status boundary.

Create scenarios for:

- clean repository;
- modified tracked file;
- untracked file;
- deleted tracked file;
- staged change;
- mixed staged/unstaged changes;
- rename where Git detects it;
- repository with no commits.

Verify AWH's result against independent git status output.

Do not assert formatting that is not part of the public AWH contract.

The important evidence is semantic equivalence and correct repository scope.

---

## 7. Git diff and staged-diff

Test the diff and staged-diff operations where implemented.

Cover:

- no changes;
- unstaged changes;
- staged changes;
- mixed changes;
- binary files;
- renamed files;
- deleted files;
- whitespace changes;
- Unicode content.

Compare the semantic result against real Git.

Verify:

- diff does not accidentally report staged-only content as unstaged;
- staged-diff does not include unrelated unstaged content;
- repository scope remains correct.

---

## 8. Git log

Test log behavior with:

- one commit;
- multiple commits;
- merge commit where supported;
- Unicode commit message;
- empty repository.

Verify:

- commits are returned from the correct repository;
- ordering matches the documented contract;
- commit identifiers are valid;
- no unrelated repository history appears.

Do not hard-code timestamps.

If pagination/limits exist, test boundary values.

---

## 9. Branch operations

Test branch operations with:

- current branch;
- multiple branches;
- branch creation;
- branch listing;
- branch deletion where supported;
- invalid branch names;
- duplicate branch creation;
- valid punctuation;
- conflicting branch names.

Verify against real Git.

For destructive branch deletion, use disposable repositories and verify resulting refs independently.

Do not silently force-delete branches.

---

## 10. Stage and unstage

Test stage and unstage with:

- one file;
- multiple files;
- nested files;
- new files;
- modified files;
- deleted files;
- nonexistent paths;
- path traversal;
- filenames with spaces/Unicode.

After each operation compare:

AWH result
↔ git status --porcelain
↔ actual index/worktree state

Verify that staging one file cannot stage an unrelated file.

Verify that unstage cannot discard working-tree changes unless the contract explicitly says so.

---

## 11. Commit

Test real commits using disposable repositories.

Cover:

- valid message;
- empty message;
- staged changes;
- no staged changes;
- multiple staged files;
- Unicode message;
- message containing quotes/newlines;
- commit after branch creation.

Verify:

- commit succeeds only under documented preconditions;
- resulting commit exists in real Git;
- staged changes are committed as expected;
- unrelated unstaged changes remain untouched;
- commit metadata is correct enough for the public contract.

Do not depend on global Git identity.

Do not assert wall-clock timestamps exactly.

---

## 12. Push and pull

Where implemented, test push/pull against a local disposable bare Git remote, not a public repository.

Fixture:

temporary bare remote
        ↑
temporary clone A
        ↕
temporary clone B

Test:

- successful push;
- pull of a new remote commit;
- no-op push/pull;
- missing remote;
- rejected non-fast-forward update;
- divergent history where the contract exposes behavior;
- network/process failure where safely injectable.

Verify real refs and working-tree state.

Never use personal GitHub credentials or a real production remote.

If the current branch does not implement push/pull, report it as unimplemented.

---

## 13. Reset and clean

Treat reset and clean as high-risk destructive operations.

Use only disposable repositories.

First determine exact public semantics and any confirmation/force mechanism.

Test:

- safe/default invocation;
- explicit destructive invocation;
- staged changes;
- unstaged changes;
- untracked files;
- mixed state;
- invalid target.

Verify resulting index and filesystem against real Git.

Never accept a test that proves only that the command returned zero.

Never allow tests to use hard reset, force-clean, or equivalent destructive behavior implicitly.

If AWH intentionally restricts destructive modes, verify the restriction.

---

## 14. Git validate

Where validate exists, test:

- healthy repository;
- missing Git metadata;
- detached HEAD;
- corrupt/invalid state where safely reproducible;
- missing branch/ref;
- worktree anomalies.

Verify that validation reports the actual condition and does not mutate the repository.

Validation must be observational unless the documented contract explicitly includes repair.

---

## 15. Worktree creation

For every implemented managed-worktree creation path, use a real Git repository.

Verify:

- a real Git worktree is created;
- the managed path is inside the approved AWH worktree area;
- the main checkout remains intact;
- a unique WorktreeId is assigned;
- workspace/agent/session ownership is recorded where implemented;
- branch/ref is correct;
- Git itself lists the worktree;
- AWH does not publish active state before verification.

Compare AWH state with git worktree list --porcelain.

Do not rely only on AWH metadata.

---

## 16. Worktree branch isolation

Test:

- default branch generation;
- explicit valid branch;
- invalid branch;
- existing branch;
- branch already checked out by another worktree;
- two sessions requesting the same branch;
- detached/start-point behavior if supported.

Verify that AWH never silently switches or resets another worktree to resolve a collision.

A branch conflict must fail safely.

Inspect all affected worktrees after a failed request.

---

## 17. Worktree path containment

Test managed worktree paths against:

- absolute paths;
- parent traversal;
- nested traversal;
- path-prefix confusion;
- symlink escape where supported;
- workspace root itself;
- sibling managed worktree;
- existing unrelated directory;
- Unicode/spaces.

A successful negative test must prove that no outside path was created or modified.

Do not use string-prefix assertions as the only containment proof.

Inspect the actual resolved filesystem path and Git worktree record.

---

## 18. Worktree list and inspect

Where implemented, test list/inspect against:

- one active worktree;
- multiple worktrees;
- removed worktree;
- missing worktree;
- corrupted record;
- branch mismatch;
- repository mismatch;
- manually removed checkout;
- unmanaged Git worktree.

Verify:

- AWH reports only what its contract permits;
- active state corresponds to actual Git state;
- anomalies are classified rather than hidden;
- unmanaged worktrees are not silently adopted;
- identities map to correct paths.

Cross-check with git worktree list --porcelain.

---

## 19. Worktree removal

Use disposable repositories and test removal conservatively.

Verify:

- owner can remove intended worktree;
- non-owner cannot remove it;
- main workspace cannot be removed through managed-worktree path;
- dirty worktree behavior preserves changes where contract requires;
- already missing worktree can be reconciled according to contract;
- unrelated worktree remains intact.

After removal compare:

- AWH lifecycle state;
- filesystem state;
- git worktree list --porcelain.

Do not substitute recursive filesystem deletion for Git's lifecycle unless the documented orphan-recovery contract explicitly permits it.

---

## 20. Worktree crash/reconciliation behavior

Where lifecycle persistence supports recovery, test controlled crash windows or equivalent failure injection.

### Before Git creation

Expected: no false active worktree.

### After Git creation but before active-state publication

Expected: reconciliation can classify/repair only when ownership is unambiguous.

### During removal

Expected: state is not falsely reported as fully removed or active without verification.

### After manual deletion

Expected: managed record becomes the documented missing/recovery state.

Use real process termination or the repository's supported failure-injection mechanism.

Do not simulate success by directly writing the expected lifecycle state.

---

## 21. Worktree isolation proof

This is a critical acceptance test.

Create:

main checkout
worktree A
worktree B

Perform a filesystem/Git mutation inside A.

Verify:

- A sees the mutation;
- B does not see the mutation;
- main checkout does not see the mutation;
- Git status in A differs as expected;
- Git status in B remains unchanged;
- main checkout remains unchanged.

Then repeat with B.

If the current agent/session model is implemented, bind A and B to distinct sessions and prove that a session cannot address the sibling worktree through the supported boundary.

This test must inspect actual filesystem contents, not only AWH records.

---

## 22. Worktree-to-edit integration boundary

Where both managed worktrees and canonical editing are implemented, verify the thin integration boundary:

session
→ assigned worktree root
→ canonical EditService/EditTransaction
→ real worktree filesystem

Test that an edit inside A is invisible to B and the parent checkout.

Do not reimplement editing semantics in this prompt.

Do not duplicate the filesystem editing test suite.

---

## 23. Ownership and authorization boundary

Where agent/session identity and PolicyEngine are implemented, verify only the Git/worktree boundary:

- allowed owner can operate on its worktree;
- wrong session is rejected;
- wrong agent is rejected;
- wrong workspace is rejected;
- denied destructive Git operation causes no Git mutation.

Possession of a path, branch, WorktreeId, or repository name must not itself authorize access.

Do not implement a second authorization engine.

---

## 24. Persistence and restart

Test worktree lifecycle across process boundaries.

Scenario:

create workspace
→ create managed worktree
→ capture WorktreeId
→ terminate awh
→ start fresh awh
→ list/inspect
→ compare with real git worktree state

Verify:

- identity remains stable;
- path remains correct;
- branch remains correct;
- ownership remains correct where persisted;
- active state is not fabricated when Git state is missing.

Repeat after manually deleting the worktree and after controlled corruption of disposable metadata.

---

## 25. Concurrent worktree operations

Run controlled concurrent operations for:

- two creations for one session;
- two sessions creating simultaneously;
- same branch requested concurrently;
- list while create runs;
- inspect while remove runs;
- remove versus reconciliation.

Verify:

- no duplicate active ownership;
- no path collision;
- no corrupted lifecycle record;
- no accidental deletion of another worktree;
- deterministic conflict behavior;
- real Git state remains coherent.

Do not require a particular locking implementation.

The test must establish the documented concurrency guarantee.

---

## 26. Git/worktree state after failures

For every failed Git/worktree operation, inspect the real repository afterward.

At minimum compare:

git status --porcelain
git branch --list
git worktree list --porcelain
filesystem tree
AWH persisted state

This is mandatory for high-risk failures.

A failed operation must not be declared safe merely because the process returned non-zero.

---

## 27. Error handling

Test:

- missing repository;
- invalid ref;
- branch collision;
- dirty worktree;
- missing worktree;
- corrupt lifecycle record;
- invalid WorktreeId;
- wrong owner;
- Git executable failure;
- timeout where supported;
- malformed arguments.

Verify:

- non-zero failure status;
- meaningful deterministic error;
- no panic;
- no secret leakage;
- no unintended repository mutation.

Do not make tests depend on exact Git-version-specific stderr text unless the AWH contract explicitly exposes it.

---

## 28. Environment variables and configuration

Read docs/configuration.md and exercise every environment variable that materially affects Git/worktree behavior.

At minimum identify whether the current implementation uses:

- persistent-state configuration;
- workspace configuration;
- sandbox configuration such as AWH_BWRAP;
- resource/timeouts affecting Git operations;
- GitHub provider variables where GitHub operations are actually part of the tested implementation.

Do not invent configuration keys.

Use isolated synthetic environments.

Never place real tokens in Git test fixtures.

---

## 29. Resource and timeout testing

Where GitService exposes timeouts/limits, test:

- normal operation below the limit;
- operation at the boundary;
- operation exceeding the limit;
- Git process failure;
- repeated timeout/failure.

Verify that timeout produces a bounded failure rather than a permanently hanging AWH process.

Do not use arbitrary sleeps as the only synchronization mechanism.

Use process completion, Git-state polling, or deterministic test hooks.

---

## 30. Cross-platform matrix

Where the product supports the platform, test Git/worktree behavior on:

- Linux x86_64;
- Linux ARM64;
- macOS x86_64;
- macOS ARM64;
- Windows x86_64;
- Android/Termux ARM64 where Git/worktree functionality is supported by the platform contract.

Test platform-sensitive behavior including:

- path separators;
- canonical paths;
- symlinks;
- executable discovery;
- Git worktree path reporting;
- filesystem permissions.

Do not mark a platform Passed from source inspection alone.

Classify unavailable platform evidence as Blocked or Unproven.

---

## 31. Human workflow acceptance tests

Automate complete workflows.

### Workflow A — Normal Git cycle

create repo
→ create file
→ awh git status
→ awh git diff
→ awh git stage
→ awh git staged-diff
→ awh git commit
→ awh git log

Cross-check every important state with real Git.

### Workflow B — Branch isolation

create branch
→ modify file
→ inspect branch
→ operate according to contract
→ verify refs and files

### Workflow C — Worktree lifecycle

create managed worktree
→ inspect
→ list
→ modify inside worktree
→ inspect Git state
→ remove
→ verify main checkout

### Workflow D — Two-agent isolation

create A and B
→ mutate A
→ inspect B
→ mutate B
→ inspect A
→ inspect main checkout

Expected: no cross-worktree leakage.

### Workflow E — Dirty removal

create worktree
→ make uncommitted change
→ request removal
→ inspect filesystem/Git

Expected: documented dirty-worktree protection; no silent data loss.

---

## 32. Independent oracle requirement

Where practical, validate AWH results against independent Git commands:

git status --porcelain
git diff
git diff --cached
git log
git branch --list
git rev-parse HEAD
git worktree list --porcelain

Do not compute the expected Git result by calling the same AWH helper under test.

For hashes/refs, use Git's independent output.

For filesystem state, read actual files.

This prevents mirrored production/test bugs.

---

## 33. Security regression requirements

Every Git/worktree security regression must become a permanent test.

At minimum preserve:

- no shell injection through Git arguments;
- no repository/worktree path traversal;
- no cross-workspace worktree access;
- no cross-agent/session worktree access;
- no silent branch switching;
- no silent hard reset/clean;
- dirty worktrees are not silently destroyed;
- unmanaged paths are not adopted;
- corrupt lifecycle records fail closed;
- WorktreeId/path/branch knowledge is not authorization;
- secrets are never logged/audited.

A denied or invalid operation must have zero unintended Git/filesystem side effects.

---

## 34. No test theater

Do not:

- use a fake Git repository for acceptance;
- mock Git when real Git can be used;
- test only AWH metadata without comparing real Git state;
- hard-code developer-specific Git configuration;
- use a real production remote;
- use personal credentials;
- assert only exit codes;
- swallow Git stderr/process failures;
- use shell interpolation in test commands to hide argument-boundary problems;
- force-clean/reset test state after every failure before inspecting it;
- fabricate tests for unimplemented roadmap commands;
- weaken assertions because Git output varies across versions;
- modify unrelated test master prompts.

A test is meaningful only when it proves observable repository/worktree behavior.

---

## 35. Execution gates

Run:

cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets

Then run:

- Git-specific integration tests;
- worktree CLI tests;
- security tests;
- concurrency tests;
- real executable tests;
- cross-platform CI jobs where available.

Also run the real Git version used by the environment and record it with test evidence.

If Git is unavailable, classify Git acceptance as Blocked rather than Passed.

---

## 36. Evidence report

Report each Git/worktree capability as:

### Passed

Real AWH operation and independent Git/filesystem observation agree.

### Failed

The real test executed and exposed a production defect.

### Blocked

The test could not run because Git/platform/dependency/environment prerequisites were unavailable.

### Unproven

The implementation or contract lacks sufficient executable evidence.

For failures include:

- test name;
- repository fixture state;
- exact AWH invocation;
- relevant Git oracle command;
- expected result;
- actual result;
- final Git state;
- final filesystem state;
- AWH persisted state;
- likely subsystem;
- production defect versus environment issue.

For unimplemented commands, state Not implemented.

---

## 37. Completion criteria

This prompt is complete only when:

- current Git implementation and worktree implementation were inspected;
- every currently implemented Git operation has meaningful tests;
- every currently implemented managed-worktree lifecycle operation has meaningful tests;
- real Git repositories are used for acceptance;
- AWH results are cross-checked with independent Git state;
- branch/ref behavior is tested;
- stage/unstage/commit behavior is tested;
- push/pull is tested with a disposable local remote where implemented;
- destructive reset/clean behavior is tested safely;
- worktree containment and ownership are tested;
- worktree create/list/inspect/remove are tested where implemented;
- crash/reconciliation behavior is tested where supported;
- real filesystem isolation between worktrees is proven;
- concurrency behavior is tested where applicable;
- failure side effects are inspected;
- relevant configuration and limits are tested;
- security regressions have permanent coverage;
- unimplemented target functionality is honestly classified;
- no unrelated test master prompt was modified.

The objective is trustworthy evidence that AWH can safely operate on Git repositories and provide isolated, correctly owned, real Git worktrees without cross-workspace leakage, unintended destructive operations, branch collisions, or false lifecycle state.

---

## 38. Scope boundary

This prompt owns Git and First-Class Worktrees testing only.

Do not create full feature suites for:

- Foundation and Distribution;
- Workspace Runtime;
- Agent-Grade Filesystem Editing;
- Capability and Policy Engine;
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

docs/testing-prompts/01-foundation-distribution.md
docs/testing-prompts/02-workspace-runtime.md
docs/testing-prompts/03-agent-grade-filesystem-editing.md

Do not modify any other existing test master prompt while executing this prompt.
