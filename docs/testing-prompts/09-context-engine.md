# Master Test Prompt 09 — Context Engine

## Mission

Create and execute a complete, human-behavior-oriented verification suite for AWH's **Context Engine** on the current `rust` branch.

This prompt owns context state and context assembly/optimization behavior: scoped context items, token budgets, deterministic selection/scoring, compression, offloading/restoration, context snapshots, context policy, persistence, stale-state handling, and integration through the canonical CLI/MCP/TUI/Control API boundaries where implemented.

**Do not treat `cargo test` alone as sufficient evidence.**

Do not implement missing production behavior merely to make tests pass. Inspect the current branch first, exercise the real canonical ContextEngine and existing authorities, and classify gaps honestly.

---

## 1. Feature boundary

The documented Context Engine contract is:

```text
awh context show
awh context save
awh context update
awh context clear
awh context search
```

The current MCP context surface may additionally expose operations such as:

```text
context.status
context.insert
context.get
context.search
context.optimize
context.assemble
```

Use the exact current interface discovered from source/help rather than assuming every target command is implemented.

Context integrates information from:

- workspace/project state;
- filesystem-derived context;
- Git state/changes;
- session history;
- memory;
- skills;
- tool results;
- other documented context sources.

Context must respect:

- workspace/project/agent/session/task scope;
- token budgets;
- deterministic selection;
- protected items;
- stale-context rules;
- persistence and integrity;
- authorization for consequential mutations/restores/clears;
- sensitive-data handling.

External agents remain responsible for reasoning, planning, model selection, and intelligence. The Context Engine does not become an LLM inference or model-routing system.

---

## 2. Canonical architecture to test

The intended flow is:

```text
CLI / MCP / TUI / Control API
          ↓
request + identity validation
          ↓
ContextEngine
          ↓
scope + resource checks
          ↓
budget / scoring / selection / policy
          ↓
active context state
       ↙       ↘
 offload      snapshot
       ↘       ↙
         restore
            ↓
 bounded context result
```

Long-term memory remains owned by the memory subsystem.

Source files remain owned by FilesService.

Authorization remains owned by the existing policy/capability authority.

Audit remains owned by the canonical audit service.

Do not create test doubles that establish a second implementation of any of these authorities.

---

## 3. Required repository forensics

Before writing or changing tests, inspect the current `rust` branch.

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
- the current implementation prompt for Context Engine.

Inspect at minimum:

```text
src/context/mod.rs
src/context/engine.rs
src/context/budget.rs
src/context/item.rs
src/context/selector.rs
src/context/scoring.rs
src/context/policy.rs
src/context/compressor.rs
src/context/offload.rs
src/context/snapshot.rs
src/context/tokens.rs
src/context/planner.rs
src/mcp/context_engine.rs
```

Also inspect:

- CLI context handlers;
- Control API context handlers when present;
- TUI context views when present;
- workspace identity;
- agent/session identity;
- memory authority;
- FilesService;
- policy/capability authority;
- audit service;
- persistence/store locking;
- all existing context tests.

Search for:

```text
ContextEngine
ContextItem
ContextBudget
ContextScope
ContextDecision
OffloadStore
ContextSnapshot
context_engine
context.status
context.insert
context.get
context.search
context.optimize
context.assemble
AWH_CONTEXT_
memory_enabled
offload
compress
stale
snapshot
```

Classify each discovered component as:

- implemented and sufficiently tested;
- implemented but insufficiently tested;
- partial;
- legacy/compatibility;
- duplicate/non-canonical;
- not implemented;
- blocked;
- unproven.

Current source is the source of truth. Historical documentation must not be treated as proof of implementation.

---

## 4. Human-first testing model

Tests must model actual AWH usage rather than only constructing Rust structs.

At minimum test workflows resembling:

### Workflow A — build usable context

```text
create isolated workspace
→ establish applicable agent/session scope
→ create representative project/files/Git state
→ add context items
→ request context for a concrete task
→ inspect assembled result
→ independently verify selected items and budget
```

### Workflow B — context lifecycle

```text
insert
→ get
→ update
→ search
→ optimize
→ clear
→ verify each resulting state
```

### Workflow C — budget pressure

```text
create more context than fits
→ request optimization/assembly
→ verify deterministic selection
→ verify protected items remain protected
→ verify budget constraints
→ verify offload/compression behavior where triggered
```

### Workflow D — persistence/restart

```text
create/offload/snapshot context
→ terminate process
→ start a new process
→ reload the same isolated workspace state
→ inspect/restore/search
→ independently verify recovered state
```

### Workflow E — stale context

Create context from one known state, change the underlying project/runtime state, then request an operation that depends on freshness.

Verify the documented stale-context behavior rather than silently accepting obsolete context.

---

## 5. Disposable isolated environment

Every integration test must use isolated disposable state.

Use unique:

- workspace roots;
- AWH persistent-state roots where configurable;
- workspace IDs;
- agent/session/task identities where applicable;
- repositories;
- context item IDs;
- offload/snapshot fixtures;
- MCP/API ports when real interfaces are tested.

Never use developer-global context state or a real project.

Never depend on another test's context store.

Explicitly control relevant environment variables and restore process environment state after tests.

The Context Engine's documented configuration includes:

```text
AWH_CONTEXT_ENABLED=true
AWH_CONTEXT_MAX_INPUT_TOKENS=128000
AWH_CONTEXT_RESERVED_OUTPUT_TOKENS=8192
AWH_CONTEXT_SAFETY_MARGIN_TOKENS=4096
AWH_CONTEXT_AUTO_OFFLOAD=true
AWH_CONTEXT_AUTO_COMPRESS=true
AWH_CONTEXT_MEMORY_ENABLED=true
```

Use these documented values as defaults where applicable, and deliberately override them in boundary tests.

Do not hard-code obsolete values discovered only in historical reports.

---

## 6. Engine enable/disable behavior

Test `AWH_CONTEXT_ENABLED`.

Verify:

- enabled context operations behave normally;
- disabled behavior follows the documented contract;
- malformed boolean configuration fails safely or uses the documented fail-safe behavior;
- environment overrides are isolated between tests;
- disabling context does not mutate source files;
- disabling context does not silently delete recoverable offloads or snapshots;
- disabled context does not accidentally bypass authorization or expose stored data;
- CLI/MCP/API behavior remains consistent with the engine state.

If the documented contract says disabled operations are no-ops, verify the exact no-op semantics rather than merely checking that no panic occurs.

---

## 7. Context item model and validation

Test real `ContextItem` creation and lifecycle.

Cover:

- valid IDs;
- duplicate IDs;
- empty IDs;
- malformed IDs;
- maximum-length IDs;
- empty content;
- Unicode content;
- multiline content;
- very large content;
- source categories;
- relevance;
- priority;
- scope;
- protected state;
- active/offloaded/archived state.

Verify:

- invalid items are rejected deterministically;
- duplicate IDs cannot silently overwrite unrelated context;
- valid Unicode round-trips exactly;
- bounded content does not truncate unexpectedly unless explicitly documented;
- oversized content is rejected or handled by the documented policy;
- protected metadata is preserved through optimization;
- item scope is explicit and not inferred from user-controlled strings.

Use exact-byte/content comparisons where the contract promises lossless behavior.

---

## 8. Scope and identity isolation

Create at least:

- two workspaces;
- two agents where supported;
- two AWH-native sessions;
- multiple scope types such as Session, Project, and Global where implemented.

Insert distinct canary context into each scope.

Verify:

- session context does not leak into another session;
- project/workspace context does not leak into another workspace;
- agent-scoped context cannot be read by an unauthorized caller;
- global context is returned only according to its explicit documented semantics;
- scope transitions do not silently broaden access;
- user-supplied agent/session/route names cannot manufacture authorization;
- persisted context retains its canonical scope.

If the product intentionally permits a global/operator context view, test that explicit authorization path separately.

---

## 9. Token accounting

Test the real token-counting/budget mechanism.

Cover:

- empty input;
- ASCII;
- Unicode;
- multilingual content;
- punctuation-heavy content;
- long repeated content;
- exact budget boundaries;
- one token over budget;
- reserved output tokens;
- safety margin;
- zero/negative/overflow configuration values;
- extremely large configured values.

Verify:

- accounting is deterministic for identical input;
- configured input, reserved-output, and safety-margin semantics are respected;
- the engine never returns a context result exceeding the effective input budget;
- arithmetic cannot overflow into a permissive budget;
- malformed configuration cannot silently disable safety bounds;
- exact boundary behavior is documented and stable.

Do not assert a particular tokenizer implementation unless it is explicitly part of the contract; assert the resulting budget invariant.

---

## 10. Selection and scoring determinism

Create a fixture containing items with deliberately varied:

- relevance;
- priority;
- source;
- scope;
- size;
- protected state;
- lifecycle state.

Run selection/assembly repeatedly.

Verify:

- identical inputs produce identical selection/order;
- ties resolve deterministically;
- protected items obey the policy;
- already offloaded/archived items are not implicitly reactivated unless the contract says so;
- lower-value items are removed/compressed/offloaded according to the documented policy;
- budget pressure cannot cause arbitrary item selection;
- selection does not depend on hash-map iteration order or process randomness.

Run the same fixture across separate processes where practical.

---

## 11. Protected items and policy decisions

Test the Context Engine's policy layer independently and through the real engine.

At minimum cover:

- protected item under budget pressure;
- protected item during optimize;
- protected item during clear;
- protected item during offload;
- protected item during restore;
- unprotected low-score item;
- invalid policy input;
- policy configuration at boundary values.

Verify protected items follow the documented `Keep` behavior and cannot be discarded by optimization merely because they score poorly.

Consequential operations must still use the existing authorization/capability authority. Context policy must not become a replacement PolicyEngine.

---

## 12. Compression

Test the real compression implementation where available.

Cover:

- normal compressible content;
- incompressible content;
- empty content;
- Unicode;
- very small content;
- large content;
- repeated compression;
- malformed compressed state;
- decompression after restart where persistence exists.

If compression is lossless by contract, independently verify:

```decompress(compress(content)) == content
```

Verify:

- compression does not mutate the original source file;
- item identity and scope remain intact;
- metadata is preserved;
- double-compression is either safely rejected or idempotently handled according to the contract;
- corrupt compressed state is detected;
- decompression failure does not fabricate content.

Do not claim quality improvement merely because compressed bytes are smaller.

---

## 13. Offloading and restoration

Exercise the actual `.agent/context-engine/offloads/` persistence boundary when present.

Test:

```text
active item
→ offload
→ process restart
→ inspect persisted offload
→ restore
→ compare content/metadata/state
```

Verify:

- offloaded content is durably stored;
- offload is scoped to the correct workspace/project;
- restore requires valid identity/scope;
- restored content matches the original according to the contract;
- item identity is preserved;
- metadata is preserved;
- repeated restore behaves deterministically;
- missing offload state is reported explicitly;
- corrupted/truncated offload state is detected;
- clearing active context does not accidentally delete unrelated offloads;
- context cleanup never deletes source files or developer memory.

Test concurrent offload/restore operations where the implementation supports them.

---

## 14. Context snapshots

Keep Context Engine snapshots distinct from file-edit snapshots and rollback.

Test the context-specific snapshot contract where implemented:

- create snapshot;
- list;
- inspect/show;
- restore;
- delete;
- duplicate snapshot handling;
- invalid snapshot ID;
- cross-scope restore;
- corrupted snapshot;
- restart persistence;
- repeated restore.

Verify:

- snapshot captures context state, not arbitrary source-file state;
- restoration returns the expected context item set and metadata;
- snapshot IDs are unique and stable;
- cross-workspace restore is rejected;
- deletion does not remove source files, canonical audit history, or long-term memory;
- corrupted snapshot data fails safely.

Never use a passing ContextSnapshot test as evidence that generic filesystem snapshot/rollback works.

---

## 15. Search semantics

Exercise context search against both active and offloaded items where supported.

Cover:

- exact query;
- partial query;
- case behavior;
- Unicode;
- empty query;
- whitespace-only query;
- very long query;
- no results;
- result-limit boundaries;
- active/offloaded matches;
- cross-scope search.

Verify:

- search is deterministic;
- result scope is correct;
- limits are enforced;
- query input cannot become a filesystem path, shell command, or arbitrary code;
- offloaded search does not accidentally expose data outside the caller's scope;
- search does not mutate context state.

If ranking is documented, verify ranking independently from storage order.

---

## 16. Optimize behavior

Exercise `context.optimize` through the real boundary where implemented.

Use fixtures containing:

- protected items;
- high-value items;
- medium-value items;
- low-value items;
- already offloaded items;
- compressible items;
- items near budget thresholds.

Verify optimization:

- is deterministic;
- respects scope;
- respects budget;
- preserves protected items;
- uses documented Keep/Compress/Archive/Offload semantics;
- does not implicitly restore old state;
- does not mutate source files;
- does not modify canonical memory records unless explicitly part of the contract;
- records the correct audit outcome where required.

Repeat optimization on an already optimized state and verify documented idempotency/stability.

---

## 17. Assemble behavior

Exercise `context.assemble` through the real engine/MCP path where implemented.

Test:

- task-only assembly;
- task + query;
- explicit token budget;
- budget near minimum;
- budget above available context;
- no matching context;
- protected items;
- offloaded candidates;
- deterministic tie cases.

Independently verify:

- result fits the effective budget;
- selected items are valid for the caller's scope;
- selected item metadata is consistent;
- source references are safe;
- assembly does not mutate source files;
- assembly does not silently change long-term memory;
- repeated identical requests produce equivalent results.

If the current branch does not expose `assemble`, classify the missing interface rather than inventing a test-only endpoint.

---

## 18. Stale-context detection

Where stale-state detection is implemented, test it as a real workflow.

Create context based on a known:

- file state;
- Git state;
- workspace metadata;
- session/runtime state.

Then change the underlying state through the canonical service.

Verify the engine:

- detects stale state where the contract requires it;
- does not silently claim old context is current;
- returns a structured stale/conflict condition;
- does not overwrite the new source state;
- permits explicit refresh/rebuild when supported.

Use independent filesystem/Git observations rather than trusting only a context status flag.

---

## 19. Filesystem boundary

Context may consume source-file information, but it must not become a source-file mutation authority.

Test:

- valid file-derived context;
- nonexistent file;
- path traversal;
- absolute path where unsafe;
- symlink escape;
- encoded traversal;
- file deletion between discovery and read;
- file replacement between discovery and read.

Verify:

- existing FilesService/path-safety rules remain authoritative;
- unsafe paths are rejected;
- context cannot write source files;
- context cleanup cannot delete source files;
- stale file reads are handled according to the documented contract.

---

## 20. Memory integration

Context must consume the existing memory authority rather than create a second memory system.

Where memory is implemented, test:

```text
create scoped memory record
→ context reads/uses it
→ inspect assembled context
→ verify source is canonical memory store
```

Verify:

- memory scope is preserved;
- context does not duplicate memory as an independent authoritative store;
- memory deletion/update semantics are respected;
- cross-workspace memory does not leak;
- sensitive memory values are not unnecessarily copied into logs/audit;
- disabling context does not delete memory.

Do not test full memory lifecycle here; only the Context Engine integration boundary belongs in this prompt.

---

## 21. Agent/session/task integration

Where AWH-native runtime identity exists, verify context is correctly bound to it.

Test:

- context created in session A;
- context accessed from session B;
- same agent with different sessions;
- different agent in same workspace;
- task-associated context;
- stopped/paused/invalid session behavior.

Verify:

- identity is resolved through the existing AgentRuntimeService;
- a persisted context record is not trusted solely because it exists;
- invalid/stopped callers cannot regain access by reusing an ID;
- context scope remains consistent after restart;
- MCP route names never substitute for actual authorization.

---

## 22. Audit and observability integration

Use the canonical audit service from the previous feature boundary.

Where context actions are designated auditable, verify:

- consequential context mutation has appropriate audit evidence;
- audit contains safe identifiers/outcomes;
- context content is not dumped into audit;
- secrets are not persisted through audit;
- audit correlation contains workspace/agent/session IDs where applicable;
- audit failure semantics do not falsely rewrite the context operation result.

Do not create a second context-specific audit log.

Do not duplicate the complete Audit & Observability suite from Master Test Prompt #08.

---

## 23. Persistence and restart

Test every context-owned durable domain that is documented as persistent:

- offloads;
- context snapshots;
- other context metadata/state if applicable.

For each:

1. create known state;
2. confirm the documented durability point;
3. terminate the process;
4. start a new process;
5. reload the same workspace;
6. inspect/list/search/restore;
7. independently compare with the pre-restart fixture.

Also test:

- repeated restart;
- empty store;
- many items;
- state-directory relocation where supported;
- old/current schema compatibility where implemented.

Do not count an in-memory round-trip as persistence evidence.

---

## 24. Corruption and partial-publication tests

Inject corruption into real disposable context-owned storage.

Cover:

- malformed JSON/serialization;
- truncated records;
- missing offload content;
- invalid metadata;
- unsupported schema;
- duplicate IDs;
- invalid snapshot IDs;
- inconsistent snapshot membership;
- invalid compressed data;
- partial publication.

Verify:

- corrupt state is detected;
- valid unrelated state remains usable where safe;
- no fabricated context is returned;
- recovery does not silently discard recoverable state;
- errors identify the affected context state without exposing sensitive content;
- source files and canonical memory/audit stores remain untouched.

Test interruption during atomic publication where the implementation provides a controllable boundary.

---

## 25. Concurrency and TOCTOU

Use concurrent threads/processes against the same isolated context state where supported.

Exercise:

- concurrent insert;
- duplicate-ID races;
- concurrent update;
- optimize while inserting;
- optimize while searching;
- offload while restoring;
- snapshot while mutating;
- clear while searching;
- restart during mutation.

Verify:

- no lost valid state beyond documented failure semantics;
- no duplicate identity;
- no impossible lifecycle state;
- no corrupted persisted files;
- locking is respected;
- scope isolation remains intact;
- stale-state checks cannot be bypassed through a race;
- no deadlock prevents bounded recovery.

Use independent counts, IDs, contents, and filesystem state as oracles.

---

## 26. Resource exhaustion and abuse limits

Test pathological inputs:

- huge context item;
- huge number of items;
- huge search result set;
- huge query;
- huge task string;
- huge token budget;
- repeated optimize calls;
- repeated restore calls;
- concurrent requests.

Verify:

- documented limits are enforced;
- allocations are bounded before expensive processing where practical;
- one malformed item cannot exhaust the entire context engine;
- search results are bounded;
- offload storage cannot grow without the documented retention/cleanup controls;
- context operations cannot disable security enforcement through resource pressure.

---

## 27. Sensitive-data protection

Treat context as potentially sensitive.

Generate unique canary values for:

- API keys;
- bearer tokens;
- passwords;
- private-key-like strings;
- environment secrets;
- credentials embedded in tool output;
- sensitive file content;
- sensitive memory values.

Pass them through applicable context sources.

Inspect:

- persisted context state;
- offload files;
- context snapshots;
- search results;
- CLI output;
- MCP responses;
- Control API responses;
- TUI displays;
- audit records;
- diagnostic logs where relevant.

Verify prohibited secrets are not unnecessarily persisted or displayed.

The engine must not solve redaction by silently corrupting legitimate non-secret content. Test the actual documented sensitive-data policy.

---

## 28. CLI black-box tests

Where implemented, test the real compiled `awh` binary.

Exercise the exact current syntax for:

```text
awh context show
awh context save
awh context update
awh context clear
awh context search
```

Verify:

- correct exit codes;
- stdout/stderr behavior;
- scope handling;
- input validation;
- bounded output;
- persistence across process restart;
- no secret leakage;
- errors are structured/deterministic where documented.

Do not call internal Rust functions and label that CLI coverage.

---

## 29. MCP integration tests

Where context MCP tools are implemented, use the real MCP boundary.

Exercise at least:

- status;
- insert;
- get;
- search;
- optimize;
- assemble.

Test:

- valid calls;
- malformed arguments;
- missing required fields;
- oversized inputs;
- unauthorized/mismatched session;
- cross-workspace access;
- deterministic results;
- bounded responses;
- context persistence/restart;
- secret filtering.

The MCP protocol/transport suite remains responsible for generic JSON-RPC/transport behavior. This prompt verifies the context feature's integration through that boundary.

---

## 30. Control API and TUI integration

Where exposed, verify Control API and TUI context operations delegate to the same ContextEngine.

Test that:

- identical requests have equivalent semantics;
- scope/authorization rules remain identical;
- there is no API-local/TUI-local context store;
- output is bounded and redacted;
- clear/restore/search affect the canonical state;
- restart does not produce interface-specific divergent context.

If an interface is not implemented, classify it as **Not implemented**, not **Passed**.

---

## 31. Property and boundary tests

Add deterministic property tests where useful.

Important properties:

```text
identical input + identical configuration => identical selection

decompress(compress(x)) == x
when compression is documented as lossless

query(scope=A) never returns item.scope=B

disabled context never mutates source files

offload → restore preserves documented item identity/content/metadata

clear(context) never deletes source files

context result token usage <= effective budget

duplicate item ID cannot silently replace unrelated state
```

Test Unicode, empty, minimum, maximum, and one-over-boundary values.

Do not generate arbitrary data without asserting a meaningful product/security invariant.

---

## 32. Independent oracles

Do not use ContextEngine output alone to prove correctness.

Cross-check with:

- independent token/budget calculations where practical;
- actual filesystem contents/hashes;
- actual Git status/diff;
- raw persisted context records;
- independent item-ID sets;
- independent scope fixtures;
- process restart;
- known secret canaries;
- canonical memory store;
- canonical audit records;
- actual CLI/MCP/API results.

For example:

```text
ContextEngine says item restored
+
raw persisted state contains expected item
+
independent content comparison matches
+
workspace scope is correct
```

All relevant facts must agree.

---

## 33. Failure injection

Where practical, inject failures at real Context Engine boundaries:

- invalid configuration;
- unwritable context directory;
- storage lock contention;
- malformed persisted state;
- truncated offload;
- corrupted snapshot;
- compression failure;
- missing referenced item;
- stale source state;
- concurrent update;
- interrupted publication;
- invalid scope;
- authorization denial;
- oversized item/query/budget.

For each failure verify:

1. exact error category;
2. operation result;
3. persisted-state result;
4. source-file state;
5. memory state;
6. audit result where applicable;
7. retry safety;
8. secret leakage absence.

Never turn a failure into a pass merely because the process did not panic.

---

## 34. Cross-platform verification

Run applicable Context Engine tests on supported environments:

- Linux x86_64;
- Linux ARM64 where available;
- macOS x86_64/ARM64 where available;
- Windows x86_64;
- Android/Termux ARM64 where practical.

Pay particular attention to:

- path handling;
- filesystem locking;
- atomic persistence;
- encoding/Unicode;
- process restart;
- permissions;
- concurrent access.

Do not claim a platform is verified unless the relevant tests actually ran.

---

## 35. Security regression suite

Every discovered Context Engine security defect must become a permanent regression test.

Examples:

- cross-workspace context leak;
- cross-session context leak;
- forged scope/identity;
- route-name-as-authorization bypass;
- path traversal;
- symlink escape;
- source-file mutation;
- memory-store bypass;
- secret leakage;
- corrupt offload accepted as valid;
- corrupt snapshot accepted as valid;
- sequence/ID collision;
- stale-context bypass;
- budget overflow;
- unbounded search/result allocation;
- concurrent state corruption;
- restore of another workspace's context.

Security regression tests must remain reproducible and isolated.

---

## 36. Test implementation requirements

Prefer:

- unit tests for budget, token, scoring, selection, compression, validation;
- integration tests through the canonical ContextEngine;
- real disposable workspaces;
- real persistent `.agent/context-engine/` state;
- real subprocesses for restart;
- real CLI/MCP/API boundaries where exposed;
- fault injection at persistence/service boundaries;
- independent security canaries.

Do not create:

- a second ContextEngine;
- a second context persistence store;
- a test-only memory store that becomes the oracle;
- a test-only FilesService;
- test-only authorization that bypasses PolicyEngine;
- fake interface adapters that duplicate production semantics.

Reuse existing test helpers when they exercise the real implementation.

---

## 37. Execution gates

Run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Then run focused Context Engine tests for:

- configuration;
- item validation;
- scope/identity;
- token budgeting;
- scoring/selection;
- protected items;
- compression;
- offload/restore;
- snapshots;
- search;
- optimize;
- assemble;
- stale-context detection;
- persistence/restart;
- corruption/recovery;
- concurrency;
- resource limits;
- memory integration;
- filesystem boundary;
- audit integration;
- CLI;
- MCP;
- Control API/TUI where implemented;
- security regressions.

Run external interoperability checks where applicable.

Report commands as passed only when they actually completed successfully.

---

## 38. Evidence reporting

Classify each behavior:

### Passed

Real behavior executed and independently verified.

### Failed

Real test executed and exposed a production defect.

### Blocked

Execution could not occur because of an external/environmental prerequisite.

### Unproven

Evidence is insufficient to establish the contract.

### Not implemented

The documented target behavior does not exist on the current branch.

For every failure include:

- operation;
- workspace;
- agent/session/task identity where applicable;
- item/snapshot ID where applicable;
- expected result;
- actual result;
- persisted-state observation;
- source/Git observation where relevant;
- error category;
- security impact;
- likely subsystem.

Never include secret values or complete sensitive content.

---

## 39. Completion criteria

This prompt is complete only when:

- current Context Engine architecture was inspected;
- one canonical ContextEngine owner was identified;
- item lifecycle and validation were tested;
- scope/identity isolation was proven;
- configuration behavior was tested;
- token-budget boundaries were tested;
- deterministic scoring/selection was independently verified;
- protected-item policy was tested;
- compression behavior was tested;
- offload/restore behavior was tested;
- ContextSnapshot behavior was tested separately from file-edit snapshots;
- search behavior and limits were tested;
- optimize behavior was tested;
- assemble behavior was tested where implemented;
- stale-context behavior was tested where implemented;
- source-file mutation/path-safety boundaries were tested;
- existing memory authority integration was tested;
- agent/session/task integration was tested where implemented;
- audit integration was tested without duplicating the audit suite;
- persistence across restart was proven;
- corruption/partial-publication behavior was tested;
- concurrent access was exercised;
- resource limits were tested;
- sensitive-data protection was independently verified;
- CLI/MCP/API/TUI integration was tested where implemented;
- security regressions were made permanent;
- final-target gaps were honestly classified;
- no duplicate context/memory/filesystem/auth/audit store was introduced;
- no other master test prompt was modified.

The objective is trustworthy evidence that AWH can build, optimize, persist, restore, and serve context that is **scope-correct, budget-bounded, deterministic, recoverable, secure, and consistent across interfaces**, without becoming a second memory system, source editor, authorization engine, or model runtime.

---

## 40. Scope boundary

This prompt owns **Context Engine testing only**.

Do not create full feature suites for:

- Foundation and Distribution;
- Workspace Runtime;
- Agent-Grade Filesystem Editing;
- Git/worktrees;
- Capability and Policy Engine;
- Snapshots/Provenance;
- Rollback & Recovery;
- Audit & Observability;
- Developer Memory;
- Skills;
- Agent Profiles/policy-routed MCP;
- MCP protocol/transport infrastructure;
- Sessions/Tasks;
- Terminal;
- Collaboration;
- Control API;
- TUI;
- Connectors;
- Advanced infrastructure.

Other feature families may be exercised only at the integration boundary required to prove Context Engine behavior.

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
```

Do not modify any other existing master test prompt while executing this prompt.
