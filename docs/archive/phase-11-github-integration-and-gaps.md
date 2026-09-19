# Phase 11 implementation plan: GitHub-native integration + remaining tool gaps

## How to use this document

This is written as a self-contained work order for an AI coding agent (or a
human) picking up this project cold, in the same spirit as `AGENTS.md` and
`docs/implementation-plan.md` from earlier phases. Each section names the
exact file to touch, the exact tool names and JSON schemas to add, and the
exact pitfalls hit while building the previous phase (PR #9,
`fix/store-locking-and-composio-accounts`) so they aren't repeated. Work the
sections in order; each is independently shippable.

Before starting, read:
- `docs/architecture.md`, `docs/mcp.md` — current tool catalog and design.
- `src/mcp/dispatcher.rs` — the single place every tool is registered and routed.
- `src/mcp/composio.rs` and `src/mcp/composio_registry.rs` — the most recent
  provider added; **GitHub-native (Section 1) is modeled on this file**, not
  copied from it, because GitHub needs first-class tool names (`github.*`),
  not the generic `connector.invoke` indirection Composio uses.

## 0. Current state and why this document exists

As of PR #9, the tool surface is (see `src/mcp/dispatcher.rs::tools_list_static`):
`skills.*`, `workspace.context/list_files/read_file`, `memory.*`, `tasks.*`,
`connectors.*` (metadata only), `connector.*` (live provider invocation,
including the new `connector.composio_*` account-management tools),
`context.*` (the token-budget context engine), `git.*` (thin wrappers around
the **local** `git` CLI via `src/services/git.rs`), and `terminal.run`.

Confirmed gaps, in priority order:

1. **No GitHub API integration at all.** `git.*` only ever shells out to the
   local `git` binary (status/branch/log/diff/stage/unstage/commit). There is
   no code anywhere that calls `api.github.com` — no way for an agent to open
   a PR, read/comment on an issue, check CI status, or cut a release, short of
   going through the generic (and much less discoverable) Composio connector
   path, which also requires the human to have a Composio account at all.
   `grep -rn "api.github.com\|GITHUB_TOKEN" src/` currently returns nothing.
2. **No `workspace.write_file` or `workspace.delete_file`.**
   `src/mcp/workspace.rs` only has `list_files` and `read_file`. An agent
   editing the project has to fall back to `terminal.run` with a shell
   redirect, which is worse UX and bypasses the size/path checks
   `read_file` already enforces via `safe_path`.
3. **No `tasks.get` / `connectors.get`.** `tasks.rs` and `connectors.rs` only
   expose list/create/update/delete-style operations; looking up one item by
   id means listing everything and filtering client-side.
4. **Composio account management is MCP-tool-only.** `connector.composio_link/accounts/register/remove`
   (added in PR #9) have no CLI command and no TUI screen — a human without an
   MCP client has no way to drive them. Not required for this phase, but
   flagged in Section 3.3 as the natural follow-up once Section 1 sets the
   pattern for exposing a new external API in the CLI too.

## 1. GitHub-native MCP provider

### 1.1 Why not just route this through Composio

Composio already proxies GitHub (that's how PR #9 and this very document got
pushed — see `Composio:COMPOSIO_SEARCH_TOOLS` / `GITHUB_CREATE_A_PULL_REQUEST`
etc. in the agent tooling that built this project). But requiring a Composio
account for the single most common connector *any* coding agent needs is a
real adoption tax, and it hides `github.*` tools behind
`connector.invoke(provider="composio", tool="GITHUB_...")` instead of a clean,
discoverable `github.pr_create` the same way `git.commit` is discoverable
today. Build a direct, first-class provider the same way `git.*` is direct —
optionally *also* reachable via Composio for people who already use it, but
never *required* to be.

### 1.2 Auth

- New env var: `GITHUB_TOKEN` (a classic PAT or fine-grained token with
  `repo` scope is enough for everything in this phase). Optional
  `GITHUB_API_URL` env var (default `https://api.github.com`) for GitHub
  Enterprise Server support — don't hardcode the host.
- Fail closed exactly like `ComposioProvider::from_env`: if `GITHUB_TOKEN` is
  unset or empty, the provider simply isn't registered (no panic, no error at
  startup) — `github.*` tools then don't appear in `tools/list` at all, which
  is the existing pattern for every optional provider in this codebase.
- The repo (`owner/repo`) is **not** a single global env var — pass it as a
  tool argument (`owner`, `repo`) on every call, or accept
  `GITHUB_DEFAULT_OWNER`/`GITHUB_DEFAULT_REPO` as optional fallbacks so a
  single-repo agent session doesn't have to repeat them. Look at
  `GitService::open` for how the local git tools infer the target from the
  workspace root; GitHub tools should default `owner/repo` from the local
  git remote's `origin` URL when not explicitly passed (parse
  `git remote get-url origin`, supporting both `https://github.com/o/r.git`
  and `git@github.com:o/r.git` forms) before falling back to the env vars.

### 1.3 New file: `src/mcp/github.rs`

Structure it like `composio.rs`: a `GithubProvider` struct holding a
`reqwest::Client` (reuse `super::config::build_http_client()`) and the token,
with private helper methods for GET/POST/PATCH/PUT against
`{base_url}/repos/{owner}/{repo}/...`. Do **not** implement
`ConnectorProvider` for it — register its tools directly in
`tools_list_static()`/`call_tool()` in `dispatcher.rs`, the same way `git.*`
and `terminal.run` are, not through `ProviderRegistry`. Reasoning: `git.*`
tools already establish that first-class, frequently-used integrations get
direct dispatcher tool names instead of the generic provider indirection;
GitHub should follow that precedent, not the Composio one.

```rust
pub struct GithubProvider {
    token: String,
    base_url: String,
    client: reqwest::Client,
}

impl GithubProvider {
    pub fn from_env() -> Result<Self> { /* fails closed like ComposioProvider::from_env */ }
    async fn get(&self, path: &str) -> Result<Value> { /* GET, bearer auth, error on non-2xx */ }
    async fn post(&self, path: &str, body: Value) -> Result<Value> { /* ... */ }
    async fn patch(&self, path: &str, body: Value) -> Result<Value> { /* ... */ }
    async fn put(&self, path: &str, body: Value) -> Result<Value> { /* ... */ }
}
```

Auth header: GitHub's REST API wants `Authorization: Bearer {token}` (or
`token {token}` for classic PATs — Bearer works for both as of the current
API version) plus `Accept: application/vnd.github+json` and
`X-GitHub-Api-Version: 2022-11-28`. Set all three on every request.

### 1.4 Tools to add, with exact JSON schemas

Add a `McpDispatcher` field `github: Option<Arc<GithubProvider>>`,
constructed in `McpDispatcher::new` next to the Composio wiring, `.ok()`'d
the same way the context engine is so a missing token doesn't fail
construction. Add a private helper `fn github(&self) -> Result<&GithubProvider>`
mirroring `fn context(&self) -> Result<&ContextEngine>` — same "disabled or
failed to initialize" error message shape for consistency.

Tool list (add to `tools_list_static()` — **see the recursion-limit warning
in 1.5 before deciding which array to put these in**):

```json
{"name":"github.pr_list","description":"List pull requests","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"state":{"type":"string","enum":["open","closed","all"]},"base":{"type":"string"}}}}
{"name":"github.pr_get","description":"Get a single pull request","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"}},"required":["number"]}}
{"name":"github.pr_create","description":"Create a pull request","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"title":{"type":"string"},"head":{"type":"string"},"base":{"type":"string"},"body":{"type":"string"},"draft":{"type":"boolean"}},"required":["title","head","base"]}}
{"name":"github.pr_merge","description":"Merge a pull request","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"},"merge_method":{"type":"string","enum":["merge","squash","rebase"]}},"required":["number"]}}
{"name":"github.pr_review","description":"Submit a pull request review","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"},"event":{"type":"string","enum":["APPROVE","REQUEST_CHANGES","COMMENT"]},"body":{"type":"string"}},"required":["number","event"]}}
{"name":"github.issue_list","description":"List issues","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"state":{"type":"string","enum":["open","closed","all"]},"labels":{"type":"string"}}}}
{"name":"github.issue_get","description":"Get a single issue","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"}},"required":["number"]}}
{"name":"github.issue_create","description":"Create an issue","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"title":{"type":"string"},"body":{"type":"string"},"labels":{"type":"array","items":{"type":"string"}}},"required":["title"]}}
{"name":"github.issue_comment","description":"Comment on an issue or pull request","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"number"},"body":{"type":"string"}},"required":["number","body"]}}
{"name":"github.checks_status","description":"Get combined CI/check status for a commit or branch","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"ref":{"type":"string"}},"required":["ref"]}}
{"name":"github.workflow_dispatch","description":"Manually trigger a workflow_dispatch run on a branch","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"workflow_file":{"type":"string"},"ref":{"type":"string"}},"required":["workflow_file","ref"]}}
{"name":"github.release_create","description":"Create a release","inputSchema":{"type":"object","properties":{"owner":{"type":"string"},"repo":{"type":"string"},"tag_name":{"type":"string"},"name":{"type":"string"},"body":{"type":"string"},"draft":{"type":"boolean"},"prerelease":{"type":"boolean"}},"required":["tag_name"]}}
```

Endpoint mapping (all relative to `{base_url}/repos/{owner}/{repo}`):

| Tool | Method | Endpoint |
|---|---|---|
| `pr_list` | GET | `/pulls?state=..&base=..` |
| `pr_get` | GET | `/pulls/{number}` |
| `pr_create` | POST | `/pulls` |
| `pr_merge` | PUT | `/pulls/{number}/merge` |
| `pr_review` | POST | `/pulls/{number}/reviews` |
| `issue_list` | GET | `/issues?state=..&labels=..` (note: GitHub's issues endpoint also returns PRs — filter out entries with a `pull_request` key if the caller only wants true issues) |
| `issue_get` | GET | `/issues/{number}` |
| `issue_create` | POST | `/issues` |
| `issue_comment` | POST | `/issues/{number}/comments` |
| `checks_status` | GET | `/commits/{ref}/status` (combined status; use `/commits/{ref}/check-runs` instead/additionally if the repo uses GitHub Actions checks rather than the legacy status API — return both if present) |
| `workflow_dispatch` | POST | `/actions/workflows/{workflow_file}/dispatches` body `{"ref": ref}` |
| `release_create` | POST | `/releases` |

`owner`/`repo` resolution order for every tool: explicit argument > parsed
`git remote get-url origin` > `GITHUB_DEFAULT_OWNER`/`GITHUB_DEFAULT_REPO`
env vars > error (`bail!("owner/repo not specified and could not be inferred from git remote 'origin'")`).

### 1.5 Wiring into `dispatcher.rs` — recursion-limit warning

**This bit us during PR #9 and will bite again if skipped.** `tools_list_static()`
already splits its tool list across two `json!([...])` macro calls (`core` and
`extended`) because a single array of ~40 entries hits rustc's default macro
recursion limit for `serde_json::json!`. Adding these 13 GitHub tools to
either existing array will very likely blow the limit again (confirmed
experimentally: going from 35 to 39 entries in `core` was already too many).
Add a **third** array instead:

```rust
let github_tools = json!([ /* the 13 schemas above */ ]);
```

and extend `tools` with it the same way `extended` is merged:

```rust
let mut tools = core;
if let (Value::Array(core_arr), Value::Array(ext_arr)) = (&mut tools, &extended) {
    core_arr.extend(ext_arr.iter().cloned());
}
if let (Value::Array(core_arr), Value::Array(gh_arr)) = (&mut tools, &github_tools) {
    core_arr.extend(gh_arr.iter().cloned());
}
```

After making this change, run `cargo check` (or at minimum
`rustfmt --edition 2021 --check` plus a careful read) before opening a PR —
better yet, since this is exactly the class of error that only surfaces at
compile time, push to a branch and let CI's `cargo check`/`clippy` catch it
immediately rather than iterating locally if a modern toolchain (1.85+,
edition2024-capable) isn't available in the working sandbox. See Section 5.

Add the `call_tool` match arms following the exact pattern of the
`connector.composio_*` arms added in PR #9 (extract args with `strval`/
`arguments.get(...).and_then(Value::as_str)`, `audit_allow(...)` before any
mutating call, `serde_json::to_value(..)?` on the result).

### 1.6 Error handling and rate limits

- On a non-2xx response, `bail!("GitHub API returned {status}: {body}")` —
  same shape as `ComposioProvider::request`'s error.
- GitHub returns rate-limit info in response headers
  (`X-RateLimit-Remaining`, `X-RateLimit-Reset`). Not required for v1, but
  worth a follow-up: surface a clearer error when `X-RateLimit-Remaining: 0`
  than the generic "403: {body}" a caller would otherwise see.
- Every `github.*` call should go through `audit_allow("github_invoke", tool_name, format!("{owner}/{repo}"))`
  before executing, matching how `connector.invoke` audits provider+tool
  (never audit the request body — PR descriptions, issue bodies, etc. may be
  long or sensitive).

### 1.7 Tests to write

Follow `composio.rs`'s lead (it currently has **zero** tests — flagged as a
gap in the original audit of this project; don't repeat that here). Since
hitting the real GitHub API in unit tests is neither desirable nor reliable:

- Unit-test the owner/repo resolution logic in isolation (git remote URL
  parsing for both HTTPS and SSH forms; explicit-arg override; env var
  fallback; error when nothing resolves) — this is pure string logic, needs
  no network and is exactly the kind of thing that silently breaks.
- Unit-test request/response shaping (building the right path + query string
  for each tool) by extracting that into small pure functions the tests can
  call directly, rather than only inside the `async fn` that also does the
  network call.
- If integration coverage against the real API is wanted, gate it behind an
  `#[ignore]` test that only runs when `GITHUB_TOKEN` is set in the test
  environment, mirroring how `tests/mcp_sandbox.rs` gates on sandbox tool
  availability — never make CI depend on a real token being present as a
  secret unless that's an explicit, separate decision.

## 2. `workspace.write_file` and `workspace.delete_file`

### 2.1 Why

`read_file` already exists and is safe (path-traversal checked via
`safe_path`, 2 MiB size cap). Writing currently requires falling back to
`terminal.run` with a shell redirect, which has none of those guarantees and
is a strictly worse tool for the common case of "edit this file."

### 2.2 Implementation — `src/mcp/workspace.rs`

```rust
/// Maximum size, in bytes, of content writable via `write_file`.
const MAX_WRITE_FILE_BYTES: usize = 5 * 1024 * 1024;

pub fn write_file(&self, relative: &str, content: &str) -> Result<()> {
    if content.len() > MAX_WRITE_FILE_BYTES {
        bail!("content exceeds {MAX_WRITE_FILE_BYTES} bytes");
    }
    let path = self.safe_path(relative)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Atomic write, same pattern as memory.rs/tasks.rs/connectors.rs
    // (StoreLock isn't needed here — a single file write is already atomic
    // via tempfile+rename; StoreLock exists specifically for read-modify-write
    // cycles across a *shared* JSON store, which a single file overwrite is not).
    let dir = path.parent().context("target has no parent directory")?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut temp, content.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(&path).map_err(|e| e.error)?;
    Ok(())
}

pub fn delete_file(&self, relative: &str) -> Result<bool> {
    let path = self.safe_path(relative)?;
    if !path.is_file() {
        return Ok(false);
    }
    fs::remove_file(&path)?;
    Ok(true)
}
```

Reuse the existing private `safe_path` — it already rejects absolute paths
and `..` components; don't reimplement that check.

### 2.3 Dispatcher wiring

```json
{"name":"workspace.write_file","description":"Write (create or overwrite) a workspace file","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}}
{"name":"workspace.delete_file","description":"Delete a workspace file","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}
```

These two are small enough to add to whichever existing array
(`core`/`extended`) has headroom after Section 1's additions — check with a
local `cargo expand` or just count entries; don't assume, verify (see 1.5's
lesson).

`audit_allow("workspace_write", path, "")` / `audit_allow("workspace_delete", path, "")`
before executing — file mutation is exactly the kind of action the audit log
exists for.

### 2.4 Tests

Mirror the size-limit and path-traversal test style already used throughout
this codebase (`memory.rs`'s `store_rejects_oversized_content`,
`connectors.rs`'s path/field validation tests): a write within limits
succeeds and is readable back via `read_file`; a write exceeding
`MAX_WRITE_FILE_BYTES` is rejected; a `path` containing `..` or an absolute
path is rejected (reuse whatever test already exists for `read_file`'s
`safe_path` rejection, if one exists — check first before duplicating).

## 3. Small completeness gaps

### 3.1 `tasks.get`

Add `pub fn get(&self, id: &str) -> Result<Option<Task>>` to `TasksMcp`
(same shape as `MemoryMcp::get`), and a `tasks.get` dispatcher tool with
schema `{"id":{"type":"string"}}`, required `["id"]`.

### 3.2 `connectors.get`

Same shape, on `ConnectorsMcp`: `pub fn get(&self, id: &str) -> Result<Option<Connector>>`,
dispatcher tool `connectors.get`.

### 3.3 CLI/TUI exposure of Composio account management (follow-up, not required this phase)

`connector.composio_link/accounts/register/remove` (PR #9) are MCP-tool-only
— reachable by an agent, not by a human without an MCP client. Once this
phase's GitHub work establishes (or re-confirms) the pattern for wiring a new
external API into the CLI (`src/main.rs`) the same way `community_registry`
and `tunnel` already are, apply the same treatment to `ComposioRegistry` +
`ComposioAuth`: `awh connector composio link --toolkit github --user-id ...`,
`awh connector composio accounts`, `awh connector composio register --label ... --account-id ...`,
`awh connector composio remove --label ...`. Not blocking — call it out as
the next natural phase rather than scope-creeping it into this one.

## 4. Suggested tools beyond this phase's scope (backlog, not required)

- `github.pr_diff` / `github.pr_files` — fetch the diff or changed-files list
  for a PR without shelling out to local git (useful when reviewing a PR
  that isn't checked out locally).
- `github.compare` — compare two branches/commits (`/compare/{base}...{head}`).
- `github.repo_search` — search code/issues across a repo or org
  (`/search/code`, `/search/issues`) for an agent doing discovery across a
  large codebase it hasn't fully loaded into context.
- A generic `connector.composio_search_tools` wrapping
  `COMPOSIO_SEARCH_TOOLS` would let an agent discover *other* Composio
  toolkits (Slack, Linear, Notion, etc.) the same self-service way this
  document specifies for GitHub, without hand-writing a first-class provider
  for every one of them. Worth it once more than 2-3 non-GitHub connectors
  are requested in practice — until then, `connector.invoke` already covers
  the long tail via Composio.

## 5. Verification checklist (apply to every section above)

1. `rustfmt --edition 2021 --check <every touched file>` — cheap, catches
   syntax errors even without a full toolchain available.
2. If a modern Rust toolchain (1.85+, edition2024-capable — check with
   `rustc --version`) isn't available locally, don't fight it: push to a
   branch and let this repo's own CI (`.github/workflows/rust.yml`, which
   installs `stable` fresh on every run) do the `cargo check`/`clippy --
   -D warnings`/`cargo test --all-targets` verification across
   ubuntu/macos/windows. That CI run is the actual source of truth for
   whether this compiles and passes — treat a clean local `rustfmt` pass as
   necessary but not sufficient.
3. Watch specifically for the macro-recursion-limit failure mode from
   Section 1.5 — it manifests as `cargo check` failing (not just `clippy`),
   which means `cargo test` never even runs; the CI failure will say
   `recursion limit reached while expanding` in the `error:` line, not
   inside any test output.
4. Watch for platform-specific test flakiness the way PR #9 hit on Windows
   (a `create_new`/delete race surfacing as `PermissionDenied` instead of
   `AlreadyExists`) — if a new test does its own file-locking or timing-
   sensitive work, don't assume Unix error-kind semantics carry over to
   Windows; let CI's windows-latest job be the judge.
5. Open a PR against `rust` (not a direct push) so the `pull_request`
   workflow trigger runs the full matrix automatically; iterate by pushing
   fix commits to the same branch rather than opening new PRs per fix.

## 6. Acceptance criteria

- `github.*` tools (Section 1) appear in `tools/list` only when `GITHUB_TOKEN`
  is set, and successfully round-trip against a real repo when it is (manual
  smoke test acceptable for v1; automated coverage per 1.7).
- `workspace.write_file`/`workspace.delete_file` (Section 2) pass path-
  traversal and size-limit tests and are documented in `docs/mcp.md`'s tool
  table (that table has previously gone stale — see the commit
  `docs(mcp): correct tool catalog to 44 tools and complete the table` in
  this branch's history; update the count and table together in the same
  commit as the code change, not as an afterthought).
- `tasks.get`/`connectors.get` (Section 3) have unit tests following the
  existing `store.get(...)` test patterns in `memory.rs`.
- CI (`fmt`, `clippy -D warnings`, `cargo test --all-targets` on all three
  platforms, dependency audit) is green on the PR before merging — no
  exceptions, per the precedent set in PR #9.
