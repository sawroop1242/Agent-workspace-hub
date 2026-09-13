# AWE-013 / #34 — Structured Persistent Audit

## Task
Extend the existing audit choke point into persistent, correlated mutation auditing.

## Forensic baseline
Current audit is a process-local 1,000-entry ring with good token redaction but no agent/session/edit identity and no restart persistence.

## Event contract
`timestamp, event_id, edit_id, agent_id, session_id, workspace_id, path, operation, policy_decision, before_hash, after_hash, snapshot_id, result, duration`.

## Requirements
- start/authorization/completion/failure/rollback events
- append-only durable sink with bounded retention/rotation
- ring remains hot cache
- centralized redaction
- no file contents/secrets in ordinary events
- MCP and Control API can correlate events
- define behavior if audit storage is unavailable

## Tests
Restart persistence, complete edit lifecycle reconstruction, failed edit, denied edit, rollback, redaction, and bounded retention.
