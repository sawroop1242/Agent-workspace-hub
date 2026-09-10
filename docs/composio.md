# Composio integration guide

Composio gives agents one authenticated surface over 250+ SaaS apps
(Gmail, Google Drive, Slack, GitHub, …). AWH works with Composio in
**two complementary ways**, and this guide shows when to use each and
how to set them up.

| | Hosted MCP server (this guide) | Native provider (`connector.composio_*`) |
| --- | --- | --- |
| Auth | One `x-consumer-api-key` header | Same key, passed as `COMPOSIO_API_KEY` to the backend REST API |
| Transport | Streamable HTTP to `connect.composio.dev/mcp` | Direct REST calls to Composio's backend |
| Setup | `awh mcp add` + `awh mcp trust` | Just export `COMPOSIO_API_KEY` |
| When | Use the hosted MCP: fewer moving parts, self-updating tool catalog, AWH's own gates (schema validation, circuit breaker) apply on top | Already have Composio connected accounts / need `connector.composio_register` multi-account labels |

## Quick start: hosted Composio MCP

You need the Composio **consumer API key** (from the Composio dashboard;
it starts with `ck_`). The hosted MCP endpoint authenticates with the
`x-consumer-api-key` header — which is exactly what AWH's custom-MCP
header support is for.

**Step 1 — add the server.** Prefer the `${secret:NAME}` indirection so
the raw key never lands in `.agent/mcps.json` (that file is local state;
the indirection keeps the key out of it entirely):

```bash
awh mcp add composio \
    --name composio \
    --transport streamablehttp \
    --url https://connect.composio.dev/mcp \
    --header 'x-consumer-api-key=${secret:COMPOSIO_CONSUMER_API_KEY}' \
    --secret COMPOSIO_CONSUMER_API_KEY
```

**Step 2 — trust it.** Enabled is not trusted; a custom server only
spawns after an explicit approval:

```bash
awh mcp trust composio
```

**Step 3 — serve with the key in the environment.** AWH resolves
`${secret:...}` references from the *serving process's* environment:

```bash
export COMPOSIO_CONSUMER_API_KEY=ck_your_key_here
awh mcp serve
```

That's it. An MCP client (Claude Desktop, OpenCode, an agent, …)
connecting to AWH now sees the dynamic `composio.*` tools:

```
composio.COMPOSIO_SEARCH_TOOLS
composio.COMPOSIO_GET_TOOL_SCHEMAS
composio.COMPOSIO_MULTI_EXECUTE_TOOL
composio.COMPOSIO_MANAGE_CONNECTIONS
composio.COMPOSIO_WAIT_FOR_CONNECTIONS
composio.COMPOSIO_REMOTE_BASH_TOOL
composio.COMPOSIO_REMOTE_WORKBENCH
```

## Connecting an app (e.g. Google Drive)

Composio tools refuse to act until the underlying app connection is
**Active**. List or initiate connections with
`composio.COMPOSIO_MANAGE_CONNECTIONS`:

```json
{"toolkits": [{"name": "googledrive", "action": "list"}]}
```

- `list` — shows each account, its status (`active` / `initiated` /
  `initializing`), and whether it is the default.
- `add` — initiates a connection and returns a `redirect_url` for the
  user to authorize in a browser. The link expires in ~10 minutes. A
  half-finished connection (`initiated`) shows up in `list` and can be
  dropped with `remove` + that account's id.
- Toolkit slugs are Composio's, and they are not always what you'd
  guess — it is `googledrive`, not `gdrive`. Never invent slugs: run
  `composio.COMPOSIO_SEARCH_TOOLS` with a use-case sentence first and
  use the exact `toolkits` it returns.

A previously-connected app may already be Active — always `list` before
`add`.

## Using Composio through AWH

The hosted MCP is Composio's own agent-oriented API: you describe what
you want in plain English, and it plans and routes to concrete tool
slugs.

1. `composio.COMPOSIO_SEARCH_TOOLS` — one or more `queries` of shape
   `{"use_case": "..."}`; returns tool slugs, a recommended plan, and
   known pitfalls. (Personal identifiers go in `known_fields`, never in
   `use_case`.)
2. `composio.COMPOSIO_GET_TOOL_SCHEMAS` — exact input schemas for the
   returned slugs.
3. `composio.COMPOSIO_MULTI_EXECUTE_TOOL` — run one or many tool calls
   in parallel; requires `sync_response_to_workbench` (set `true` for
   large results, which land in the workbench).

The equivalent AWH-native path is `connector.invoke` /
`connector.tools` with `provider: "composio"` — same effect, and AWH's
argument-validation gate checks every call against the schema Composio
advertised (malformed calls fail closed with `-32602` before Composio
ever sees them).

## Gotchas learned from the live validation

- **Don't reuse `COMPOSIO_API_KEY` as the secret name for the hosted
  server.** That exact name also switches on AWH's *native* Composio
  provider (`ComposioProvider::from_env`), which talks to Composio's
  backend REST API. The `ck_` consumer key is valid **only** on the
  hosted MCP endpoint — the backend rejects it (401,
  `APIKey_InvalidAPIKey`). Use a dedicated name like
  `COMPOSIO_CONSUMER_API_KEY` (as in the commands above) so only the
  hosted server consumes it.
- **Schemas can differ between `tools/list` and
  `COMPOSIO_GET_TOOL_SCHEMAS`.** AWH validates `connector.invoke`
  arguments against the `tools/list` advertisement (e.g.
  `COMPOSIO_MANAGE_CONNECTIONS` takes `toolkits: [{name, action}]`
  there, while `GET_TOOL_SCHEMAS` documents a plain slug array). When a
  call is rejected with `MCP argument validation failed`, print the
  advertised schema and re-shape the arguments.
- **Two connections can exist for one toolkit.** `MANAGE_CONNECTIONS`
  `add` always creates a new one (up to Composio's account limit). Pass
  `account` (alias or account id) in `COMPOSIO_MULTI_EXECUTE_TOOL` when
  more than one is connected; otherwise the default is used.
- **Unfinished OAuth flows linger.** An `initiated` connection that is
  never completed in the browser stays in `list` until it expires; use
  `action: "remove"` with its `account_id` to drop it.
- **Root-only caveat:** `${secret:...}` resolves from the serving
  process environment — if `awh mcp serve` runs elsewhere (a service
  unit, another user), the variable must be present in *that*
  environment.

## Uninstall

```bash
awh mcp remove composio        # drops it from .agent/mcps.json
awh mcp revoke composio       # also removes the stored approval
```
