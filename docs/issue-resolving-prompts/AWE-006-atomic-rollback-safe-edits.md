# AWE-006 / #27 — Atomic and Rollback-Safe Edits

## Task
Extend the AWE-004 transaction into a safe commit pipeline with rollback-on-failure.

## Forensic baseline
Some writes are atomically persisted, but there is no semantic transaction executor or file rollback.

## Pipeline
`validate → prepare → snapshot boundary → apply → verify → commit`

Failure: `apply/verify failure → rollback already-applied mutations → report failure`.

## Requirements
- single- and multi-file atomicity
- stable edit ID
- temporary artifact cleanup
- verification failure triggers recovery
- rollback is itself conflict-aware
- document exact crash guarantees; do not claim ACID semantics

## Tests
Failure in a later file must restore all earlier files. Include write/verification failure injection and process-interruption tests where practical.

## Boundary
File snapshots and durable provenance are AWE-012; explicit user-requested rollback is AWE-010. Do not create parallel stores.
