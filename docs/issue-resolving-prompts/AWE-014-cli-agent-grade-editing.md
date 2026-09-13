# AWE-014 / #35 — CLI Agent-Grade Editing

## Task
Expose the canonical edit service through the final `awh fs` CLI contract.

## Commands
- `awh fs patch`
- `awh fs replace`
- `awh fs insert`
- `awh fs delete-range`
- `awh fs apply-diff`
- `awh fs rollback`

## Requirements
- thin adapter; no editing algorithms in `main.rs`
- human output by default and stable machine-readable output
- non-zero exit on conflict, policy denial, verification failure, rollback failure
- show edit ID and useful state/conflict information
- semantics identical to MCP

## Forensic constraint
The final CLI list is an architecture contract, not proof of implementation. Add only commands that are actually backed by working services.

## Tests
Real command/integration tests, help/discovery tests, policy denial, stale conflict, success, and rollback.
