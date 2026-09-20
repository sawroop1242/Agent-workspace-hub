# TW-006 — Conflict-Aware Rollback + Recovery

## Master implementation prompt
Implement a first-class safe rollback operation using existing edit and file-snapshot state. This issue is standalone.

Rollback must resolve the edit, resolve recoverable pre-state, validate current state, refuse unsafe overwrite, restore exact prior bytes when safe, verify restoration, return a structured result, and preserve correlation/provenance metadata.

Missing edit or snapshot must return structured errors. Current-state mismatch must be a conflict, never a blind overwrite. External changes must remain intact after rejected rollback. Rollback needs its own correlation identity.

Expose or reconcile awh fs rollback <edit-id>. Keep algorithms in services, not main.rs.

Tests must use real temporary files and prove successful byte-identical restore, verification, missing state, external-change conflict, non-destructive conflict handling, restart behavior, and deterministic repeated calls.

Do not use blind file-copy restore, silently destroy external changes, or create a second snapshot system.
