# AWE-009 / #30 — MCP Agent-Grade Editing

## Task
Expose the canonical EditService through MCP.

## Required tools
`filesystem.patch`, `filesystem.replace`, `filesystem.insert`, `filesystem.delete_range`, `filesystem.apply_diff`.

## Forensic baseline
AWH has a strong MCP dispatcher and schema validator, but zero `filesystem.*` editing tools and zero production callers of `EditTransaction`.

## Requirements
- complete JSON schemas
- schema validation before service dispatch
- structured conflict/verification/policy errors
- edit ID and before/after state in results
- snapshot/provenance references when available
- explicit risk/capability metadata
- no MCP-local editing algorithm

## Security
Do not let new tools inherit unsafe default authorization accidentally. They must pass the authoritative policy/capability boundary.

## Tests
Real `tools/list` and `tools/call` integration tests, including invalid arguments, conflict, policy denial, and successful minimal patch.
