# AWE-015 / #36 — Agent-Grade Editing Test Suite

## Task
Create behavior-focused tests proving the editing safety invariants.

## Forensic baseline
AWH has a large existing test suite, but no production edit executor existed in the forensic snapshot. Do not measure success by test count.

## Required coverage
- unit tests for every primitive
- real filesystem integration
- stale/conflict tests
- atomicity and rollback
- multi-file no-partial-mutation
- UTF-8/CRLF/LF/EOF/empty-file boundaries
- traversal/symlink security
- property-based operation invariants
- failure injection
- concurrent stale-read scenario
- MCP and CLI adapters

## Hard invariants
`conflict/invalid → zero mutation`

`apply → verify → rollback → original bytes`

`prepare failure → every file unchanged`

## Rule
Prefer real temp directories and real process boundaries where the invariant depends on filesystem behavior; mocks are supplemental, not the only proof.
