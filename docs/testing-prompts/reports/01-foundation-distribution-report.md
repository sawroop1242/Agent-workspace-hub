# Master Test Prompt 01 — Foundation & Distribution: Completion Report

- **Feature:** Foundation & Distribution (CLI skeleton, `awh init/version/status`, configuration precedence, install/upgrade, release artifacts)
- **Branch:** `testing-tp01-foundation` (from `rust` @ caade1b)
- **Date:** 2026-09-27
- **Suite:** `tests/foundation_cli.rs` (19 executable black-box tests against the real compiled `awh` binary), plus manual release-artifact/installer verification recorded below.

## Implementation status

Implemented on the current branch: `awh init` (+`--path`), `awh --version`/`-V`, `awh status`, `awh mcp serve` (stdio/SSE transports), `scripts/install.sh`, `release-rust.yml` artifact + checksum workflow, `.agent/workspace.json` durable identity.

**Not implemented** (documented roadmap/CLI.md items, not part of the current shipped contract; reported, not built — out of TP01 verification scope): `awh doctor`, `awh config`, `awh completion *`, installer `uninstall` command, android-aarch64 asset for v0.1.0 (workflow builds it; the published v0.1.0 release does not include it).

## Real interfaces exercised

- Real compiled binary via `CARGO_BIN_EXE_awh` subprocess (never in-process mocks) for every CLI assertion.
- Child-environment construction per invocation (`Command::env`); the test process's global environment is never mutated (parallel-safe).
- Real filesystem: `tempfile` roots, real `.agent/workspace.json` bytes, spaces/Unicode/nested/traversal paths.
- Real network boundaries: TCP `TcpListener` for port-occupation proofs; real stdio MCP JSON-RPC session for the resource-limit test.
- Real external HTTP: GitHub Releases API + release asset downloads (installer tests).
- Fresh-process boundaries: each `run()` spawns a new process; persistence proven across 3 fresh processes.

## Environment variables / configuration used

All values synthetic; no real credentials appear in source, tests, logs, or this report. Token-shaped probe: `ghp_AWHTEST synthetic-LEAKCHECK-…` (deliberately non-functional).

| Variable/category | Default tested | Override tested | Invalid tested | Secret-safe |
|---|---:|---:|---:|---:|
| `AWH_PORT` | yes (8443 path via missing-key probe) | yes (occupied-port bind proof; CLI `--port` precedence proof) | yes (`not-a-number`, `65536`, `-1`, huge, `""`, `"  "`) | yes |
| `AWH_HOST` | yes | yes (`127.0.0.1` bind) | yes (invalid host → deterministic bind error) | yes |
| `AWH_API_KEY` | yes (absence) | presence (synthetic) | yes (empty/whitespace → fail closed) | yes (leak sweep across 7 CLI surfaces) |
| `AWH_TLS_CERT` / `AWH_TLS_KEY` | yes (unset → plain HTTP path not asserted beyond config layer) | yes (fake PEM paths) | yes (half-configured both directions; both-set invalid material) | yes |
| Resource limits (`AWH_MAX_MCP_LINE_BYTES` representative) | yes | yes (via `config.rs` injected-source unit tests, pre-existing) | yes (`not-a-number` → `config_invalid` on stderr, server keeps serving on conservative defaults) | yes |
| `AWH_NGROK_AUTHTOKEN`, `AWH_BWRAP` | contract-level only | contract-level only | not tested here | yes (grep-verified consumption sites) |

`AWH_NGROK_AUTHTOKEN` (tunnel family) and `AWH_BWRAP` (sandbox family) belong to other feature families; TP01 §5/§25 defers their full runtime verification. Their parsing/precedence contracts live with `tunnel start` and the MCP sandbox; the trust gate in front of `terminal.run` was incidentally re-verified to fail closed (`-32003`, no trust record), which protects the sandbox boundary.

## Results

### Passed (direct evidence)

| # | Verification | Evidence |
|---|---|---|
| 1 | `awh --version`: exit 0, prints `awh 0.1.0` matching Cargo.toml, stable across runs, works uninitialized | `version_flag_reports_crate_version_without_workspace_or_secret_leak` |
| 2 | `awh status`: exit 0 uninitialized and initialized, no state creation, byte-stable output | `status_succeeds_on_uninitialized_and_initialized_directories` |
| 3 | `awh init`: fresh dir, durable `.agent/workspace.json`, `ws-` id | pre-existing `tests/init_cli.rs` (11 tests) + `foundation_state_persists_across_fresh_processes` |
| 4 | `awh init`: spaces, Unicode, deep-nested, traversal-lexically-resolved, unrelated files preserved | 4 foundation tests |
| 5 | `awh init` re-invocation idempotent; corrupt/unsupported/foreign manifests fail closed without reset | `tests/init_cli.rs` (pre-existing, re-run green) |
| 6 | Unwritable root (`/proc/...`) → non-zero, clean error naming the failure, no panic | `init_fails_cleanly_on_unwritable_root` |
| 7 | Config precedence `default < AWH_PORT < --port` proven via occupied-port bind errors | `sse_awh_port_override_...`, `sse_cli_flag_overrides_awh_env_for_port` |
| 8 | Invalid `AWH_PORT` (non-numeric, >u16, negative, empty, whitespace-only) → non-zero exit + error naming `AWH_PORT`, never silent fallback | `sse_invalid_awh_port_...`, `sse_awh_port_boundary_values_...` (regression for the fixed defect) |
| 9 | Invalid `AWH_HOST` → deterministic bind failure, no secret in error | `sse_invalid_awh_host_...` |
| 10 | `AWH_API_KEY` missing/empty → fail closed before serving, error names the variable | `sse_missing_or_empty_api_key_...` |
| 11 | TLS half-configured (either direction) → non-zero + explicit error; both-set invalid PEM → no silent plain-HTTP downgrade | `sse_tls_half_configured_fails_closed_both_ways` |
| 12 | Invalid resource limit (`AWH_MAX_MCP_LINE_BYTES=not-a-number`): real stdio MCP session still completes `initialize`, stderr carries `config_invalid` naming the variable | `stdio_server_reports_invalid_resource_limit_and_keeps_serving` |
| 13 | Persistence: 3 fresh processes observe identical workspace id; manifest bytes stable | `foundation_state_persists_across_fresh_processes` |
| 14 | Concurrency: two workspaces initialized in parallel → distinct ids, zero cross-contamination | `two_workspaces_initialize_concurrently_and_stay_isolated` |
| 15 | Secret-safety: synthetic token never appears in stdout/stderr across `--help`, `status`, `--version`, `init`, `mcp --help`, `mcp serve --help`, `agent --help` | `foundation_outputs_never_leak_token_shaped_secrets` |
| 16 | Failures are clean typed errors: no `panicked`, no `RUST_BACKTRACE`, deterministic clap usage errors (exit 2) for unknown subcommands | `foundation_failures_are_clean_errors_not_panics` |
| 17 | Install (isolated prefix): platform detection linux/x86_64, asset `awh-linux-x86_64`, checksum **verified against `sha256sums.txt`**, binary executes `awh --version` = 0.1.0, `init`/`status` work from installed artifact independent of source checkout | manual: `AWH_PREFIX=/tmp/tp01-prefix bash scripts/install.sh` |
| 18 | Upgrade-in-place: reinstall over prefix, user file preserved, binary replaced, version correct | manual (second install run) |
| 19 | Checksum tamper rejection: byte-flipped asset → `Checksum mismatch for awh-linux-x86_64: expected 332e…, got 4e1d…`, exit 1, **no `awh` link installed** | manual tamper simulation |
| 20 | Missing-checksum release → install aborts (`refusing to install unverified bytes`), exit 1, does NOT silently fall back to source build | manual (pre-checksum-upload run) |
| 21 | Release artifacts: 5 assets present, all verify against `sha256sums.txt`; format verified per-platform (ELF x86_64/ARM64, Mach-O 64 LE ×2, PE/Windows) | `sha256sum -c` + magic-byte inspection |
| 22 | `mcp serve --transport stdio` protocol discipline: stdout JSON-RPC only (diagnostics on stderr) | `stdio_server_reports_invalid_resource_limit_and_keeps_serving` + pre-existing `mcp_executable.rs` |

### Failed → fixed (defects found in production code)

1. **`AWH_PORT` non-numeric values were silently ignored** (`src/main.rs` `serve_sse`): `.parse().ok()` fell back to 8443, violating the documented invariant that invalid configuration is never silently ignored (docs/configuration.md). Fix: parse with an explicit error — `invalid value for AWH_PORT: …`, startup fails closed. Regression tests: `sse_invalid_awh_port_fails_closed_with_clear_error` + boundary table test. **Re-tested: green.**
2. **Installer never verified release checksums** (`scripts/install.sh`): downloaded bytes were trusted unconditionally; FEATURES.md/release.md promise checksums as part of the distribution contract, and the release workflow publishes `sha256sums.txt`. Fix: mandatory `verify_checksum` — missing checksums file, missing per-asset entry, tool absence, or hash mismatch aborts the install (exit 1); integrity failures never fall back to a source build (which would mask tampering). Re-tested: happy path green, tamper rejected, missing-checksum release rejected.
3. **Release hygiene gap (process, not code):** v0.1.0 was published without running the `checksums` workflow job, so `sha256sums.txt` was absent despite the workflow supporting it. Remediation: computed SHA-256 from the published assets and attached `sha256sums.txt` to the v0.1.0 release (verified: all 5 assets OK). Future releases must run the full workflow.

### Blocked

- **Uninstall:** the current contract (FEATURES.md: "Install, upgrade, uninstall, release artifacts and checksums") lists uninstall, but `install.sh` implements no uninstall command. Classified **Not implemented** (see below) — cannot test what does not exist; no production fix was made because TP01 forbids implementing missing features merely to pass tests. Recommendation: `--uninstall` flag deleting only prefix-installed files.
- **`awh doctor` / `awh config` / `awh completion *`:** documented in CLI.md/roadmap but **Not implemented** in the binary (clap: `unrecognized subcommand`, exit 2). Per README §3 these are roadmap (phase-0-adjacent) surface, not shipped behavior; building them is feature work outside a verification prompt. Recorded as a gap for the maintainer.

### Unproven (cannot execute in this environment)

- **macOS x86_64/ARM64 assets:** structurally verified (Mach-O 64 LE magic + checksum) but not runtime-verified — no macOS host in this sandbox.
- **Windows x86_64.exe:** structurally verified (PE magic + checksum) but not runtime-verified — no Windows host.
- **Android/Termux ARM64:** no `awh-android-aarch64` asset exists on the v0.1.0 release (although the release workflow matrix builds it), and no Android runtime is available. Both a distribution gap and an execution Blocked platform.
- **Archive extraction escape / "artifact contains only expected material":** not applicable — release assets are raw single binaries, not archives; the sha256 verification covers content integrity.

## Cross-platform matrix

| Target | Build | Structural | Runtime |
|---|---|---|---|
| linux-x86_64 | yes (CI) | yes (ELF) | **verified** (installed + executed + full CLI suite on this host) |
| linux-aarch64 | yes (CI) | yes (ELF/ARM64) | not runtime verified (no ARM host) |
| macos-x86_64 | yes (CI) | yes (Mach-O 64 LE) | not runtime verified |
| macos-aarch64 | yes (CI) | yes (Mach-O 64 LE) | not runtime verified |
| windows-x86_64 | yes (CI) | yes (PE) | not runtime verified |
| android-aarch64 | workflow target | asset absent from v0.1.0 | not runtime verified |

## Test-environment notes (honest disclosure)

- A test-fixture bug was found and fixed during development, not a product defect: an early version of the `AWH_PORT` whitespace test bound a real server on 127.0.0.1:9 (this container permits low-port binds), hanging `.output()`. Rewritten so the proof (config parsed, startup then fails on the missing API key) can never reach the bind phase. Lesson recorded: any test that *can* start a serving process must fail it before bind.
- Tests run as an unprivileged user; the `/proc` unwritable-root probe gives a deterministic permission failure that does not depend on `chmod` (which would not affect root).

## Gates (all green after fixes)

```
cargo fmt --all            # clean
cargo clippy --all-targets -- -D warnings   # clean
cargo test --workspace     # 1352 passed, 0 failed (was 1350 before TP01)
bash -n scripts/install.sh # clean
```

## Files changed

- `src/main.rs` — `AWH_PORT` fail-closed parse fix (serve_sse).
- `scripts/install.sh` — mandatory sha256 checksum verification; integrity failures abort (no source-build fallback); doc comment.
- `docs/configuration.md` — document `AWH_PORT` invalid-value behavior.
- `tests/foundation_cli.rs` — new 19-test TP01 suite.
- `docs/testing-prompts/reports/01-foundation-distribution-report.md` — this report.
- (Release-side, outside the repo: `sha256sums.txt` attached to the published v0.1.0 GitHub release.)

## Remaining limitations

1. `awh doctor`, `awh config`, `completion`, installer `uninstall` — not implemented (roadmap items).
2. Cross-platform runtime verification for macOS/Windows/ARM64/Android requires those hosts (CI matrix runs unit/integration tests, but the *release binary* runtime behavior is only proven on linux-x86_64).
3. Installer release-mode tests depend on network + the published GitHub release; they are reproducible but not wired into `cargo test` (documented manual steps above).
4. `AWH_NGROK_AUTHTOKEN`/`AWH_BWRAP` runtime behavior deferred to their feature families per TP01 §25.

TP01 is complete per README §"Important rule": all current-contract behavior either passed with real evidence or is explicitly classified with reasons.

## PR-review / CI round (PR #136 follow-up)

PR #136's first CI run failed `cargo test --all-targets` on all three OSes
(check + clippy passed). Job-log analysis plus local reproduction isolated
two environment-dependent tests and one genuine product defect:

1. **Product defect (fixed)** — `stdio_server_reports_invalid_resource_limit_and_keeps_serving`
   failed on all CI runners: the `config_invalid` warning for an invalid
   `AWH_MAX_MCP_LINE_BYTES` was only emitted from `circuit_breaker_config()`,
   which runs inside the *custom-MCP-server* registration loop. On a clean
   machine (no custom servers registered) the warning never fired, so an
   invalid limit was **silently ignored** — violating the documented
   "never silently ignored" contract. The test had passed locally only
   because this development container has user-level custom MCP servers
   registered from earlier phases. Fix: `McpDispatcher::construct` now parses
   the resource limits once up front (`breaker_config`), reporting
   `config_invalid` on every surface (stdio, SSE, TUI, Control API)
   regardless of how many custom servers exist. Verified with
   `env -u GITHUB_TOKEN cargo test …` (clean-machine simulation).
2. **Windows: `/proc` portability (fixed)** — `init_fails_cleanly_on_unwritable_root`
   used `/proc/not-writable-awh`; on Windows that resolves to
   `\\?\C:\proc\not-writable-awh` on a *writable* drive, so `awh init`
   succeeded and the test failed (CI log shows `initialized workspace \\?\C:\proc\...`).
   Replaced with a portable file-as-parent probe (`ENOTDIR` on every platform)
   and kept the true unwritable-filesystem case as a Linux-only variant
   (`/proc` does not exist on macOS and is a writable path on Windows).
3. **Test env hygiene (fixed)** — the stdio server test built the child with
   the raw parent environment, inheriting machine-local state
   (`GITHUB_TOKEN`, `COMPOSIO_API_KEY`, stray `AWH_*` values). It now strips
   those like every other test in the suite.
4. **Test robustness (fixed)** — child stderr is drained on a dedicated
   thread before `wait()` (a chatty child could otherwise fill the pipe
   buffer and deadlock the test); occupied-port helpers hold their listener
   alive for the whole child run instead of bind-drop-re-bind (racy);
   `status` is no longer pinned byte-identical across the init boundary
   (same-state determinism is still asserted).
5. **`AWH_PORT` set-but-empty (fixed)** — review flagged that
   `AWH_PORT="${SOME_PORT:-}"` (common CI/env_file pattern) aborted startup.
   Empty/whitespace-only values are now treated as unset (documented
   default `8443` applies); non-numeric values still fail closed. Docs
   updated in `docs/configuration.md`, `docs/security.md`, `docs/INSTALL.md`;
   boundary test covers both halves.
6. **`scripts/install.sh` hardening (verified)** — download goes to a
   `.part` file and only earns the real name after checksum verification
   (a failed verify never leaves unverified bytes installed);
   `resolve_tag` resolves the release API URL once and it is reused for both
   asset-URL and checksum lookup; `sha256sums.txt` entries in BSD `shasum`
   binary form (`*filename`) are normalized before matching; the trust
   boundary (integrity ≠ authenticity, no signatures in v1) is documented.
   All paths verified end-to-end against a mock release server:
   happy + `*`-form installs succeed; mismatch / missing-entry /
   missing-checksum-file abort with only the `.part` file left behind.

Review findings addressed: all 11 inline comments from the PR review
(empty-`AWH_PORT` semantics, env inheritance, stderr drain, port race,
cross-state equality pin, `/proc` portability, installer trust-boundary
documentation, checksum-entry normalization, release-API reuse,
unverified-bytes removal, plus the config_invalid product fix they
collectively surfaced).

### Round 2 (Kilo re-review of the fix commit)

The automated re-review of the fix commit raised 4 more findings —
all accepted and fixed:

1. **Stdio-test env sanitization was narrower than its comment claimed**
   (WARNING-class on tests): only the vars the test deliberately sets were
   stripped; the rest of the `AWH_*` resource-limit family
   (`AWH_MAX_HTTP_BODY_BYTES`, `AWH_MCP_REQUEST_TIMEOUT_SECS`,
   `AWH_HTTP_CLIENT_TIMEOUT_SECS`, `AWH_CIRCUIT_FAILURE_THRESHOLD`,
   `AWH_CIRCUIT_COOLDOWN_SECS`) could still leak machine-local values into
   the child. Introduced `SANITIZED_AWH_VARS` — the complete list of
   environment variables the server and dispatcher read — and applied it
   in both `run_env` (every spawned `awh` child in the suite) and the
   stdio test, with explicit `env(...)` sets applied after the strip.
2. **Default port 8443 was never pinned observably** — the empty-`AWH_PORT`
   loop proved "does not abort" but not "chose 8443". New test
   `sse_default_port_is_8443_when_awh_port_unset_or_empty` holds
   `127.0.0.1:8443` and asserts the bind error names `8443` for unset,
   `""`, and `"  "` — proving the documented default is actually chosen.
   (Deliberately binds loopback: the default `0.0.0.0` host is rejected by
   the TLS guard before bind, so the test pins `AWH_HOST=127.0.0.1` to
   reach the observable bind failure.)
3. **Installer "one query" claim was not literally true** — `resolve_tag`
   was still called for the tag, then the release API was re-fetched for
   the asset URL and again inside `verify_checksum`. Restructured:
   `resolve_tag` now emits `<tag>\n<release_json_body>` from a single
   `curl`, and `download_binary` extracts BOTH the asset URL and the
   sha256sums.txt URL from that one body; `verify_checksum` receives the
   pre-extracted sums URL and never re-fetches the release. Verified
   against the committed request-counting mock
   (`tests/installer-mock/`): exactly one release-API hit per install.
4. **`AWH_GITHUB_API` override was undocumented** (WARNING-class): it
   redirects where release metadata — and therefore checksum/binary URLs
   — come from. It is now listed in `usage()` output AND the installer
   prints a visible NOTE whenever it is set to anything other than
   `https://api.github.com`, so a redirected trust path is never silent.

### Round 3 (Kilo re-review of the round-2 commit)

Five more findings — all accepted and fixed:

1. **`SANITIZED_AWH_VARS` was still not complete** despite its doc
   comment claiming so: the context-engine family
   (`AWH_CONTEXT_ENABLED`, `AWH_CONTEXT_MEMORY_ENABLED`,
   `AWH_CONTEXT_AUTO_COMPRESS`, `AWH_CONTEXT_AUTO_OFFLOAD`,
   `AWH_CONTEXT_MAX_INPUT_TOKENS`, `AWH_CONTEXT_RESERVED_OUTPUT_TOKENS`,
   `AWH_CONTEXT_SAFETY_MARGIN_TOKENS`), the roots
   (`AWH_GLOBAL_SKILLS_ROOT`, `AWH_TRUST_DIR`) and the helpers
   (`AWH_BWRAP`, `AWH_NGROK_AUTHTOKEN`) were missing. The list is now
   generated from the actual `env::var` call sites in `src/` (documented
   in the constant's comment), and a second constant
   `SANITIZED_PROVIDER_VARS` covers the non-`AWH_*` credentials/routing
   the binary reads (`GITHUB_TOKEN`, `GITHUB_PERSONAL_ACCESS_TOKEN`,
   `GITHUB_API_URL`, `GITHUB_DEFAULT_OWNER`, `GITHUB_DEFAULT_REPO`,
   `COMPOSIO_API_KEY`, `NGROK_AUTHTOKEN`); both are applied in `run_env`
   and the stdio test.
2. **Pinned `--version` metadata failure was silent** (WARNING-class): a
   typo'd tag 404'd, the installer logged nothing (`curl -fsSL` hides
   the HTTP error), and the generic "Prebuilt binary unavailable" path
   silently built from a *branch* — different code than the release the
   user asked for. Now `resolve_tag` logs the failing URL, and
   `download_binary` makes an unresolvable pinned version FATAL with
   "Release <tag> was not found (wrong tag, or unreachable API)". Only
   an unresolvable `latest` (e.g. zero releases published) keeps the
   source-build fallback.
3. **Shell hygiene**: `download_binary` now declares all of its variables
   `local` (incl. the new `resolved`/`release_json`/`sums_url`) and the
   dead `release_api` declaration is gone.
4. **The `AWH_GITHUB_API` NOTE overstated the trust path**: the fallback
   download URL still hardcoded `https://github.com` even when metadata
   came from a redirected API. New `download_host()` derives the
   fallback host from the API base (`https://api.github.com` →
   `https://github.com`, GHES `https://host/api/v3` → `https://host`),
   so the fallback stays on the same instance the metadata came from,
   and the NOTE wording now covers downloads too.
5. **"Verified against a request-counting mock server" had no committed
   artifact**: the mock now lives in the repository as
   `tests/installer-mock/` — `serve.py` (mock GitHub release server with
   a request counter and tag-suffix-selected failure modes:
   `-star`/`-mismatch`/`-missing`/`-nosums`/`-noasset`/`-badtag`) and
   `run.sh`, a self-asserting driver that runs the 8-scenario matrix
   (exit codes, leftover files, transcript lines, and exactly one
   release-API hit per install). Reproduce with
   `bash tests/installer-mock/run.sh`; it also runs in CI as the
   "Validate installer against mock GitHub release server" step of the
   `release-readiness` job on every PR, so the matrix cannot silently
   regress.

### Round 4 (Kilo re-review of the round-3 commit)

Five findings — 4 accepted+fixed, 1 reframed with a stronger assertion:

1. **`log` diagnostic was swallowed by the command substitution**
   (WARNING, real): `resolve_tag` runs as `resolved="$(resolve_tag)"`, and
   `log()` writes to stdout — so "Could not fetch release metadata from:
   <url>" was captured into `$resolved` and discarded, never reaching the
   terminal (confirmed: the round-2/3 transcripts show curl's own stderr
   line but not ours). Added `log_err()` (stderr) for diagnostics emitted
   from inside `$(...)` and used it in `resolve_tag`; the behavior is
   reproducible via `bash tests/installer-mock/run.sh` (also run in the
   `release-readiness` CI job).
2. **`run_install` would break under `set -e`** (WARNING, latent): the
   driver currently runs `set -Euo pipefail` without `-e`, so failure
   scenarios work — but a bare `RUN_OUT="$(...)"` pattern aborts under
   errexit before `RUN_RC` can be asserted, so anyone re-adding `-e`
   would silently break the failure scenarios. Rewrote as an if-form
   (errexit never fires for commands in an if-condition), with a comment
   explaining why the shape matters.
3. **`SANITIZED_PROVIDER_VARS` missed two Composio variables**
   (`COMPOSIO_CONNECTED_ACCOUNT_ID`, `COMPOSIO_TOOLKIT`, read at
   `src/mcp/composio.rs`). Both added; the constant's comment now also
   records that dynamic reads (`--api-key-env`, registry-driven
   `${secret:NAME}` expansion) are user/registry-named and cannot be
   enumerated statically — and are fail-closed behind allow-lists in the
   product.
4. **`download_host` did not normalize a trailing slash** in
   `AWH_GITHUB_API`, producing a double-slash fallback URL. Now stripped;
   the `asset-url-fallback` scenario of
   `bash tests/installer-mock/run.sh` (CI: `release-readiness` job)
   exercises the derived-host path end-to-end.
5. **The harness pinned `.part` leftovers as expected output** — fair
   reframing: the assertable security property is that a failed install
   leaves NOTHING installable under the real names. `run.sh` now also
   asserts the stronger negatives on every failure scenario: no `awh`
   link or symlink — including a dangling one — no `awh-linux-x86_64`,
   no `awh.exe`, on top of the exact-leftovers list. The matrix
   (`bash tests/installer-mock/run.sh`, CI: `release-readiness` job)
   passes all 8 scenarios.

### Round 5 (Kilo re-review of the round-4 commit)

Two suggestions — both accepted:

1. **`[ ! -e ... ]` cannot see a dangling symlink**: install.sh creates
   `awh` via `ln -sf`, so a failure between link creation and cleanup
   could leave a dangling `awh` that `-e` (which follows links) misses
   — exactly the case the new negative claimed to guard. The `awh`
   check now fails on `-e OR -L`; the two regular-file negatives keep
   `-e`.
2. **Report claims were not reproducible**: "verified live" / "all N
   scenarios still pass" phrasings were replaced with the concrete
   reproduction command (`bash tests/installer-mock/run.sh`) and the CI
   hook that runs it (the `release-readiness` job), so a reader can
   re-check any claim and the harness — not prose — is the source of
   truth.


