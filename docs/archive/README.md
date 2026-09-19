# Archive

Historical documents: point-in-time status snapshots, completed-phase reports, and design notes for the retired three-agent pipeline. Everything here reflects the repository as it stood on the date embedded in the file — none of it is auto-updated, and none of it should be treated as current guidance. Where something here conflicts with a file in `docs/` or `docs/roadmap/`, the live document wins.

## Superseded roadmap/status snapshots

Superseded by [`../roadmap/STATUS.md`](../roadmap/STATUS.md), which reconciled these three as of 2026-09-13:

- `PROJECT_ROADMAP_STATUS.md` (2026-09-12 snapshot)
- `ROADMAP_GAP_MATRIX.md` (2026-09-12 snapshot, derived from the above)
- `ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md` (2026-09-12 Claude-authored synthesis; source material for several `issue-resolving-prompts/` items)

`RECOMMENDED_PRODUCT_ROADMAP.md` marks itself superseded in its own header — kept for the historical rationale behind early scope cuts.

## Point-in-time completion / audit reports

Each of these was accurate as evidence at the commit it names, not as an ongoing status page:

- `PROJECT_STATUS.md` — Rust-migration / MCP-security-hardening snapshot
- `MCP_INFRASTRUCTURE_COMPLETION_REPORT.md` — MCP infrastructure completion evidence
- `completeness-audit.md` — subsystem-by-subsystem audit
- `awh-evolution-report.md` — evolution report against `MASTER_PROMPT.md` §1-42
- `implementation-plan.md` — Phase 0 baseline plan
- `phase-11-github-integration-and-gaps.md` — completed Phase 11 work order

## Origin / vision document

- `MASTER_PROMPT.md` — the original evolution mission this report/roadmap lineage was built from.

## Retired three-agent pipeline

The `.openhands/state.json`-based `Agent 1 (plan) -> Agent 2 (build) -> PR -> Agent 3 (review)` pipeline these describe has been replaced by `.github/agent-engine/`. Kept for archaeology, not as setup instructions:

- `autonomous-development.md` — GitHub Actions control-plane setup for the old three-agent loop
- `PIPELINE_ANALYSIS_FIXES.md` — issue analysis against that pipeline
- `pipeline-autonomous-pipeline.md` (formerly `docs/pipeline/autonomous-pipeline.md`)
- `pr-reviews-README.md` and `pr-reviews-agent3-v2.md` (formerly `docs/pr-reviews/`) — the old Agent 3 review-storage convention and hybrid review architecture
