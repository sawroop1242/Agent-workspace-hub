# Prompt 14 — Filesystem TOCTOU + Mutation Coordination (FS-001)

## Mission
Harden the canonical filesystem mutation boundary against practical final-component TOCTOU races and coordinate concurrent mutations without weakening containment, symlink protection, atomicity, or expected-state semantics. This prompt is standalone.

## Required behavior
- Revalidate target identity/state at the final mutation boundary as far as the platform permits.
- Coordinate conflicting mutations with the smallest correct synchronization scope.
- Never claim universal race freedom when the OS/filesystem cannot provide it.
- Preserve workspace containment, symlink protection, stale-state conflict detection, and atomic commit semantics.
- Fail closed on ambiguous target identity.

## Forensics
Inspect filesystem mutation helpers, locks, expected-state checks, atomic writes, path canonicalization, and concurrent tests. Identify the actual race window before choosing synchronization.

## Tests
Use real temporary files and concurrent tasks/processes where practical. Cover replacement races, deletion/recreation, symlink substitution, same-file concurrent edits, multi-file transactions, and cleanup.

## Non-goals
No distributed locking service, container sandbox, or unrelated filesystem rewrite.

## Final report
State the race model, synchronization scope, test evidence, and remaining platform limits.
