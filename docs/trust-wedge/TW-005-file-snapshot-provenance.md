# TW-005 — Durable File Snapshots + Provenance

## Master implementation prompt
Implement one durable file snapshot/provenance subsystem. This task is standalone.

Inspect src/services/snapshot.rs and any context snapshot implementation first. Context-engine snapshots and file-recovery snapshots must remain explicitly separate.

Capture pre-edit state with SnapshotId, exact prior bytes, content hash, required file metadata, and edit linkage. Provenance should include edit ID, agent ID, session ID, workspace ID, path, operation, timestamp, before hash, after hash, and snapshot ID when available.

Use workspace-local durable storage, atomic writes, corruption detection, safe locking, restart-safe lookup, and explicit retention semantics. Normal logs must not expose file contents.

Tests must cover byte/hash capture, edit linkage, provenance round trip, restart persistence, corruption handling, exact recovery bytes, redaction, and failed-edit/no-false-success behavior.

Run formatting, compilation, all tests, and targeted persistence tests.

Do not create separate MCP/CLI/TUI snapshot implementations and do not silently reuse context snapshots as file backups.
