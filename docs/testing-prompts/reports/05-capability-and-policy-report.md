# TP05 — Capability & Policy Engine: Evidence Report

**Prompt:** `docs/testing-prompts/05-capability-and-policy.md`
**Base:** `rust` @ `9c04ee7` (post-merge of PR #141)
**Branch:** `tp05-capability-policy-verification`
**Method:** TP01–TP04 methodology — repository forensics of every
authorization layer, then a black-box verification suite that treats AWH
as a real product and the resource itself as the oracle. Every denial is
proven by the untouched resource (file bytes never changed, marker file
never created); every allow is proven by the mutated resource. No test
accepts an exit code or a JSON boolean as its only evidence.

**Suite (new):**

| Suite | Tests | Scope |
|---|---|---|
| `tests/capability_policy_boundaries.rs` | 12 | The authorization gaps the per-layer suites leave open: CLI↔enforcement coherence, least privilege at the gate, malformed/corrupt state fail-closed at enforcement, policy matching precision, precedence cell, live expiry, concurrent revoke, deterministic re-grant |

**Existing suites this prompt relies on (re-verified green, not
duplicated):**

| Suite | Tests | What it already pins |
|---|---|---|
| `tests/mcp_builtin_tool_gate.rs` | 43 | SEC-001 `awh.builtin` floor: High-risk default-deny, least-privilege trust records, coarse-gate-before-policy ordering, trust CLI round-trip, audit allow/deny, unreadable trust store fails closed |
| `tests/mcp_policy_gate.rs` | 15 | Deny rules reject before service for all resource-scoped tools; precise matching; CLI deny→list→remove round-trip; corrupt policy store fails closed; coarse gate runs before policy |
| `tests/mcp_agent_routes.rs` | 19 | Route resolution (unknown/inactive/disabled/malformed agents reject before session creation), session binding immutability, workspace mismatch, discovery filtering ≠ authority, scoped/expired grants, two-agent concurrency, policy deny overrides capability allow |
| `tests/acceptance_edit_gates.rs` | 3+ | Edit-plane capability gate: zero side effects (bytes, provenance, snapshots), scoped grants, rollback denial |
| `tests/trust_wedge_e2e.rs` | 1 (composed) | The full wedge: identity → capability → trust → policy → service → durable audit, live revocation mid-session, policy override, restart durability with correlated identities, worktree isolation |
| `tests/agent_cli.rs` | 4 | CLI grant/revoke persistence, invalid permission, unknown agent, unsafe ids |
| `src/services/authorization.rs` unit tests | 18 | `scope_covers` component semantics, malformed expiry fail-closed, precedence chain, principal/workspace binding, determinism |

**Gates:** `cargo fmt` clean, `cargo clippy --all-targets -- -D
warnings` clean, `cargo test --workspace` green (includes the 12 new
tests).

---

## Authorization layers established by forensics (§2)

The current branch has FIVE authorization layers, in enforced order for a
bound MCP caller:

1. **Route/identity resolution** (`src/mcp/agent_route.rs`) —
   `resolve_route_agent` structurally validates the id, requires the
   agent record to exist, be `Active` AND enabled (via the canonical
   TW-002 `resolve_session_relation`), and binds the workspace manifest
   identity. The caller identity is *constructed here from trusted
   state*, never taken from the request. `set_caller` on the lifecycle
   is write-once — a session can never migrate agents.
2. **Per-agent capability gate** (TW-003, dispatcher `-32005`) — for
   bound callers only: `tools/call` reads the caller's live grants
   (`CapabilityGrantStore.list_for_agent` re-read per call — revocation
   and expiry are live), requires the tool's declared
   `required_permissions`, checks `grant_is_expired` (malformed expiry
   fails closed) and `scope_covers` (component-boundary prefix,
   traversal denies).
3. **Built-in tool trust floor** (SEC-001, `awh.builtin`, `-32003`) —
   coarse, workspace-independent category gate for every Medium/High
   static tool. No record = Medium allowed, High denied. Unbound
   (legacy `/sse`) sessions have ONLY this floor — the documented
   architectural boundary (§15 of the prompt), pinned by
   `legacy_unscoped_session_has_no_binding`.
4. **Workspace-local policy** (`.agent/policy.json`, `-32004`) —
   DENY-only, for exactly the tools whose call carries a resource
   (`workspace.write_file`/`workspace.delete_file` by `path`,
   `terminal.run` by exact `program`, and the six `filesystem.*` edit
   tools by `path`).
5. **Service boundary** — `EditAuthorizer`
   (`src/services/authorization.rs`) is the single
   transport-independent authorization point for the edit plane
   (CLI/MCP/API all converge); `McpPermissions` gates custom MCP
   servers at the execution gate.

Gate order (pinned by tests, not inferred from names): capability →
builtin trust → policy → service. A capability denial means the policy
layer is never consulted — now explicitly pinned by
`missing_capability_wins_over_matching_policy_deny_rule`.

---

## Capability evidence

### Passed

**§8/§18 — CLI↔enforcement coherence (the cross-surface property)**
- `cli_created_grant_authorizes_and_cli_revoke_removes_across_restart` —
  a grant written by the REAL `awh agent grant` binary (with
  `--scope src`) authorizes the MCP boundary: the in-scope write really
  mutates the file; the out-of-scope write is `-32005` with no file; the
  same binary's `agent revoke` (derived id `alpha-filesystem`) removes
  authority for a FRESH dispatcher over the same root (the restart
  path) and the revoked write leaves the probe bytes untouched.
  `agent_cli.rs` proves the CLI persists records; the route suite proves
  the gate enforces records; this test proves they are the SAME
  records.
- `re_granting_the_same_pair_overwrites_deterministically` —
  re-granting the same agent/permission pair through the CLI overwrites
  in place: exactly one record afterward, carrying the LATEST scope,
  rendered identically by `agent inspect`. Duplicate ids are
  deterministic (store is keyed by id).
- `cli_grant_rejects_bad_input_without_mutating_the_store` — invalid
  permission vocabulary and unknown agent both exit non-zero with stable
  messages (`invalid permission "…"`, `agent not found: ghost`), and
  the store stays empty for both agents. Revoke-missing is the pinned
  idempotent shape (exit 0 + explicit `grant not found: …` — see
  Contract facts).

**§7 — least privilege at the gate**
- `capability_categories_never_imply_one_another` — a Process-only
  grant does NOT enable `workspace.write_file` (denied, bytes
  unchanged) and a Filesystem-only grant does NOT enable `terminal.run`
  (denied, marker never created) — with a positive control: the Process
  holder runs the IDENTICAL command and the marker exists. `tools/list`
  mirrors the truth: the Process holder does not see Filesystem tools.
- `expired_cli_shaped_grant_never_authorizes` — past-expiry grant
  denies (no file); far-future grant allows (file exists). Same record
  shape, only the timestamp differs.

**§10 — expiry is enforced live**
- `a_grant_expires_live_between_two_calls` — a grant expiring now+2s
  authorizes the first call (file written), and 2.5s later the
  IDENTICAL call from the SAME session is `-32005` with the file
  untouched. Expiry takes effect without restart or re-grant — the
  gate re-reads and re-compares wall time on every call
  (`expiry <= now`, so the instant itself is already expired).
- `malformed_expiry_fails_closed_at_enforcement` — an unparseable
  `expires_at` denies at the enforcement boundary (a broken grant
  never becomes an eternal one), bytes unchanged.

**§13/§27 — fail-closed at enforcement, on both surfaces**
- `corrupt_grant_store_fails_closed_at_call_and_listing` — a garbage
  JSON grant file makes `tools/call` return a JSON-RPC error (never a
  false allow, never a panic) with no file written, AND collapses
  `tools/list` to the empty static catalog for that caller ("announce
  nothing you cannot authorize" — the code path existed; this pins it).
- Corrupt policy store and absent trust floor were already pinned
  (`corrupt_policy_store_fails_closed`, `no_record_denies_*`) —
  re-verified green.

**§11 — policy matching precision on the wire**
- `policy_path_patterns_match_components_not_substrings` — deny
  `src/foo` → `src/foo/x.txt` denied (-32004, no file), the exact
  resource `src/foo` denied, but the sibling `src/foobar.txt` ALLOWED
  (component semantics — substring/prefix-byte matching would wrongly
  deny it); an empty pattern denies nothing.
- `traversal_shaped_policy_pattern_matches_nothing_legitimate` — a
  `src/../etc` pattern never policy-denies a legitimate in-workspace
  path.

**§12 — precedence (the missing matrix cell)**
- `missing_capability_wins_over_matching_policy_deny_rule` — no
  capability + matching deny rule → `-32005` (capability), NOT
  `-32004`: the policy layer is never the surfaced verdict for a
  caller with no capability; the error names the caller. The inverse
  cell (capability allow + matching deny → `-32004`) is pinned by
  `policy_deny_overrides_capability_allowance`; no-cap + no-deny and
  trust-layer cells are pinned by the builtin-gate and route suites.

**§19 — concurrency**
- `revoking_one_agents_grant_never_disturbs_a_concurrent_other_agent` —
  alpha's grant revoked, then both agents dispatch concurrently
  (`tokio::join!`): alpha denied (no file), beta allowed (file landed)
  — independent decisions from independent grants, no cross-principal
  effect, no lost grant.

**§6/§16/§17 — identity, route, discovery (existing, re-verified)**
- Route names, `tools/list` visibility, and possession of ids never
  become authority: unknown/inactive/disabled agents reject before
  session creation; a listed-but-ungranted tool re-authorizes and fails
  closed on direct call; session bindings never migrate; workspace
  mismatch is a distinct failure.

**§14 — zero-side-effect denials (every new test)**
- Filesystem denials assert exact bytes unchanged; process denials
  assert the marker file never exists; every denial fires before the
  service runs.

**§20 — audit (existing, re-verified)**
- `trust_wedge_e2e` pins allow+deny events with full
  workspace/agent/session correlation, durable across a fresh
  `AuditLog::open`, newest-first ordering, no secret values (the audit
  choke point redacts token-shaped text; capability denials name agent
  + tool + missing category only).

### Not implemented (honestly classified — §1/§22)

The final-target command vocabulary does not exist on this branch; the
current narrower contract was tested instead (per §2, test the real
narrower contract and record the gap):

- `awh capability list|show|grant|revoke|check` — **Not implemented as a
  separate family.** The implemented equivalents are `awh agent grant
  --permission <category> [--scope <prefix>]` and `awh agent revoke
  <grant-id>`, plus `awh agent inspect <id>` (renders grants). Tested
  through the real binary.
- `awh policy show|check|validate|explain` — **Not implemented.**
  Implemented: `awh policy deny|list|remove` (pinned by
  `mcp_policy_gate.rs` CLI round-trip).
- Capability granularity `filesystem.read` vs `filesystem.write` vs
  `git.read` etc. — **Not implemented.** The implemented vocabulary is
  the five-category `Permission` enum (network, filesystem,
  environment, process, secrets) shared between grants and tool
  metadata via `required_permissions`. The final-target fine-grained
  resource/action model is roadmap; per §31 no test fabricates it.
- A `PolicyEngine` type with allow rules, rule precedence, or
  approval flows — **Not implemented.** Policy is DENY-only with
  first-match-wins; no allow rule can ever widen authority (only
  capability grants can allow).
- `awh mcp block|revoke|permissions` — **Not implemented** as such;
  the implemented trust surface is `awh mcp trust <id> [--network
  --process --filesystem]` + `awh mcp revoke <id>` (pinned by the
  builtin-gate suite's CLI round-trip tests).

### Unproven

- **Exactly-at-expiry as a standalone instant.** The comparison is
  `expiry <= Utc::now()` (unit-pinned in `authorization.rs`), so the
  boundary instant is already expired — but observing the exact
  wall-clock instant from a black-box test is a race by construction.
  The deterministic before/after pair
  (`a_grant_expires_live_between_two_calls`) plus the unit pin is the
  honest evidence. No injectable clock exists (§10 would prefer one).
- **Linux ARM64 / macOS ARM64 / Android-Termux runtime legs.** CI
  covers ubuntu/macos/windows x86_64; the other platforms are compile-
  verified only by CI's build matrix where present. Classifying per
  §30: no runtime evidence on those legs.

### Failed

None. No production defect was found in the capability/policy engine on
this round. (The one candidate — `agent revoke` of a missing grant
exiting 0 — turned out to be deliberately pinned idempotent behavior in
`tests/agent_cli.rs`, not a defect; see Contract facts.)

### Blocked

None. Every authorization boundary is testable in the sandbox with
synthetic state; no credential-gated surface was needed (the github.*
connector family is gated by `GITHUB_TOKEN`, which is out of this
prompt's scope per §36 and covered by the builtin trust floor).

---

## Contract facts discovered while testing (pinned, do not "fix")

1. **`agent revoke` of a missing grant exits 0** with
   `grant not found: <id>` on stdout — deliberately idempotent, pinned
   by `tests/agent_cli.rs` ("revoking a missing grant must not fail").
   This is a *documented exception* to the repo's ghost-id `bail!`
   discipline (which lists skill read/uninstall, registry add-dup/
   remove-missing, mcp remove/enable/disable/uninstall — not agent
   revoke). Re-granting the same pair is likewise an overwrite, and both
   are now pinned from the enforcement side too.
2. **Grant ids are derived, not free-form**: `awh agent grant` keys the
   record as `{agent_id}-{permission}` — one grant per agent/category
   pair, predictable for `agent revoke`, and re-granting updates in
   place. The store itself (`create()`) accepts arbitrary safe ids.
3. **`grant_is_expired` is `expiry <= now`** — the boundary instant is
   already expired; unset `expires_at` never expires; malformed
   timestamps fail closed (treated as expired).
4. **The capability gate runs before the policy layer** for bound
   callers — a no-capability caller surfaces `-32005` even when a deny
   rule also matches (previously implied by ordering code, now pinned
   on the wire).
5. **`tools/list` fails closed to the EMPTY static catalog** when the
   grant store is unreadable (dynamic provider tools remain, none are
   configured by default) — the listing can never advertise more than
   it can authorize.
6. **`awh.builtin` trust is keyed to the home dir** (`~/.agent-workspace-hub`
   or `AWH_TRUST_DIR`), NOT the workspace — it is a per-user-machine
   floor; policy and grants are per-workspace. Tests isolate it via
   `AWH_TRUST_DIR` or injected stores.
7. **Unbound (legacy `/sse`) sessions are intentionally outside the
   per-agent capability gate** — they rely on the SEC-001 floor alone.
   This is the documented architectural boundary (§15): the capability
   gate exists precisely because the legacy route cannot establish an
   agent principal. Pinned by `legacy_unscoped_session_has_no_binding`.

---

## Test-engineering notes

- Bound sessions are constructed exactly as `/{agent}/sse` does:
  `resolve_route_agent` (trusted resolution) →
  `CallerContext::to_session_identity` → write-once `set_caller`. No
  test supplies a client-side identity string.
- The full built-in trust floor (`awh.builtin` with network+process+
  filesystem) is injected via `with_trust_store` so the capability and
  policy layers are the deciding layers — mirroring
  `trust_wedge_e2e::builtin_trust`.
- Workspaces are leaked `ManuallyDrop<TempDir>`s (same pattern as the
  http/wedge suites): the process-wide durable audit store holds the
  first-bound root for the life of the test binary.
- The live-expiry test's 2.5s bounded wait is the prompt-sanctioned way
  to observe a real temporal transition (§10: no long sleeps); every
  other test is sleep-free.
- No test reads a secret, a real HOME trust store, or anything outside
  its temp dirs; CLI children are spawned hermetically via
  `common::sanitized_command` (all `AWH_*` + provider credentials
  stripped).
- Cross-platform: process probes are cfg-gated (`touch` / `cmd /C type
  nul`), matching `mcp_builtin_tool_gate.rs`; no autocrlf-sensitive
  fixtures exist in this suite (no git fixtures needed).

---

## Completion criteria (§35) — status

| Criterion | Status |
|---|---|
| Current capability/policy implementation inspected | Yes (five layers established with sources; § sources cited in suite header) |
| Authorization layers distinguished | Yes (identity → capability → builtin trust → policy → service) |
| Capability vocabulary enumerated from source | Yes (5-category `Permission`; fine-grained model classified Not implemented) |
| Principal identity binding tested | Yes (existing route suite + trusted-identity session construction in every new test) |
| Least privilege tested | Yes (`capability_categories_never_imply_one_another` + positive control) |
| Grant lifecycle tested | Yes (CLI coherence, deterministic overwrite, rejections, store-empty residue) |
| Revocation tested | Yes (CLI revoke across fresh dispatcher, zero side effects; existing live-revocation wedge) |
| Scope tested | Yes (CLI-scope survives to enforcement; existing component-boundary suites) |
| Expiry tested | Yes (live before/after transition, past/future shapes, malformed fails closed) |
| Policy lifecycle & matching tested | Yes (existing CLI round-trip; new component-precision, empty-pattern, traversal-pattern) |
| Authorization precedence tested | Yes (missing cell added; all cells covered across suites) |
| Default-deny/fail-closed tested | Yes (corrupt grant store at call AND listing; malformed expiry; existing corrupt policy/trust) |
| Denied consequential ops have no unintended side effects | Yes (bytes/marker oracle in every new test) |
| Internal/direct bypass resistance | Yes at the strongest public boundary (dispatcher authorize path is the single gate; EditAuthorizer is the shared service boundary; unbound-session floor documented per §15) |
| MCP route/discovery independence | Yes (existing suite, re-verified) |
| Persistence across restart tested | Yes (fresh dispatchers over same root; CLI subprocess writes) |
| Concurrent state changes tested | Yes (concurrent two-agent revoke) |
| Audit behavior checked at the boundary | Yes (existing wedge suite re-verified; no duplication) |
| Environment variables tested safely | Yes (`AWH_TRUST_DIR` in trust CLI tests; `GITHUB_TOKEN` classified as discovery-only, never authority) |
| Security regressions permanently covered | Yes (all 12 tests are permanent regression pins) |
| Final-target gaps honestly classified | Yes (capability/policy command families, fine-grained model, PolicyEngine allow rules) |
| No real secrets used | Yes |
| No unrelated test master prompt modified | Yes |
