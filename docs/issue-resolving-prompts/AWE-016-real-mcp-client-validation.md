# AWE-016 / #37 — Real MCP Client Validation

> **Status:** Issue-resolving master prompt  
> **Branch:** `rust`  
> **Primary scope:** validate the production agent-grade editing workflow through real MCP clients and the real MCP transport/protocol boundary.  
> **Dependencies:** AWE-009 / #30 and AWE-015 / #36. Reuse AWE-004 / #25, AWE-006 / #27, AWE-007 / #28, AWE-008 / #29, AWE-010 / #31, AWE-011 / #32, AWE-012 / #33, AWE-013 / #34, and AWE-014 / #35 only where their implementation already exists on `rust`.

## 1. Mission

Validate that an actual MCP client can discover and execute AWH's agent-grade editing workflow without any client-specific editing implementation inside AWH.

This issue is an **interoperability and end-to-end acceptance gate**, not a substitute for the canonical EditService implementation or the AWE-015 service-level test suite.

The required evidence is:

```text
real client
  → real MCP transport
  → initialize
  → tools/list
  → filesystem read
  → minimal edit
  → post-edit verification
  → stale-state/conflict scenario
  → rollback
  → final filesystem verification
```

The test must prove that the semantics already established by the canonical editing service survive the MCP protocol boundary intact.

Do not add client-specific behavior, client-name conditionals, alternate edit algorithms, or protocol hacks merely to make an individual client pass.

---

## 2. Issue contract and acceptance boundary

The authoritative issue requires validation against:

- OpenCode
- OpenHands
- Claude Code
- Qwen Code
- Codex
- a generic MCP client where practical

Required workflow:

```text
connect
→ initialize
→ tools/list
→ read
→ minimal edit
→ verify result
→ inspect conflict/rollback
```

Acceptance requires:

- real client connection and session initialization,
- correct discovery of implemented editing tools,
- minimal patches without full-file rewrites,
- structured conflict and policy errors surviving transport,
- rollback against a real file,
- stale-read conflict demonstration,
- Android/Termux-compatible validation/documentation where technically supported,
- no client-specific editing implementation in AWH. fileciteturn165file0

Treat every item as an evidence requirement, not merely a documentation checkbox.

---

## 3. Forensic preflight — establish the actual implementation

Before modifying code, inspect the current `rust` branch.

At minimum inspect:

```text
src/services/edit.rs
src/mcp/server.rs
src/mcp/mod.rs
src/mcp/dispatcher.rs
src/mcp/tool_registry.rs
src/mcp/custom_mcp.rs
src/cli/
examples/mcp-interop/
docs/mcp.md
docs/testing.md
docs/CLI.md
docs/PROJECT_STATUS.md
docs/PROJECT_ROADMAP.md
```

Also locate the actual implementations or registrations of:

- `tools/list`
- `tools/call`
- `initialize`
- `workspace.read_file`
- all agent-grade editing tools implemented by AWE-009
- `EditService`
- `EditTransaction`
- `ExpectedState`
- `EditId`
- rollback
- policy/capability authorization
- snapshot/provenance
- audit
- MCP stdio transport
- MCP SSE/HTTP transport
- existing interoperability harnesses

Do not infer tool names from the issue text if the branch has a different canonical name. The real implementation and registry are authoritative.

The repository already contains a reference-SDK interoperability harness. Its documented role is to spawn the real `awh` binary and exercise the real MCP protocol rather than merely testing internal Rust functions. fileciteturn169file0

The MCP documentation currently describes stdio as the default local transport and HTTP+SSE as the remote transport, with initialization required before normal requests and JSON-schema-based argument validation at the protocol boundary. fileciteturn170file0

### Required forensic report

Before implementation, determine:

1. Which editing tools actually exist on `rust`.
2. Which tools are advertised by `tools/list`.
3. Which tool names and input schemas are canonical.
4. Which transport(s) can be exercised reproducibly in CI.
5. How an isolated workspace is supplied to the server/client.
6. How authentication is configured for SSE/HTTP, if used.
7. How MCP sessions are initialized and terminated.
8. How tool errors are represented by the server.
9. How filesystem state can be inspected after a client operation.
10. How rollback is invoked through the public MCP surface.
11. Whether policy/capability identity is derived from the MCP session or another canonical mechanism.
12. Which existing interop harnesses can be extended rather than duplicated.
13. Which target clients are realistically installable in CI versus manual validation only.
14. Which Android/Termux path can be reproduced without weakening security.

Do not mark a client “verified” based solely on protocol compatibility if its actual client application was not exercised.

---

## 4. Non-negotiable architecture

### 4.1 MCP is an adapter, not an editing engine

The MCP handler must delegate to the same canonical services used elsewhere in AWH.

The test suite must detect accidental divergence between:

```text
MCP
CLI
TUI
Control API
        ↓
shared application/services
        ↓
canonical EditService
        ↓
secure filesystem / snapshot / audit / policy boundaries
```

Do not add:

- OpenCode-specific edit behavior,
- Claude-specific edit behavior,
- Codex-specific edit behavior,
- OpenHands-specific edit behavior,
- Qwen-specific edit behavior,
- alternate MCP-only patching,
- client-name detection that changes semantics,
- client-specific error rewriting that destroys structured information.

If a client fails because the MCP contract is incorrect, fix the shared MCP contract or canonical service—not the client path.

### 4.2 Real transport is mandatory

A test that calls a Rust dispatcher function directly is not sufficient for AWE-016.

The acceptance path must cross:

```text
client process
→ transport
→ JSON-RPC/MCP parsing
→ session state
→ tools/list/tools/call
→ canonical service
→ real filesystem
```

Internal unit tests remain useful, but they belong to AWE-015 and lower-level MCP tests.

### 4.3 Reference SDK is evidence, not a fake client

Use the official MCP reference SDK harness where practical. The repository already documents `examples/mcp-interop/stdio-client.mjs` and `sse-client.mjs` as real interoperability harnesses that spawn the actual binary and drive it through MCP. fileciteturn169file0

Do not replace the real protocol exchange with mocked JSON objects passed directly to handlers.

---

## 5. Canonical end-to-end test fixture

Create or reuse one deterministic isolated test workspace.

The fixture should contain a small text file such as:

```text
alpha
beta
charlie
```

The exact fixture is less important than these properties:

- known initial bytes,
- known hash,
- known line count,
- writable location inside the workspace,
- no dependency on the user's real home directory,
- no dependency on the repository's working tree.

The client test must capture the original bytes before mutation and assert the final bytes directly.

Never use a developer's real project or home directory as the acceptance fixture.

The existing interop harness pattern of using an isolated `HOME`, temporary resources, and cleanup should be retained or generalized rather than duplicated. fileciteturn169file0

---

## 6. MCP session handshake validation

Every real-client editing workflow must begin with a valid MCP session.

Verify:

1. transport connection succeeds,
2. `initialize` succeeds,
3. negotiated protocol version is supported,
4. server identity/version is returned correctly,
5. session is considered initialized before normal tools are used,
6. `tools/list` succeeds after initialization,
7. the client can cleanly terminate/disconnect.

Do not bypass initialization merely because an internal dispatcher permits direct testing.

Where the transport supports multiple sessions, verify that the test client uses its own isolated session.

### Negative handshake checks

At least one interop-level negative test should prove that an uninitialized client cannot invoke the editing tool.

Expected behavior must be asserted using the stable protocol/error contract rather than brittle full error strings.

---

## 7. `tools/list` discovery contract

The real client must discover the editing capability through `tools/list`.

Verify:

- every implemented editing tool that is intended for MCP exposure is advertised,
- unimplemented editing tools are not advertised merely because they appear in roadmap/docs text,
- each editing tool has a valid JSON `inputSchema`,
- required fields are correctly represented,
- argument types match the canonical service contract,
- tool names are stable,
- metadata comes from the canonical Tool Registry where applicable,
- no client-specific alias is required.

Do not hard-code an expected total tool count if unrelated tools may evolve. Assert the presence and schema of the required editing tools instead.

Where the project intentionally gates tools by policy/capability, test the appropriate exposure/authorization behavior rather than assuming every client sees every mutation capability.

---

## 8. Real minimal-edit workflow

The primary positive workflow must perform the smallest meaningful edit possible.

Prefer a localized edit such as:

```text
beta
```

to:

```text
BETA
```

or the equivalent canonical replace operation.

The purpose is to prove that an agent can perform a precise edit without sending the entire file as a replacement.

### Required assertions

After the client calls the editing tool:

- request succeeds,
- a valid `edit_id` is returned where the contract exposes one,
- result identifies the operation or resulting state,
- file contains the expected edited bytes,
- unrelated lines remain unchanged,
- file size/line count/hash match the actual filesystem,
- verification is not merely inferred from the MCP response,
- no unexpected files were created.

Explicitly test that the payload represents a minimal edit rather than a whole-file rewrite.

If the canonical operation is `replace`, test its exact matching semantics. If the canonical public MCP operation is `patch`, test the patch contract. Do not invent a second minimal-edit protocol solely for this issue.

---

## 9. Prove the MCP response is not the only oracle

A dangerous false positive is:

```text
MCP says success
→ test passes
```

The required oracle is:

```text
MCP says success
AND
real filesystem contains the exact expected bytes
AND
real post-state matches the canonical FileState semantics
```

After every successful mutation, independently inspect the target through the filesystem/test harness rather than trusting only the tool response.

Where possible, also read the file back through the public MCP read tool and compare that result with the actual filesystem.

This produces two independent checks:

1. public MCP observation,
2. direct filesystem observation.

If they disagree, the test must fail.

---

## 10. Structured policy and validation errors

The real client must receive actionable structured failures without AWH leaking internal implementation details.

Test at least:

### Invalid arguments

Send a malformed editing request through `tools/call`.

Verify:

- MCP returns the correct protocol-level invalid-params classification,
- the response identifies the relevant field or validation class where the public contract promises it,
- the canonical editor is not invoked,
- the file remains byte-identical.

### Policy/capability denial

Use a fixture/session where the mutation is not authorized.

Verify:

- client receives a deterministic structured policy/authorization failure,
- denial is distinguishable from a filesystem failure,
- no mutation occurs,
- secrets or sensitive internal policy state are not exposed.

Do not make the test depend on exact prose if the stable contract is an error code/type plus structured fields.

---

## 11. Stale-read conflict through a real client

This is a mandatory acceptance scenario.

Sequence:

```text
real MCP client
  ↓
read target + obtain expected state
  ↓
external process modifies target
  ↓
client submits edit using stale expected state
  ↓
canonical service detects conflict
  ↓
MCP returns structured conflict
  ↓
filesystem still contains external modification
```

The external modification must happen outside the MCP edit call so the test proves actual stale-state protection rather than a simulated error response.

Assert:

- conflict is returned,
- the conflict is attributable to the stale state,
- current external bytes are preserved,
- no partial edit is visible,
- subsequent fresh read observes the external modification.

If the protocol exposes expected/observed state metadata, assert those stable fields as well.

Do not solve the test by disabling expected-state validation.

---

## 12. Real rollback workflow

Rollback must be exercised through the real public MCP surface when rollback is exposed by AWE-009.

Sequence:

```text
original bytes
→ MCP read
→ MCP minimal edit
→ verify edited bytes
→ MCP rollback(edit_id)
→ verify original bytes
```

Assert exact byte restoration.

Also verify:

- rollback targets the correct `edit_id`,
- unrelated files remain unchanged,
- rollback of an unknown/nonexistent edit is a structured failure,
- repeated rollback follows the canonical deterministic behavior,
- a target modified after the edit is not silently overwritten by rollback when conflict protection requires refusal.

If rollback is not yet exposed through MCP on the branch, do **not** invent a fake tool. Record the dependency/blocker precisely and validate every preceding supported acceptance stage instead.

---

## 13. Multi-file and transaction-level interoperability

Where AWE-009 exposes multi-operation transactions, validate one real MCP request affecting multiple files.

Use at least two files.

Test:

1. both edits succeed,
2. resulting bytes are correct in both files,
3. one stale/conflicting file causes the transaction to fail safely,
4. no unrelated file is partially mutated after preparation failure,
5. returned transaction/edit identity remains correlated.

Do not reduce a transaction to several unrelated client calls if the canonical API exposes a single transaction operation.

---

## 14. Transport coverage

### 14.1 stdio — primary local path

The stdio path is mandatory because local MCP clients such as OpenCode and Codex use it according to the repository documentation. fileciteturn170file0

The test must spawn the actual `awh mcp serve` process and communicate over stdin/stdout using a standards-compliant client.

Do not parse the server's implementation internals to simulate the transport.

### 14.2 HTTP/SSE — remote path where supported

Where the current branch exposes the SSE transport, validate:

- server startup,
- authentication,
- session establishment,
- initialize,
- tools/list,
- editing call,
- response correlation,
- shutdown/cleanup.

Use TLS when the acceptance environment requires it.

Never disable certificate verification globally merely to make the test pass.

The existing interop harness documents authenticated SSE and certificate handling and should be reused/extended rather than replaced. fileciteturn169file0

### 14.3 Transport-equivalence invariant

For the same fixture and authorized operation:

```text
stdio result semantics == SSE result semantics
```

The JSON-RPC envelope may differ only where the transport legitimately requires it. Editing semantics, error classification, file result, and state transitions must remain equivalent.

---

## 15. Target-client matrix

Build an explicit evidence matrix for:

| Client | Required evidence |
|---|---|
| OpenCode | connect + initialize + discover + minimal edit |
| OpenHands | connect + initialize + discover + minimal edit where supported |
| Claude Code | connect + initialize + discover + minimal edit where supported |
| Qwen Code | connect + initialize + discover + minimal edit where supported |
| Codex | connect + initialize + discover + minimal edit where supported |
| Generic MCP/reference client | reproducible automated end-to-end workflow |

Do not claim a client is verified merely because it uses the same MCP SDK or protocol version.

Distinguish these evidence levels:

- **Automated verified:** client executable actually ran in the test environment.
- **Manual verified:** real client was run manually and evidence was recorded.
- **Protocol-compatible:** validated only through the reference MCP client/harness.
- **Not verified:** required external account/install/runtime was unavailable.

This distinction is essential for truthful interoperability reporting.

---

## 16. OpenCode validation

Use the real OpenCode MCP configuration supported by the repository/documentation.

Validate that OpenCode can:

1. launch/connect to `awh mcp serve`,
2. complete initialization,
3. discover the editing tool,
4. perform a minimal edit against the isolated workspace,
5. observe the resulting file state,
6. receive a structured conflict when stale state is deliberately supplied, where the client exposes that operation,
7. invoke rollback where supported.

Do not add an OpenCode-specific server mode or tool alias.

If OpenCode cannot expose a particular low-level acceptance scenario through its UI/agent interface, use the reference MCP client for that protocol-level scenario and clearly mark OpenCode evidence as partial rather than fabricating coverage.

---

## 17. OpenHands validation

Exercise the real OpenHands MCP integration where an executable/testable environment is available.

The acceptance criterion is MCP interoperability, not a custom OpenHands adapter inside AWH.

Validate the same canonical editing semantics and filesystem oracle.

If OpenHands Cloud or a remote environment cannot be deterministically executed in repository CI, document it as manual/external validation and retain an automated protocol-level substitute.

Never introduce cloud-provider-specific code into AWH solely for this test.

---

## 18. Claude Code, Qwen Code, and Codex validation

For each client that is actually available:

- configure AWH as an MCP server using the supported client mechanism,
- establish a real session,
- discover tools,
- perform the minimal edit,
- independently verify filesystem state,
- execute one failure/conflict case where the client permits it.

For clients unavailable in the CI environment:

- preserve a deterministic reference-client test,
- document exact manual commands/configuration,
- mark the client as not verified rather than passing it automatically.

Do not store credentials, API keys, access tokens, or user-specific configuration in the repository.

---

## 19. Android / Termux validation

Provide a reproducible Android/Termux-compatible path where technically supported.

At minimum document:

```text
architecture
→ binary acquisition/build
→ `awh mcp serve`
→ client transport
→ isolated workspace
→ minimal edit
→ verification
```

Prefer a local stdio workflow because it minimizes network/security dependencies.

If a particular desktop client cannot run on Android, do not pretend otherwise. Validate the AWH server and a compatible MCP client/reference harness on Termux and document the limitation.

The Android path must preserve the same security semantics; do not introduce `--no-sandbox`, disabled TLS verification, permissive workspace roots, or client-specific filesystem bypasses merely to make the demo work.

---

## 20. Negative interoperability tests

A production acceptance suite must prove failure behavior as well as success.

Include real MCP-client scenarios for:

- malformed tool arguments,
- unknown editing tool,
- uninitialized request,
- invalid path,
- workspace escape attempt,
- stale expected state,
- policy denial,
- nonexistent rollback ID,
- malformed patch/diff where exposed,
- target deleted before edit,
- target changed before rollback where conflict-aware rollback applies.

For every mutation-capable negative case assert:

```text
client-visible failure
AND
expected stable error classification
AND
zero unintended filesystem mutation
```

---

## 21. Protocol error preservation

The MCP layer must preserve enough information for a real agent to recover.

Verify that errors are:

- valid MCP/JSON-RPC responses,
- correlated to the request ID,
- structured where the contract defines structured fields,
- safe for clients to parse,
- free of secrets and raw sensitive file contents,
- distinguishable between validation, authorization, conflict, apply, verification, and rollback classes where the public contract exposes those distinctions.

Do not expose Rust debug dumps, backtraces, absolute host paths, credentials, snapshot bytes, or internal security state.

Avoid assertions on unstable prose.

---

## 22. Client cleanup and isolation

Every real-client test must clean up:

- spawned AWH process,
- client process where controlled by the harness,
- temporary workspace,
- temporary HOME/configuration,
- TLS certificates/keys,
- temporary environment variables,
- SSE sessions/connections.

Tests must remain isolated when run concurrently.

Use ephemeral ports for network tests.

Never reuse a developer's persistent AWH state directory.

Never allow a failed test to leave a server process running indefinitely.

---

## 23. Determinism and flake resistance

Do not use arbitrary sleeps as synchronization when process readiness, MCP messages, or filesystem state can be observed directly.

Use:

- explicit readiness detection,
- bounded retries only for documented asynchronous boundaries,
- request/response correlation,
- deterministic temporary directories,
- ephemeral ports,
- process exit checks,
- bounded timeouts.

Every timeout must produce diagnostics sufficient to identify whether failure occurred during:

```text
spawn
connect
initialize
tools/list
tools/call
filesystem verification
rollback
shutdown
```

Never convert a timeout into a pass or skip without a documented environment-level reason.

---

## 24. Failure diagnostics

A failed interop test must print enough information to reproduce the failure without leaking secrets.

Include, where safe:

- client name/version,
- AWH version/build identifier,
- transport,
- MCP protocol version,
- test case name,
- operation/tool name,
- request correlation ID,
- expected error class,
- observed error class,
- workspace fixture identifier,
- expected versus observed file hash/size/line count,
- process exit code,
- bounded stderr/stdout diagnostics.

Never print:

- API keys,
- bearer tokens,
- secrets,
- snapshot file contents,
- complete sensitive environment variables.

---

## 25. Test organization

Follow existing repository conventions.

Prefer extending:

```text
examples/mcp-interop/
tests/mcp_*.rs
tests/mcp_*.mjs
```

or the actual current equivalent discovered during forensic inspection.

Do not create an unrelated parallel interoperability framework if the existing harness already provides:

- real binary spawning,
- official MCP SDK client behavior,
- stdio transport,
- SSE transport,
- process cleanup,
- isolated HOME,
- assertion/reporting.

If the existing harness is insufficient for editing-specific workflows, extend it minimally and keep its generic transport responsibilities separate from editing-specific assertions.

---

## 26. CI strategy

Separate tests by environmental requirements.

### Mandatory deterministic CI

Must run without paid external accounts and should include:

- reference MCP client,
- real AWH binary,
- stdio transport,
- isolated workspace,
- minimal edit,
- invalid argument failure,
- stale-state conflict,
- rollback where exposed,
- filesystem verification.

### Conditional/manual client validation

Desktop agents requiring installation, login, proprietary services, or external accounts may be manual/optional.

Their absence must not weaken the mandatory reference-client acceptance gate.

Do not add secrets to CI merely to force an external commercial client into the default test matrix.

---

## 27. Backward compatibility

Do not break existing MCP protocol behavior while adding editing validation.

Run the existing MCP suite before and after the change.

The project already has established protocol tests covering initialization, `tools/list`, JSON-RPC errors, version negotiation, malformed messages, and other protocol behavior. AWE-016 must extend rather than replace that coverage. fileciteturn168file11

Preserve:

- existing protocol versions,
- existing tool schemas,
- existing non-editing tools,
- authentication behavior,
- transport behavior,
- error codes,
- session lifecycle semantics.

Any intentional contract change must be explicit and justified rather than hidden inside an interop workaround.

---

## 28. Documentation and evidence

Update documentation only if required by the implementation and repository conventions; do not use this issue to perform unrelated documentation cleanup.

The evidence must clearly state:

```text
client
transport
protocol version
AWH build
workflow tested
result
verification method
limitations
```

For example:

```text
Reference MCP SDK | stdio | 2025-06-18 | release binary
initialize PASS
 tools/list PASS
 minimal edit PASS
 stale conflict PASS
 rollback PASS
 filesystem oracle PASS
```

For each target client, explicitly classify the evidence as:

- VERIFIED,
- MANUAL VERIFIED,
- PROTOCOL-COMPATIBLE ONLY,
- NOT VERIFIED.

Do not claim that OpenCode/Codex/etc. were “tested” merely because the reference SDK was used.

---

## 29. Security requirements

All interop validation must preserve AWH's fail-closed security posture.

Never:

- disable path validation,
- broaden workspace containment,
- disable policy checks,
- bypass capability checks,
- disable TLS verification in production-style tests,
- print secrets,
- trust arbitrary MCP servers automatically,
- use a privileged host directory as a test workspace,
- add client-specific security bypasses.

The MCP documentation describes authentication for remote SSE/MCP requests and schema-first validation at the protocol boundary. Tests must validate those contracts rather than bypass them. fileciteturn170file0

Treat a successful client edit as valid only when the same security boundaries that protect non-client execution are active.

---

## 30. Performance and resource safety

Interop tests must remain bounded.

Use:

- small deterministic fixtures for normal cases,
- bounded maximum message sizes,
- bounded process lifetime,
- bounded retries,
- bounded log capture,
- bounded temporary disk usage.

Do not create large files or large MCP payloads unless a specific regression test requires them.

The purpose is correctness of the agent-grade editing path, not load testing.

---

## 31. Required implementation workflow

Execute in this order:

### Phase 1 — Forensic audit

- inspect current MCP transport/server/dispatcher/tool registry,
- inspect editing tools and schemas,
- inspect existing interop harnesses,
- inspect AWE-015 coverage,
- identify exact reusable fixtures and helpers.

### Phase 2 — Reference-client acceptance path

Implement or extend the official SDK harness to execute:

```text
spawn
→ initialize
→ tools/list
→ read
→ minimal edit
→ independent filesystem verification
→ stale conflict
→ rollback
→ cleanup
```

### Phase 3 — Transport coverage

Validate stdio first.

Then validate SSE/HTTP where supported without weakening TLS/authentication requirements.

### Phase 4 — Target clients

Run real OpenCode/OpenHands/Claude Code/Qwen Code/Codex validation where technically available.

Record evidence level for each.

### Phase 5 — Failure/recovery validation

Exercise invalid arguments, policy denial, stale state, rollback conflict, and cleanup failures.

### Phase 6 — Android/Termux

Validate a reproducible local path where technically supported.

### Phase 7 — Full verification

Run all required repository CI commands and all applicable interop harnesses.

Do not stop at a passing unit test suite.

---

## 32. Full verification commands

At minimum run the repository's established Rust gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also run the actual MCP interoperability harnesses, using the repository's documented commands where available, for example:

```bash
cd examples/mcp-interop
npm install
npm run stdio
npm run sse
```

Do not claim success if only Rust unit tests pass.

For any environment-dependent client that cannot run, record the exact reason and classify the evidence honestly.

---

## 33. Definition of Done

AWE-016 is complete only when all applicable conditions are true:

- [ ] real MCP reference client connects to the real AWH server,
- [ ] initialize succeeds through the actual transport,
- [ ] tools/list exposes the implemented editing capability correctly,
- [ ] minimal edit works without full-file rewrite semantics,
- [ ] actual filesystem state independently verifies the edit,
- [ ] structured invalid-argument behavior is validated,
- [ ] policy/capability denial is validated where available,
- [ ] stale-read conflict is demonstrated against an externally changed real file,
- [ ] no stale edit silently overwrites newer bytes,
- [ ] rollback works through the real MCP surface where exposed,
- [ ] rollback restores exact original bytes,
- [ ] rollback conflict behavior is validated where applicable,
- [ ] multi-file transaction behavior is validated where exposed,
- [ ] stdio interoperability is validated,
- [ ] SSE/HTTP interoperability is validated where supported,
- [ ] no client-specific editing implementation was added,
- [ ] OpenCode/OpenHands/Claude Code/Qwen Code/Codex evidence is classified honestly,
- [ ] an Android/Termux-compatible path is documented/tested where technically supported,
- [ ] credentials and secrets are absent from tests and logs,
- [ ] existing MCP protocol tests remain green,
- [ ] full Rust CI gates pass,
- [ ] interoperability harnesses pass,
- [ ] documentation accurately records verified and unverified clients,
- [ ] no unrelated repository cleanup is included.

---

## 34. Explicit non-goals

Do **not** use AWE-016 to:

- redesign the EditService,
- create a second editing implementation,
- replace AWE-015 service-level tests,
- build a generic MCP client framework,
- add support for every possible MCP client,
- add paid external services to mandatory CI,
- redesign authentication,
- redesign policy/capability architecture,
- redesign snapshot storage,
- redesign audit storage,
- introduce a generic workflow engine,
- weaken security for local development,
- rewrite unrelated documentation,
- refactor unrelated MCP tools.

If a prerequisite implementation is genuinely missing, make the smallest production correction required for a valid interoperability contract, document the dependency, and do not expand the issue into an unrelated architecture project.

---

## 35. Final implementation report

Before declaring AWE-016 complete, report:

1. exact files changed,
2. exact MCP editing tools validated,
3. transports validated,
4. MCP protocol versions exercised,
5. reference-client results,
6. OpenCode result,
7. OpenHands result,
8. Claude Code result,
9. Qwen Code result,
10. Codex result,
11. stale-state conflict result,
12. rollback result,
13. filesystem verification method,
14. Android/Termux result,
15. any environment limitations,
16. security-sensitive behavior verified,
17. full CI command results,
18. any production code change required to correct a real protocol/contract defect.

Do not report a client as verified without actual evidence.

The final report must distinguish **implementation success** from **interoperability evidence**.

---

## 36. HARD STOP

After AWE-016 is implemented, tested, and documented, **STOP**.

Do not begin AWE-017 or modify future issue-resolving prompt files.

Do not modify unrelated repository files.

The objective is a trustworthy, reproducible real-client acceptance gate for agent-grade editing—not merely another MCP demo.
