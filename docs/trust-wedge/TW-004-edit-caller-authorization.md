# TW-004 — EditService Caller Identity + Authorization

## Master implementation prompt
Integrate the existing canonical EditService with caller identity and pre-mutation authorization. This issue is standalone.

Read src/services/edit.rs completely before modifying it. Inspect authorization, MCP execution, CLI adapters, filesystem security, and current tests.

Consequential edits should carry agent ID, session ID, workspace ID, task ID when available, and edit ID, reusing existing identity types.

Required flow: caller context -> authorization -> prepare -> expected-state validation -> mutation -> verification -> result.

Invariants: unauthorized edit causes zero mutation; wrong workspace/scope causes zero mutation; stale/conflicting edit causes zero mutation; successful edit has stable edit ID; caller identity reaches the edit result/provenance hook; MCP and CLI call the same EditService; no full-file overwrite fallback.

Use real temporary files for authorization denial, scope denial, stale conflict, successful identity, and adapter equivalence.

Run formatting, compilation, all tests, and targeted filesystem integration tests.

Do not rewrite EditService, create a second editor, duplicate authorization, implement blind overwrite, or add a separate audit/snapshot system.
