# Prompt 12 — CLI Agent-Grade Editing (AWE-014)

## Mission
Expose canonical editing and rollback services through a production CLI without creating CLI-specific mutation semantics. This prompt is standalone.

## Required behavior
- Commands validate arguments then call canonical services.
- Preserve caller/session/workspace identity and policy decisions.
- Expose stable success, conflict, denial, verification, and failure states through output/exit behavior.
- Support operations and rollback actually implemented by the current service layer.
- Never bypass authorization, expected-state, snapshot, verification, or audit boundaries.
- Avoid secrets and full sensitive file contents in output.

## Forensics
Inspect `src/main.rs`, CLI definitions, service wiring, serialization, conventions, and tests.

## Tests
Cover valid/invalid arguments, denial, stale state, multi-file edits, rollback conflicts, non-zero exits, machine-readable output where supported, and workspace isolation.

## Non-goals
No second editor, TUI redesign, model execution, or remote agent runtime.

## Final report
Describe commands, service boundaries, output/exit contracts, tests, and compatibility.
