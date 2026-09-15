# Issue-Resolving Prompts

This directory contains implementation-grade prompts for resolving AWH issues.
They are engineering specifications, not general brainstorming notes.

## How Agent 1 uses this directory

Agent 1 must:

1. inspect `.openhands/backlog.json` and `.openhands/state.json`;
2. inspect the complete `docs/` tree relevant to the selected feature;
3. inspect this directory for a matching issue/feature prompt;
4. compare the prompt with the current source, tests, Git history, PR state, and CI;
5. select one ready feature whose dependencies are satisfied;
6. copy the applicable requirements into `.openhands/generated-task.md` with exact
   repository paths, acceptance criteria, tests, and non-goals.

A prompt is never authoritative over the current source. If a prompt is stale or
conflicts with the current architecture, Agent 1 must record the conflict and
use verified repository behavior as the source of truth.

## Prompt naming

Prefer:

```text
AWE-<number>-<short-description>.md
```

for issue-specific implementation prompts, while architectural prompts may use
an explicit category such as `ARCH-001-...` or `AGENT-001-...`.

## Required prompt structure

A high-quality issue-resolving prompt should contain:

- issue/feature identity;
- mission and scope;
- repository preflight;
- evidence hierarchy;
- current behavior;
- desired behavior;
- exact files/subsystems to inspect;
- implementation requirements;
- security invariants;
- compatibility constraints;
- tests and verification commands;
- acceptance criteria;
- explicit non-goals;
- change-scope rules;
- failure/rollback expectations where applicable.

## Agent 2 rules

Agent 2 receives the generated task plus any review/fix feedback. It must read
the applicable prompt and relevant documentation before editing. It must preserve
existing architecture and security invariants, add focused regression tests for
behavior changes, and run the deterministic Rust verification commands required
by the workflow.

## Agent 3 rules

Agent 3 independently reads the relevant prompt and documentation before review.
It must verify that the implementation satisfies the issue's actual requirements,
not merely that the code compiles or the PR description sounds complete.

## Documentation access

Agents have read access to the complete repository `docs/` tree through their
workspace. `docs/` is treated as a primary engineering context source, but claims
must still be reconciled against current source code, tests, Git history, and CI.

Do not place secrets, credentials, or private CI data in these prompts.
