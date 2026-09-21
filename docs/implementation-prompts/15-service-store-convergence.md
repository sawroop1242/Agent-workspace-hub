# Prompt 15 — MCP/Service Store Convergence (ARCH-001)

## Mission
Unify divergent MCP and canonical service/core stores so domain behavior and persistence have one source of truth. This prompt is standalone.

## Required behavior
- Identify duplicate stores/models and establish canonical service/core ownership.
- Make MCP and adapters call canonical services rather than maintain parallel state.
- Preserve serialization and compatibility where practical.
- Prevent identity, capability/policy, workspace, edit, snapshot, and audit state from diverging because two stores are updated differently.
- Remove dead duplicate paths only after proving callers are migrated.
- Keep adapters thin.

## Forensics
Map every store/model read/write and caller before changing code. Inspect tests and persistence formats.

## Tests
Cover equivalent operations through service and MCP boundaries, restart persistence, concurrency, migration/compatibility, and prevention of divergent writes.

## Non-goals
No new database platform, event bus, generic ORM, or unrelated architecture rewrite.

## Final report
Include the before/after ownership map, migrated callers, compatibility behavior, tests, and remaining duplication.
