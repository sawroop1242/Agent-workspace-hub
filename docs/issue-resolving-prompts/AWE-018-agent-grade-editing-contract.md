# AWE-018 / #39 — Agent-Grade Editing Contract

## Task
Document the stable editing contract after implementation and keep all documentation honest during the build.

## Must document
- transaction model
- MCP schemas
- CLI commands
- expected-state/conflict semantics
- atomicity limits
- snapshots/provenance
- rollback
- capability/policy requirements
- error taxonomy
- caller/session/workspace identity
- minimal-patch recovery examples

## Forensic constraint
Explicitly distinguish file snapshots from context-engine snapshots. Do not describe roadmap commands or target CLI entries as shipped until source/tests prove them.

## Acceptance
- examples match real schemas/help
- security guarantees state exact limits
- AWE-017 results are linked
- MCP and CLI contracts agree
- no claim exceeds implementation evidence
