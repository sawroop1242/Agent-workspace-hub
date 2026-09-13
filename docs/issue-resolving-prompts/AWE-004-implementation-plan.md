# AWE-004 Implementation Plan

## Preconditions
- AWE-001/#22, AWE-002/#23, AWE-003/#24 are complete and verified.
- Read the current repository; do not assume the prompt matches code.

## Target structure
Prefer a small canonical service boundary such as:

```text
EditService
├── validate transaction
├── load/inspect files
├── prepare operations in memory
├── commit prepared mutations
└── verify results
```

Use `PreparedFileMutation`-style internal state when useful:
- path
- original content
- before `FileState`
- final content
- after `FileState`

## Algorithm
1. Validate transaction shape.
2. Resolve every path through the canonical containment boundary.
3. Load every affected file before writing anything.
4. Capture current state.
5. Validate expected hash/context.
6. Transform content in memory using the AWE-002/AWE-003 helpers.
7. Validate all resulting states.
8. Commit prepared mutations.
9. Verify actual filesystem state.
10. Return a structured result.

## Important distinction
Preparation atomicity is required here. Crash-safe semantic rollback is a later issue. Do not over-engineer AWE-004 into a snapshot system.

## Tests
- one operation
- multiple same-file operations
- multiple files
- invalid operation after valid operation
- stale state
- path rejection
- UTF-8/newline/EOF
- real filesystem no-partial-mutation test

## Verification
```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
