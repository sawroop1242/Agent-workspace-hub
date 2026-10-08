# AWH Documentation

Start here. This index tells you where things live and why.

## Current, living documentation (this folder)

- [`PROJECT_CONTEXT.md`](PROJECT_CONTEXT.md) — read this first. Long-lived context for anyone (human or agent) making architectural changes.
- [`architecture.md`](architecture.md) — component map, why the Rust implementation is canonical over the legacy `main` branch.
- [`FEATURES.md`](FEATURES.md) — the final target feature contract (what AWH owns vs. what agents own).
- [`CLI.md`](CLI.md) — command reference.
- [`INSTALL.md`](INSTALL.md) — install guide.
- [`configuration.md`](configuration.md) — configuration options.
- [`development.md`](development.md) — dev setup.
- [`testing.md`](testing.md) — running the test suite.
- [`security.md`](security.md) — security policy and how to report a vulnerability.
- [`threat-model.md`](threat-model.md) — STRIDE-style threat model for the MCP execution surface.
- [`mcp.md`](mcp.md) — MCP protocol surface, transports, tool catalog.
- [`composio.md`](composio.md) — Composio integration guide.
- [`community-mcp-registry.md`](community-mcp-registry.md) — community MCP registry notes.
- [`error.md`](error.md) — known errors and troubleshooting.
- [`release.md`](release.md) — release process.

## Roadmap and status

See [`roadmap/`](roadmap/) — the canonical forward-looking architecture (`PROJECT_ROADMAP.md`), its agent-profiles/policy-routed-MCP addendum, and the current reconciled status doc (`STATUS.md`).

## Feature work items

See [`implementation-prompts/`](implementation-prompts/) — individual, self-contained implementation specs (one file per feature), consolidated from the former AWE-*, SEC-*, ARCH-*, FS-*, GIT-*, AGENT-* issue-resolving prompts. See that folder's own `README.md` for the consolidation map and reading contract.

## Historical record

See [`archive/`](archive/) — superseded status snapshots, completed phase reports, and documentation of the retired three-agent (`.openhands/state.json`-based) pipeline. Kept for provenance, not current guidance. If something in `archive/` conflicts with a file listed above, the file above wins.

## One known gap worth fixing separately

`architecture.md` states `rust` "must be branch-protected." As of this reorganization, the `rust` branch has **no branch protection configured** (verified via the GitHub API) — the doc and the repo setting disagree. Worth a follow-up.
