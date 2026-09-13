# AWE-009 / #30 — MCP Agent-Grade Editing

## Role

You are implementing **AWE-009 — Expose agent-grade editing through MCP** in the `sawroop1242/Agent-workspace-hub` repository on branch `rust`.

This is a **P0 integration milestone**. Your job is to expose the already-canonical editing service through the existing MCP server without creating a second editor, bypassing the service/policy architecture, weakening authorization, or changing unrelated behavior.

The end state is:

```text
MCP initialize/session
        ↓
tools/list discovery
        ↓
tools/call
        ↓
JSON/schema validation
        ↓
MCP session + authorization/capability boundary
        ↓
canonical EditService
        ↓
AWE-004/AWE-005/AWE-006/AWE-007/AWE-008 editing pipeline
        ↓
actual filesystem mutation
        ↓
post-edit verification
        ↓
structured MCP result/error
```

**MCP is an adapter. It is not an editor.** Do not implement file mutation, diff application, stale-state logic, rollback, or verification algorithms inside `dispatcher.rs` or another MCP-only module.

---

## 1. Issue and forensic baseline

Implement GitHub issue **#30 — AWE-009: Expose agent-grade editing through MCP**.

The forensic baseline states:

- AWH has a mature MCP dispatcher/schema-validation path.
- The repository has a large existing static MCP catalog but currently has **no `filesystem.*` editing tools**.
- `src/services/edit.rs` defines the canonical `EditTransaction`, `EditOperation`, `ExpectedState`, `FileState`, lifecycle states, and structured edit vocabulary, but the forensic baseline found no production MCP caller of that transaction model.
- The MCP layer must expose the canonical service rather than implement another editor in the dispatcher.
- Existing MCP schema validation must happen before handler/service dispatch.
- Mutation tools must have explicit risk/capability metadata and must not accidentally inherit a weaker authorization posture.

Before changing code, independently verify these facts against the current `rust` branch. Do not assume the repository has the exact implementation shape described by historical reports.

---

## 2. Mandatory repository preflight

Before implementation, read and understand at minimum:

```text
docs/PROJECT_CONTEXT.md
docs/PROJECT_ROADMAP.md
docs/PROJECT_ROADMAP_STATUS.md
docs/PROJECT_STATUS.md
docs/FEATURES.md
docs/CLI.md
docs/architecture.md
docs/mcp.md
docs/issue-resolving-prompts/AWE-001-canonical-edit-transaction-model.md
docs/issue-resolving-prompts/AWE-004-multi-operation-filesystem-patch.md
docs/issue-resolving-prompts/AWE-005-unified-diff-application.md
docs/issue-resolving-prompts/AWE-006-atomic-rollback-safe-edits.md
docs/issue-resolving-prompts/AWE-007-stale-state-conflict-detection.md
docs/issue-resolving-prompts/AWE-008-post-edit-verification.md
src/services/edit.rs
```

Then inspect the current MCP implementation, especially:

```text
src/mcp/dispatcher.rs
src/mcp/tool_registry.rs
src/mcp/schema.rs
src/mcp/permissions.rs
src/mcp/  (relevant handlers, service adapters, and tests)
```

Locate the exact existing mechanisms for:

1. `tools/list` registration.
2. `tools/call` dispatch.
3. static tool schemas.
4. schema validation and `-32602` errors.
5. session initialization/lifecycle enforcement.
6. tool registry metadata.
7. permission/capability authorization.
8. audit/deny behavior.
9. application-service access from MCP handlers.
10. integration-test helpers and real MCP transport tests.

Do not invent a parallel abstraction when an existing repository abstraction already provides the required behavior.

---

## 3. Dependency contract

AWE-009 depends on the completed canonical editing milestones:

```text
AWE-004 #25 — multi-operation filesystem patch
AWE-006 #27 — atomic and rollback-safe edits
AWE-007 #28 — stale-state conflict detection
AWE-008 #29 — post-edit verification
```

The MCP adapter must consume those contracts.

Do not reimplement any of them in MCP.

AWE-010 separately exposes explicit rollback of completed edits. **Do not add an MCP rollback tool in AWE-009.**

AWE-011 owns the complete future capability/policy model. AWE-009 must use the authoritative policy/capability boundary that exists now and must not pretend that descriptive tool metadata is authorization.

---

## 4. Required MCP tools

Expose exactly these five editing tools when their underlying implementation is actually available:

```text
filesystem.patch
filesystem.replace
filesystem.insert
filesystem.delete_range
filesystem.apply_diff
```

Do not advertise an unimplemented tool merely to make `tools/list` appear complete.

Do not add `filesystem.rollback`; that belongs to AWE-010.

Do not create aliases with different names unless an existing MCP compatibility contract explicitly requires them.

---

## 5. Canonical service rule

All five MCP tools must construct/translate requests into the canonical editing model and invoke the shared application service.

The intended dependency direction is:

```text
MCP transport/dispatcher
        ↓
MCP handler/adapter
        ↓
canonical EditService
        ↓
filesystem/edit transaction machinery
```

Never:

```text
MCP dispatcher
   ↓
std::fs / tokio::fs
   ↓
hand-written replacement logic
```

MCP handlers must not directly implement:

- string replacement;
- occurrence selection;
- line insertion;
- line-range deletion;
- unified-diff parsing/application;
- expected-state comparison;
- stale-state detection;
- atomic-write logic;
- transaction rollback;
- post-edit verification.

If the current service layer does not yet expose a callable production API required by these tools, add the **smallest transport-independent service API necessary** rather than putting the missing logic in MCP. Keep the canonical semantics in the service layer.

---

## 6. JSON input contracts

Design complete, explicit JSON schemas for every tool based on the canonical Rust edit model and the already-established MCP schema conventions.

Schemas must:

- identify every required field;
- use correct JSON types;
- represent optional expected-state fields explicitly;
- document operation-specific semantics through descriptions;
- reject structurally invalid values before service dispatch;
- remain compatible with the repository's existing schema-validator subset;
- use the same schema-version metadata convention as other static tools;
- avoid inventing schema keywords unsupported by the existing validator.

At minimum, the contracts must be capable of expressing the canonical data needed for:

### `filesystem.replace`

```text
path
old
new
occurrence (optional)
expected state (optional, according to the canonical service contract)
```

### `filesystem.insert`

```text
path
line
content
expected state (optional)
```

Use the canonical one-based line-boundary semantics. `line = 0` means beginning of file; otherwise insertion is before the specified one-based line boundary.

### `filesystem.delete_range`

```text
path
start_line
end_line
expected state (optional)
```

Use the canonical inclusive one-based range semantics.

### `filesystem.patch`

Represent the canonical patch operation/transaction semantics rather than creating an MCP-specific patch algorithm. Preserve the service's expected-state and transaction semantics.

### `filesystem.apply_diff`

Accept the canonical unified-diff input required by AWE-005 and preserve its service-layer validation, security, atomicity, stale-state, and verification behavior.

Do not silently accept a different semantic contract merely because it is convenient for MCP.

---

## 7. Expected-state and conflict semantics

MCP must preserve the canonical stale-state contract.

The canonical model includes:

```text
ExpectedState.hash
ExpectedState.context
ExpectedState.size
ExpectedState.line_count
```

Do not drop these fields merely because the MCP tool schema is simpler.

The MCP boundary must distinguish:

```text
invalid request/schema
        ≠
policy denial
        ≠
stale-state conflict
        ≠
apply failure
        ≠
post-edit verification failure
```

A stale-state conflict must remain machine-readable and must identify enough structured information for an agent to recover deterministically without exposing unnecessary sensitive file contents.

If the service already returns structured conflict data, translate it losslessly into the MCP error/result contract. Do not flatten everything into a generic string.

Invalid arguments must fail before any mutation.

---

## 8. Authorization, capability, and risk boundary

This is a security-critical requirement.

The repository's Tool Registry metadata is **descriptive**. `risk` and `requiredPermissions` are not themselves an authorization decision. Do not implement an unsafe shortcut such as:

```text
if tool.risk == high { allow }
```

or:

```text
if tool_name starts_with filesystem. { allow }
```

Instead, route each mutation through the same authoritative MCP authorization/capability boundary used by existing protected tools.

For the new editing tools:

1. Declare explicit Tool Registry metadata.
2. Declare the appropriate filesystem mutation permission/capability vocabulary.
3. Use the authoritative authorization gate before mutation.
4. Preserve session lifecycle requirements.
5. Preserve audit/deny behavior.
6. Fail closed when authorization context is missing or malformed.
7. Never make a descriptive registry field substitute for policy enforcement.
8. Do not silently inherit the weaker default authorization posture of an unrelated tool.

If the current policy system cannot express the required edit authorization cleanly, document the limitation and use the strongest existing authoritative gate without implementing the full AWE-011 policy redesign inside this issue.

---

## 9. Tool metadata

Register every new static tool explicitly in the canonical Tool Registry.

For each tool provide:

```text
exact name
category
tool version
schema version
provider = awh
explicit risk
required permissions
enabled state
```

The registry must remain name-sorted if the current implementation requires binary search.

Add/update exhaustiveness tests so a tool cannot exist in the static catalog without explicit registry metadata.

The five tools are workspace-local mutation capabilities and must not be incorrectly categorized as read-only.

Do not classify them as high-risk merely because the registry's `High` label has a particular descriptive meaning; use the repository's established semantics and security model. The important invariant is explicit, intentional metadata plus authoritative authorization.

---

## 10. MCP `tools/list` behavior

Each implemented editing tool must appear in real `tools/list` output with:

- exact name;
- useful description;
- complete `inputSchema`;
- correct metadata according to the existing MCP response format.

If the architecture has a static catalog plus registry, update both consistently.

Do not advertise a tool before its handler and service path are real.

Do not break the existing catalog count or optional-tool behavior accidentally.

Add an integration assertion that all five expected tools are discoverable once implemented and that no placeholder/unknown editing tool is advertised.

---

## 11. MCP `tools/call` behavior

For each tool, the request flow must remain:

```text
JSON-RPC structural validation
→ MCP session/lifecycle validation
→ tool lookup
→ schema validation
→ authorization/capability check
→ canonical service invocation
→ service-level validation/conflict detection
→ mutation transaction
→ actual post-edit verification
→ structured MCP response
```

Schema validation must occur before the handler performs mutation-related work.

Do not duplicate JSON type checks in handler code when the existing schema validator already guarantees them; only perform semantic checks that belong at the service boundary.

Preserve the existing JSON-RPC error conventions, especially `-32602` for invalid tool arguments.

---

## 12. Result contract

Successful editing calls must expose the canonical transaction identity and actual observed state.

At minimum, the result should preserve:

```text
edit_id
status
before state
operations / relevant operation information
actual after state
```

Where the underlying layers already provide them, include structured:

```text
snapshot reference
provenance reference
audit reference
```

Do not manufacture references that do not exist.

Do not report success merely because the service returned after a write. AWE-008 requires actual post-edit verification before successful completion.

The `after` state must represent actual filesystem observation, not merely a predicted candidate state.

Do not expose full sensitive file contents in results merely to make verification easier for an agent.

---

## 13. Error mapping

Create a deliberate, documented mapping from canonical service failures to MCP-visible failures.

At minimum distinguish:

```text
invalid arguments/schema
validation failure
policy/capability denial
stale-state conflict
apply failure
verification failure
rollback/recovery failure
unknown/internal failure
```

Preserve stable machine-readable fields where the MCP error contract permits them, such as:

```text
code/category
edit_id
operation index
path when safe
current/expected state metadata when safe
policy/capability reason when safe
rollback attempted/succeeded when applicable
verification status when applicable
```

Do not leak:

- secrets;
- environment credentials;
- arbitrary absolute host paths when the protocol should expose workspace-relative paths;
- complete sensitive file contents;
- internal stack traces unless the existing MCP error policy explicitly allows them.

Do not convert a conflict or policy denial into a successful JSON-RPC response containing an error-looking string.

---

## 14. Transaction and multi-operation behavior

`filesystem.patch` must preserve the canonical transaction semantics from AWE-004 and AWE-006.

If a request contains multiple operations:

- use one stable transaction/edit ID;
- validate the whole transaction before mutation;
- preserve operation order;
- preserve same-file composition semantics;
- enforce expected-state requirements consistently;
- preserve atomic commit/rollback guarantees provided by the service;
- verify affected files after mutation;
- never report partial success as transaction success.

MCP must not implement a second transaction coordinator.

The MCP adapter may translate JSON into `EditTransaction`, but transaction lifecycle ownership remains in the service layer.

---

## 15. `filesystem.apply_diff`

Treat unified-diff execution as a service responsibility established by AWE-005.

The MCP adapter must:

- accept the canonical diff representation;
- pass it to the canonical service;
- preserve multi-file behavior;
- preserve path traversal/symlink protections;
- preserve exact context validation;
- preserve expected-state/conflict semantics;
- preserve atomic commit/rollback behavior;
- preserve post-edit verification.

Do not parse hunks or directly write diff results in MCP.

Do not silently introduce a second diff parser.

---

## 16. Security invariants

The following are non-negotiable:

- No MCP-local filesystem mutation algorithm.
- No direct unvalidated path use from tool arguments.
- No path traversal bypass.
- No absolute-path escape from the workspace contract.
- No symlink escape through an MCP-only shortcut.
- No authorization bypass through direct handler invocation.
- No default-allow assumption for new mutation tools.
- No schema-validation bypass through alternate dispatch paths.
- No mutation before schema/semantic/policy validation succeeds.
- No false success before post-edit verification.
- No rollback implementation in AWE-009.
- No leakage of sensitive file contents in errors.

Use the canonical filesystem security helpers and service boundaries instead of reproducing them in MCP.

---

## 17. Session lifecycle compatibility

Existing MCP lifecycle behavior must remain intact.

The new tools must work only after successful MCP initialization according to the current protocol implementation.

Verify that adding mutation handlers does not bypass:

- initialize requirements;
- protocol-version negotiation;
- request structural validation;
- notification semantics;
- session isolation;
- authentication for remote MCP transport;
- existing audit/observer hooks.

Do not alter unrelated lifecycle semantics as part of this issue.

---

## 18. Testing requirements

Tests must exercise the **real MCP path**, not only helper functions.

### A. `tools/list`

Verify:

- all five tools appear when implemented;
- names are exact;
- schemas are present and structurally valid;
- metadata is present and intentional;
- no unregistered tool is advertised.

### B. Schema rejection

For every tool test malformed requests including:

- missing required fields;
- wrong primitive types;
- null where disallowed;
- invalid line values/ranges where representable at schema level;
- malformed nested expected-state objects;
- malformed diff payloads where schema can detect them.

Assert that invalid arguments fail **before mutation**.

### C. Successful minimal mutation

Use a real temporary workspace and perform at least one minimal successful edit through:

```text
tools/call → MCP dispatcher → canonical service → filesystem
```

Verify the actual file contents and returned `edit_id`, before-state, after-state, and committed/verified status.

### D. Each operation

Exercise successful real calls for:

```text
filesystem.replace
filesystem.insert
filesystem.delete_range
filesystem.patch
filesystem.apply_diff
```

Cover relevant edge cases inherited from their service-level contracts rather than re-testing every service unit test through MCP.

### E. Conflict

Cause a stale-state conflict between expected state and the actual file.

Verify:

- no mutation occurs;
- MCP returns a machine-readable conflict;
- the `edit_id` is preserved where the service creates it;
- the conflict does not become generic success.

### F. Policy denial

Exercise a request that reaches the authoritative authorization boundary but is denied.

Verify:

- mutation does not occur;
- denial is machine-readable;
- the audit/deny behavior is preserved;
- the tool does not become authorized merely because it is registered.

### G. Verification failure

Use the repository's existing failure-injection strategy where available to cause post-edit verification failure.

Verify that MCP reports verification failure and preserves AWE-006 recovery semantics.

### H. Multi-file transaction failure

Exercise a transaction where a later operation/verification fails and verify the canonical service's rollback behavior through MCP without adding an MCP rollback algorithm.

### I. Session lifecycle

Confirm editing calls before initialization remain rejected according to the existing MCP contract.

### J. Transport coverage

Prefer existing real MCP integration infrastructure. At minimum cover the canonical dispatcher path; if the repository already has both stdio and SSE integration harnesses, ensure the new tools do not regress either transport.

Do not create a heavyweight new test framework when the repository already has one.

---

## 19. Compatibility requirements

The implementation must preserve the shared-service architecture:

```text
MCP ─┐
CLI ─┼─→ same EditService / same semantics
TUI ─┤
API ─┘
```

A future CLI implementation must be able to invoke the same executor without copying MCP behavior.

Do not make the canonical edit service depend on MCP types merely to expose these tools.

Avoid public API churn outside what is necessary to connect the existing service to MCP.

---

## 20. Scope control — do NOT implement these here

Do not expand AWE-009 into unrelated roadmap work.

Explicit non-goals:

- AWE-010 explicit edit-level rollback exposure.
- AWE-011 complete capability/policy redesign.
- AWE-012 snapshots/provenance architecture beyond consuming existing references.
- AWE-013 persistent audit redesign.
- AWE-014 CLI exposure.
- AWE-015 complete editing test-suite redesign.
- AWE-016 real external MCP-client validation beyond existing test infrastructure.
- AWE-017 complete acceptance workflow.
- AWE-018 final editing contract.
- agent profile redesign.
- worktree isolation.
- remote/cloud architecture.
- model routing.
- connector redesign.
- generic filesystem API redesign.
- a second editing engine.
- a new policy engine.
- unrelated MCP protocol changes.

If a prerequisite is genuinely missing, implement only the smallest transport-independent compatibility surface required to connect the already-defined canonical service. Do not silently absorb a later milestone.

---

## 21. Documentation requirements

Update documentation only when required to make the implemented MCP contract discoverable and consistent with the repository's existing documentation architecture.

If documentation changes are necessary, keep them tightly scoped to the AWE-009 implementation and MCP tool contract.

Do not rewrite roadmap documents or unrelated milestone prompts as part of the implementation task.

---

## 22. Code-quality requirements

Follow existing Rust conventions.

Prefer:

- small MCP adapters;
- explicit types;
- existing error types;
- existing schema helpers;
- existing policy/permission helpers;
- existing audit hooks;
- existing filesystem security helpers;
- existing integration-test utilities.

Avoid:

- duplicated logic;
- stringly-typed authorization decisions;
- giant dispatcher branches;
- silent fallback behavior;
- `unwrap()`/`expect()` on untrusted MCP input;
- hidden mutation before validation;
- broad refactors unrelated to AWE-009.

Run formatting and clippy-clean code. Do not suppress warnings merely to make the milestone compile.

---

## 23. Required verification commands

Before declaring completion, run the repository's standard gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also run the most targeted MCP/editing tests available and any relevant integration tests.

Then inspect the final diff carefully:

```bash
git status --short
git diff --check
git diff
```

Confirm that the diff contains only intentional AWE-009 implementation/test/documentation changes.

---

## 24. Definition of Done

AWE-009 is complete only when all of the following are true:

- [ ] `filesystem.patch` is exposed through real MCP dispatch.
- [ ] `filesystem.replace` is exposed through real MCP dispatch.
- [ ] `filesystem.insert` is exposed through real MCP dispatch.
- [ ] `filesystem.delete_range` is exposed through real MCP dispatch.
- [ ] `filesystem.apply_diff` is exposed through real MCP dispatch.
- [ ] Every tool has a complete, validator-compatible JSON schema.
- [ ] Schema validation occurs before service mutation.
- [ ] Every tool has explicit canonical registry metadata.
- [ ] Mutation tools pass the authoritative authorization/capability boundary.
- [ ] No new tool accidentally inherits unsafe default authorization.
- [ ] MCP handlers contain no independent editing algorithm.
- [ ] All five tools call the canonical EditService/application service.
- [ ] Stable `edit_id` is preserved.
- [ ] Before-state is reported from actual pre-edit observation.
- [ ] After-state is reported from actual post-edit verification.
- [ ] Snapshot/provenance references are preserved when already available.
- [ ] Conflicts are machine-readable.
- [ ] Policy denials are machine-readable.
- [ ] Verification failures are machine-readable.
- [ ] Invalid arguments are proven not to mutate files.
- [ ] Successful minimal MCP editing is covered by real integration tests.
- [ ] Conflict, policy denial, and verification/recovery paths are tested.
- [ ] Existing MCP session lifecycle remains intact.
- [ ] Existing transports are not regressed.
- [ ] Standard Rust CI gates pass.
- [ ] No unrelated roadmap milestone has been implemented accidentally.

---

## 25. Final implementation report

When implementation is complete, report:

1. exact files changed;
2. exact MCP tools added;
3. exact service entry points used;
4. schema/registry changes;
5. authorization/capability path used;
6. result/error mapping;
7. tests added and what each proves;
8. verification commands and outcomes;
9. any narrowly scoped limitation that remains;
10. confirmation that no MCP-local editor or duplicate transaction engine was introduced.

Do not claim completion if any Definition-of-Done item is false.

## Hard STOP

This issue is **AWE-009 / #30 only**.

Implement and verify **only this MCP agent-grade editing milestone**, then stop.

Do not automatically continue to AWE-010, AWE-011, or any later issue.
