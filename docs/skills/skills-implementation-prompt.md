# 20 — Skills (SKL-001)

> Standalone implementation contract for the current `rust` branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity

**Prompt:** 20 / SKL-001  
**Feature:** Skills  
**Primary roadmap area:** Skills and capability packages  
**Target branch:** `rust`  
**Canonical folder:** `docs/skills/`

### Mission

Implement, converge, and verify the Skills subsystem inside AWH's documented product boundary.

This prompt is standalone:

- inspect current `rust` before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH;
- treat the current repository implementation as evidence to reconcile, not as proof that every target behavior already exists.

# 1. Scope

## In scope

- Establish one canonical Skills domain/service boundary over the existing `src/skills/*` implementation.
- Converge the existing global registry, project references, registry configuration, installer, parser, package validation, remote sources, and lockfile behavior.
- Complete and verify:
  - `list`, `show`, `install`, `remove`, `enable`, `disable`;
  - local and remote source resolution;
  - `SKILL.md`/manifest parsing and validation;
  - package/path/integrity validation;
  - project-scoped references;
  - lock/pinning semantics where the product contract exposes them;
  - declared capability/requirement metadata and runtime authorization where supported by the canonical capability model;
  - MCP, TUI, CLI, and Control API adapters.
- Harden installation and remote-source handling against path traversal, symlink escape, oversized content, integrity mismatch, malicious metadata, and partial publication.
- Preserve the distinction between a **declared requirement** and an **actually granted capability**.

## Out of scope

- No arbitrary plugin runtime or general code-execution framework.
- No automatic capability grant from a skill manifest.
- No replacement MCP/tool security boundary.
- No unrestricted remote Git execution.
- No model routing, agent reasoning, autonomous orchestration, or generic workflow/DAG scheduling.
- No second package manager or independent registry security model.

## Product boundary

AWH owns skill metadata, validation, package lifecycle, project references, integrity/pinning state, and the exposure of skill metadata through supported interfaces.

The canonical PolicyEngine/authorization boundary decides whether a consequential capability or tool action is permitted. A skill declaration can request or describe capabilities; it cannot grant them.

# 2. Required repository forensics

Read at minimum:

`Cargo.toml`, `README.md`, `AGENTS.md`, `docs/PROJECT_CONTEXT.md`, `docs/FEATURES.md`, `docs/architecture.md`, `docs/security.md`, `docs/threat-model.md`, `docs/CLI.md`, `docs/roadmap/PROJECT_ROADMAP.md`, `docs/roadmap/GROWTH_STRATEGY.md`, `docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md`, and implementation prompts 01–17.

Inspect the complete current Skills implementation before designing replacements:

- `src/skills/mod.rs`
- `src/skills/model.rs`
- `src/skills/parser.rs`
- `src/skills/package.rs`
- `src/skills/registry.rs`
- `src/skills/registries.rs`
- `src/skills/installer.rs`
- `src/skills/project.rs`
- `src/skills/references.rs`
- `src/skills/lockfile.rs`
- `src/skills/remote.rs`
- `src/mcp/skills.rs`
- `src/tui/screens/skills.rs`
- `src/tui/backend.rs`
- CLI wiring in `src/main.rs`
- capability/policy/authorization and audit services
- Control API routing if Skills are exposed there.

Search before adding abstractions:

```text
rg -n "Skill|SkillRegistry|GlobalSkillRegistry|SkillInstaller|SkillLock|SkillReferences|SKILL.md|awh skill|capability.*skill" src docs
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\(" src/skills src/mcp/skills.rs src/tui/screens/skills.rs
```

### Existing-contract inventory

The implementation must start from this repository-grounded inventory and refresh it if the current branch has changed:

| Existing contract | Location | Action | Reason |
|---|---|---|---|
| `Skill` domain model | `src/skills/model.rs` | Reuse/extend | Existing validated skill representation; do not create a second model |
| `parse_skill` and front-matter parser | `src/skills/parser.rs` | Reuse/extend | Existing `SKILL.md` parsing boundary |
| Package validation/SHA-256/path helper | `src/skills/package.rs` | Reuse/harden | Existing package integrity and path-safety primitives |
| `GlobalSkillRegistry` | `src/skills/registry.rs` | Reuse/extend | Existing global installed-skill authority |
| `RegistryStore` / registry URL config | `src/skills/registries.rs` | Reuse/extend | Existing configured-registry persistence |
| `SkillInstaller` | `src/skills/installer.rs` | Extend/converge | Existing install flow needs transactional publication and security enforcement |
| `ProjectSkillReferences` / `SkillReferences` | `src/skills/project.rs`, `src/skills/references.rs` | **Converge to one authority** | Two reference implementations exist; they must not remain competing writers/readers |
| Lockfile types/store | `src/skills/lockfile.rs` | Reuse/extend | Existing pinning/integrity persistence |
| Remote source handling | `src/skills/remote.rs` | Harden/reuse | Existing GitHub/community source model; enforce bounded and authorized acquisition |
| MCP skill adapter | `src/mcp/skills.rs` | Adapt | Must expose canonical Skills semantics without owning storage |
| TUI Skills screen/backend | `src/tui/screens/skills.rs`, `src/tui/backend.rs` | Adapt | Presentation/control adapter only |
| Agent identity | `src/services/agent_runtime.rs` | Reuse | Skill actions must retain caller identity/scope |
| Authorization/policy | `src/services/authorization.rs` and PolicyEngine contracts | Reuse | Authoritative consequential-action authorization |
| Filesystem safety | `src/services/files.rs` | Reuse | Canonical workspace/path/resource validation |
| Persistent audit | `src/services/audit.rs` | Reuse | Correlated redacted observability |
| Workspace/project state | existing workspace/project services | Reuse | Scope and ownership remain outside Skills |

Do not retain both `src/skills/project.rs` and `src/skills/references.rs` as independent persistence authorities. Choose and document the canonical implementation based on the current `rust` contracts, then migrate/adapt callers without creating a third model.

# 3. Architecture

## Canonical architecture flow

```text
CLI / MCP / TUI / Control API
            |
            v
Caller identity + workspace/project scope validation
            |
            v
Canonical Skills service/domain
            |
            +--> parse + validate SKILL.md/manifest
            +--> source/package/path/integrity validation
            +--> dependency/reference/lock resolution
            |
            v
Transactional install/reference/enable/disable/remove operation
            |
            +--> PolicyEngine / authorization for consequential actions
            +--> FilesService / canonical filesystem safety
            +--> AuditLog / correlated redacted event
            |
            v
Canonical persisted skill state
   +--------+---------+----------------+
   |                  |                |
global registry   project refs     lock/registry config
   |                  |                |
   +------------------+----------------+
                      |
                      v
             MCP/TUI/CLI/API read paths
                      |
                      v
        runtime tool exposure/execution
                      |
                      v
       normal capability/policy checks
```

## Canonical responsibility

`Source → parse/validate → package/path/integrity validation → authorization → prepare/stage → atomic publish → registry/reference/lock update → enable/disable state → policy-gated runtime exposure`.

The implementation must not collapse these stages into an unreviewable “install and trust” operation.

### Architectural invariants

1. One canonical Skills domain/service owner.
2. One canonical `Skill` model; adapters do not define competing semantics.
3. One authoritative writer/reader for each persisted Skills state.
4. Existing workspace, project, agent, session, and worktree identity remain authoritative.
5. PolicyEngine/authorization remains authoritative for granted capabilities.
6. A manifest's declared capabilities/requirements are data, not authority.
7. Existing FilesService, AuditLog, Git, MCP, and identity boundaries are reused.
8. Interface adapters do not create alternate install, enable, reference, or authorization semantics.
9. A failed install never leaves a partially trusted skill presented as installed.
10. Restart, corruption, concurrent mutation, and migration behavior are explicit.
11. Remote source trust and AWH authorization are separate checks.
12. “Installed”, “referenced”, “enabled”, “declared”, and “authorized” are distinct states.

## State/lifecycle model

At minimum distinguish:

`Discovered → Parsed → Validated → IntegrityVerified → Staged → Installed → Referenced → Enabled/Disabled`.

Removal and failed operations must define their effect on each state.

A skill is **not authorized merely because it is installed, referenced, enabled, signed, hashed, or listed in a lockfile**.

# 4. Interfaces

## Rust/service interface

Preserve the existing parser/registry/installer/package/lock/reference contracts where compatible, but expose one canonical domain/service boundary.

Typed errors must distinguish, at minimum:

- malformed manifest;
- invalid name/version;
- unsupported requirement;
- duplicate/version conflict;
- integrity mismatch;
- unsafe package path or symlink escape;
- source/registry failure;
- lock/reference inconsistency;
- authorization/policy denial;
- invalid lifecycle transition;
- persistence/publication failure;
- corruption/recovery failure.

The service must accept explicit caller/scope context for consequential mutations.

## CLI interface

Support the documented Skills command family:

`awh skill list|show|install|remove|enable|disable`.

Acceptance requirements:

- compiled CLI routes to the canonical service;
- output distinguishes installed/referenced/enabled state;
- declared requirements are not displayed as granted authority;
- source/ref/version/integrity information is deterministic where applicable;
- errors use stable structured semantics consistent with the CLI contract;
- secrets and credentials never appear in stdout/stderr;
- install/remove/enable/disable use the same authorization and audit path as other interfaces.

## MCP interface

`src/mcp/skills.rs` remains an adapter over the canonical Skills service/domain.

Acceptance requirements:

- MCP discovery/read uses the same canonical registry and project scope;
- project references are enforced;
- MCP cannot bypass authorization by directly invoking registry/installer internals;
- skill-provided tools remain subject to normal MCP capability/policy checks;
- malformed or unauthorized requests have structured errors;
- no MCP-local persistence authority is introduced.

## TUI interface

The TUI Skills screen is presentation/control only.

Acceptance requirements:

- reads through the canonical backend/service;
- project reference changes call the same canonical operation as CLI/API/MCP;
- no TUI-local writer for registry, references, or lock state;
- stale selections are invalidated after scope/project changes;
- UI clearly distinguishes declared/requested capability from granted capability.

## Control API interface

If Skills are exposed through `/api/v1/`, the API must:

- delegate to the canonical service;
- preserve versioned API semantics;
- apply authenticated caller identity and scoped authorization;
- use bounded request/response DTOs;
- return structured errors without leaking secrets or local filesystem details;
- never implement a second installer/registry/reference store.

### Interfaces acceptance

- [ ] CLI, MCP, TUI, and Control API, where applicable, invoke the same canonical Skills semantics.
- [ ] No adapter writes a Skills persistence file directly except through its canonical owner.
- [ ] The same caller identity, scope, policy, limits, lifecycle rules, and error classes apply across interfaces.
- [ ] Real compiled CLI and MCP paths are tested where available.
- [ ] TUI behavior is verified through backend/service integration rather than only rendering tests.
- [ ] API compatibility is verified if the feature is exposed through the versioned Control API.

# 5. Security

Security requirements are implementation requirements.

## Authorization and capability semantics

Use the existing authorization/PolicyEngine boundary.

Required distinction:

```text
Skill manifest declares requirement
        ↓
AWH validates declaration
        ↓
PolicyEngine evaluates requested operation
        ↓
Authorized capability/tool action is granted only if policy permits
```

A skill must never be able to:

- grant itself a capability;
- widen its caller's capabilities;
- bypass session/agent/workspace scope;
- invoke a consequential tool merely because that tool is declared by the skill.

## Package and filesystem safety

Validate:

- skill names and versions;
- package roots and relative paths;
- absolute paths and traversal;
- encoded/alternate path forms where relevant;
- symlinks and links escaping the package root;
- file count and total package size;
- `SKILL.md` size and encoding assumptions;
- archive/extraction paths if archive support is introduced;
- destination ownership and replacement rules.

Installed files must remain under the canonical skill root.

## Remote-source safety

For GitHub/community/registry sources:

- validate repository/reference/source syntax;
- require explicitly supported schemes;
- bound network response size and time;
- verify declared integrity before publication;
- do not treat a successful download/clone as proof of trust;
- do not execute repository content as part of installation;
- avoid shell interpolation; use structured process arguments through the canonical Git boundary where Git is required;
- make source and revision part of durable provenance/lock state when the product contract requires them;
- ensure remote failure cannot silently fall back to a different source or unverified package.

## Scope/isolation

Global installed skills and project references are different scopes.

Required invariants:

- project references cannot escape their project/workspace scope;
- global installation does not automatically make a skill active in every project;
- cross-workspace mutation is rejected;
- disabled/removed skills are not exposed as active;
- identity is propagated into audit/provenance.

## Secrets and sensitive data

Never persist or log:

- API keys;
- access tokens;
- credentials;
- secret values;
- authorization headers;
- raw sensitive provider payloads.

Use existing secret-reference/redaction mechanisms.

## Required threat cases

Test:

- forged/mismatched caller identity;
- unauthorized install/remove/enable/disable;
- capability self-grant;
- cross-workspace/project access;
- malicious manifest metadata;
- traversal and symlink escape;
- oversized package/download;
- malicious registry URL/reference;
- integrity mismatch;
- registry/package replacement race;
- interrupted publication;
- corrupt state;
- concurrent mutation;
- secret leakage;
- remote-source confusion/fallback;
- replay of stale lifecycle requests.

# 6. Persistence and recovery

## Persistence ownership

The implementation must explicitly assign ownership as follows, adjusting only if current `rust` contains a stronger established contract:

| State | Current location | Canonical owner | Required behavior |
|---|---|---|---|
| Globally installed skill packages | `~/.agent-workspace-hub/skills` | Global Skills registry/service | validate before publish; no partial trusted package |
| Project skill references | `.agent/skills.json` | One converged ProjectSkillReferences implementation | atomic/scoped persistence; no duplicate writer |
| Skill lock/pinning state | `.agent/skills.lock.json` | Lockfile store/service | schema/version validation; deterministic reload |
| Configured registry URLs | Skills registry config / `registries.json` | Registry configuration store | URL validation; atomic persistence |
| Remote/package cache | configured installer/remote cache | Skills cache owner | bounded, non-authoritative, safe cleanup |
| Audit records | existing persistent AuditLog | Audit subsystem | Skills emits events; Skills does not own audit storage |

The domain model is not itself a persistence authority.

## Publication protocol

For any durable mutation use an explicit:

`load → validate current state → prepare/stage → validate staged state → publish atomically → reload/verify → emit audit`.

Do not delete the currently trusted installation before the replacement has passed all validation required by the contract.

If replacement cannot be atomic on the target filesystem, implement a documented recovery marker/backup protocol using existing repository primitives and prove crash behavior.

## Corruption and restart

For each persisted state prove:

`write/publication → process termination → new process → reload → validation → identical intended state`.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. Never silently reinterpret corrupt data as an empty or newly trusted registry.

## Migration and retention

- Document schema versions for registry/reference/lock state where schemas are durable.
- Provide deterministic migration for supported older versions.
- Preserve backward compatibility only where the existing product contract requires it.
- Define cleanup ownership for remote caches and obsolete package versions.
- Cleanup must never delete a package still referenced by an active project without an explicit lifecycle rule.
- Do not invent destructive retention policies merely to simplify implementation.

# 7. Integration boundaries

| Boundary | Existing authority | Skills responsibility |
|---|---|---|
| Workspace/project | init/workspace/project services | resolve and enforce scope |
| Agent/session | AgentRuntimeService | propagate caller identity |
| Capability/policy | PolicyEngine/authorization | request/check consequential authority |
| Filesystem | FilesService | reuse canonical path/resource safety |
| Edit/recovery | EditService/Snapshot/Rollback | reuse if skill lifecycle mutates governed workspace content |
| Audit | persistent AuditLog | emit correlated redacted events |
| Git/worktree | canonical Git/worktree service | use for supported Git sources; no shell Git implementation |
| MCP | MCP dispatcher/session | adapter only |
| TUI | TUI backend/application services | presentation/control only |
| Control API | versioned Control API | thin adapter only |
| Secrets | existing secret references/redaction | never create skill-local secret storage |

## No-duplication rule

Do not introduce parallel:

- identity/session resolution;
- authorization or capability evaluation;
- filesystem safety;
- package execution/security;
- audit persistence;
- Git invocation;
- MCP session/authentication;
- project/workspace scope;
- skill registry/reference models;
- package-manager semantics.

Adapters must call canonical services rather than reproduce business logic.

# 8. Testing

## Unit tests

Test:

- strict `SKILL.md` parsing;
- required metadata and name/version validation;
- capability/requirement declaration validation;
- package size/path/symlink checks;
- SHA-256/integrity verification;
- source/ref parsing;
- registry/reference/lock serialization;
- lifecycle transition validation;
- deterministic sorting and duplicate handling;
- structured error classification.

## Integration tests

Use real temporary filesystem state to test:

- local install;
- remote/registry installation where an available test fixture exists;
- atomic replacement;
- project reference add/remove/resolve;
- enable/disable;
- lock persistence and reload;
- registry configuration persistence;
- restart after each major mutation;
- canonical CLI → service path;
- MCP → service path;
- TUI backend → service path;
- Control API → service path if exposed.

## Security/adversarial tests

Include:

- traversal and symlink attacks;
- malformed/oversized package;
- corrupt manifest/lock/reference state;
- integrity mismatch;
- remote URL/reference abuse;
- capability self-grant attempts;
- authorization bypass attempts;
- cross-project/workspace access;
- stale identity/session;
- concurrent install/remove/enable;
- interrupted publication;
- secret leakage.

## Failure/recovery tests

At minimum:

- failed download;
- failed clone;
- invalid package after download;
- integrity failure;
- destination publication failure;
- crash between stage and publish;
- crash during registry/reference/lock update;
- corrupt persisted state;
- repeated install/remove/enable/disable;
- concurrent conflicting operations.

## Compatibility tests

Verify existing supported:

- `SKILL.md` packages;
- global registry behavior;
- project references;
- lockfile format;
- remote source syntax;
- CLI behavior;
- MCP skill discovery/read;
- TUI project reference behavior.

## Verification gates

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

If an external service, registry, GitHub target, platform, terminal emulator, remote target, or release environment is unavailable, record the missing evidence instead of claiming verification.

# 9. Rollout

## Skills-specific rollout sequence

1. **Inventory current contracts**
   - inspect all `src/skills/*`, MCP/TUI/API/CLI adapters, and policy/audit boundaries;
   - identify the exact duplicate reference implementations and any additional writers/readers.

2. **Select canonical domain ownership**
   - establish one Skills service/domain boundary;
   - retain the existing `Skill` model and compatible parser/package primitives;
   - converge `project.rs` and `references.rs` to one project-reference authority.

3. **Harden package/source validation**
   - validate metadata, paths, symlinks, size, source syntax, integrity, and remote bounds;
   - ensure untrusted content is never treated as installed/trusted before validation completes.

4. **Make persistence transactional**
   - define registry/reference/lock/cache ownership;
   - add schema/version/corruption/restart behavior;
   - publish installations and metadata atomically or with explicit recovery semantics.

5. **Integrate identity and authorization**
   - carry caller/workspace/project/session context;
   - enforce PolicyEngine/authorization for consequential lifecycle actions;
   - ensure declared capabilities never become granted authority automatically.

6. **Converge interfaces**
   - route CLI, MCP, TUI, and Control API through the same service;
   - remove adapter-local persistence/business semantics.

7. **Add lifecycle and adversarial verification**
   - test normal lifecycle, concurrency, crash/restart, corruption, scope isolation, and remote/package attacks.

8. **Run repository verification**
   - execute all required cargo gates and report exact evidence/limitations.

9. **Update documentation only for proven behavior**
   - document the canonical contract, persistence locations, security semantics, and known limitations without overstating implementation status.

No rollout step may depend on another prompt or PR being merged first.

## Compatibility

Preserve established `SKILL.md`, registry, project-reference, lockfile, CLI, MCP, and TUI semantics where they do not conflict with the canonical security and ownership invariants.

Existing trusted/valid state must not be discarded merely to simplify migration.

## Failure behavior

- Integrity, policy, authorization, validation, or publication failure preserves the prior trusted state.
- A failed remote acquisition cannot silently install a different package.
- A failed migration cannot silently replace durable state with empty state.
- An interrupted operation is either safely recoverable or deterministically reported as incomplete.

## Observability

Use the existing structured logging and persistent AuditLog.

Audit lifecycle mutations with correlation identifiers where supported, including skill name/version/source and caller/scope identifiers as appropriate.

Never log tokens, credentials, secret values, or unnecessarily raw package/provider payloads.

# 10. Acceptance criteria

## Functional

- [ ] One canonical Skills domain/service owns lifecycle semantics.
- [ ] One authoritative global skill registry exists.
- [ ] One authoritative project-reference implementation exists; duplicate reference writers are removed/converged.
- [ ] Install/remove/enable/disable have explicit lifecycle semantics.
- [ ] Local and remote sources are validated before publication.
- [ ] Integrity is verified before a package becomes trusted/installed.
- [ ] Lock/pinning semantics are deterministic where exposed.
- [ ] Declared skill requirements do not self-grant capabilities.
- [ ] Installed, referenced, enabled, declared, and authorized states are distinct.
- [ ] CLI/MCP/TUI/Control API interfaces converge on the same semantics.

## Architecture

- [ ] Architecture flow is implemented as one canonical service boundary.
- [ ] Existing `Skill`, parser, package, registry, lock, identity, policy, filesystem, audit, Git, and MCP contracts are reused or explicitly extended.
- [ ] No duplicate Skills persistence authority exists.
- [ ] No interface adapter contains an alternate business-logic implementation.
- [ ] Global vs project scope is explicit.
- [ ] Restart and lifecycle behavior are explicit.

## Security

- [ ] Authorization is enforced for consequential lifecycle actions.
- [ ] A skill cannot self-grant capabilities.
- [ ] Runtime skill-provided tool actions remain policy/capability checked.
- [ ] Path traversal and symlink escape are rejected.
- [ ] Package/source/resource limits are enforced.
- [ ] Remote sources are bounded and integrity checked.
- [ ] Cross-scope access is rejected.
- [ ] Secrets are redacted and never persisted as skill metadata.
- [ ] Security failures fail closed where required.

## Persistence/recovery

- [ ] Persistence owner is documented for global registry, project references, lock state, registry configuration, and cache.
- [ ] Durable state survives restart.
- [ ] Publication is atomic or has explicit crash recovery.
- [ ] Corrupt/truncated/incompatible state is detected.
- [ ] Schema/migration behavior is deterministic.
- [ ] Cache cleanup cannot silently destroy active skill state.
- [ ] Recovery tests prove prior trusted state is preserved after failed mutation.

## Interfaces

- [ ] CLI uses canonical service semantics.
- [ ] MCP uses canonical registry/reference semantics.
- [ ] TUI uses canonical backend/service operations and does not write persistence directly.
- [ ] Control API, if applicable, uses versioned canonical service semantics.
- [ ] Interfaces expose declared/requested capability separately from granted authority.
- [ ] Real-interface tests cover supported surfaces.

## Testing

- [ ] Unit tests pass.
- [ ] Integration tests pass.
- [ ] Adversarial/security tests pass.
- [ ] Failure/recovery tests pass.
- [ ] Concurrency/restart tests pass.
- [ ] Applicable CLI/MCP/TUI/API tests pass.
- [ ] Repository verification gates pass.

## Documentation

- [ ] Canonical Skills ownership and lifecycle are documented.
- [ ] Persistence locations and migration/recovery semantics are documented.
- [ ] Capability declaration vs authorization semantics are documented.
- [ ] Runtime behavior is not overstated.
- [ ] Known evidence gaps and limitations are reported.

# 11. Explicit non-goals

- No arbitrary skill/plugin code-execution framework.
- No automatic capability grants.
- No second package/tool registry.
- No second project-reference store.
- No provider-specific security boundary.
- No cloud skill marketplace or mandatory hosted registry.
- No model routing or agent orchestration.
- No generic workflow/DAG engine.
- No unrelated infrastructure redesign.

# 12. Final implementation report

Report:

Feature:  
Canonical implementation:  
Existing contracts reused:  
Duplicate/legacy contracts converged:  
Files changed:  
Interfaces added/changed:  
Persistence owners and migrations:  
Source/integrity controls:  
Capability/policy controls:  
Audit/observability:  
Tests added:  
Real-interface evidence:  
Verification results:  
Known limitations:

## Standalone execution rule

A developer must be able to read this prompt, inspect current `rust`, identify existing implementations, implement only this feature's missing/incomplete contract, test it, and verify acceptance without another prompt, PR, or undocumented assumption.
