# AWE-007 / #28 — Stale-State and Conflict Detection

## Task
Make expected-state mismatch a hard conflict across every canonical edit operation.

## Forensic baseline
The existing `workspace.write_file` can overwrite newer content silently. `ExpectedState` is currently unused by production execution.

## Invariant
`expected != current → conflict → zero mutation`.

## Requirements
- hash comparison
- context comparison
- optional size/line-count checks
- deterministic expected/actual conflict payload
- stale line-operation protection
- reread/resubmit recovery guidance
- conflict-aware rollback

## Tests
Simulate a read, external modification, then edit. Prove bytes remain unchanged. Include concurrent task/process scenarios where practical.

## Rule
Never solve a conflict by silently replacing the caller's expected state with current state.
