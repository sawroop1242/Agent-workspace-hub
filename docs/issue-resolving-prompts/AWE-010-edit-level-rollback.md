# AWE-010 / #31 — Edit-Level Rollback

## Task
Expose explicit rollback for one completed edit transaction.

## Forensic baseline
No file rollback exists. Context-engine snapshots are not file recovery.

## Contract
`filesystem.rollback(edit_id)` must:
1. locate the exact prior state;
2. verify the current state still matches the expected post-edit state;
3. refuse if newer changes exist;
4. restore exact prior bytes;
5. verify restoration;
6. emit a linked audit/provenance event.

## Determinism
Repeated rollback must return a stable state such as `already_rolled_back`; never silently apply an older snapshot over newer work.

## Tests
Apply → rollback → byte-identical original. Apply → modify → rollback must produce a conflict and preserve the newer modification.
