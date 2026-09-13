# AWE-017 / #38 — Complete Agent-Grade Editing Acceptance

## Task
Create the canonical real-workspace end-to-end acceptance test.

## Workflow
```text
Agent
→ caller/session identity
→ read/locate
→ prepare patch
→ capability/policy
→ expected-state check
→ snapshot
→ apply
→ verify
→ provenance/audit
→ result
→ rollback
→ verify restoration
```

## Must prove
- policy denial = zero mutation
- stale state = conflict
- snapshot links to edit
- provenance identifies caller/session/workspace and hashes
- audit survives restart
- verification failure recovers safely
- rollback restores exact original bytes
- MCP and CLI call the same service
- real filesystem, not mock-only

## Release rule
Do not call agent-grade editing implemented until this acceptance workflow passes.
