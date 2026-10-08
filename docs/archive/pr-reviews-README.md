# Agent 3 PR Reviews

This directory stores durable Agent 3 analysis for PRs that are **not** the active Agent 2 pipeline PR.

## Purpose

Analysis-only reviews are repository knowledge. They do not advance, recover, or otherwise mutate `.openhands/state.json` or the main Agent pipeline.

Each report should contain:

- reviewed PR and exact head SHA
- scope and changed behavior
- concrete findings with evidence
- security, concurrency, API, error-handling, testing, and maintainability analysis
- documentation/architecture conflicts
- an `Agent 2 Working Prompt` with ordered implementation steps, constraints, acceptance criteria, and verification commands
- exactly one final `VERDICT:` line

## Agent 1 contract

Agent 1 must read relevant reports in this directory before planning related work. Reports are evidence and implementation guidance, not authority over current source code, executable tests, security invariants, or the checkpoint.

If a report conflicts with current source or tests, Agent 1 must record the conflict and follow the repository's documented evidence precedence.

## Agent 2 contract

When Agent 2 is assigned work related to a report, it must read the report and use its `Agent 2 Working Prompt` section as implementation guidance. Agent 2 must still reconcile the report with current source, tests, CI, and the matching issue-resolving prompt.
