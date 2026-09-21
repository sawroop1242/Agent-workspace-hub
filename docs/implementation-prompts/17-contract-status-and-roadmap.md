# Prompt 17 — Agent-Grade Editing Contract + Status/Roadmap (AWE-018 / AWE-019)

## Mission
Produce authoritative implementation-backed documentation for the AWH editing contract and current status/strategic analysis. This prompt is standalone.

## Required behavior
- Inspect current code, tests, CI, issue/PR evidence available in the repository, and existing documentation before making claims.
- Document only behavior that is implemented and verified.
- Separate implemented, partial, scaffolded, planned, and unverified behavior.
- Describe the canonical contract: identity, authorization, expected state, mutation, verification, snapshots/provenance, rollback, audit, MCP, and CLI.
- Record known security/architecture gaps with concrete evidence.
- Do not create duplicate implementation specifications; this is documentation/status work.
- Preserve the distinction between AWH runtime responsibilities and external development/evaluation agents.

## Verification
Every claim of completion should be backed by source, tests, CI, or recorded verification. Roadmap intent is not completion evidence.

## Non-goals
No source refactor, speculative feature implementation, new issue chain, or agent orchestration.

## Final report
Summarize evidence reviewed, status categories, remaining gaps, documentation changes, and claims that could not be verified.
