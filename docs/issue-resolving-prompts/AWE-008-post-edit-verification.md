# AWE-008 / #29 — Post-Edit Verification

## Task
Verify the actual filesystem result before an agent-grade edit is reported successful.

## Forensic baseline
No edit executor currently verifies resulting state; atomic persistence alone is insufficient.

## Minimum verification
- expected existence/deletion state
- readable result where text applies
- intended mutation occurred
- actual after hash/size/line count
- structured verification failure
- rollback integration when verification fails

## Architecture
Keep project/build/language checks as optional bounded hooks. The editor must not become a build system.

## Tests
Include success, failed verification, failure injection, and rollback-after-verification-failure.
