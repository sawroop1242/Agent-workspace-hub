# Development

## Prerequisites

- Rust 1.7x stable (`rustup default stable`)
- Linux: `bubblewrap` (`bwrap`) for sandbox tests (`apt install bubblewrap`)
- Node 18+ (optional) — to run the MCP interop harnesses

## Build

```bash
cargo build            # debug
cargo build --release  # the shipped `awh` binary
```

## Layout

See [`architecture.md`](architecture.md). Tests live in `tests/`
(integration) and inline `#[cfg(test)]` modules (unit).

## Commands (all must pass before a PR)

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Dependency security (matches CI):

```bash
cargo audit
```

## Conventions

- **Deny-by-default**: any new capability must go through the centralized
  execution gate. Never add a tool that performs its own inline permission
  check instead of the gate's.
- **Typed errors**: no `unwrap()`/`expect()`/`panic!()` on network input, MCP
  requests, configuration, filesystem paths, or external data. JSON-RPC
  errors use the stable codes in `src/mcp/error.rs`.
- **No secrets in logs**: audit events carry names/slugs, never values.
- **Tests never mutate global process environment**: use
  `with_overrides_from(lookup)` (see `src/mcp/config.rs`) or
  `wrap_command_with()` (see `src/mcp/sandbox.rs`) instead.
- **Sandbox changes must fail closed**: if the sandbox binary is missing or
  the project root is not an absolute, existing path, execution is denied.
- **Small commits**: one logical change + its regression test + doc update
  in the same commit where practical.

## Adding a tool

1. Add the tool descriptor (name, description, `inputSchema`) in
   `src/mcp/dispatcher.rs`.
2. Implement the handler in the tool's module and route it in the dispatcher
   match arm.
3. Route it through the execution gate — no inline authorization in the
   handler.
4. Add tests: valid call, invalid arguments, permission denial, and (if
   applicable) sandbox/path-security cases.
5. Document it in [`mcp.md`](mcp.md) and the completeness audit.

## Branching and PRs

- `rust` is the production branch (branch-protected; see
  [`security.md`](security.md)).
- Work on feature branches off `rust`; open a PR against `rust`.
- CI (`.github/workflows/rust.yml`) runs on every push/PR: fmt,
  build+test on Ubuntu/macOS/Windows, clippy, cargo audit.

## Master hardening prompt tracking

The production-hardening work items (18.x phases) are recorded in commit
messages and summarized in [`completeness-audit.md`](completeness-audit.md).
