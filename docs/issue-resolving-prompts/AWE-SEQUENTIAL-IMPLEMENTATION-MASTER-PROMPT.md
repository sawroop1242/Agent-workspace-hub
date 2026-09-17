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
  tracking_state: "OPEN"

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
  status: "NONE"
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
  status: "NONE"
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
- `SUPERSEDED` — issue no longer represents the current implementation path.

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

# 5. Current AWE-001 through AWE-008 status

The following is the **current tracking snapshot**, not a permanent truth. It records the GitHub issue state known at the time this prompt was updated and must be revalidated during every tracking cycle.

| Issue | GitHub Issue | Issue State | Implementation Status | Verification Status | Blockers | Next Implementation Unit |
|---|---:|---|---|---|---|---|
| AWE-001 | #22 | **CLOSED** | **IMPLEMENTED** | **VERIFIED / reconcile against current branch** | None recorded | No — complete; continue dependency graph |
| AWE-002 | #23 | **CLOSED** | **IMPLEMENTED** | **VERIFIED / reconcile against current branch** | None recorded | No — complete; continue dependency graph |
| AWE-003 | #24 | **CLOSED** | **IMPLEMENTED** | **VERIFIED / reconcile against current branch** | None recorded | No — complete; continue dependency graph |
| AWE-004 | #25 | **CLOSED** | **IMPLEMENTED** | **VERIFIED / reconcile against current branch** | None recorded | No — complete; continue dependency graph |
| AWE-005 | #26 | **CLOSED** | **IMPLEMENTED** | **VERIFIED / reconcile against current branch** | None recorded | No — complete; continue dependency graph |
| AWE-006 | #27 | **CLOSED** | **IMPLEMENTED** | **VERIFIED / reconcile against current branch** | None recorded | No — complete; continue dependency graph |
| AWE-007 | #28 | **OPEN** | **OPEN / remaining implementation** | **NOT VERIFIED** | None recorded; issue remains actionable | **YES — AWE-007 stale-state/edit conflict detection** |
| AWE-008 | #29 | **CLOSED** | **IMPLEMENTED** | **VERIFIED / reconcile against current branch** | None recorded | No — complete; continue dependency graph |

### Current sequencing interpretation

AWE-007 is the **active unresolved implementation unit** in the AWE-001..AWE-008 range. GitHub currently reports #28 as open, while #22, #23, #25, #26, #27, and #29 are closed; AWE-003/#24 is also tracked as closed. fileciteturn314file0L3-L7 fileciteturn315file0L3-L7 fileciteturn317file0L3-L7 fileciteturn318file0L3-L7 fileciteturn319file0L3-L7 fileciteturn320file0L3-L7 fileciteturn321file0L3-L7

AWE-007 depends on AWE-002 and AWE-004, with AWE-006 required to preserve the invariant during commit/rollback. Its core invariant is `expected_state != current_state → conflict → zero mutation`. fileciteturn320file0L6-L7

**Do not infer that AWE-008 being closed makes AWE-007 complete.** Issue state and implementation state remain separate tracking dimensions.

---

# 6. Implementation commit tracking

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

# 7. Acceptance-criteria tracking

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

# 8. Test tracking

Tests are tracked independently from CI.

Record exact commands, relevant targets, passed tests, failed tests, skipped tests and reasons, and platform/environment limitations.

A successful test command proves only what that command actually exercised.

For implementation work, missing or failing required behavioral tests prevents `VERIFIED`.

---

# 9. PR tracking

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

# 10. CI tracking

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

# 11. Merge tracking

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

# 12. Dependency schema

Every issue must record:

```yaml
dependencies: []
satisfied_dependencies: []
blocking_dependencies: []
blocked_by: []
```

Dependencies can be another AWE issue, a security prerequisite, an architecture prerequisite, a required source/API contract, or an explicitly documented external capability.

Do not mark a dependency satisfied merely because its GitHub issue is closed. Verify the implementation evidence required by the dependent issue.

---

# 13. Blocker schema

Every blocker must be concrete and actionable:

```yaml
blockers:
  status: "BLOCKED"
  items:
    - id: "BLOCKER-001"
      type: "DEPENDENCY"
      description: "<precise blocker>"
      evidence: "<issue/commit/PR/test/CI reference>"
      blocking_since: "<date or commit>"
      resolution_condition: "<what must become true>"
```

Do not use vague blockers such as `waiting`, `needs work`, or `agent failed`.

A blocker is cleared only when its resolution condition is demonstrated by current evidence.

---

# 14. Next implementation unit schema

The tracker MUST identify the next actionable AWE implementation unit whenever one exists:

```yaml
next_implementation_unit:
  id: "AWE-007"
  description: "Implement stale-state and edit conflict detection."
  reason: "AWE-007/#28 is the unresolved issue in the AWE-001..AWE-008 sequence; its declared dependencies are already closed."
  prerequisites:
    - "AWE-002"
    - "AWE-004"
    - "AWE-006"
```

The next unit must be unresolved, dependency-ready, backed by a current issue/prompt or explicit repository requirement, and small enough to implement and verify coherently.

If no safe next unit exists, use `id: null` and explicitly explain why.

---

# 15. Current AWE dependency model

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

# 16. Sequential tracking procedure

For every tracking cycle:

1. Inspect the tracked branch and HEAD.
2. Inspect GitHub state for AWE-001..AWE-019.
3. Reconcile each issue against prompt, acceptance criteria, source, tests, implementation commits, PRs, CI, and merge state.
4. Populate every schema dimension; use `null`, empty, or `UNKNOWN` when evidence is unavailable.
5. Recalculate dependencies and blockers.
6. Recalculate the next implementation unit.
7. Update the record after implementation commits, test evidence, CI completion, PR creation/update/review, merge, issue closure/reopening, blocker resolution, or dependency discovery.

---

# 17. False-completion prevention

Never mark an AWE issue complete because:

- its prompt exists;
- an agent says `done`;
- a PR was opened or approved;
- a commit exists without verification;
- tests were added but not run;
- one local test passed while required acceptance criteria remain untested;
- documentation claims completion;
- a historical report says it was complete;
- a dependency issue is closed without confirming the implementation required by the dependent issue.

Completion must be backed by current evidence.

---

# 18. Stale-tracking prevention

Do not permanently encode claims such as `0/19 complete`, `AWE-001 is open`, or `AWE-002 is next` unless freshly verified.

The tracker must be rerunnable after commits, merges, issue changes, CI changes, dependency changes, architecture changes, or discovery of pre-existing implementations.

Historical status may be retained only when explicitly labeled historical.

---

# 19. Final tracking report

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

# 20. Separation from development-agent orchestration

This document does **not** define OpenHands Agent 1/2/3 behavior, planner/worker/reviewer prompts, model/provider selection, autonomous agent loops, GitHub Actions implementation details, or AWH runtime agent behavior.

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

# 21. Hard rules

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

# 22. Definition of a complete tracking record

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
