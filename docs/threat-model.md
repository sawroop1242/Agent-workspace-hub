# Threat Model

Scope: the `awh` binary as an MCP server (stdio and HTTPS/SSE transports).
The model below follows STRIDE-style reasoning with, for every threat: the
asset at risk, the attacker, the attack, the implemented mitigation, the test
that proves the mitigation, and the residual risk after mitigation.

Trust boundaries:

```
untrusted MCP client ──(stdio)──┐
untrusted MCP client ──(HTTPS)──┤
                                ▼
                        auth → session → execution gate
                                ▼
        trusted: workspace files, trust store, secrets, host process
```

Anything arriving over a transport is untrusted: tool arguments, tool names,
session identifiers, headers, registry content, and external connector data.

## T1 — Malicious MCP client (remote, unauthenticated)

| | |
| --- | --- |
| **Asset** | MCP server availability; workspace data |
| **Attacker** | Network peer reaching `--host:--port` without a token |
| **Attack** | Connect to `/sse` or POST `/mcp` directly |
| **Mitigation** | Mandatory bearer auth on `/sse` and `/mcp` (constant-time compare); server refuses to start the remote transport without `AWH_API_KEY` |
| **Test** | `tests/mcp_http.rs`: missing token → 401; malformed Bearer → 401; wrong token → 401; valid token → 200 |
| **Residual risk** | Token theft by a local process or network interception without TLS. TLS is strongly recommended; plain HTTP is only for private networks |

## T2 — Session hijacking

| | |
| --- | --- |
| **Asset** | Another client's session state and project access |
| **Attacker** | A client guessing or reusing a `sessionId` |
| **Attack** | POST `/mcp?sessionId=<someone-else's-id>` |
| **Mitigation** | Sessions are opaque server-generated IDs; unknown ID → 404; each session is isolated per-connection state; concurrent sessions capped at 100 |
| **Test** | `tests/mcp_http.rs`: `unknown_session_is_rejected`, `enforces_session_limit` |
| **Residual risk** | IDs are unguessable but not cryptographically rate-limited per source IP; a determined attacker who *possesses* a valid token can open the session anyway (token == full trust) |

## T3 — Path traversal / sandbox escape

| | |
| --- | --- |
| **Asset** | Host filesystem outside the workspace |
| **Attacker** | Malicious MCP client, malicious skill, or malicious project |
| **Attack** | `workspace.read_file` with `../../etc/passwd`, absolute paths, symlinks, null bytes, Unicode tricks |
| **Mitigation** | Project root must be absolute and existing; relative paths resolved against it; all file access goes through the centralized path validation; subprocess execution runs under `bwrap` with read-only bind mounts |
| **Test** | `tests/mcp_sandbox.rs`: relative project root rejected; sandbox requires absolute existing root; relative filesystem paths rejected; fails closed without `bwrap` |
| **Residual risk** | Platforms without `bwrap` cannot run sandboxed execution at all (fail-closed — no code runs, but the capability is absent rather than degraded). Symlink-time-of-check windows inside the project directory are narrowed by canonicalization but not provably zero on all filesystems |

## T4 — Credential theft

| | |
| --- | --- |
| **Asset** | `AWH_API_KEY`, connector credentials, referenced secrets |
| **Attacker** | Any MCP client reading tool responses, or anyone reading logs |
| **Attack** | Trick a tool into echoing an env var or secret; grep the logs |
| **Mitigation** | `src/mcp/security.rs` redacts secrets from responses; audit events carry only names and slugs; bearer tokens never logged and never accepted via URL; responses sanitized before leaving the dispatcher |
| **Test** | `tests/mcp_security.rs`: secret access requires environment permission; dangerous environment names blocked |
| **Residual risk** | Secrets legitimately granted to a tool via explicit `${secret:NAME}` permission can be read by that tool once approved; approval is the control, not concealment |

## T5 — Tool abuse / confused deputy

| | |
| --- | --- |
| **Asset** | Workspace and (via connectors) external systems |
| **Attacker** | A compromised agent or malicious client that can call any tool |
| **Attack** | Invoke high-risk tools directly, hoping the tool forgot a check |
| **Mitigation** | Centralized execution gate: permission + trust + limits checks run before every tool handler; no handler can bypass the gate |
| **Test** | `tests/mcp_security.rs`: unknown/blocked/wrong-version/over-permissioned MCP servers denied; revoked trust denies |
| **Residual risk** | A tool correctly authorized for a legitimate user can still be driven to destructive effect within its granted scope — scope minimization is the remaining control |

## T6 — Resource exhaustion / DoS

| | |
| --- | --- |
| **Asset** | Server memory, CPU, file descriptors, disk |
| **Attacker** | Remote client (or local skill) sending huge payloads or many sessions |
| **Attack** | 100 MiB JSON body; 10 000 sessions; slow SSE readers; long-running tools |
| **Mitigation** | Body/line caps (10 MiB default), session cap (100), per-request timeout (30 s), outbound HTTP timeout, circuit breaker (5 failures / 30 s cooldown), sandbox CPU/memory/wall/output caps for subprocesses |
| **Test** | `tests/mcp_http.rs`: oversized body rejected, session limit enforced; `src/mcp/config.rs` unit tests prove limits parse and bounds-check |
| **Residual risk** | A single authenticated client can still consume its full per-request allowance; authentication is the first-line throttle for remote attackers |

## T7 — Malicious skill or project

| | |
| --- | --- |
| **Asset** | Host integrity and data of *other* projects |
| **Attacker** | A skill file or AGENTS.md-style context planted in a cloned repo |
| **Attack** | A project's own files request elevated behavior when merely *loaded* |
| **Mitigation** | Loading context and listing skills is read-only; no skill gains filesystem/network/process/secrets access merely by being discovered; access requires the same gate as any tool call |
| **Test** | `tests/mcp_server.rs`: workspace read/list operate only within the project; `tests/mcp_security.rs`: dangerous environment/secret access denied without explicit permission |
| **Residual risk** | A *user* who explicitly approves a malicious skill can still be socially engineered into granting it — the gate requires an approval decision, it does not judge intent |

## T8 — Trust-store corruption

| | |
| --- | --- |
| **Asset** | The record of which external MCP servers are trusted |
| **Attacker** | Local disk corruption or tampering |
| **Attack** | Truncate/garble `~/.agent-workspace-hub` trust records so unknown servers are treated as trusted |
| **Mitigation** | Trust store fails **closed**: unreadable or corrupt store = zero approvals |
| **Test** | `tests/mcp_security.rs`: `corrupted_trust_store_fails_closed`, `empty_trust_store_defaults_to_no_approvals` |
| **Residual risk** | A *valid* store maliciously rewritten with plausible records by an attacker with write access to the user's home directory — that attacker already owns the user's session |

## T9 — Supply-chain dependency attack

| | |
| --- | --- |
| **Asset** | Build integrity of `awh` itself |
| **Attacker** | Compromised crate in the dependency graph |
| **Attack** | Malicious or vulnerable transitive dependency introduced into `Cargo.lock` |
| **Mitigation** | `cargo audit` runs in CI on every push/PR to `rust` and fails the build on any RUSTSEC vulnerability; warning-level advisories (unmaintained/yanked) are surfaced but non-blocking |
| **Test** | CI job `audit` in `.github/workflows/rust.yml` (verified locally: 0 vulnerabilities; 2 transitive warnings: `rustls-pemfile` unmaintained, `chacha20` yanked) |
| **Residual risk** | Advisories only cover *known* RUSTSEC entries; new CVEs before database publication, and typosquatting during a future dependency addition, remain possible — the audit gate is the detection, review of `Cargo.lock` diffs is the prevention |

## T10 — Network abuse via connectors

| | |
| --- | --- |
| **Asset** | External accounts reachable through connectors (Composio etc.) |
| **Attacker** | A client or malicious project invoking connector tools |
| **Attack** | Invoke arbitrary external actions merely because a connector is *enabled* |
| **Mitigation** | Explicit authorization layer: enabled → action allowed → permission check → approval if dangerous → execute → audit; connector credentials never exposed to the MCP client; outbound calls timeout-bounded and circuit-broken |
| **Test** | `tests/mcp_server.rs`: unknown qualified provider tool fails closed; `src/mcp/connectors.rs` unit tests for store limits and enable/disable flows |
| **Residual risk** | Connector behavior ultimately depends on the upstream provider's own authorization model; AWH gates *whether* an action may be invoked, not what the provider then does |

## Cross-cutting guarantees

- **Fail-closed everywhere**: every gate defaults to deny on error
  (missing `bwrap`, corrupt trust store, half-configured TLS, invalid
  limits, unknown tool).
- **Deterministic errors**: malformed input yields stable JSON-RPC codes
  (-32600/-32601/-32603), never a stack trace or panic.
- **Auditability**: allow-side and deny-side events (`mcp_audit`,
  `mcp_security_denied`, `mcp_secret_denied`, `mcp_circuit_open`) are
  emitted without secret values, sufficient for incident reconstruction.
