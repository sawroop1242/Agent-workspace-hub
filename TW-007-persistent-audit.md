# TW-007 — Persistent Structured Trust-Wedge Audit

## Master implementation prompt
Extend the existing audit choke point into durable, structured, restart-safe provenance. This task is standalone.

Inspect src/services/audit.rs, src/mcp/audit.rs, src/mcp/observability.rs, authorization, edit, snapshot, and rollback integration points.

Minimum applicable fields: event ID, timestamp, agent ID, session ID, workspace ID, action, tool/command, authorization decision, result, edit ID, snapshot ID, path, before/after hashes, duration, and structured error code.

Cover agent/session lifecycle, authorization allow/deny, edit start/success/failure, snapshot creation, rollback start/success/failure, verification and conflict outcomes.

Use a durable append-oriented logical source of truth. An in-memory ring may remain as a cache. Define safe retention/rotation, corruption handling, and audit-unavailable behavior. Never log file contents or secrets.

Support query/filter by agent, session, workspace, edit, and correlation IDs.

Tests: persistence, restart, identity, allow/deny, edit success/failure, rollback linkage, hashes, redaction, filters, corruption, and full lifecycle reconstruction.

Do not scatter audit writes across transports or create a second audit abstraction.
