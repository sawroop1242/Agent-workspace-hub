# SEC-002 — Public MCP TLS Guard

## Task
Refuse non-loopback MCP HTTP/SSE binds without valid TLS.

## Forensic baseline
PR #43 found public `0.0.0.0` binding with optional TLS and bearer-token authentication, allowing plaintext token exposure.

## Contract
- loopback + plaintext: allowed for local development
- non-loopback + no TLS: reject before serving
- non-loopback + invalid TLS: reject with actionable error
- non-loopback + valid TLS: serve

## Requirements
Centralize the bind/TLS safety check, preserve existing TLS validation, and do not weaken tunnel-specific protections.

## Tests
Cover loopback/public and TLS/no-TLS combinations, startup rejection, and successful secure startup.
