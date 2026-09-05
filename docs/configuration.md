# Configuration

All configuration is via environment variables and CLI flags. There is no
configuration file; secrets never appear in command output or logs.

## Precedence

```
built-in defaults
        │
        ▼
AWH_* environment variables
        │
        ▼
CLI flags (highest)
```

Example: the HTTP bind host resolves as
`--host` > `AWH_HOST` > `0.0.0.0`. The same pattern applies to port and TLS
material. This precedence is unit-tested (`src/mcp/config.rs` tests use an
injected key-value source; tests never mutate the process environment).

## Remote transport (SSE)

| Variable | Default | Meaning |
| --- | --- | --- |
| `AWH_API_KEY` | *(required)* | Bearer token for `/sse` and `/mcp`. Rejects startup if missing/empty |
| `AWH_HOST` | `0.0.0.0` | HTTPS bind address |
| `AWH_PORT` | `8443` | HTTPS port |
| `AWH_TLS_CERT` | *(unset → plain HTTP)* | PEM certificate path |
| `AWH_TLS_KEY` | *(unset → plain HTTP)* | PEM private key path |
| `AWH_ALLOWED_ORIGINS` | *(unset → CORS disabled)* | Comma-separated origin allow-list for browser clients |

TLS is enabled when **both** cert and key are set. Setting only one is a
configuration error: startup fails with a typed error
(`tls_cert_and_key_required`) rather than a panic.

Equivalent CLI flags: `--host`, `--port`, `--tls-cert`, `--tls-key`,
`--api-key-env` (name of the environment variable holding the key, default
`AWH_API_KEY`).

## Resource limits

| Variable | Default | Meaning |
| --- | --- | --- |
| `AWH_MAX_MCP_LINE_BYTES` | `10485760` (10 MiB) | Maximum stdio JSON-RPC line |
| `AWH_MAX_HTTP_BODY_BYTES` | `10485760` (10 MiB) | Maximum HTTP request body |
| `AWH_MCP_REQUEST_TIMEOUT_SECS` | `30` | Per-request dispatch timeout |
| `AWH_HTTP_CLIENT_TIMEOUT_SECS` | `30` | Outbound HTTP client timeout |
| `AWH_CIRCUIT_FAILURE_THRESHOLD` | `5` | Consecutive failures before the circuit opens |
| `AWH_CIRCUIT_COOLDOWN_SECS` | `30` | How long an opened circuit stays open |

Non-numeric values are rejected with a clear error (and audited as
`config_invalid`), never silently ignored. Fixed server-side values:
`max_sessions: 100` concurrent SSE sessions, `sse_keepalive: 15s`.

## Context engine

| Variable | Default | Meaning |
| --- | --- | --- |
| `AWH_CONTEXT_ENABLED` | `true` | Master switch; `false` makes every engine operation a no-op |
| `AWH_CONTEXT_MAX_INPUT_TOKENS` | `128000` | Maximum context window input tokens |
| `AWH_CONTEXT_RESERVED_OUTPUT_TOKENS` | `8192` | Tokens reserved for model output |
| `AWH_CONTEXT_SAFETY_MARGIN_TOKENS` | `4096` | Extra headroom before offload/compression triggers |
| `AWH_CONTEXT_AUTO_OFFLOAD` | `true` | Allow optimize passes to offload items |
| `AWH_CONTEXT_AUTO_COMPRESS` | `true` | Allow optimize passes to compress items |
| `AWH_CONTEXT_MEMORY_ENABLED` | `true` | Enable long-term memory extraction |

Boolean variables accept `1`/`true`/`yes`/`on` (case-insensitive).
Token counts must parse as unsigned integers; invalid values fail with a
typed error naming the variable.

## Tunnel

| Variable | Default | Meaning |
| --- | --- | --- |
| `AWH_NGROK_AUTHTOKEN` | *(unset)* | ngrok authtoken for `awh tunnel start` |

The token is passed to the ngrok child through its environment
(`NGROK_AUTHTOKEN`) — never through argv, which is world-readable via
`/proc/<pid>/cmdline`. Precedence: `--ngrok-authtoken` CLI flag >
`AWH_NGROK_AUTHTOKEN` environment variable; prefer the variable so the
secret never appears in `ps` output or shell history. `awh tunnel start`
runs in the foreground and stops the child on Ctrl-C.

## Sandbox

| Variable | Default | Meaning |
| --- | --- | --- |
| `AWH_BWRAP` | `bwrap` (from `PATH`) | Path to the `bubblewrap` binary used for sandboxing |

If `bwrap` cannot be executed when sandboxing is required, execution **fails
closed** — it never silently runs unsandboxed.

## Data locations

| Path | Contents |
| --- | --- |
| `~/.agent-workspace-hub/` | Persistent state: trust store, skills, connectors |
| `<project>/` | Per-project memory, tasks, context (AGENTS.md) |

## Security notes

- The API key is compared in constant time and is never logged; audit and
  error events carry names and reason slugs only.
- Bearer tokens are accepted only from the `Authorization` header, never from
  URLs or query strings.
- Setting `AWH_API_KEY` is mandatory for the remote transport; the stdio
  transport does not require it because access is already scoped to the
  local process.
