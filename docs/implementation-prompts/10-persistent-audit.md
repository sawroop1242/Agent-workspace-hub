# Prompt 10 — Structured Persistent Audit (AWE-013 / TW-007)

## Mission
Implement one durable, structured, correlated audit subsystem for consequential AWH operations. This prompt is standalone.

## Required behavior
- Record successful, denied, failed, and conflict outcomes at the authoritative service boundary.
- Correlate events with workspace, agent, session, task, edit, snapshot, policy decision, and operation IDs when available.
- Persist events restart-safely with deterministic ordering metadata.
- Detect corruption safely; never fabricate history.
- Keep secrets and full sensitive file contents out of audit records.
- Make writes durable according to the repository's stated persistence guarantees.
- Preserve append-oriented history; normal operation must not silently rewrite prior events.
- Provide diagnostic querying without exposing unrelated workspace data.

## Forensics
Inspect audit/event code, persistence, edit, MCP, CLI, identity, and error paths. Identify the canonical write/choke point and eliminate duplicate writers.

## Tests
Cover success/deny/failure/conflict, restart, ordering, duplicate/collision IDs, corruption, concurrent writes, sensitive-data filtering, and cross-service correlation.

## Non-goals
No analytics platform, remote telemetry service, or second event store.

## Final report
Document event schema, durability, correlation, security filtering, tests, and limitations.
