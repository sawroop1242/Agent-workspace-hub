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

## Remote MCP transport (HTTP/SSE/HTTPS)

In addition to the default stdio transport (`awh mcp serve`), the server can
expose its MCP interface to remote agents over HTTPS with a Server-Sent Events
stream.

### TLS certificates

Provide a PEM certificate chain and private key. Generate a self-signed pair for
internal testing:

```bash
openssl req -x509 -newkey rsa:2048 -keyout /etc/awh/key.pem \
  -out /etc/awh/cert.pem -days 365 -nodes -subj "/CN=awh.example.com"
```

For production, use a certificate signed by a trusted CA (e.g. Let's Encrypt).
The server negotiates TLS 1.2+ and refuses to start with a half-configured TLS
setup (certificate without key, or key without certificate).

### Authentication

Remote access requires a bearer token. Set the API key (never committed to the
repository) in an environment variable:

```bash
export AWH_API_KEY="$(openssl rand -hex 32)"
```

Requests must send `Authorization: Bearer <token>`. Requests without a valid
token receive `401 Unauthorized`.

### Starting the server

```bash
AWH_API_KEY="..." \
AWH_TLS_CERT="/etc/awh/cert.pem" \
AWH_TLS_KEY="/etc/awh/key.pem" \
AWH_HOST="0.0.0.0" \
AWH_PORT="8443" \
awh mcp serve --transport sse
```

Runs an HTTPS server exposing:

- `GET /health` - liveness probe (unauthenticated).
- `GET /sse` - SSE stream; each connection gets an isolated session.
- `POST /mcp?sessionId=...` - submit JSON-RPC for an SSE session.

Omit `AWH_TLS_CERT`/`AWH_TLS_KEY` to serve plain HTTP (development only).

### Firewall requirements

Expose only the TLS port (default `8443`) to the remote agents that need it:

```bash
# AWH_TLS_CERT + AWH_TLS_KEY set -> TCP 8443 (HTTPS)
```

Do not expose the stdio transport over the network. Bind to a loopback address
(`AWH_HOST=127.0.0.1`) unless remote agents are expected.

### Secure production deployment

- Generate a strong API key (`openssl rand -hex 32`) and store it in a secret
  manager, not the repository or shell history.
- Set restrictive file permissions on key material: `chmod 600 /etc/awh/key.pem`.
- Terminate TLS with a CA-signed certificate; keep TLS 1.2+ enabled.
- Set `AWH_ALLOWED_ORIGINS` to an explicit allow-list (empty by default disables
  CORS entirely).
- Place the server behind a reverse proxy or network firewall that limits source
  addresses.
- Run as an unprivileged user with no filesystem write access beyond `.agent/`.

### Troubleshooting

- `refusing to serve remote MCP without an API key` - `AWH_API_KEY` is unset or
  empty; the server fails closed.
- `TLS certificate provided without a private key` / `TLS private key provided
  without a certificate` - set both `AWH_TLS_CERT` and `AWH_TLS_KEY`, or neither.
- `failed to bind 0.0.0.0:8443` - the port is in use or `AWH_HOST` is unreachable.
- `401 Unauthorized` - the client is missing or sending an incorrect bearer token.

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