# Prompt 01 — AWH Init + Runtime Contracts (TW-001)

## Mission
Implement a durable `awh init` and runtime bootstrap contract. This prompt is standalone.

## Required behavior
- Validate the selected workspace root and establish one stable workspace identity.
- Create the minimum AWH state required for runtime use.
- Make repeated initialization idempotent and preserve existing identity/configuration/state.
- Detect malformed or corrupt bootstrap state instead of silently replacing it.
- Establish typed Workspace, Agent, Session, Task, Edit, Snapshot, and Audit/Event identities when suitable types do not already exist.
- Never activate an agent, grant unrestricted capabilities, or expose remote control merely because init was requested.
- Keep bootstrap state restart-safe and safe under concurrent invocation as far as the persistence layer permits.

## Forensics
Inspect CLI, config/state, persistence, workspace/path security, identity types, tests, and current `awh init` behavior before editing.

## Tests
Use real temporary directories. Cover fresh init, repeated init, preserved state, malformed/partial state, invalid roots, concurrent init, and CLI output/exit codes.

## Verification
Run formatting, check, tests, Clippy, targeted CLI tests, and `git diff --check`.

## Non-goals
No agent orchestration, MCP routing, editing, snapshots, rollback, persistent audit, model routing, or remote execution.

## Final report
State the baseline, changes, persistence/idempotence behavior, tests, verification, files changed, and limitations.
