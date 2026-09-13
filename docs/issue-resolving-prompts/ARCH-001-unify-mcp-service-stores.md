# ARCH-001 — Unify MCP and Service/Core Stores

## Task
Eliminate divergent business-logic/store implementations across MCP, CLI, TUI, and Control API.

## Forensic baseline
MCP currently has its own filesystem, memory, task, and connector stores while other planes use different implementations. Memory and task schemas/locking differ.

## Goal
One canonical service/store owner per domain.

## Requirements
- migrate MCP to canonical services
- choose one authoritative memory format with migration
- choose one task model/status vocabulary
- centralize filesystem security and mutation behavior
- remove/deprecate orphaned legacy stores
- add architecture/conformance tests preventing new MCP-local duplicates

## Constraint
Do not block AWE-001..004 on a broad migration. Establish the edit service boundary first, then migrate surrounding stores incrementally.
