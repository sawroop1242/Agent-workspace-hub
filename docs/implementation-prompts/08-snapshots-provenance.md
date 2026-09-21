# Prompt 08 — File Snapshots + Provenance (AWE-012 / TW-005)

## Mission
Implement one durable file-recovery snapshot/provenance subsystem for exact edit recovery and traceability. This prompt is standalone.

## Required behavior
- Snapshot exact file bytes/state at the correct edit boundary.
- Distinguish file-recovery snapshots from context-engine snapshots.
- Give snapshots stable IDs and correlate workspace, agent/session, edit, and affected paths.
- Persist enough metadata to validate a snapshot before restoration.
- Make storage restart-safe and corruption-detectable.
- Never treat a snapshot as authorization.
- Avoid unnecessary secrets or unrelated workspace contents.
- Make retention/cleanup explicit and never silently delete data still required for eligible recovery.

## Forensics
Inspect snapshot/context code, persistence, edit service, file-state helpers, identity types, and storage conventions. Reuse durable storage where appropriate.

## Tests
Cover create/read/list/inspect, exact bytes, zero-byte files, Unicode/newlines, multi-file snapshots, restart, corruption, missing data, duplicate IDs, path containment, and provenance correlation.

## Non-goals
No whole-workspace backup product, generic event store, or independent editor.

## Final report
Document snapshot format, lifecycle, provenance links, recovery guarantees, tests, and storage limitations.
