# Prompt 04 — Canonical Edit Transaction Model (AWE-001 / #22)

## Mission
Maintain exactly one transport-independent edit transaction vocabulary for AWH. This prompt is standalone.

## Required model
Represent, using existing canonical types where possible:
- stable EditId;
- optional agent/session/workspace identity;
- workspace-relative target resource;
- Replace, Insert, DeleteRange, Patch, and ApplyDiff operations;
- ExpectedState (hash, context/location, size, line count as applicable);
- observed FileState;
- lifecycle status and structured errors;
- optional snapshot/provenance/audit/policy correlation IDs.

Expected-state semantics must be deterministic. Hash means exact content hash; size/line count describe observed state; context must be tied to the intended edit location rather than a decorative global substring.

## Constraints
The model must not depend on MCP, HTTP, CLI, TUI, database, Git, or filesystem executor implementations. Structural validation is pure and never mutates files.

## Forensics
Inspect `src/services/edit.rs`, identity/error types, serialization, callers, tests, and duplicate edit models. Extend the canonical model instead of creating competing models.

## Tests
Validate every operation, invalid input, expected-state match/conflict, identity, status, structured errors, and Serde round trips.

## Non-goals
No file mutation, diff executor, rollback, snapshot store, audit store, MCP tool, CLI command, or authorization engine.

## Final report
State canonical types, semantics, compatibility impact, tests, and verification.
