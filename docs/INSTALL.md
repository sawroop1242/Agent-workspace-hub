# Installation and Upgrade Guide

Agent Workspace Hub is a Rust binary (`awh`) that exposes project context,
memory, tasks, skills, connectors, and MCP tooling to an agent.

## Prerequisites

- Rust toolchain (MSRV: stable; the codebase targets recent stable Rust).
  Install via <https://rustup.rs>.
- Cargo (bundled with Rust).

## Installation from source

```bash
git clone https://github.com/sawroop1242/Agent-workspace-hub.git
cd Agent-workspace-hub
cargo build --release
```

The binary is produced at `target/release/awh`. Install it onto your PATH:

```bash
cargo install --path .
```

## Verifying the build

The project's release gate is:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

These run in CI on `ubuntu-latest`, `macos-latest`, and `windows-latest`.

## Configuration

Runtime resource limits are tuned through `AWH_*` environment variables. See
[`SECURITY.md`](SECURITY.md) for the full list and precedence rules.

## Upgrading

Because there are no backward-compatible data-format guarantees in the pre-1.0
release:

1. Back up your per-project `.agent/` state and any `~/.config/agent-workspace-hub`
   data before upgrading.
2. Pull the latest source and rebuild:

   ```bash
   git fetch origin
   git checkout rust
   git pull --ff-only origin rust
   cargo build --release
   ```

3. Re-run the verification gates above.
4. Review `docs/PROJECT_STATUS.md` for any newly completed or remaining
   hardening items that may affect behavior.

## Reproducible release validation

Before tagging a release, confirm the following on a clean checkout:

- `cargo fmt --all -- --check` passes.
- `cargo test --all-targets` is green on the release commit.
- `cargo clippy --all-targets --all-features -- -D warnings` is warning-free.
- `cargo build --release` produces a runnable `awh`.

Performance targets (e.g. p99 latency or peak memory) must be measured under a
documented workload; they are not considered met merely because the code compiles.