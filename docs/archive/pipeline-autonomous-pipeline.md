# AWH Autonomous Pipeline

AWH adopts the useful orchestration ideas from `zxkane/autonomous-dev-team`, but does **not** copy its label-driven state machine. AWH's durable `.openhands/state.json` checkpoint, operation IDs, CAS transitions, recovery leases, and event identity are authoritative.

## Pipeline

```text
Checkpoint dispatcher (5 min)
        |
        +--> IDLE/COMPLETED ----> Agent 1 Planner
        |                              |
        |                         generated-task.md
        |                              |
        +--> BUILDING ----------> Agent 2 Builder ----> feature branch ----> PR
        |                                                            |
        +--> PR_OPEN/REVIEWING <-----------------------------------+
        |                                                            |
        |                         Agent 3 Reviewer                    |
        |                              |                              |
        |                 +------------+------------+                 |
        |                 |                         |                 |
        |              APPROVE               CHANGES_REQUIRED         |
        |                 |                         |                 |
        |              MERGING                  FIXING                |
        |                 |                         |                 |
        |              merge <---------------- Agent 2 fix ----------+
        |                 |
        +-------------- COMPLETED

BLOCKED --> Recovery claim --> inspect GitHub reality --> safe continuation
```

## Mapping from autonomous-dev-team

| Reference concept | AWH implementation |
|---|---|
| Dispatcher cron tick | `awh-dispatcher.yml` every 5 minutes |
| `autonomous` issue label | checkpoint status + backlog feature identity |
| Dev Agent | Agent 2 builder (`awh-builder.yml`) |
| Review Agent | Agent 3 reviewer (`awh-reviewer.yml`) |
| Review-fix loop | `awh-review-fix.yml` + `awh.fix` |
| Resume after feedback | persisted `generated-task.md`, `last_review`, operation ID |
| Retry/stale detection | `awh_pipeline.py` CAS + recovery workflow |
| Worktree isolation | feature branch/workspace contract used by Agent 2 |
| TDD/verification | deterministic Rust fmt/check/test/clippy gate |
| Provider/agent abstraction | AWH OpenHands agent wrapper + agent profile roadmap |
| Human safety boundary | policy engine + explicit workflow permissions |

## Dispatcher contract

`scripts/awh_dispatcher.py` is deliberately small and deterministic. It reads the checkpoint and emits only a small event payload. It never edits the checkpoint and never contains LLM-generated task text.

Allowed dispatches:

- `IDLE`, `COMPLETED` -> `awh.start`
- `BUILDING` -> `awh.build`
- `PR_OPEN`, `REVIEWING` -> `awh.review`
- `FIXING` -> `awh.fix`
- `MERGING` -> `awh.review-complete` with `APPROVE`
- `BLOCKED` -> `awh.recover`
- `PLANNING`, `RECOVERING` -> wait; the active stage owns continuation

## Safety invariants

1. The dispatcher never resets `BLOCKED` to `IDLE`.
2. Large task documents never travel through GitHub Actions outputs or event payloads.
3. Every build/fix/review event carries the active operation identity.
4. Agent workflows validate the event against the authoritative checkpoint before mutation.
5. Recovery uses a stale check, CAS claim, and lease before resuming work.
6. A stale repository-dispatch recovery request is a safe no-op.
7. Review is pinned to the exact PR head SHA; a changed head invalidates the review.
8. Review rounds are bounded; repeated failures become `BLOCKED` instead of looping forever.
9. Deterministic verification remains mandatory before PR progression.
10. The dispatcher is an orchestrator, not an authority: agents and checkpoint scripts own state mutations.

## Why AWH differs

The reference project uses GitHub issue labels as its shared state machine. AWH instead needs durable cross-agent workspace state and recovery semantics, so labels are intentionally not the source of truth. This prevents a label edit, duplicate cron tick, stale webhook, or agent crash from silently creating a second operation.

See `docs/PROJECT_ROADMAP.md`, `docs/FEATURES.md`, and `docs/security.md` for the broader AWH product contracts.
