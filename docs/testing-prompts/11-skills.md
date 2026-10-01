# Master Test Prompt 11 — Skills

## Mission

Create and execute a complete, human-behavior-oriented verification suite for AWH's **Skills** feature on the current `rust` branch.

The suite must verify the complete lifecycle of developer skills: discovery, parsing, validation, global installation, project references, enable/disable state, removal, lock/pinning where implemented, local/remote source handling, integrity, filesystem safety, capability declarations, authorization boundaries, persistence/restart/recovery, and real CLI/MCP/TUI/Control API behavior where implemented.

**Do not treat `cargo test` alone as sufficient evidence.**

Inspect the current implementation first. Test real production boundaries and independent observable state. Do not implement missing production behavior merely to make tests pass. Classify defects and gaps honestly.

---

## 1. Feature boundary

AWH Skills own:

- skill metadata and `SKILL.md` parsing;
- package/path validation;
- global installed-skill registry;
- project skill references;
- enable/disable runtime exposure;
- install/remove lifecycle;
- registry/source configuration;
- lock/pinning/integrity state where implemented;
- supported local/remote acquisition;
- interface adapters.

The current repository identifies:

```text
GlobalSkillRegistry
ProjectSkillReferences
SkillInstaller
Skill parser/package validation
RegistryClient / registry configuration
Lockfile state
MCP SkillMcp
TUI Skills screen/backend
CLI skill commands
```

as relevant production boundaries.

Current implementation evidence indicates:

- global skills are rooted under `~/.agent-workspace-hub/skills`, with `AWH_GLOBAL_SKILLS_ROOT` available as a test/embedding seam;
- project references are persisted under `.agent/skills.json`;
- project disabled state is represented in that same reference store;
- MCP `skills.list` exposes project references and enabled state;
- MCP `skills.read` refuses disabled or unreferenced skills;
- the canonical project-reference implementation is `ProjectSkillReferences`;
- older duplicate reference implementations must not become independent authorities.

Verify these against the current branch rather than treating this prompt or historical documentation as proof.

Out of scope:

- arbitrary plugin execution;
- automatic capability grants;
- a second policy/authorization system;
- unrestricted remote Git execution;
- model routing;
- agent reasoning;
- autonomous orchestration;
- generic workflow/DAG scheduling;
- a second package manager.

A skill's declared capabilities/requirements are **data**. They are never equivalent to an actual AWH capability grant.

---

## 2. Required repository forensics

Before adding tests, inspect the current `rust` branch.

Read at minimum:

- `README.md`
- `AGENTS.md`
- `Cargo.toml`
- `docs/FEATURES.md`
- `docs/testing.md`
- `docs/configuration.md`
- `docs/security.md`
- `docs/threat-model.md`
- `docs/architecture.md`
- `docs/CLI.md`
- `docs/roadmap/PROJECT_ROADMAP.md`
- `docs/roadmap/GROWTH_STRATEGY.md`
- `docs/implementation-prompts/README.md`
- `docs/skills/skills-implementation-prompt.md`
- testing prompts `01`–`10` sufficiently to avoid overlap.

Inspect:

```text
src/skills/mod.rs
src/skills/model.rs
src/skills/parser.rs
src/skills/package.rs
src/skills/registry.rs
src/skills/registries.rs
src/skills/installer.rs
src/skills/project.rs
src/skills/references.rs
src/skills/lockfile.rs
src/skills/remote.rs
src/mcp/skills.rs
src/mcp/tool_registry.rs
src/mcp/dispatcher.rs
src/tui/screens/skills.rs
src/tui/backend.rs
src/main.rs
src/services/authorization.rs
src/services/audit.rs
src/services/files.rs
tests/
```

Search before adding test helpers or duplicate abstractions:

```text
rg -n "Skill|SkillSummary|GlobalSkillRegistry|ProjectSkillReferences|SkillInstaller|SkillLock|SkillReferences|SKILL.md|awh skill|skills.list|skills.read" src tests docs
rg -n "capability|requirement|authorize|policy|audit" src/skills src/mcp src/tui
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\(" src/skills src/mcp/skills.rs src/tui/screens/skills.rs
```

Document:

- canonical Skill model;
- canonical global registry;
- canonical project reference store;
- canonical lock/pinning store;
- canonical installer/source boundary;
- existing identity/policy/filesystem/audit authorities;
- all interface adapters;
- any historical/duplicate implementation still present.

Do not create a test-only Skills architecture.

---

## 3. Human-first end-to-end workflows

Tests must model realistic developer behavior.

### Workflow A — create, install, reference, use

```text
create disposable global registry
→ create or acquire valid skill package
→ validate/parse
→ install
→ create disposable project
→ reference skill
→ list project skills
→ read skill
→ independently inspect installed/project state
```

Verify every lifecycle state independently.

### Workflow B — disable and re-enable

```text
install skill
→ add project reference
→ verify enabled
→ disable
→ verify reference remains
→ verify runtime read/exposure is denied
→ enable
→ verify runtime exposure returns
```

The disabled state must not be confused with removal.

### Workflow C — remove

```text
reference skill
→ remove project reference
→ verify absent from project references
→ verify runtime exposure is absent
→ reinstall/re-reference if supported
```

Do not assume removing a project reference deletes the global installed package unless the current contract explicitly says so.

### Workflow D — restart

```text
install/reference/enable/disable
→ terminate process
→ start a fresh process
→ reload registry/project state
→ verify identical intended lifecycle state
```

An in-memory result is not persistence evidence.

### Workflow E — interface parity

```text
mutate via CLI
→ observe through MCP/API/TUI
→ mutate via another supported interface
→ observe through CLI and raw state
```

Every applicable interface must converge on the same domain semantics.

---

## 4. Disposable isolated environment

Never use the developer's real global skill registry.

Use:

- unique temporary global registry roots;
- unique project/workspace roots;
- disposable package/cache directories;
- local HTTP servers for remote-source tests;
- unique ports;
- controlled environment variables;
- unique agent/session identities where authorization requires them.

Prefer `AWH_GLOBAL_SKILLS_ROOT` over mutating `HOME`.

If the current implementation exposes additional Skills environment variables, discover and use their exact names.

Never allow tests to modify:

```text
~/.agent-workspace-hub/skills
real .agent/skills.json
real lockfiles
real registry configuration
real user credentials
```

Tests must be order-independent and cleanup-safe.

---

## 5. Skill manifest parsing and validation

Test the real `SKILL.md` parser and model.

Cover:

- valid front matter;
- missing front matter;
- malformed YAML/front matter;
- missing name;
- invalid name;
- name length boundary;
- uppercase names;
- path traversal names;
- absolute/path-like names;
- invalid version;
- missing/empty description;
- Unicode description;
- multiline body;
- empty body;
- malformed metadata;
- unknown metadata according to the current parser contract;
- duplicate metadata;
- very large manifest;
- invalid encoding where relevant.

Verify:

- parser errors are deterministic;
- invalid metadata cannot become an installed trusted skill;
- parsed name matches package/reference identity;
- parser does not silently normalize an unsafe name into a different skill;
- body and metadata are preserved according to the documented model.

Use the current implementation's actual validation rules.

---

## 6. Package and filesystem safety

Build disposable malicious packages containing:

- `../` paths;
- absolute paths;
- nested traversal;
- symlinks escaping package root;
- symlink chains;
- files whose names collide with destination metadata;
- unexpected directories;
- excessive file counts;
- excessive total size;
- oversized `SKILL.md`;
- malformed package layout;
- unreadable files where practical.

Verify:

- package validation rejects unsafe layouts;
- extraction/copy cannot escape the allowed package root;
- symlink behavior is explicit and safe;
- destination remains within canonical Skills storage;
- validation happens before trust/publication;
- rejected packages do not leave partially installed trusted state;
- an attack against skill A cannot modify skill B or unrelated project state.

Inspect filesystem state after every failed installation.

---

## 7. Global registry behavior

Exercise the real `GlobalSkillRegistry`.

Cover:

- missing registry root;
- empty registry;
- one skill;
- multiple skills;
- deterministic list ordering;
- get existing;
- get missing;
- invalid names;
- duplicate creation;
- Unicode metadata;
- malformed installed `SKILL.md`;
- missing `SKILL.md`;
- unexpected directory;
- registry corruption.

Verify:

- only valid installed skills are exposed;
- list ordering is deterministic;
- invalid names cannot address arbitrary filesystem paths;
- a missing global skill is not fabricated;
- malformed installed metadata fails according to the current contract;
- global installation does not automatically activate a skill in every project.

---

## 8. Project reference lifecycle

Exercise the canonical `ProjectSkillReferences` implementation.

Cover:

- empty project;
- add installed skill;
- add duplicate;
- add uninstalled skill;
- remove existing;
- remove missing;
- list/resolve;
- reference ordering;
- disabled state;
- re-enable;
- invalid names;
- unreferenced enable;
- unreferenced disable;
- disabled skill re-add;
- missing global installation after reference;
- legacy `.agent/skills.json` without disabled state.

Verify:

- references are project-scoped;
- duplicate references are not created;
- add does not silently grant runtime authority;
- remove does not accidentally delete unrelated global state;
- disable preserves the reference;
- enable/disable only operate on referenced skills;
- failed enable/disable cannot mutate state;
- legacy files default according to the current compatibility contract;
- missing globally installed skills are handled according to current resolution semantics.

Independently inspect `.agent/skills.json`.

---

## 9. Enable/disable security semantics

This is a critical boundary.

Verify:

```text
installed ≠ referenced
referenced ≠ enabled
enabled ≠ authorized
declared capability ≠ granted capability
```

Test:

- installed but unreferenced skill;
- referenced and enabled skill;
- referenced and disabled skill;
- removed skill;
- re-enabled skill;
- stale disabled state;
- forged disabled/reference state;
- direct MCP read of disabled skill;
- direct MCP read of unreferenced skill.

Expected behavior must be based on the current implementation.

For a disabled project reference, verify it remains visible to management/listing if that is the current contract, while runtime exposure is denied.

---

## 10. CLI black-box testing

Use the real compiled `awh` binary and exact current syntax discovered from `awh --help`.

Exercise every implemented command:

```text
awh skill list
awh skill show
awh skill install
awh skill remove
awh skill enable
awh skill disable
```

Where command names/options differ, use the current implementation rather than inventing syntax.

Verify:

- exit codes;
- stdout;
- stderr;
- invalid arguments;
- missing skill;
- invalid skill name;
- installed/reference/enabled state;
- install from supported local/remote source;
- remove semantics;
- enable/disable persistence;
- human-readable output;
- no secret leakage;
- no local-path confusion;
- no accidental writes outside the disposable environment.

Do not call internal Rust methods and claim CLI coverage.

---

## 11. MCP Skills boundary

Use the real MCP dispatcher/session/tool-registration path.

At minimum test implemented tools such as:

```text
skills.list
skills.read
skills.add
skills.remove
skills.enable
skills.disable
```

and any additional current Skills tools.

Verify:

- tool registration matches actual implementation;
- project scope is preserved;
- list shows the current project references;
- enabled state is accurately represented where applicable;
- disabled references remain management-visible if documented;
- read of disabled skill fails closed;
- read of unreferenced skill fails closed;
- read of missing globally installed skill fails correctly;
- add/remove/enable/disable use canonical project state;
- malformed arguments fail safely;
- MCP cannot directly bypass policy/authorization;
- no MCP-local persistence store exists.

Generic MCP protocol/transport correctness remains outside this prompt; test only Skills-specific semantics.

---

## 12. TUI integration

Where the TUI Skills screen is implemented:

- start/use the real TUI backend path or the closest production integration seam;
- list installed/project skills;
- inspect enabled/disabled state;
- perform supported enable/disable/reference actions;
- switch project/workspace;
- verify stale selections cannot mutate another project;
- verify mutations persist;
- verify CLI/MCP observe the same state.

Do not rely solely on snapshot/rendering tests.

If TUI interaction cannot be automated in the environment, mark it **Blocked/Unproven** with evidence rather than claiming success.

---

## 13. Control API integration

Where Skills are exposed through the versioned Control API:

- list project skills;
- inspect state;
- perform supported lifecycle mutations;
- use authenticated caller identity;
- test project/workspace scoping;
- verify CLI/MCP and raw persistence see identical state;
- test bounded request/response behavior;
- test structured errors;
- inspect that API handlers do not write skill state directly.

Do not duplicate the generic Control API suite.

If the API surface is absent, classify it as **Not implemented**.

---

## 14. Local installation lifecycle

Use real local skill directories.

Test:

```text
valid source
→ validate
→ install
→ inspect destination
→ parse installed skill
→ restart
→ get/list
```

Also test:

- duplicate installation;
- replacement of an existing skill;
- source name mismatch;
- destination collision;
- source disappears during installation;
- source changes during installation where controllable;
- invalid source;
- source inside destination;
- source containing symlinks;
- read failure;
- write failure.

Verify failed/rejected installations leave no partially trusted skill.

Do not claim atomic installation merely because the final directory looks correct after a successful run.

---

## 15. Remote registry/source behavior

Use a disposable local HTTP server or other controlled source supported by the implementation.

Never test against an uncontrolled production registry as the only evidence.

Cover:

- valid registry manifest;
- valid skill download;
- missing skill;
- HTTP 404/4xx;
- HTTP 5xx;
- invalid JSON/manifest;
- unsafe returned path;
- absolute returned path;
- traversal path;
- oversized response;
- truncated response;
- timeout;
- connection failure;
- redirect behavior;
- malformed URL;
- unsupported scheme;
- source/revision mismatch;
- integrity mismatch;
- valid SHA-256;
- changed bytes after manifest digest;
- remote source disappearing between discovery and download.

Verify:

- response sizes/timeouts are bounded;
- only supported source forms are accepted;
- downloaded content is validated before publication;
- integrity mismatch never becomes installed state;
- remote errors do not silently fall back to another source;
- no shell interpolation is used for source acquisition;
- failed remote operations leave no trusted partial installation.

Use the actual configured timeout/size values from the current implementation.

---

## 16. Integrity and lock/pinning behavior

Where lockfiles or integrity metadata are implemented, test:

- first lock creation;
- deterministic serialization;
- pinned version/revision;
- recorded digest;
- matching digest;
- mismatching digest;
- changed remote content;
- missing lock record;
- stale lock record;
- duplicate entries;
- malformed lockfile;
- unsupported schema/version;
- restart/reload;
- lock/reference inconsistency.

Verify:

- integrity verification happens before trust/publication;
- lock data cannot itself grant authorization;
- stale or corrupt lock state fails according to the current contract;
- deterministic lock state can be independently parsed;
- removal/cleanup does not silently corrupt unrelated locks.

If lock/pinning is not implemented for a path, mark that path **Not implemented**.

---

## 17. Transactionality and partial publication

For every mutating operation test:

```text
load current state
→ validate
→ prepare/stage
→ publish
→ reload
→ verify
```

Inject failures at:

- validation;
- package copy;
- temporary file creation;
- file write;
- fsync;
- rename;
- destination creation;
- registry/reference persistence;
- lock persistence;
- remote download;
- integrity validation.

Verify:

- failure is reported;
- no false success is emitted;
- no partially trusted skill appears installed;
- prior valid state remains usable;
- unrelated skills/projects remain unchanged;
- retry behavior is deterministic;
- temporary artifacts are cleaned or follow an explicit recovery protocol.

Inspect filesystem and persisted JSON state independently.

---

## 18. Persistence and restart

For each durable Skills state:

```text
mutate
→ terminate process
→ fresh process
→ reload
→ independently inspect state
```

Test persistence of:

- installed skill metadata;
- project references;
- disabled state;
- lock/pinning state;
- registry configuration where applicable.

Verify:

- intended state survives restart;
- failed operations do not become successful after restart;
- corrupt state is not silently replaced with empty state;
- project A's state cannot appear in project B.

---

## 19. Corruption and recovery

Inject corruption into disposable:

- `SKILL.md`;
- `.agent/skills.json`;
- lockfiles;
- registry metadata;
- downloaded package cache;
- temporary publication state.

Cover:

- malformed JSON;
- truncated JSON;
- invalid fields;
- duplicate identities;
- invalid names;
- unsupported versions;
- invalid references;
- inconsistent disabled lists;
- malformed manifest;
- corrupted package bytes.

Verify:

- corruption is detected;
- no corrupt skill becomes trusted;
- no unrelated project is changed;
- recovery is deterministic;
- automatic recovery, if any, is explicitly validated;
- the system does not silently convert corruption into an empty registry/project.

---

## 20. Concurrency and TOCTOU

Use multiple real threads/processes where supported.

Test:

- concurrent project reference additions;
- concurrent enable/disable;
- concurrent remove and enable;
- concurrent installation of the same skill;
- concurrent replacement of the same global skill;
- list while mutation occurs;
- read while replacement occurs;
- lock contention;
- source changes between validation and publication;
- symlink changes between validation and copy where controllable.

Verify:

- no lost valid updates beyond documented semantics;
- no duplicate references;
- no malformed persisted state;
- no unauthorized lifecycle transition;
- no package escape;
- no partial trusted installation;
- lock/serialization behavior is deterministic.

---

## 21. Authorization and capability boundary

Reuse the existing AWH authorization/PolicyEngine/capability system.

Test:

- authorized skill lifecycle operation;
- denied install;
- denied remove;
- denied enable/disable;
- wrong agent identity;
- wrong session identity;
- wrong workspace/project;
- disabled/inactive agent where applicable;
- forged capability;
- skill manifest requesting a powerful capability;
- skill attempting to use a capability without an actual grant.

Verify:

1. declared requirements remain metadata;
2. installation does not grant capability;
3. enabling does not grant capability;
4. being listed/read does not grant capability;
5. runtime consequential actions still pass through canonical policy;
6. denied actions have zero unintended side effects;
7. caller identity reaches audit/provenance where required.

Do not build a Skills-local authorization bypass.

---

## 22. Audit and observability

Use the canonical persistent audit service where Skills operations are audited.

Verify memory-independent Skills events for:

- install;
- remove;
- add/reference;
- enable;
- disable;
- authorization denial;
- integrity failure where auditing is required.

Check:

- actor/agent/session/workspace correlation;
- action;
- outcome;
- correlation identifiers;
- timestamps;
- secret redaction;
- no raw credential/token leakage;
- no misleading successful event for a failed mutation.

Do not duplicate Master Test Prompt #08's general audit suite.

---

## 23. Sensitive-data protection

Use disposable canaries representing:

- API-key-like strings;
- bearer tokens;
- passwords;
- private-key-like content;
- credential-bearing registry URLs;
- sensitive source content.

Inspect:

- CLI stdout/stderr;
- MCP responses;
- API responses;
- TUI data paths;
- logs;
- audit records;
- registry configuration;
- cache files.

Distinguish legitimate skill content stored as package data from information that must not leak into logs/audit/errors.

Verify secrets are not expanded, echoed, or accidentally persisted outside the intended skill/package state.

---

## 24. Resource limits and abuse resistance

Test actual current limits for:

- manifest size;
- package size;
- package file count;
- path length;
- skill name length;
- description/body size;
- registry response size;
- cache size where bounded;
- number of installed skills;
- number of project references;
- lockfile size;
- concurrent installation requests.

Use:

- exact limit;
- one byte/item over limit;
- very large pathological value.

Verify limits fail before unsafe publication and cannot be bypassed through another interface.

A limit enforced only by CLI but not the underlying service is insufficient.

---

## 25. Replay and lifecycle edge cases

Replay:

- duplicate install;
- duplicate reference;
- repeated enable;
- repeated disable;
- remove twice;
- enable after remove;
- disable after remove;
- reinstall after removal;
- stale lock;
- repeated failed install;
- retry after integrity failure;
- restart after lifecycle mutation.

Verify current documented semantics rather than imposing generic idempotency.

Especially ensure that repeated operations cannot:

- resurrect removed references;
- silently enable disabled skills;
- create duplicate references;
- bypass integrity;
- widen authorization.

---

## 26. Independent oracles

Never prove a Skills operation solely from its return value.

Cross-check with:

- raw global registry files;
- raw `.agent/skills.json`;
- raw lockfile;
- independent `SKILL.md` parsing;
- filesystem existence and containment;
- SHA-256 calculated independently;
- CLI output;
- MCP output;
- API/TUI output;
- audit records;
- fresh-process reload.

Example:

```text
CLI reports install success
+
installed directory exists under disposable global root
+
SKILL.md independently parses
+
independent digest matches expected content
+
fresh process lists the skill
```

All applicable observations must agree.

---

## 27. Property and boundary tests

Add meaningful property tests where useful.

Important properties:

```text
invalid skill names never escape the canonical registry root

project A references cannot mutate project B

installed skill does not imply project activation

disabled reference remains non-exposed

enable/disable only operate on valid references

declared capability never becomes granted capability automatically

integrity mismatch never produces trusted installation

failed publication leaves no partially trusted skill

restart preserves durable intended state

list ordering is deterministic

duplicate project references do not multiply

same source bytes produce stable digest

migration/legacy loading does not silently broaden authority
```

Include:

- empty;
- minimum;
- maximum;
- one-over-limit;
- Unicode;
- malformed;
- traversal;
- symlink;
- concurrent values.

---

## 28. Cross-platform verification

Run applicable tests on:

- Linux x86_64;
- Linux ARM64;
- macOS x86_64/ARM64;
- Windows x86_64;
- Android/Termux ARM64 where practical.

Focus on:

- path containment;
- symlink behavior;
- atomic rename;
- permissions;
- filesystem locking;
- Unicode;
- process restart;
- local HTTP source handling.

Do not claim a platform passed unless its relevant tests actually executed.

---

## 29. Test implementation requirements

Prefer:

- unit tests for parser/model/package validation;
- integration tests for registry/reference/installer lifecycle;
- disposable real filesystem state;
- subprocess CLI tests;
- real MCP dispatcher/session tests;
- real API/TUI backend integration where available;
- local HTTP server for remote behavior;
- independent hashing/parsing;
- actual authorization/audit authorities;
- failure injection at publication boundaries.

Do not create:

- a second Skills registry;
- a second project-reference store;
- a fake policy engine that replaces production authorization;
- a fake audit authority;
- a fake filesystem security layer;
- a test-only package format;
- an interface-local persistence implementation.

Tests must exercise the production path.

---

## 30. Verification gates

Run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Then run focused Skills tests covering:

- parser/model;
- package safety;
- global registry;
- project references;
- enable/disable;
- local installation;
- remote installation;
- integrity;
- lock/pinning;
- persistence/restart;
- corruption/recovery;
- concurrency;
- CLI;
- MCP;
- TUI/API where implemented;
- authorization;
- audit;
- sensitive-data handling;
- resource limits;
- security regressions.

A passing unit suite is necessary but not sufficient.

---

## 31. Evidence classification

Classify every behavior as:

### Passed
Real behavior executed and independently verified.

### Failed
Real behavior executed and exposed a production defect.

### Blocked
Required external/environmental prerequisite was unavailable.

### Unproven
Evidence is insufficient.

### Not implemented
The target behavior is absent from the current branch.

For failures report:

- operation;
- interface;
- project/workspace;
- skill name;
- expected result;
- actual result;
- persisted-state observation;
- error category;
- security impact;
- relevant subsystem.

Never print actual secret values.

---

## 32. Completion criteria

This prompt is complete only when:

- the current Skills implementation was inspected;
- canonical model/registry/reference/installer boundaries were identified;
- duplicate persistence authorities were identified and not introduced by tests;
- manifest parsing and validation were tested;
- package/path/symlink safety was tested;
- global registry behavior was tested;
- project reference lifecycle was tested;
- enable/disable semantics were tested;
- installed/referenced/enabled/authorized states were kept distinct;
- local installation was tested;
- remote-source behavior was tested where implemented;
- integrity verification was independently tested;
- lock/pinning behavior was tested where implemented;
- atomic/transactional publication was exercised;
- persistence across restart was proven;
- corruption fails closed or follows an explicitly verified recovery contract;
- concurrency/TOCTOU behavior was exercised;
- CLI behavior was tested where implemented;
- MCP Skills behavior was tested through the real dispatcher;
- TUI/API integration was tested where implemented;
- authorization/capability boundaries were tested;
- audit integration was tested without duplicating #08;
- sensitive-data handling was independently inspected;
- resource limits and abuse cases were tested;
- replay/lifecycle edge cases were tested;
- security regressions became permanent tests;
- final gaps were honestly classified;
- no other testing prompt was modified.

The objective is trustworthy evidence that AWH Skills are **validated, scoped, persistent, integrity-aware, lifecycle-safe, policy-respecting, and consistent across supported interfaces**, without turning a skill declaration into authority.

---

## 33. Scope boundary

This prompt owns **Skills testing only**.

Do not create full feature suites for:

- Foundation and Distribution;
- Workspace Runtime;
- Agent-Grade Filesystem Editing;
- Git/worktrees;
- Capability and Policy Engine;
- Snapshots/Provenance;
- Rollback & Recovery;
- Audit & Observability;
- Context Engine;
- Developer Memory;
- Agent Profiles/policy-routed MCP;
- generic MCP protocol/transport;
- Sessions/Tasks;
- Terminal;
- Collaboration;
- Control API;
- TUI;
- Connectors;
- Advanced infrastructure.

Other feature families may be exercised only at the integration boundary required to prove Skills behavior.

Do not modify:

```text
docs/testing-prompts/01-foundation-distribution.md
docs/testing-prompts/02-workspace-runtime.md
docs/testing-prompts/03-agent-grade-filesystem-editing.md
docs/testing-prompts/04-git-and-worktrees.md
docs/testing-prompts/05-capability-and-policy.md
docs/testing-prompts/06-snapshots-and-provenance.md
docs/testing-prompts/07-rollback-and-recovery.md
docs/testing-prompts/08-audit-and-observability.md
docs/testing-prompts/09-context-engine.md
docs/testing-prompts/10-developer-memory.md
```

Do not modify any other master test prompt while executing this prompt.
