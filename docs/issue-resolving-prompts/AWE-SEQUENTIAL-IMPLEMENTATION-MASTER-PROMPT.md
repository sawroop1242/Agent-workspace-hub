# AWE Sequential Implementation Master Prompt — Current-State / OpenHands-Aligned Execution Contract

## 0. Purpose

Use this document as the **single execution contract for sequential issue resolution** in the AWH repository.

The current `rust` branch, current source/tests, GitHub issue state, merged commits, open PRs, and CI evidence are authoritative. Individual prompt files are implementation instructions; they are never proof that an issue is implemented or resolved.

This master prompt is designed to work with the repository's OpenHands-based GitHub Actions development pipeline while keeping OpenHands **outside the AWH runtime architecture**. OpenHands is an external build/review executor. AWH remains an agent-agnostic MCP-first workspace runtime.

### Core rule

> Select exactly one unresolved implementation issue, execute only that issue, verify it, produce evidence, and HARD STOP. The next issue is selected only by a new orchestration cycle or explicit user instruction.

---

# 1. Current repository execution model

The repository now contains an autonomous-development control plane using OpenHands agents and GitHub Actions. Current repository evidence includes:

```text
Agent 1 (plan)
      ↓
Agent 2 (build)
      ↓
PR / isolated branch
      ↓
Agent 3 (review)
      ↓
CI / deterministic gate
      ↓
merge decision
      ↓
next feature
```

The OpenHands workflow is an **external development mechanism**. It must not be treated as an AWH runtime feature and must not introduce OpenHands SDK/runtime dependencies into the Rust product merely to execute repository work.

The current agent-engine work also introduces persistent workflow state and dependency-aware feature selection. The latest OpenHands-created PR observed on the repository is PR #55, which adds dependency-aware feature selection, persistent `state.json`, real PR creation on PASS, scoped GitHub permissions, and planner/worker/repair prompt routing. It is still a PR-level workflow change and must not be counted as an AWH product issue resolution until its state and merge evidence say so.

### Agent-engine invariants

- `state.json` is workflow orchestration state, not AWH domain state.
- Agent-engine state must not be confused with AWH `.agent/` workspace state.
- One feature is processed per orchestration cycle.
- Registered unresolved dependencies block dependent features.
- Explicit feature selection must still enforce registered dependencies.
- Unregistered dependencies are treated as external/pre-existing capabilities and do not block selection.
- PASS may create an implementation PR; PASS does not imply merge.
- FAIL and BLOCKED must not push an implementation branch.
- State persistence must survive workflow runs without mutating the protected product branch directly.
- GitHub write permissions must remain scoped to the agent-engine job that needs them.
- The automation must never bypass branch/push guardrails.
- OpenHands may modify the repository only through the controlled development workflow; it is not part of the shipped AWH runtime.

---

# 2. AWH product boundary

AWH is an **agent-agnostic, MCP-first workspace runtime**, not an AI agent.

Do not add to AWH runtime:

- LLM reasoning;
- autonomous planning;
- model routing;
- prompt orchestration;
- generic agent workflow intelligence;
- OpenHands SDK/runtime integration merely for product functionality.

External agents such as OpenHands, Codex, Claude, Qwen, OpenCode, or other clients decide what should happen. AWH provides controlled workspace state, capabilities, mutations, verification, reversibility, identity, isolation, and observability.

The development automation may use LLM agents, but that automation is operational infrastructure and must remain separated from the AWH runtime.

---

# 3. Issue-state truth: never use the old static roadmap as truth

The master prompt must not contain a permanently trusted claim such as `0/25 resolved` or `security is still open`.

At the beginning of every execution cycle, dynamically inspect GitHub issue state and the current `rust` branch.

Current known examples demonstrate why this is mandatory:

- AWE-001 / #22 is closed.
- SEC-001 / #44 is closed.
- SEC-002 / #45 is closed.
- AGENT-001 / #46 remains open.
- ARCH-001 / #47 remains open.
- FS-001 / #48 remains open.

Therefore the sequence is a **dependency-aware execution order**, not a claim that every earlier issue is unresolved.

For each canonical issue record one of:

```text
PROMPT_ONLY
OPEN
IN_PROGRESS
IMPLEMENTED_UNVALIDATED
VERIFIED
CLOSED
BLOCKED
SUPERSEDED
```

An issue is considered resolved for sequencing only when implementation evidence and acceptance criteria are satisfied. GitHub `closed` status is strong evidence but should still be reconciled with the current branch when correctness matters.

A prompt existing in `docs/issue-resolving-prompts/` means only that instructions exist.

---

# 4. Canonical issue set and dependency graph

The canonical issue prompts cover:

```text
AWE-001..AWE-019
SEC-001..SEC-002
AGENT-001
ARCH-001
FS-001
GIT-001
```

The current post-edit architecture dependency chain is:

```text
Security prerequisites
        ↓
Agent-grade editing core
        ↓
Editing exposure / recovery / policy / identity prerequisites
        ↓
Editing acceptance / contract
        ↓
AGENT-001
        ↓
ARCH-001
        ↓
FS-001
        ↓
GIT-001
```

However, **do not blindly execute this list by number**. Before selecting an issue, evaluate actual GitHub state, dependencies, current branch implementation, and acceptance evidence.

### Security

- SEC-001 / #44 — deny High-risk built-in MCP tools by default — **closed; re-verify current branch when security-gating later work**.
- SEC-002 / #45 — require TLS for non-loopback MCP HTTP/SSE binds — **closed; current branch contains the reconciled implementation**.

SEC-002 competing PRs #52/#53 must not be counted as separate issues. They represented competing implementations of the same #45 issue.

### Editing foundation

```text
AWE-001 #22
   ↓
AWE-002 #23 + AWE-003 #24
   ↓
AWE-004 #25
   ↓
AWE-005 #26
   ↓
AWE-006 #27
   ↓
AWE-007 #28
   ↓
AWE-008 #29
```

AWE-002 and AWE-003 may proceed in parallel only when AWE-001's acceptance criteria are satisfied. AWE-004 waits for both.

### Editing exposure and acceptance

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

AWE-017 is the behavioral gate for claiming agent-grade editing is implemented. AWE-018 documents only implementation-backed behavior. AWE-019 is documentation/status analysis and must never become an implementation umbrella.

### Post-edit architecture

```text
AGENT-001 #46
      ↓
ARCH-001 #47
      ↓
FS-001 #48
      ↓
GIT-001 #49
```

AGENT-001 must establish real caller identity/session/policy semantics before ARCH-001 is treated as ready.

ARCH-001 must establish canonical filesystem ownership before FS-001 is treated as ready.

FS-001 must define filesystem mutation coordination before GIT-001 relies on filesystem mutation assumptions.

---

# 5. Dynamic issue-selection algorithm

Every orchestration cycle MUST perform this process:

### Step 1 — Read current repository state

Inspect:

- current branch/ref;
- working tree status if executing in a checkout;
- recent commits;
- open/closed issue state;
- open PRs affecting the candidate issue;
- relevant CI/check status;
- current prompt file;
- current implementation/tests.

### Step 2 — Build an issue evidence record

For every candidate issue:

```text
issue_id
issue_state
prompt_exists
implementation_present
acceptance_criteria
dependencies
blocking_dependencies
current_branch_evidence
open_prs
recent_relevant_commits
verification_evidence
remaining_gaps
```

### Step 3 — Select the next issue

Choose the earliest issue in the dependency graph whose:

- prerequisites are actually satisfied;
- issue is not already verified/closed;
- implementation is not blocked;
- no unresolved higher-priority security prerequisite exists;
- current branch does not already satisfy it completely.

If a candidate is already implemented, verify it rather than duplicating it.

If a candidate is blocked, record the blocker and select the next issue only if the dependency graph explicitly permits it. Otherwise HARD STOP.

### Step 4 — One issue only

Once selected:

> **Do not implement, refactor, or partially execute any later issue in the same run.**

---

# 6. OpenHands execution contract

OpenHands agents must treat the selected issue prompt as a constrained implementation contract.

Before coding, Agent 1/2/3 must inspect the complete relevant documentation tree and current source rather than relying only on the issue body.

At minimum inspect:

```text
AGENTS.md
Cargo.toml
README.md
docs/PROJECT_CONTEXT.md
docs/PROJECT_STATUS.md
docs/PROJECT_ROADMAP.md
docs/PROJECT_ROADMAP_STATUS.md
docs/MASTER_PROMPT.md
docs/mcp.md
docs/CLI.md
docs/FEATURES.md
docs/testing.md
docs/issue-resolving-prompts/
```

Then inspect issue-specific modules/tests and relevant recent commits.

OpenHands must not:

- assume a prompt means the issue is unresolved;
- assume an old forensic report describes current code exactly;
- overwrite unrelated work;
- modify `rust`/`main` directly when the workflow requires an isolated branch;
- merge its own PR automatically unless an explicit repository workflow authorizes that action;
- weaken tests or CI to obtain PASS;
- turn an issue into a broad refactor without evidence;
- implement future issues opportunistically.

---

# 7. Issue execution protocol

For the selected issue:

1. Read the issue prompt completely.
2. Read the GitHub issue and dependencies.
3. Inspect current source/tests.
4. Inspect relevant recent commits and open PRs.
5. Identify already-implemented portions.
6. Build a gap list against acceptance criteria.
7. Define the smallest coherent implementation.
8. Implement only the selected issue.
9. Add behavior-focused tests for every new invariant.
10. Run the applicable verification gate.
11. Inspect `git status` and `git diff`.
12. Verify no unrelated files changed.
13. Commit only the selected issue's work.
14. Produce evidence.
15. HARD STOP.

If the issue is documentation-only, perform documentation/static/repository validation instead of pretending that Rust implementation tests prove it.

---

# 8. Current architecture requirements

## 8.1 Identity and security ordering

For security-sensitive operations preserve:

```text
identify caller
→ resolve agent/session/workspace
→ capability/policy decision
→ domain validation
→ canonical service
→ persistence/mutation
→ audit/provenance where required
```

URL namespaces such as `/{agent}/mcp` or `/{agent}/sse` may identify or route traffic but are never authorization by themselves.

## 8.2 Canonical service/store architecture

ARCH-001 requires:

```text
MCP / CLI / TUI / Control API
            ↓
transport/interface adapters
            ↓
canonical services/domain
            ↓
canonical stores
```

A canonical owner may already exist or may need to be extracted/created.

MCP must not retain duplicate domain persistence for migrated domains.

Policy and trust stores are security authorities and must not be merged with ordinary domain persistence merely for symmetry.

Service construction must explicitly bind workspace, agent/session context, store ownership, dependencies, lifetime, and test injection.

Shared state does not prove shared implementation.

## 8.3 Filesystem ownership

Filesystem mutation must converge on one canonical service boundary.

FS-001 owns the deeper final-component TOCTOU and mutation-coordination implementation. ARCH-001 establishes the ownership boundary but must not absorb FS-001's detailed implementation scope.

FS-001 must address:

- final-component races;
- parent-directory replacement races;
- symlink/traversal containment;
- create vs replace semantics;
- rename/delete safety;
- process-aware advisory coordination where required;
- expected-state conflicts;
- atomicity/durability semantics;
- deterministic/stress/cross-process testing;
- no unsafe fallback.

The current issue explicitly requires an explicit Git index coordination/error policy, even though Git isolation itself belongs to GIT-001. fileciteturn301file0L2-L2

---

# 9. AWE editing invariants

The AWE editing pipeline remains:

```text
Agent
→ caller/session identity
→ read/locate
→ prepare patch
→ capability/policy
→ expected-state check
→ snapshot
→ apply
→ verify
→ provenance/audit
→ result
→ optional rollback
→ verify restoration
```

Never:

- silently overwrite stale state;
- treat a model field as implemented behavior when it is ignored;
- duplicate edit algorithms across MCP/CLI/TUI;
- expose an edit operation without the applicable authorization boundary;
- claim rollback without restoration tests;
- claim provenance without persisted correlated evidence;
- use context snapshots as substitutes for file snapshots;
- bypass the canonical expected-state model.

---

# 10. OpenHands agent roles

## Agent 1 — Planner / investigator

Must:

1. inspect current issue state;
2. inspect dependencies;
3. inspect relevant docs and source;
4. inspect recent commits/PRs;
5. determine whether the issue is already solved, partially solved, blocked, or unstarted;
6. produce a bounded implementation plan;
7. identify exact files expected to change;
8. define verification commands/tests;
9. never perform unrelated implementation.

Agent 1's plan must distinguish:

```text
CONFIRMED — direct repository evidence
LIKELY — strong but incomplete evidence
POSSIBLE — plausible, needs verification
SPECULATION — not actionable without evidence
```

## Agent 2 — Builder

Must:

1. read the selected prompt and Agent 1 plan;
2. independently inspect relevant source before editing;
3. preserve existing correct implementation;
4. implement only unmet acceptance criteria;
5. add focused tests;
6. run verification;
7. report limitations honestly;
8. commit only the selected issue.

Agent 2 must not treat Agent 1's plan as authoritative when current source contradicts it.

## Agent 3 — Reviewer

Must review:

- issue acceptance criteria;
- actual diff;
- source/tests;
- security implications;
- architecture boundaries;
- regression coverage;
- CI results;
- unrelated changes;
- documentation accuracy.

Agent 3 must produce actionable findings with repository evidence and verification status.

A deterministic failure cannot be downgraded merely because an LLM reviewer disagrees.

---

# 11. Dependency and checkpoint safety

The agent-engine workflow state is separate from issue state.

A workflow checkpoint must not claim an issue is completed merely because:

- an agent returned PASS text;
- a patch artifact exists;
- a PR exists;
- a prompt exists;
- a previous run recorded success.

Completion requires current repository evidence.

If a checkpoint is `BLOCKED`, the workflow must not force it into `PLANNING`, `RUNNING`, or another state without satisfying the state-machine transition contract.

State recovery must preserve operation identity and prevent duplicate execution where the workflow contract requires idempotence.

---

# 12. PR and branch safety

Implementation work must normally occur on an isolated feature branch.

The protected product branches are:

```text
rust
main
```

The agent-engine must never create a hidden direct-push path to these branches.

On PASS:

```text
verified isolated branch
→ push agent/<feature-id>-<run>
→ create PR against rust
```

On FAIL/BLOCKED:

```text
no implementation-branch push
```

State persistence may use its dedicated state ref/branch when the workflow explicitly defines it, but that state is not product implementation and must not contaminate the working tree.

No auto-merge is implied by PASS.

---

# 13. Verification gate

Run the strongest applicable checks after every implementation issue:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also inspect:

```bash
git status
git diff --stat
git diff
```

For Python agent-engine changes, also run the repository's applicable Python tests and syntax checks, for example:

```bash
python3 -m pytest tests/ -k agent_engine
python3 -m py_compile .github/agent-engine/orchestrator.py
```

Do not claim any command passed unless it actually ran successfully.

If a toolchain is unavailable, report that limitation and use actual CI evidence when available.

For documentation-only issues, use repository/document validation and do not claim Rust CI success without running it.

---

# 14. Security gate

Security is a prerequisite for exposing new privileged behavior.

Before any issue that expands mutation, remote access, identity, connector invocation, or agent control, verify that the relevant security prerequisites are actually satisfied on the current branch.

Required principles:

- High-risk built-in tools remain explicitly authorized;
- unknown tools fail closed;
- external-MCP trust semantics remain distinct from built-in authorization;
- non-loopback MCP HTTP/SSE requires the established TLS policy;
- policy checks occur before sensitive execution;
- no migration creates an authorization bypass;
- no filesystem fallback bypasses containment/security;
- no URL route is treated as authorization.

---

# 15. Migration and architecture evidence

For ARCH-001 and later architecture work, every migration must record:

```text
Current owner
Canonical owner
Persistence format/location
Schema/version
Locking model
Workspace scope
Security authority
Callers
Migration path
Rollback path
Restart behavior
Legacy deletion gate
```

Do not delete a legacy implementation until:

- zero production callers remain;
- no test treats it as authoritative;
- migration is verified;
- restart persistence is verified;
- rollback/recovery is understood;
- repository search finds no unintended references;
- documentation no longer presents it as authoritative.

---

# 16. Unresolved-work rule

If the selected issue is already implemented:

1. verify it against the current issue acceptance criteria;
2. inspect tests and CI evidence;
3. identify any remaining gap;
4. do not duplicate implementation;
5. close/mark resolved only when evidence supports it.

If partially implemented:

1. preserve working code;
2. implement only unmet acceptance criteria;
3. do not silently expand scope.

If blocked:

1. identify the exact blocker;
2. determine whether the blocker is a prerequisite issue;
3. do not mark the issue complete;
4. HARD STOP if no permitted alternative exists.

If a future issue depends on an unresolved prerequisite, do not implement the future issue simply because its prompt exists.

---

# 17. Documentation truthfulness

Documentation must reflect implementation evidence.

Never document as complete:

- an unmerged PR;
- a prompt-only issue;
- an untested implementation;
- an implementation that passes only mocked tests when integration behavior is required;
- an architecture target that has not been migrated;
- a strategic recommendation as if it were repository behavior.

When updating status documentation, include dates/commit references where useful and distinguish:

```text
Implemented + tested
Implemented + validation pending
Partially implemented
Planned
Blocked
Superseded
```

---

# 18. Commit discipline

Use focused commits.

One implementation issue should normally produce one focused implementation commit, plus narrowly justified follow-up fixes if the workflow explicitly permits them.

A reconciliation commit is valid when competing branches implement the same issue and must be consolidated. It must explain which behavior was retained and why.

Never count two competing PRs for one issue as two completed issues.

Documentation prompt updates are separate from product implementation commits.

---

# 19. Absolute rules

1. Current repository source/tests/CI are authoritative.
2. GitHub issue state is authoritative for issue lifecycle, but not a substitute for implementation evidence.
3. Prompt files are not proof of completion.
4. Execute one implementation issue per run.
5. Never silently skip an unresolved prerequisite.
6. Never implement a later issue opportunistically.
7. Never mutate protected branches directly through an agent-engine shortcut.
8. Never weaken tests or CI to obtain PASS.
9. Never claim a command passed unless it ran.
10. Never duplicate canonical edit algorithms.
11. Never create a second policy engine.
12. Never treat URL namespaces as authorization.
13. Never treat context snapshots as file snapshots.
14. Never claim rollback, provenance, identity, filesystem safety, or worktree isolation without the applicable evidence.
15. Preserve MCP-first, agent-agnostic product architecture.
16. Keep OpenHands/development-agent dependencies outside the shipped AWH runtime unless a separate product issue explicitly requires otherwise.
17. Keep agent-engine state separate from `.agent/` workspace state.
18. Preserve workflow dependency ordering and checkpoint state-machine invariants.
19. Do not count superseded competing PRs as separate issue completions.
20. Do not delete legacy stores until the explicit deletion gate passes.
21. Do not use `StoreLock` as a universal filesystem lock without evidence that its semantics fit.
22. Do not treat shared persistence as proof of unified business logic.
23. Do not allow MCP domain adapters to retain direct persistence after canonical migration unless an explicit reviewed exception exists.
24. Do not claim perfect cross-platform TOCTOU elimination without a platform-specific proof.
25. After completing the selected issue, HARD STOP.

---

# 20. Final reporting protocol

Every execution cycle must report:

```text
Issue: <ID and title>
Issue state before: <state>
Issue state after: <state>
Selection reason: <dependency/evidence>
Files changed: <exact list>
Implementation outcome: <summary>
Acceptance criteria: <met/unmet list>
Tests executed: <exact commands/results>
CI evidence: <links/results if available>
Recent relevant commits: <SHAs>
Open PRs considered: <numbers/status>
Known limitations/blockers: <list>
Diff scope: <exact scope>
Commit SHA: <SHA if committed>
GitHub issue closure status: <can close / cannot close / already closed>
```

Explicitly state:

> **No file outside the selected issue's approved scope was intentionally modified.**

If unrelated changes are discovered, stop and report them rather than hiding them.

---

# 21. HARD STOP

After the selected issue is implemented, verified, committed, and reported:

```text
STOP.
DO NOT SELECT THE NEXT ISSUE.
DO NOT IMPLEMENT THE NEXT PROMPT.
WAIT FOR THE NEXT ORCHESTRATION CYCLE OR EXPLICIT USER INSTRUCTION.
```

This rule applies even when the next issue appears trivial, already planned, or directly related.
