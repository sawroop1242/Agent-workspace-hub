# AWE-004 — Verification Checklist

## Static checks

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`

## Unit/integration tests

- [ ] `cargo test edit`
- [ ] `cargo test edit -- --nocapture`
- [ ] `cargo test`

## Functional verification

- [ ] Single replace works.
- [ ] Single insert works.
- [ ] Single delete works.
- [ ] Mixed same-file patch works.
- [ ] Multi-file patch works.
- [ ] Same-file operations execute in transaction order.
- [ ] Intermediate same-file states are never written.
- [ ] Each affected file receives one final mutation during normal commit.

## Preparation safety

- [ ] Empty transaction rejected.
- [ ] Invalid operation rejected before mutation.
- [ ] Invalid path rejected before mutation.
- [ ] Invalid expected state rejected before mutation.
- [ ] Valid/invalid/valid multi-file scenario leaves all files unchanged.

## Conflict safety

- [ ] Hash mismatch returns conflict.
- [ ] Size mismatch returns conflict.
- [ ] Line-count mismatch returns conflict.
- [ ] External stale modification is detected.
- [ ] Same-file expected-state transitions work correctly.

## Filesystem security

- [ ] Absolute paths rejected.
- [ ] `../` traversal rejected.
- [ ] Nested traversal rejected.
- [ ] Symlink escape rejected.
- [ ] Existing file-size limits remain enforced.

## Text handling

- [ ] UTF-8 text.
- [ ] Hindi/Devanagari text.
- [ ] Emoji.
- [ ] LF files.
- [ ] CRLF files.
- [ ] Trailing newline.
- [ ] No trailing newline.

## Regression review

- [ ] Existing AWE-001 tests remain green.
- [ ] Existing AWE-002 tests remain green.
- [ ] Existing AWE-003 tests remain green.
- [ ] No unrelated tests removed.
- [ ] No lint suppression added without justification.
- [ ] No security check bypass added.
- [ ] No full-file rewrite API introduced.

## Diff review

- [ ] `git status` reviewed.
- [ ] `git diff --stat` reviewed.
- [ ] `git diff` reviewed.
- [ ] Only intended files changed.
- [ ] No generated artifacts committed.
- [ ] Commit is focused.

## Acceptance invariant

The following must always hold for preparation failures:

```text
prepare(patch) fails
        ↓
filesystem state before == filesystem state after
```

AWE-004 does not claim crash-safe rollback after commit begins. That responsibility belongs to AWE-006 (#27).