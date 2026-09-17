# AWE Sequential Implementation Master Prompt — Implementation Tracking Contract

## 0. Purpose

This document is the **implementation-tracking contract for AWE work** in Agent Workspace Hub.

It answers, from current repository evidence:

- What AWE issue exists?
- What is its implementation state?
- Which commit implements it?
- Which tests verify it?
- Which PR contains it?
- What does CI report?
- Has it merged?
- Which dependencies are satisfied or blocking it?
- What blockers remain?
- What is the next implementation unit?

This is **not an agent-behavior prompt**. It does not define OpenHands Agent 1/2/3 roles, planner/worker/reviewer behavior, model routing, or autonomous-agent architecture.

> **Core rule:** Track repository evidence, not intentions. A prompt, plan, agent claim, PR, or documentation statement never proves implementation by itself.

---

# 1. Scope

Track the canonical AWE implementation issues:

```text
AWE-001 .. AWE-019
```

`SEC-*`, `AGENT-*`, `ARCH-*`, `FS-*`, and `GIT-*` may be recorded as dependencies or blockers, but they are not AWE implementation records merely because they are prerequisites.

AWE-018 and AWE-019 are documentation/status-oriented work and must not be used as proof that earlier AWE implementation is complete.

---

# 2. Evidence precedence

When evidence conflicts, use this order:

1. Current source on the tracked branch.
2. Current automated test results.
3. Current CI/check results.
4. Merged implementation commits.
5. Actual PR diff and current PR state.
6. GitHub issue state and acceptance criteria.
7. Current documentation.
8. Historical reports, old prompts, plans, and agent claims.

GitHub issue closure is evidence, but implementation correctness must still be reconciled with the current branch.

---

# 3. Canonical tracking schema

Every AWE issue MUST have a record with all of these dimensions:

```yaml
issue:
  id: "AWE-001"
  github_issue: 22
  title: "<current title>"
  issue_state: "OPEN"              # OPEN | CLOSED
  tracking_state: "OPEN"           # see state enum below

implementation:
  status: "NOT_STARTED"             # NOT_STARTED | PARTIAL | IMPLEMENTED | IMPLEMENTED_UNVALIDATED | VERIFIED
  implementation_commit:
    sha: null
    message: null
    branch: null
    merged_to_tracked_branch: false
  files_changed: []
  acceptance_criteria_met: []
  acceptance_criteria_remaining: []

verification:
  tests:
    status: "NOT_RUN"              # NOT_RUN | RUNNING | PASSED | FAILED | PARTIAL | NOT_APPLICABLE
    commands: []
    passed: []
    failed: []
    skipped: []
  ci:
    status: "NOT_RUN"              # NOT_RUN | RUNNING | PASSED | FAILED | CANCELLED | UNKNOWN
    workflow: null
    run_id: null
    conclusion: null
    evidence_url: null
  manual_verification: []

pull_request:
  status: "NONE"                   # NONE | OPEN | DRAFT | CHANGES_REQUESTED | APPROVED | MERGED | CLOSED_NOT_MERGED
  number: null
  title: null
  head_branch: null
  head_sha: null
  base_branch: "rust"
  mergeable: null
  review_status: "UNKNOWN"
  merged: false
  merged_at: null
  merge_commit_sha: null

relationships:
  dependencies: []
  satisfied_dependencies: []
  blocking_dependencies: []
  blocked_by: []

blockers:
  status: "NONE"                   # NONE | BLOCKED
  items: []

next_implementation_unit:
  id: null
  description: null
  reason: null
  prerequisites: []

tracking_evidence:
  source_refs: []
  last_verified_at: null
  last_verified_commit: null
  notes: []
```

**Do not collapse these dimensions into one status field.**

---

# 4. Tracking-state semantics

`tracking_state` MUST be one of:

```text
PROMPT_ONLY
NOT_STARTED
OPEN
IN_PROGRESS
IMPLEMENTED_UNVALIDATED
VERIFIED
MERGED
BLOCKED
SUPERSEDED
```

Meaning:

- `PROMPT_ONLY` — tracking prompt exists, but no implementation evidence is established.
- `NOT_STARTED` — no implementation evidence exists.
- `OPEN` — issue is actionable but implementation is not yet complete.
- `IN_PROGRESS` — implementation work is currently underway.
- `IMPLEMENTED_UNVALIDATED` — implementation commit exists but required verification is incomplete.
- `VERIFIED` — acceptance criteria and required verification evidence are satisfied.
- `MERGED` — verified implementation is integrated into the tracked branch with merge evidence.
- `BLOCKED` — progress cannot safely continue because a concrete blocker exists.
- `SUPERSEDED` — issue no longer represents the current implementation path; record the replacement/evidence in `notes`.

Important distinctions:

```text
OPEN issue      != unimplemented source
PR exists       != implementation verified
PR approved     != merged
commit exists   != verified
merged          != automatically verified
issue closed    != proof that current branch still satisfies it
```

---

# 5. Implementation commit tracking

Once implementation exists, record:

```text
implementation_commit.sha
implementation_commit.message
implementation_commit.branch
implementation_commit.merged_to_tracked_branch
```

Do not use a PR number as a substitute for a commit SHA.

If multiple commits form the implementation, record the complete relevant commit set in `notes` and identify the final implementation SHA as the primary SHA.

`files_changed` must identify the implementation surface relevant to the issue, not unrelated repository changes.

---

# 6. Acceptance-criteria tracking

Every issue record must separate:

```text
acceptance_criteria_met
acceptance_criteria_remaining
```

Each criterion should reference evidence where practical:

```yaml
acceptance_criteria_met:
  - criterion: "<criterion>"
    evidence: "<test/commit/source/CI reference>"
```

Never mark an issue `VERIFIED` while required acceptance criteria remain unresolved.

---

# 7. Test tracking

Tests are tracked independently from CI.

Record:

```text
exact command
relevant test target
passed tests
failed tests
skipped tests and reason
platform/environment limitation
```

A successful test command proves only what that command actually exercised.

For implementation work, missing or failing required behavioral tests prevents `VERIFIED`.

---

# 8. PR tracking

Track the PR independently from implementation state:

```text
number
title
head_branch
head_sha
base_branch
mergeable
review_status
merged
merged_at
merge_commit_sha
```

PR states:

```text
NONE
OPEN
DRAFT
CHANGES_REQUESTED
APPROVED
MERGED
CLOSED_NOT_MERGED
```

Never infer implementation completion merely from PR creation or approval.

---

# 9. CI tracking

CI is separate from local tests.

Record:

```text
workflow
run_id
status/conclusion
evidence_url
```

If required CI is still running or unavailable, do not claim complete verification solely from local tests.

If CI fails because of infrastructure, record the failure and documented cause rather than changing it to `PASSED`.

---

# 10. Merge tracking

Merge is an explicit tracking dimension:

```yaml
merged: true
merged_at: "<timestamp>"
merge_commit_sha: "<sha>"
merged_to_tracked_branch: true
```

The primary tracked integration branch is currently `rust` unless repository evidence changes that contract.

A feature is `MERGED` only when the implementation is present on the tracked branch and merge evidence is recorded.

---

# 11. Dependency schema

Every issue must record:

```yaml
dependencies: []
satisfied_dependencies: []
blocking_dependencies: []
blocked_by: []
```

Dependencies can be:

- another AWE issue;
- a security prerequisite;
- an architecture prerequisite;
- a required source/API contract;
- an explicitly documented external capability.

Example:

```yaml
dependencies:
  - "AWE-002"
  - "AWE-003"
satisfied_dependencies:
  - "AWE-002"
blocking_dependencies: 
  - "AWE-003"
blocked_by:
  - "AWE-003"
```

Do not mark a dependency satisfied merely because its GitHub issue is closed. Verify the implementation evidence required by the dependent issue.

---

# 12. Blocker schema

Every blocker must be concrete and actionable:

```yaml
blockers:
  status: "BLOCKED"
  items:
    - id: "BLOCKER-001"
      type: "DEPENDENCY"          # DEPENDENCY | CODE | TEST | CI | PR | ENVIRONMENT | SECURITY | ARCHITECTURE | UNKNOWN
      description: "<precise blocker>"
      evidence: "<issue/commit/PR/test/CI reference>"
      blocking_since: "<date or commit>"
      resolution_condition: "<what must become true>"
```

Do not use vague blockers such as `waiting`, `needs work`, or `agent failed`.

A blocker is cleared only when its resolution condition is demonstrated by current evidence.

---

# 13. Next implementation unit schema

The tracker MUST identify the next actionable AWE implementation unit whenever one exists:

```yaml
next_implementation_unit:
  id: "AWE-004"
  description: "<smallest coherent remaining implementation scope>"
  reason: "AWE-002 and AWE-003 are verified and AWE-004 remains unresolved."
  prerequisites:
    - "AWE-002"
    - "AWE-003"
```

The next unit must be:

1. unresolved;
2. dependency-ready;
3. backed by a current issue/prompt or explicit repository requirement;
4. small enough to implement and verify coherently;
5. selected from current repository evidence rather than stale roadmap assumptions.

If no safe next unit exists:

```yaml
next_implementation_unit:
  id: null
  description: null
  reason: "No dependency-ready unresolved AWE implementation unit exists."
  prerequisites: []
```

---

# 14. Current AWE dependency model

The known implementation ordering is:

```text
AWE-001 #22
   ↓
AWE-002 #23 ─┐
             ├→ AWE-004 #25
AWE-003 #24 ─┘
   ↓
AWE-005 #26
   ↓
AWE-006 #27
   ↓
AWE-007 #28
   ↓
AWE-008 #29
```

Then:

```text
AWE-009 #30
AWE-010 #31
AWE-011 #32
AWE-012 #33
AWE-013 #34
AWE-014 #35
AWE-015 #36
      ↓
AWE-016 #37
      ↓
AWE-017 #38
      ↓
AWE-018 #39
      ↓
AWE-019 #40
```

This is a **dependency model, not a completion claim**. Current source, tests, commits, PRs, CI, and issue acceptance criteria override this model when they demonstrate a changed dependency.

AWE-017 is the behavioral gate for claiming the agent-grade editing capability is implemented.

AWE-018 documents implementation-backed behavior.

AWE-019 is roadmap/status/strategic analysis and must never become an umbrella implementation issue.

---

# 15. Sequential tracking procedure

For every tracking cycle:

### Step 1 — Inspect current repository state

Record:

```text
tracked branch
HEAD SHA
current relevant commits
current tests
current documentation
```

### Step 2 — Inspect GitHub state

For `AWE-001..AWE-019`, record:

```text
issue state
acceptance criteria
open/closed PRs
review state
CI/check state
merge state
```

### Step 3 — Reconcile implementation

For each issue compare:

```text
issue
→ prompt
→ acceptance criteria
→ source
→ tests
→ implementation commit
→ PR
→ CI
→ merge
```

### Step 4 — Populate every schema dimension

Every field must be:

- populated with evidence;
- explicitly `null`/empty because it does not exist; or
- `UNKNOWN` when evidence cannot be obtained.

Never silently omit a status dimension.

### Step 5 — Recalculate dependencies

Mark each dependency as satisfied, blocking, or informational using current evidence.

### Step 6 — Recalculate blockers

Do not leave stale blockers after their resolution condition is satisfied.

### Step 7 — Recalculate next implementation unit

Select the earliest dependency-ready unresolved AWE implementation unit. If the earliest candidate is blocked, select another only when the dependency graph permits independent progress.

### Step 8 — Update after material events

Material events include:

```text
implementation commit
new test evidence
CI completion
PR creation
review decision
PR update
merge
issue closure/reopening
blocker resolution
new dependency discovery
```

---

# 16. False-completion prevention

Never mark an AWE issue complete because:

- its prompt exists;
- an agent says `done`;
- a PR was opened;
- a PR was approved;
- a commit exists without verification;
- tests were added but not run;
- one local test passed while required acceptance criteria remain untested;
- documentation claims completion;
- a historical report says it was complete;
- a dependency issue is closed without confirming the implementation required by the dependent issue.

Completion must be backed by current evidence.

---

# 17. Stale-tracking prevention

Do not permanently encode claims such as:

```text
0/19 complete
AWE-001 is open
AWE-002 is next
AWE-019 means all implementation is complete
```

unless they are freshly verified.

The tracker must be rerunnable after commits, merges, issue changes, CI changes, dependency changes, architecture changes, or discovery of pre-existing implementations.

Historical status may be retained only when explicitly labeled historical.

---

# 18. Final tracking report

A tracking cycle should produce:

```text
AWE IMPLEMENTATION STATUS
Tracked branch: <branch>
HEAD: <sha>

| Issue | Tracking State | Implementation | Tests | CI | PR | Merged | Blocked By |
|------|----------------|----------------|-------|----|----|--------|------------|
| AWE-001 | ... | ... | ... | ... | ... | ... | ... |
| AWE-002 | ... | ... | ... | ... | ... | ... | ... |
| ... | ... | ... | ... | ... | ... | ... | ... |

NEXT IMPLEMENTATION UNIT
ID: <AWE-xxx or NONE>
Description: <scope>
Reason: <evidence>
Prerequisites: <list>

BLOCKERS
- <blocker or NONE>

EVIDENCE
- Implementation commit: <sha or NONE>
- Tests: <commands/results>
- CI: <workflow/run/conclusion>
- PR: <number/status>
- Merge: <sha/status>
```

The table is a reporting view. The canonical source is the complete per-issue tracking schema.

---

# 19. Separation from development-agent orchestration

This document does **not** define:

- OpenHands Agent 1/2/3 behavior;
- planner/worker/reviewer prompts;
- model/provider selection;
- autonomous agent loops;
- GitHub Actions implementation details;
- AWH runtime agent behavior.

Its only development-automation responsibility is to expose accurate AWE implementation state that another system may consume.

```text
AWE TRACKER
    ↓
What is implemented / verified / merged / blocked?

DEVELOPMENT ORCHESTRATOR
    ↓
How should external agents perform development work?

AWH RUNTIME
    ↓
What does the shipped product do?
```

These are separate concerns.

---

# 20. Hard rules

1. **Track evidence, not intentions.**
2. **Separate issue state from implementation state.**
3. **Separate implementation commits from PR state.**
4. **Separate local tests from CI.**
5. **Separate PR approval from merge.**
6. **Record dependencies explicitly.**
7. **Record blockers explicitly.**
8. **Always identify the next implementation unit or explicitly state that none is selectable.**
9. **Never infer completion from documentation alone.**
10. **Never infer implementation from an open PR alone.**
11. **Never infer verification from a commit alone.**
12. **Never treat AWE-019 as proof that earlier implementation is complete.**
13. **Do not make OpenHands or another development agent an AWH runtime dependency.**
14. **Reconcile the tracker against the current `rust` branch before sequencing.**
15. **When evidence conflicts, preserve the conflict instead of silently choosing a convenient status.**

---

# 21. Definition of a complete tracking record

A complete record means the current state is accurately documented, not necessarily that the issue is complete.

The record must contain, or explicitly mark unavailable:

```text
✓ GitHub issue/state
✓ tracking state
✓ implementation status
✓ implementation commit(s)
✓ affected files
✓ acceptance criteria status
✓ tests and commands
✓ CI status and run evidence
✓ PR status and number
✓ merge status and merge SHA
✓ dependencies
✓ blockers
✓ next implementation unit
✓ evidence references
✓ last verification commit/time
```
