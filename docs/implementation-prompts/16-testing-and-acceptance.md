# Prompt 16 — Agent-Grade Editing Test + Trust-Wedge Acceptance (AWE-015 / AWE-017 / TW-008)

## Mission
Build executable acceptance evidence for the current AWH editing/trust boundary. This prompt is standalone and tests whatever is actually present on the current `rust` branch.

## Required acceptance properties
- caller identity is resolved correctly;
- authorization is enforced before mutation;
- workspace/path containment holds;
- expected-state conflicts prevent mutation;
- canonical edit operations work on real files;
- snapshots/provenance are durable where implemented;
- verification reflects actual filesystem state;
- rollback is conflict-aware;
- audit records consequential outcomes;
- MCP and CLI preserve the same service semantics;
- multiple agents cannot cross workspace/worktree boundaries;
- denied consequential operations produce zero mutation.

## Test strategy
Use real temporary workspaces and real service boundaries. Add unit tests for pure logic, integration tests for services, transport tests for MCP/CLI, concurrency/failure-injection tests, and one end-to-end workflow. Include LF/CRLF, no-final-newline, Unicode/Devanagari/emoji, empty files, multi-file edits, stale state, symlink/path traversal, authorization denial, restart persistence, and failure recovery.

## Evidence rules
Do not replace missing production implementation with mocks. A test passes only when the actual boundary under test enforces the contract. Separate implementation defects from test-harness defects.

## Verification
Run formatting, check, tests, Clippy, repository security/audit checks, and real transport validation where applicable. Record exact commands/results.

## Non-goals
No new product feature solely to make the suite pass; no test-only security bypass.

## Final report
Provide an acceptance matrix with behavior, test, result, and evidence location; list failures honestly and identify unproven guarantees.
