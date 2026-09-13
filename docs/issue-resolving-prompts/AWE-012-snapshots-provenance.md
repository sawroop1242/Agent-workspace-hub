# AWE-012 / #33 — File Snapshots and Provenance

## Task
Build the persistent file snapshot/provenance substrate and connect it to edit transactions.

## Forensic baseline
File snapshots are missing. `context::snapshot` snapshots context-item state only and must remain conceptually separate.

## Requirements
- capture pre-edit bytes/state and hash
- stable snapshot ID linked to edit ID
- persistent `.agent` storage with atomic writes and locking
- exact prior bytes for rollback
- provenance: path, operation, agent, session, workspace, timestamp, before/after hash, snapshot ID
- corruption/failure is fail-safe
- no content leakage into normal audit logs

## Design
One canonical snapshot service shared by MCP/CLI/TUI. Do not build separate per-plane stores.

## Tests
Restart persistence, byte-identical restore, corrupted snapshot handling, multi-file snapshots, and edit→snapshot linkage.
