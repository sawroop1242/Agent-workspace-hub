# Agent 3 v2 — Hybrid Review Architecture

Agent 3 v2 is a hybrid reviewer. **Deterministic checks establish facts; the LLM performs contextual reasoning; a final evidence pass prevents unsupported findings from becoming actionable.**

## Review pipeline

1. Validate PR identity, base branch, open state and exact review SHA.
2. Run `scripts/reviewer_deterministic.py` against PR metadata and textual diff.
3. Never execute or checkout untrusted PR code from `pull_request_target`.
4. Give the deterministic evidence to the independent Agent 3 LLM.
5. Require every actionable LLM finding to contain file/line or concrete repository evidence and a verification status: `CONFIRMED`, `LIKELY`, `POSSIBLE`, or `SPECULATION`.
6. Deterministic `CONFIRMED` findings cannot be downgraded by the LLM without explicit evidence explaining why the rule is inapplicable.
7. Deduplicate overlapping findings and assign severity and confidence.
8. Produce `.openhands/review.md` and `.openhands/review-result.json`.
9. Analysis-only reviews are durable repository knowledge under `docs/pr-reviews/` and are also posted to the PR. They never mutate the main checkpoint.
10. Active Agent 2 pipeline reviews may update the checkpoint only after exact-SHA validation and the existing CAS/operation-identity checks.

## Deterministic rules

The deterministic engine currently detects high-risk workflow permissions, privileged workflow execution patterns, force pushes, credential-shaped literals, unsafe Rust, `unwrap`/`expect`, process execution, non-loopback listeners, checkpoint-state changes, and workflow changes without accompanying test-path changes.

These are review signals, not automatic proof of a defect except where the rule explicitly says the condition is confirmed. The engine deliberately does not run PR code.

## LLM responsibilities

The LLM reviews intent, architecture, correctness, concurrency, state transitions, security boundaries, API compatibility, error handling, maintainability, tests, and documentation conflicts. It must use the deterministic report as evidence and must not invent test results.

## Evidence quality

Every finding should answer:

- What changed?
- Why is it a problem?
- Where is the evidence?
- Can the claim be reproduced or proven?
- What is the minimal corrective action?

Unverified speculation should not become a blocking review finding.

## Agent 1 / Agent 2 knowledge loop

Agent 1 reads analysis reports before planning related work. Agent 2 reads the report's `Agent 2 Working Prompt` before implementation and reconciles it with current source, tests, CI and issue-resolution prompts.
