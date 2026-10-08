---
name: commit-hygiene
description: Keep commits and pull requests clean before pushing shared history
version: 0.1.0
---

# commit-hygiene

## When to use

Use this skill before creating a commit or pull request that other people will review, or when a repository requires strict validation gates.

## Workflow

1. Run the repository's formatting check (for example `cargo fmt --all -- --check`).
2. Run the linter with warnings denied (for example `cargo clippy --all-targets -- -D warnings`).
3. Run the full test suite.
4. Review `git status` and stage only files that belong to the change.
5. Write a commit message with a concise summary line and a body that explains why the change is needed.
6. Never commit secrets, credentials, or generated artifacts.

## Rules

- Do not silence failing checks; fix the underlying cause.
- Keep each commit focused on one logical change.
- If a check fails after committing, fix forward with a follow-up commit rather than rewriting pushed history.
