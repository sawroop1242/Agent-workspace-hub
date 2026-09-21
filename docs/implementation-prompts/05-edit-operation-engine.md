# Prompt 05 — Canonical Edit Operation Engine (AWE-002..005)

## Mission
Implement production editing operations through one canonical EditService: contextual replacement, line-range insert/delete, multi-operation filesystem patch, and unified-diff application. This prompt is standalone.

## Required behavior
- All operations enter one canonical transaction/service path.
- Resolve and validate every affected path before mutation.
- Enforce workspace containment and existing symlink/path protections.
- Contextual replacement must match intended context unambiguously and reject stale/ambiguous input.
- Line-range insert/delete must define boundaries precisely and handle EOF/newline conventions deterministically.
- Multi-operation patch must prepare all operations before commit and reject the entire plan on preparation failure.
- Unified diff parsing/application must use the canonical transaction model and reject malformed or stale hunks structurally.
- Never fall back to blind whole-file replacement when a structured operation fails.
- Preserve exact bytes and newline semantics.

## Forensics
Inspect edit service, filesystem helpers, path security, atomic writes, and tests. Merge the old AWE-004 coding checklist, implementation plan, and verification checklist into this one contract; do not keep separate specifications.

## Tests
Use real temporary workspaces. Cover repeated/missing matches, EOF, Unicode/Devanagari/emoji, LF/CRLF, no-final-newline, multi-file preparation failure, malformed diffs, stale content, traversal/symlink attempts, and zero mutation on rejected plans.

## Verification
Run full Rust gates and targeted edit integration tests. Inspect the final diff for duplicate executors.

## Non-goals
No MCP/CLI transport, model routing, agent orchestration, worktree manager, or separate editor.

## Final report
Describe operation semantics, canonical execution path, preparation/commit boundary, tests, and limitations.
