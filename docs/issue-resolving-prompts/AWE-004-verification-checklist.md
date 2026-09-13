# AWE-004 Verification Checklist

## Static
- [ ] `EditTransaction` is the canonical input.
- [ ] No duplicate operation algorithms.
- [ ] No writes occur during preparation.
- [ ] All affected paths are resolved before commit.
- [ ] Expected state is checked before mutation.
- [ ] Result contains edit ID and before/after states.

## Behavioral
- [ ] Valid same-file sequence produces expected content.
- [ ] Valid multi-file patch produces expected contents.
- [ ] Invalid operation causes zero mutation everywhere.
- [ ] Stale hash/context causes zero mutation everywhere.
- [ ] Traversal/symlink escape is rejected.
- [ ] UTF-8/newline/EOF behavior is correct.

## Commands
```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
git status
git diff
```

## Explicit non-claims
AWE-004 passing does **not** mean file snapshots, persistent audit, explicit rollback, MCP editing tools, CLI editing commands, or per-agent capability enforcement are implemented. Those are later AWE issues.
