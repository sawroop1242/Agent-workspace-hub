# Master Test Prompt 10 — Developer Memory

## Mission

Create and execute a complete, human-behavior-oriented verification suite for AWH's **Developer-oriented Memory** feature on the current `rust` branch.

This prompt owns durable developer workflow memory: one canonical memory authority, scoped records, CRUD/search behavior, bounded resources, persistence, legacy migration, corruption/recovery, concurrency, authorization/identity boundaries, and integration through the real CLI/MCP/Context/TUI/Control API boundaries where implemented.

**Do not treat `cargo test` alone as sufficient evidence.**

Inspect the current implementation first. Test the real canonical memory authority and existing security/identity/audit services. Do not implement missing production behavior merely to make tests pass. Classify gaps honestly.

---

## 1. Feature boundary

The documented product contract is:

```text
awh memory list
awh memory get
awh memory search
awh memory add
awh memory update
awh memory delete
```

The canonical persistence target currently described by the implementation is:

```text
<project>/.agent/memory.json
```

Legacy compatibility may involve:

```text
<project>/.agent/memory.jsonl
<project>/.agent/memory.jsonl.migrated
```

The memory domain includes:

- `MemoryEntry`;
- `MemoryScope`: Session, Project, Global;
- content;
- tags;
- creation/update timestamps;
- deterministic identity;
- bounded storage and query behavior.

AWH memory is **developer workflow state**, not an LLM reasoning system.

Out of scope:

- vector databases;
- semantic/vector search;
- LLM memory summarization;
- secret-vault functionality;
- generic model routing;
- autonomous orchestration;
- a second memory database/store.

Context consumes memory; Context Engine testing remains owned by Master Test Prompt #09. Test only the memory integration boundary needed to prove that both use the same authority.

---

## 2. Canonical architecture to test

The intended path is:

```text
CLI / MCP / TUI / Control API / Context
                ↓
identity + workspace/scope validation
                ↓
authorization / capability / policy
                ↓
       canonical MemoryStore
                ↓
validated MemoryEntry
                ↓
bounded read/search or atomic mutation
                ↓
       persistent audit where applicable
```

The current Rust implementation identifies `src/core/memory.rs` as the canonical store and `.agent/memory.json` as canonical persistence. MCP is an adapter/re-export, not an independent persistence owner.

Tests must verify:

1. exactly one authoritative memory state;
2. all applicable interfaces converge on it;
3. workspace/project identity remains authoritative;
4. authorization remains owned by existing policy/capability services;
5. audit remains owned by the canonical audit service;
6. memory identifiers are data identifiers, not arbitrary filesystem paths;
7. interface adapters do not create alternate CRUD/search semantics.

Do not create a test-only memory implementation and then use it as the oracle.

---

## 3. Required repository forensics

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
- `docs/memory/memory-implementation-prompt.md`
- testing prompts `01`–`09` sufficiently to avoid overlap.

Inspect at minimum:

```text
src/core/memory.rs
src/models/memory.rs
src/mcp/memory.rs
src/cli/memory.rs
src/context/
src/services/authorization.rs
src/services/audit.rs
src/mcp/store_lock.rs
src/tui/
src/api/                 # where present
tests/
```

Search for:

```text
MemoryStore
MemoryEntry
MemoryScope
memory.json
memory.jsonl
memory.jsonl.migrated
awh memory
memory.list
memory.get
memory.search
memory.add
memory.update
memory.delete
append_scoped
update_partial
MAX_MEMORY_ENTRIES
MAX_MEMORY_CONTENT_BYTES
MAX_MEMORY_ID_LEN
MAX_MEMORY_TAGS
MAX_MEMORY_TAG_LEN
```

Record which implementation is canonical and which adapters are compatibility layers.

Do not assume historical documentation equals current implementation.

---

## 4. Human-first test model

Tests must model real developer behavior.

### Workflow A — create and retrieve memory

```text
create isolated project/workspace
→ establish identity/scope
→ add several realistic memory notes
→ list
→ get exact entries
→ independently inspect persisted state
```

Verify IDs, scope, content, tags and timestamps.

### Workflow B — search

```text
add distinct notes/tags
→ search by content
→ search by tag
→ apply scope filter
→ apply result limit
→ verify deterministic results
```

### Workflow C — update

```text
add entry
→ update content only
→ verify scope/tags/created_at preserved
→ update scope/tags
→ verify replacement semantics
→ restart
→ verify persistence
```

### Workflow D — delete

```text
add entry
→ delete it
→ verify it is absent
→ restart
→ verify it remains absent
→ repeat delete
```

The final repeated-delete behavior must match the documented/current CLI and store contract; do not assume idempotent success if the implementation reports missing entries as errors.

### Workflow E — restart

```text
mutate memory
→ terminate process
→ start a new process
→ list/get/search
→ compare with independent persisted-state observations
```

An in-memory round trip is not persistence evidence.

---

## 5. Disposable isolated environment

Every integration test must use unique disposable state.

Use isolated:

- project/workspace roots;
- AWH state roots where configurable;
- memory files;
- repositories where Git is needed only as an integration boundary;
- agent/session identities;
- CLI/MCP/API ports where real services are started.

Never read or write a developer's real `.agent` directory.

Do not depend on test execution order.

Explicitly control environment variables used by the application and restore process environment state after tests.

If the current branch exposes additional memory-related configuration, discover and test the real names rather than inventing variables.

---

## 6. Memory record validation

Test real `MemoryEntry` validation.

Cover:

- valid IDs;
- empty IDs;
- IDs at maximum length;
- IDs over maximum length;
- duplicate IDs;
- Unicode IDs;
- path-like IDs;
- traversal-like IDs;
- empty content;
- Unicode/multilingual content;
- multiline content;
- content exactly at the configured storage limit;
- content one byte over the limit;
- zero tags;
- maximum tag count;
- one tag over the maximum;
- tag exactly at maximum length;
- overlong tag;
- duplicate tags if the contract defines behavior;
- invalid scopes;
- valid Session/Project/Global scopes.

Verify invalid input fails deterministically and cannot partially publish state.

Where limits are documented in the implementation, test the actual constants/current configuration rather than copying obsolete values.

---

## 7. Scope and workspace isolation

Create at least two isolated project/workspace roots.

Insert unique canary memory into each.

Verify:

- project A cannot accidentally read project B's memory;
- identical IDs in different projects remain isolated;
- scope is stored as a field and does not create a second persistence store;
- Session/Project/Global semantics match the current implementation;
- scope filters do not broaden access;
- a caller cannot manufacture authorization by supplying `Global`, a route name, or another scope string;
- persisted records retain their intended scope after restart;
- migration cannot move memory into an unintended scope.

If the current implementation treats Global as a record scope inside a project store, test that exact behavior rather than assuming cross-project storage.

---

## 8. CRUD semantics and invariants

Test:

### Add

- explicit content;
- stdin content where the CLI supports it;
- explicit scope;
- default scope;
- repeated tags;
- generated IDs;
- timestamp creation;
- duplicate-ID paths where lower-level APIs allow explicit IDs.

### Get

- existing ID;
- missing ID;
- malformed ID;
- cross-project ID;
- output shape;
- no unintended file/path interpretation.

### Update

- content-only update;
- scope-only update;
- tag replacement;
- multiple-field update;
- no-field update;
- missing ID;
- oversized new content;
- invalid scope/tag;
- created timestamp preservation;
- updated timestamp behavior;
- atomic failure semantics.

### Delete

- existing ID;
- missing ID;
- repeated deletion;
- deletion of one entry among many;
- persistence after restart.

For every mutation, independently verify the resulting persisted state.

---

## 9. Search correctness and determinism

Build fixtures with:

- content matches;
- tag matches;
- case differences;
- substrings;
- Unicode;
- multiline text;
- overlapping matches;
- unrelated entries;
- multiple scopes.

Verify:

- documented case sensitivity/normalization;
- content and tag matching semantics;
- scope filtering;
- missing/empty queries according to the current contract;
- result ordering is deterministic;
- repeated identical searches return identical results;
- limits are enforced;
- search cannot expose entries outside the allowed scope;
- searching does not mutate memory.

Test query sizes at and above documented bounds.

Do not label a search implementation correct merely because it returns one expected fixture.

---

## 10. CLI black-box verification

Use the real compiled `awh` binary where the commands exist.

Exercise:

```text
awh memory list
awh memory get --id <id>
awh memory search --query <query>
awh memory add
awh memory update --id <id>
awh memory delete --id <id>
```

Use the exact current syntax discovered from `awh --help` and source.

Verify:

- exit codes;
- stdout;
- stderr;
- missing/invalid arguments;
- bounded list/search output;
- stdin content;
- Unicode;
- persistence;
- errors for missing entries;
- no secret leakage;
- no accidental path interpretation;
- output remains usable for a human.

Do not call Rust functions and claim CLI coverage.

Where the CLI currently clamps output limits, test both ordinary and pathological limits.

---

## 11. MCP memory boundary

Where memory MCP tools are implemented, use the real MCP dispatcher/session boundary.

Exercise:

- list/get/search/add/update/delete equivalents that actually exist;
- valid arguments;
- malformed arguments;
- missing required fields;
- invalid scopes;
- oversized content;
- oversized queries;
- missing IDs;
- cross-scope/cross-workspace attempts;
- identity mismatch;
- repeated operations;
- bounded responses.

Verify MCP memory calls and direct canonical-store operations produce equivalent domain semantics.

Generic MCP JSON-RPC/transport correctness remains owned by the existing MCP suites; this prompt verifies memory-specific behavior through MCP.

---

## 12. Context integration

Verify that ContextEngine consumes the canonical memory authority.

Test:

```text
add memory through canonical/CLI path
→ request context memory consumption
→ verify the expected memory is visible

mutate memory
→ request context again
→ verify current canonical state is reflected

restart
→ request context
→ verify durable memory remains available
```

The context test must prove shared authority, not re-test Context Engine optimization, token selection, compression or offload behavior.

A second memory store under `src/context` is a failure.

---

## 13. Control API and TUI integration

Where implemented:

- invoke memory through the real Control API;
- inspect/list memory through the real TUI;
- perform applicable mutations;
- verify persistence using the canonical store;
- verify API/TUI changes are visible through CLI/MCP and vice versa;
- verify identical scope and validation semantics;
- verify no interface-local JSONL/JSON writer exists.

If an interface is not implemented, classify it as **Not implemented**, not Passed.

Do not duplicate broad Control API or TUI suites; test only memory-specific delegation.

---

## 14. Legacy JSONL migration

This is a critical compatibility boundary.

Use disposable fixtures containing:

- valid legacy records;
- multiple records;
- blank lines;
- existing canonical records;
- duplicate migrated IDs;
- malformed JSON;
- truncated lines;
- unreadable legacy file where permissions permit testing;
- migration interruption/crash window where a real controllable boundary exists.

Verify:

- migration is deterministic;
- legacy IDs are stable;
- blank-line handling matches the current contract;
- migration is idempotent;
- canonical records are preserved;
- legacy data is not silently discarded;
- archive/rename behavior matches the implementation;
- malformed legacy input fails closed;
- a failed migration does not fabricate an empty store;
- rerunning after an interrupted publication does not duplicate records;
- scope assignment for legacy records is explicit and stable;
- migrated data survives restart.

Independently inspect raw files before and after migration.

Do not invent migration behavior if current source has changed; test the current contract.

---

## 15. Canonical persistence

Verify the canonical memory file is the sole authoritative state.

Test:

- empty store;
- one entry;
- many entries;
- update;
- delete;
- restart;
- concurrent access;
- relocation of project root;
- missing `.agent` directory;
- invalid parent path;
- unreadable store;
- malformed JSON;
- duplicate IDs;
- unsupported/unknown serialized fields according to serde behavior;
- serialization round trips.

Verify:

```text
successful mutation
→ atomic publication
→ process termination
→ fresh process
→ reload
→ independent comparison
```

A successful API response before durable publication is a failure.

---

## 16. Atomicity, locking, and crash recovery

Exercise concurrent readers/writers and real persistence boundaries.

Cover:

- concurrent add;
- concurrent updates to different IDs;
- concurrent updates to the same ID;
- update vs delete race;
- list/search while mutation occurs;
- two processes opening the same store;
- restart during publication where injectable;
- lock contention;
- failed rename/write/fsync boundary where controllable.

Verify:

- no malformed canonical file;
- no impossible duplicate identity;
- no lost valid state beyond documented semantics;
- no cross-project corruption;
- no partial successful mutation;
- lock behavior is deterministic;
- failed operations do not claim success;
- recovery leaves a valid authoritative store.

Use independent filesystem inspection as an oracle.

---

## 17. Corruption and fail-closed behavior

Inject corruption into disposable memory state.

Cover:

- malformed JSON;
- truncated JSON;
- malformed legacy JSONL;
- duplicate entry IDs;
- missing required fields;
- invalid enum/scope values;
- oversized serialized content;
- impossible metadata;
- partially published temporary files where relevant.

Verify:

- corruption is detected;
- the store does not silently become empty;
- valid unrelated project state remains safe;
- no fabricated entries are returned;
- errors are deterministic enough for diagnosis;
- recovery never modifies another project's memory;
- raw sensitive content is not emitted in errors/logs.

If automatic recovery exists, verify it against an independent pre-corruption fixture.

---

## 18. Authorization and identity boundaries

Memory is not a bypass around AWH authorization.

Where authorization is implemented, test:

- allowed read;
- allowed mutation;
- denied read;
- denied add/update/delete;
- forged agent identity;
- forged session identity;
- wrong workspace;
- wrong project;
- scope mismatch;
- disabled/inactive agent;
- route-name-as-authorization attempts;
- direct/internal service invocation where an authorization boundary is required.

For every denied operation verify:

1. correct error;
2. no persisted mutation;
3. no source-file mutation;
4. no audit false-positive success;
5. no sensitive-data disclosure.

Do not invent a new memory-specific authorization system.

---

## 19. Audit and observability

Use the canonical audit service where memory mutations/accesses are required to be audited.

Verify:

- add/update/delete events as documented;
- actor/agent/session/workspace correlation where available;
- action and outcome;
- event uniqueness/order according to the audit contract;
- failed authorization is represented according to audit policy;
- secrets and raw sensitive memory content are not unnecessarily persisted;
- audit records survive restart where durable audit is required.

Do not duplicate Master Test Prompt #08's general audit/observability suite.

This prompt proves only memory-specific event integration.

---

## 20. Sensitive-data protection

Create unique canaries representing:

- API-key-like strings;
- bearer-token-like strings;
- passwords;
- private-key-like content;
- environment secrets;
- sensitive source snippets.

Store them only in disposable memory fixtures.

Inspect:

- canonical memory file;
- legacy migration artifacts;
- CLI output;
- MCP responses;
- API responses;
- TUI views;
- audit records;
- diagnostic logs.

Verify the documented sensitive-data policy.

Do not assume all legitimate memory content must be redacted from the canonical memory store; distinguish storage from logs/audit/output policy.

---

## 21. Resource-exhaustion tests

Exercise:

- content at maximum size;
- content above maximum;
- maximum number of entries;
- one entry above maximum count;
- maximum tag count;
- overlong tags;
- huge IDs;
- huge search queries;
- huge result sets;
- maximum CLI output;
- concurrent request floods within a bounded test.

Verify:

- limits are enforced;
- updates to existing entries remain possible at the entry-count boundary where documented;
- oversized input fails before unsafe publication;
- search/output is bounded;
- one bad record cannot cause unbounded allocation;
- repeated operations cannot bypass limits;
- resource pressure does not weaken authorization or isolation.

Use the actual current constants/configuration.

---

## 22. Replay, idempotency, and duplicate handling

Replay realistic operations:

- add same logical operation twice;
- update same entry repeatedly;
- delete same entry repeatedly;
- retry after a failed write;
- reopen after migration;
- replay migration after restoring the legacy file;
- repeat search.

Verify behavior matches the current contract.

Especially verify that migration and generated IDs cannot create duplicate durable entries after interruption/retry.

Do not impose idempotency where the public contract deliberately reports an error.

---

## 23. Cross-platform behavior

Run applicable memory tests on supported environments:

- Linux x86_64;
- Linux ARM64;
- macOS x86_64/ARM64;
- Windows x86_64;
- Android/Termux ARM64 where practical.

Focus on:

- path handling;
- rename/atomic publication;
- file locking;
- permissions;
- Unicode;
- process restart;
- concurrent access.

Do not claim a platform was verified unless the relevant tests actually ran.

---

## 24. Property and boundary tests

Add meaningful property tests where useful.

Important properties:

```text
store(project-A) is independent of store(project-B)

persist(entry) → reload(entry) preserves documented fields

update(content-only) preserves scope/tags/created_at

delete(id) removes exactly that record

search(scope=A) never returns a record outside scope A

invalid input never publishes a partially valid mutation

legacy migration is deterministic and idempotent

successful restart reload == independently observed persisted state

bounded input cannot create an unbounded result

memory IDs never become filesystem traversal primitives
```

Include empty, minimum, maximum, one-over-limit, Unicode and malformed values.

---

## 25. Independent oracles

Never prove correctness solely from MemoryStore return values.

Cross-check with:

- raw `.agent/memory.json`;
- raw legacy/migration files;
- independent JSON parsing;
- filesystem existence/content;
- independent ID sets;
- process restart;
- CLI output;
- MCP output;
- API/TUI output where available;
- audit records;
- workspace/project identity.

Example:

```text
CLI says memory was added
+
canonical file contains expected record
+
fresh process can retrieve it
+
independent JSON parse agrees
```

All relevant observations must agree.

---

## 26. Failure injection

Where practical, inject failures at real memory boundaries:

- invalid input;
- invalid scope;
- duplicate ID;
- unwritable directory;
- malformed canonical store;
- malformed legacy record;
- lock contention;
- failed temporary-file publication;
- rename failure;
- interrupted migration;
- concurrent update;
- concurrent delete;
- authorization denial;
- oversized content/query;
- restart during mutation.

For every injected failure inspect:

1. returned error;
2. exit status;
3. persisted memory state;
4. filesystem state;
5. audit state where applicable;
6. retry behavior;
7. secret leakage.

Never convert “did not panic” into a passing result.

---

## 27. Test implementation requirements

Prefer:

- unit tests for validation, scope matching, search, serialization and migration primitives;
- integration tests through the canonical MemoryStore/service;
- disposable real project roots;
- real JSON persistence;
- real subprocess restart;
- real CLI boundary;
- real MCP boundary where implemented;
- real API/TUI boundary where implemented;
- real authorization/audit authorities;
- failure injection at persistence boundaries.

Do not create:

- a second MemoryStore;
- a test-only persistence format;
- a test-only authorization service;
- a fake audit authority;
- a fake ContextEngine memory store;
- an interface-local memory implementation.

Reuse existing helpers only when they exercise the real production path.

---

## 28. Verification gates

Run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Then run focused memory tests covering:

- record validation;
- limits;
- scope;
- CRUD;
- search;
- persistence/restart;
- locking/concurrency;
- corruption;
- legacy migration;
- CLI;
- MCP;
- Context integration;
- API/TUI where implemented;
- authorization;
- audit;
- sensitive-data handling;
- security regressions.

Run external interoperability checks only where applicable.

Report a command as passed only if it actually completed successfully.

---

## 29. Evidence classification

Classify every behavior:

### Passed
Real behavior executed and independently verified.

### Failed
Real test executed and exposed a production defect.

### Blocked
Execution could not occur because an external/environmental prerequisite was unavailable.

### Unproven
Available evidence is insufficient.

### Not implemented
The documented target behavior is absent on the current branch.

For failures include:

- operation;
- project/workspace;
- agent/session identity where relevant;
- memory ID where relevant;
- expected result;
- actual result;
- persisted-state observation;
- interface used;
- error category;
- security impact;
- likely subsystem.

Never include actual secret values.

---

## 30. Completion criteria

This prompt is complete only when:

- the current canonical memory implementation was inspected;
- one authoritative memory store was identified;
- all existing memory adapters were mapped;
- real CRUD behavior was tested;
- search semantics and limits were tested;
- Session/Project/Global scope behavior was tested;
- cross-project isolation was proven;
- validation and size/count limits were tested;
- persistence across restart was proven;
- atomic publication/locking behavior was exercised;
- corruption fails closed;
- legacy JSONL migration was independently verified;
- migration is deterministic/idempotent;
- CLI memory behavior was tested where implemented;
- MCP memory behavior was tested where implemented;
- ContextEngine shared-authority integration was tested;
- Control API/TUI delegation was tested where implemented;
- authorization/identity boundaries were tested;
- audit integration was tested without duplicating #08;
- sensitive-data handling was independently inspected;
- concurrency/replay/failure behavior was tested;
- security regressions were made permanent;
- final-target gaps were honestly classified;
- no duplicate memory store or authorization/audit implementation was introduced;
- no other master test prompt was modified.

The objective is trustworthy evidence that AWH memory is **durable, scoped, deterministic, bounded, recoverable, secure, and consistent across interfaces**, while remaining a developer-workflow memory subsystem rather than an autonomous reasoning or model system.

---

## 31. Scope boundary

This prompt owns **Developer Memory testing only**.

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
- Skills;
- Agent Profiles/policy-routed MCP;
- generic MCP protocol/transport;
- Sessions/Tasks;
- Terminal;
- Collaboration;
- Control API;
- TUI;
- Connectors;
- Advanced infrastructure.

Other feature families may be exercised only at the integration boundary required to prove Memory behavior.

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
```

Do not modify any other existing master test prompt while executing this prompt.
