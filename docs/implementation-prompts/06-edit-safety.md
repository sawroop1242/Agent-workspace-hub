# Prompt 06 — Edit Safety: Atomicity, Conflicts, Verification (AWE-006..008)

## Mission
Make canonical edit execution safety-critical through atomic/rollback-safe application, stale-state conflict detection, and actual post-edit verification. This prompt is standalone.

## Required behavior
- Validate expected state immediately before mutation.
- Refuse stale/conflicting state rather than overwriting newer content.
- Prepare affected targets before commit for multi-file operations.
- Use existing atomic filesystem primitives and deterministic temporary cleanup.
- If mutation fails, recover to the safest provable state.
- Verify actual resulting filesystem bytes/state; a successful write alone is never success.
- Distinguish validation, conflict, apply, verification, and recovery failures.
- Preserve exact bytes/newlines and workspace containment.
- Never claim perfect TOCTOU protection unless implementation/tests establish it.

## Forensics
Inspect edit, filesystem, expected-state, atomic-write, error, and concurrency code. Reuse canonical helpers; do not create a second verification or recovery mechanism.

## Tests
Cover single/multi-file edits, stale state, concurrent changes, injected apply/verification failures, recovery, exact-byte verification, Unicode/newlines, and zero mutation before conflict detection.

## Non-goals
No public MCP/CLI surface, persistent snapshot history, persistent audit subsystem, or generic distributed lock service.

## Final report
Explain mutation boundary, conflict rule, recovery semantics, verification evidence, tests, and race limitations.
