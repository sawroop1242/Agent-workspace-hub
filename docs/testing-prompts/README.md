# AWH Master Test Prompts

This folder contains one independent **Master Test Prompt** for each AWH feature family.

## How to use these prompts

Execute the prompts **one by one, in numerical order**:

1. Start with the lowest-numbered prompt that has not been completed.
2. Read the prompt completely and inspect the current AWH implementation before writing tests.
3. Test the feature through real user-facing boundaries where they exist:
   - CLI
   - filesystem/persistent state
   - subprocesses and fresh-process restart
   - MCP
   - Control API
   - TUI/backend
   - external/local HTTP boundaries
   - real authorization and audit services
4. Do **not** treat `cargo test` alone as complete evidence. Unit/integration tests are only one part of the verification.
5. Use isolated disposable directories, projects, registries, ports, identities, and other environment values. Never damage real user state.
6. Use the exact environment variables and configuration supported by the current implementation. If a prompt names an environment variable, set a safe disposable value for the test environment. If the implementation has additional relevant variables, discover and test them.
7. Test both normal human workflows and failure/security boundaries.
8. Independently verify important results using persisted files, filesystem state, CLI/MCP/API output, hashes, audit records, and fresh-process reloads as appropriate.

## If a feature is missing or not working

A prompt is **not** a documentation checklist that can be marked complete merely because tests were written.

For every behavior described by the current prompt:

### 1. Test it first

Run the real behavior and determine its status:

- **Passed** — implemented and independently verified.
- **Failed** — implemented, but the observed behavior is incorrect.
- **Blocked** — the required environment/dependency is unavailable.
- **Unproven** — there is not enough evidence to claim it works.
- **Not implemented** — the required feature or behavior does not exist in the current branch.

### 2. Fix real production defects

If the feature is present but does not work correctly:

- inspect the production implementation;
- identify the root cause;
- fix the production code/configuration/docs as appropriate;
- do not weaken the test or change the expected behavior just to obtain a pass;
- add or improve a regression test for the defect;
- re-run the failed test;
- re-run the relevant integration/interface tests;
- re-run the required verification gates.

### 3. Implement genuinely missing required behavior

If the prompt describes a feature that is part of the current AWH feature contract but it is **Not implemented**:

- inspect the existing architecture and canonical authorities first;
- implement the missing production behavior using the existing AWH design;
- do not create a parallel service, registry, persistence store, policy engine, or test-only implementation;
- preserve existing behavior and security invariants;
- add tests proving the implementation;
- re-run the complete applicable test workflow after implementation.

If a behavior is explicitly optional, platform-specific, future roadmap work, or outside the current feature contract, do **not** implement it merely because the test prompt mentions it as a possible surface. Mark it **Not implemented** or **Blocked/Unproven** with evidence and continue according to the prompt's scope.

### 4. Re-test after every fix or implementation

Never move to the next Master Test Prompt with a known failed required behavior that has not been re-tested.

Use this loop:

```text
READ PROMPT
   ↓
INSPECT CURRENT IMPLEMENTATION
   ↓
TEST REAL BEHAVIOR
   ↓
PASSED? ── YES ──→ RECORD EVIDENCE
   │
   NO
   ↓
FAILED? → FIX PRODUCTION CODE → ADD/UPDATE REGRESSION TEST
   │
NOT IMPLEMENTED? → IMPLEMENT REQUIRED FEATURE
   │
BLOCKED/UNPROVEN? → RECORD EXACT LIMITATION
   ↓
RE-TEST THE FAILED/MISSING BEHAVIOR
   ↓
RUN RELATED REGRESSION + INTERFACE TESTS
   ↓
VERIFY PERSISTENCE / SECURITY / SIDE EFFECTS
   ↓
ONLY THEN COMPLETE THIS PROMPT
   ↓
MOVE TO NEXT PROMPT
```

## How to test a feature completely

For each Master Test Prompt, use this general testing sequence:

### A. Repository and architecture inspection

Inspect:

- the feature implementation;
- its canonical models/services;
- CLI commands;
- MCP tools/dispatcher;
- API handlers;
- TUI/backend integration;
- persistence/state files;
- authorization/policy boundaries;
- audit/provenance integration;
- existing tests and fixtures;
- relevant documentation.

Search for duplicate implementations before adding helpers.

### B. Environment setup

Create an isolated test environment.

Typical pattern:

```text
temporary root
├── project/workspace
├── state
├── cache/registry
├── local test source
└── logs/audit
```

Set only the required AWH environment variables to safe disposable values. Record the names and effective values used for the test, but **never print secrets**.

Examples of environment/configuration categories include:

- AWH state/storage roots;
- feature-specific AWH_* variables;
- API host/port;
- TLS certificate/key paths;
- size/time limits;
- context/memory settings;
- GitHub test configuration;
- sandbox configuration;
- feature-specific test seams such as `AWH_GLOBAL_SKILLS_ROOT`.

Always discover the exact current variable names from `docs/configuration.md` and the implementation before testing.

### C. Human workflow test

Test the feature as a user would:

```text
prepare clean environment
→ invoke supported interface
→ observe result
→ inspect real state
→ restart when state should persist
→ invoke another supported interface
→ verify the same state
```

Do not substitute a direct Rust function call when the behavior is supposed to be exposed through CLI/MCP/API/TUI.

### D. Boundary and negative testing

For each important operation test:

- valid input;
- empty input;
- minimum boundary;
- maximum boundary;
- one beyond the maximum;
- malformed input;
- missing resource;
- duplicate operation;
- stale resource;
- wrong project/workspace;
- wrong agent/session where applicable;
- unauthorized caller;
- path traversal;
- absolute path;
- symlink escape;
- corrupted persisted state;
- concurrent access;
- timeout/network failure where applicable;
- partial write/publication failure where controllable.

### E. Independent verification

Do not trust only the command response.

Cross-check with the appropriate independent oracle:

- filesystem;
- persisted JSON/database/state;
- Git state;
- independently calculated hash;
- fresh process;
- CLI;
- MCP;
- API;
- TUI/backend;
- audit record;
- external/local HTTP server.

The observed state must agree with the reported result.

### F. Security verification

For every feature, check the applicable AWH invariants:

- fail closed on invalid/unauthorized operations;
- no path traversal or symlink escape;
- no privilege escalation;
- identity remains bound to consequential actions;
- policy/capability checks cannot be bypassed;
- secrets are not logged/audited/exposed;
- denied operations do not leave unintended side effects;
- one project/workspace/agent cannot modify another's state;
- malformed or corrupted state does not silently become trusted state.

### G. Persistence and recovery

If the feature has durable state:

```text
mutate
→ verify
→ terminate process
→ start fresh process
→ reload
→ independently verify
```

Also test corruption and interrupted/partial operations where practical.

### H. Cross-interface parity

Where multiple interfaces exist:

```text
CLI mutation → MCP/API observation
MCP mutation → CLI observation
API mutation → persisted-state observation
TUI mutation → CLI/MCP observation
```

Only claim parity after actually exercising the interfaces.

## Required completion report for each prompt

Before moving to the next prompt, record:

- prompt number and feature;
- implementation status;
- tests executed;
- real interfaces exercised;
- environment variables/configuration used;
- Passed / Failed / Blocked / Unproven / Not implemented results;
- defects found;
- production fixes made;
- regression tests added;
- re-test evidence after fixes;
- persistence/restart evidence;
- security/failure-injection evidence;
- cross-platform results where applicable;
- remaining limitations;
- files changed;
- commit(s) containing the work.

### Important rule

**Do not move to the next Master Test Prompt until all required behavior in the current prompt has either:**

1. **Passed after real testing and independent verification**, or
2. **Been fixed/implemented and re-tested successfully**, or
3. **Been explicitly classified as Blocked, Unproven, or Not implemented because it is genuinely outside the current contract or cannot be verified in the available environment, with the reason and evidence recorded.**

Never hide a failure by changing the test expectation.

## Prompt isolation

Each numbered prompt owns one feature family. While executing a prompt:

- do not rewrite another Master Test Prompt;
- do not silently move its scope into another prompt;
- exercise other AWH subsystems only at the integration boundary necessary to verify the current feature;
- preserve the canonical authorities and security invariants established by earlier prompts.

The goal is not simply a green test suite. The goal is **real, reproducible evidence that each AWH feature works correctly in production boundaries, survives failure and restart where required, respects security/policy, and behaves consistently for real users.**
