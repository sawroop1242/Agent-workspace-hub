# Agent Workspace Hub — Growth Strategy

> **Status:** Canonical go-to-market and positioning companion to `docs/roadmap/PROJECT_ROADMAP.md`.
>
> **Branch:** `rust`
>
> This document introduces no new product scope. Every phase and guardrail referenced here is already
> defined in `PROJECT_ROADMAP.md`; this document sequences *adoption*, not *implementation*.

## 1. Positioning

AWH is the governance layer for autonomous coding agents — the infrastructure that makes agent-driven
file edits, Git operations, and terminal execution safe enough to run unattended.

One-line pitch: **"Give your coding agents real permissions to do the risky stuff, with an audit trail
and an undo button."**

## 2. The reference point: OpenClaw, and where the lesson stops

OpenClaw is the fastest-growing open-source project in GitHub history — from a weekend project to
roughly 346,000 stars and 38M monthly visitors in about five months, without venture funding or
marketing, by connecting LLMs directly to WhatsApp, Telegram, Discord, and Slack and letting them
execute real actions.

Two lessons apply directly:

- **Frictionless single-process install.** OpenClaw runs as one local Gateway process. AWH already
  commits to this via the single static Rust binary target (`docs/FEATURES.md` → Foundation and
  distribution).
- **Integrate with tools people already use rather than asking them to switch.** AWH's `/{agent}/mcp`
  routing for Claude/Codex/OpenCode/Qwen is the same move, aimed at coding agents instead of chat apps.

One lesson does **not** apply:

- **Maximal scope, maximal permission, zero governance.** This is the exact failure mode security
  researchers point to in OpenClaw — uncontrolled execution from over-permissioned agents, with an
  estimated 22% of organizations already running it as unauthorized shadow IT. That is not a growth
  model for AWH to copy; it is the market gap AWH exists to close. AWH's pitch to the same audience
  OpenClaw unsettled is: *"the agent autonomy you already have, with the audit trail and rollback you
  don't."*

## 3. Adoption phases (mapped to the build phases in PROJECT_ROADMAP.md §5)

| Adoption stage | Anchor phases | What "done" looks like | Primary audience |
|---|---|---|---|
| Trust wedge | 0-5 (Foundation -> Snapshots/rollback) | `awh init && awh agent start claude` gives verifiable capability-checked edits plus one-command rollback | Individual devs already running Claude Code/Codex/OpenCode who got burned by an unreviewed agent edit |
| Daily driver | 6-10 (Context -> Audit/observability) | Agents keep working state across sessions without re-deriving context every run; every consequential action is queryable after the fact | Same devs, now using AWH for every session, not just risky ones |
| Team/platform | 11-13 (TUI -> Control API) | A team lead can see what every agent did across every workspace from one place | Small teams running multiple agents against shared repos |
| Ecosystem | 14-16 (Remote -> Advanced infrastructure) | Connectors, remote execution, enterprise RBAC | Orgs standardizing agent governance |

Do not market stage N+1 capabilities before stage N is real. A growth claim ahead of the phase-exit
rules in PROJECT_ROADMAP.md §9 creates the same credibility risk as documenting an unimplemented
command as available.

## 4. Distribution tactics

- **Incident-story content, not feature-list content.** "My agent overwrote a file mid-edit" or "my
  agent pushed to the wrong branch" resonates the way OpenClaw's own viral anecdotes did — concrete,
  relatable, provable. Feature lists don't spread; stories do.
- **Show up where the agents already are.** MCP registries, Claude/Codex/OpenCode plugin ecosystems,
  r/LocalLLaMA, Hacker News — not generic dev-tool marketing channels.
- **Single binary, single command to value.** `awh init` to a working capability-checked agent session
  in under a minute, mirroring OpenClaw's own zero-setup pitch, without the zero-governance part.
- **Security researchers as an unpaid distribution channel.** Every "AI agent went rogue" news cycle,
  OpenClaw's own coverage included, is a moment to publish "here's what would have stopped this" - not
  to dunk on OpenClaw, but to be the credible answer to the fear it created.

## 5. Explicit non-goals (do not chase these growth patterns)

1. Do not add consumer messaging-app integrations (WhatsApp/Telegram/Discord). Out of scope per
   `PROJECT_ROADMAP.md` §11.1-3; AWH's channel is coding agents, not personal assistants.
2. Do not loosen default policy/capability posture to reduce onboarding friction. Friction that exists
   *because* of governance is the product, not a bug to remove.
3. Do not chase star-count virality metrics over trust metrics (rollback usage, audit query volume,
   policy-denial rate) - see §6.

## 6. Metrics that matter, by stage

| Stage | Vanity metric to ignore | Metric that actually indicates product-market fit |
|---|---|---|
| Trust wedge | GitHub stars | % of sessions that use at least one snapshot/rollback |
| Daily driver | Downloads | Sessions per user per week |
| Team/platform | Logo count | Audit-log queries per team per week |
| Ecosystem | Connector count | Connector invocations that passed policy vs. were denied |
