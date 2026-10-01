# Master Test Prompt 01 — Foundation & Distribution

## Mission

Create and execute a complete, human-behavior-oriented verification suite for AWH Foundation and Distribution features on the current `rust` branch.

This prompt owns testing only. Do not implement missing production features merely to make tests pass. The goal is to prove what a real user, developer, CI runner, installer, or release consumer can actually do with AWH.

The test must exercise real AWH boundaries wherever practical:

```text
human action
→ real CLI/binary
→ real environment/configuration
→ real filesystem/process boundary
→ observable result
```

Do not treat `cargo test` alone as sufficient evidence.

---

## 1. Product scope

Test only the Foundation and Distribution feature family:

- deterministic configuration precedence;
- persistent state/storage behavior relevant to foundation;
- cross-platform path handling;
- structured logging and error behavior;
- version/build information;
- `awh init`;
- `awh status`;
- `awh doctor`;
- `awh config`;
- install behavior;
- upgrade behavior;
- uninstall behavior, where supported by the current implementation;
- release artifacts and checksums;
- supported binary/platform detection;
- shell completion, where implemented.

The feature contract is defined by the current repository documentation and source. Do not infer that a planned feature is implemented.

---

## 2. Required repository forensics

Before writing or changing tests, inspect the current `rust` branch completely enough to establish the real contract.

At minimum read:

- `README.md`
- `Cargo.toml`
- `docs/FEATURES.md`
- `docs/testing.md`
- `docs/configuration.md`
- `docs/development.md`
- `docs/release.md`
- `docs/INSTALL.md`
- `docs/architecture.md`
- `docs/security.md`
- `docs/roadmap/PROJECT_ROADMAP.md`
- `docs/roadmap/GROWTH_STRATEGY.md`
- `docs/implementation-prompts/README.md`
- relevant current implementation prompts, especially prompts covering configuration, runtime identity, filesystem safety, and testing.

Inspect:

- `src/main.rs`;
- configuration modules;
- CLI command definitions and handlers;
- filesystem/path helpers;
- logging/error modules;
- install/release scripts;
- release workflows;
- existing unit, integration, executable, and security tests;
- current test fixtures/helpers.

Search the whole repository for every command and environment variable involved in this feature family before deciding what to test.

Historical documentation is evidence only. Current source, current tests, and current contracts win when they conflict.

---

## 3. Test philosophy — simulate a real human

Build tests around realistic user journeys, not only internal function calls.

A strong test should answer questions such as:

1. Can a new user create an empty project directory?
2. Can they run `awh init` from it?
3. Does the resulting workspace contain the expected state?
4. Can they immediately run `awh status`?
5. Does `awh doctor` report useful information?
6. Does `awh config` expose safe configuration without leaking secrets?
7. Can a user set configuration through environment variables?
8. Do CLI flags override environment variables when the contract says they should?
9. Does invalid configuration fail clearly instead of silently falling back?
10. Can the actual released binary be installed and executed?
11. Does an upgrade preserve required user state?
12. Does an uninstall remove only what it is supposed to remove?
13. Do errors remain deterministic and machine-readable where the contract requires it?
14. Does behavior remain correct when paths contain spaces, Unicode, nested directories, or unusual but valid names?

Prefer subprocess tests that invoke the actual `awh` executable for user-facing behavior.

Internal unit tests are still required for pure parsing and deterministic logic, but they are supporting evidence rather than the complete acceptance proof.

---

## 4. Environment isolation

Every test must use an isolated temporary environment.

Never modify the developer's real:

- `HOME`;
- current project;
- `PATH`;
- persistent AWH directory;
- Git configuration;
- shell configuration;
- credential store;
- global environment.

Use temporary directories and explicit environment construction.

Tests must be deterministic and safe to run:

- locally;
- in GitHub Actions;
- repeatedly;
- in parallel where the suite permits it.

Do not depend on the developer's installed `awh`, personal files, shell aliases, credentials, or machine-specific configuration.

When testing environment-variable behavior, pass variables directly to the child process or use an injectable configuration source. Do not globally mutate process environment from parallel tests unless the implementation explicitly requires it and the test is serialized.

---

## 5. Environment-variable contract

Test every Foundation/Distribution environment variable actually documented by the current branch.

At minimum, if present in the current contract, cover:

- `AWH_*` configuration variables;
- `AWH_HOST`;
- `AWH_PORT`;
- `AWH_API_KEY` only for safe presence/absence behavior relevant to foundation/configuration;
- `AWH_TLS_CERT`;
- `AWH_TLS_KEY`;
- resource-limit variables;
- context variables;
- `AWH_NGROK_AUTHTOKEN`;
- `AWH_BWRAP`.

Do not put real secrets in source code, test output, snapshots, fixtures, or assertions.

Use clearly synthetic test values.

For every supported variable test:

- default behavior;
- valid override;
- invalid value;
- empty value where meaningful;
- precedence against CLI flags where applicable;
- deterministic error behavior;
- absence of secret leakage.

If a variable belongs to another feature family, do not duplicate its full feature suite here; verify only the foundation/configuration contract necessary to prove correct parsing and precedence.

---

## 6. Configuration precedence tests

Prove the documented precedence:

```text
built-in defaults
    < AWH_* environment
    < CLI flags
```

For representative configuration values, test all three layers.

Example methodology:

1. run with no configuration;
2. run with an environment override;
3. run with both environment override and CLI override;
4. assert the observable result matches the documented precedence.

Also test invalid combinations.

A half-configured pair must fail closed where required. For example, if the current contract requires both TLS certificate and key, test:

- neither;
- certificate only;
- key only;
- both valid;
- both but invalid paths/material.

Do not assert implementation details that are not part of the public contract.

---

## 7. CLI black-box tests

For every Foundation command implemented on the current branch, test the actual binary.

### `awh version`

Verify:

- exits successfully;
- emits version/build information required by the contract;
- output is stable enough for automation;
- does not require an initialized workspace unless documented;
- does not leak environment variables or secrets.

### `awh init`

Test:

- fresh directory;
- nested directory;
- existing unrelated files;
- repeated invocation;
- invalid/non-writable location when reproducible;
- paths containing spaces;
- Unicode paths;
- expected persistent state;
- correct exit status;
- useful error messages.

Repeated initialization must not silently corrupt existing state.

### `awh status`

Test:

- initialized project;
- uninitialized project;
- valid isolated environment;
- missing/corrupted expected state where applicable;
- machine-readable behavior if the contract provides it.

### `awh doctor`

Test:

- healthy environment;
- missing optional dependency;
- invalid configuration;
- missing required runtime dependency;
- useful diagnostic output;
- non-zero exit status when the contract defines a failing diagnosis.

### `awh config`

Test:

- defaults;
- environment overrides;
- CLI overrides;
- safe display;
- invalid configuration;
- secret redaction.

If `config` has subcommands or output formats, test every documented public mode.

---

## 8. Filesystem/path behavior

Use real temporary directories.

Test:

- absolute paths;
- nested paths;
- spaces;
- Unicode;
- long-but-valid names within platform limits;
- existing files;
- existing directories;
- missing paths;
- relative paths where accepted;
- relative paths where rejected;
- path normalization;
- platform-specific separators where relevant.

The test must verify that AWH does not accidentally resolve a user-selected path against the test runner's real project.

Do not create tests that access real system files such as `/etc/passwd`.

---

## 9. Logging and error behavior

Test user-visible failures, not only successful paths.

For representative failures verify:

- non-zero exit status;
- deterministic error category/message where contractually required;
- no panic;
- no Rust backtrace unless explicitly requested;
- no secret values in stdout;
- no secret values in stderr;
- no API keys/tokens in logs;
- useful context for humans.

If structured logging is part of the implementation, parse and validate the structured record rather than matching fragile formatting.

Use synthetic token-shaped secrets to catch accidental leakage.

---

## 10. Persistence tests

Where Foundation state is persisted, prove persistence across process boundaries.

Pattern:

1. create isolated temporary HOME/project;
2. run the real binary;
3. write or initialize state;
4. terminate the process;
5. start a fresh process;
6. inspect the state;
7. assert the expected state survives.

Do not use in-memory mocks as proof of persistence.

Test:

- fresh state;
- restart;
- repeated invocation;
- empty state;
- malformed state;
- partial/corrupt state where the implementation has defined behavior;
- permission failures where reproducible.

---

## 11. Install / upgrade / uninstall tests

If the current branch contains an installer or lifecycle scripts, test them as a user would.

### Install

Use an isolated prefix/home.

Verify:

- supported platform detection;
- correct artifact selection;
- executable installation;
- executable permission where relevant;
- successful `awh version`;
- no writes outside the intended prefix/home;
- clear failure when required tools are absent.

### Upgrade

Install version A into an isolated location, create representative user state, then upgrade to version B.

Verify:

- the new binary is selected;
- required user state is preserved;
- unrelated files are preserved;
- obsolete files are handled according to the documented contract;
- failure does not leave a silently unusable installation.

### Uninstall

If supported, verify:

- only intended installation files are removed;
- user/project data is preserved or removed exactly as documented;
- repeated uninstall behaves deterministically;
- unrelated files are untouched.

Never run destructive installer tests against the developer's real home directory.

---

## 12. Release-artifact tests

When release artifacts are available in CI or can be built locally, validate:

- expected artifact names;
- supported target/architecture;
- executable starts;
- `awh version` matches the release;
- checksums exist;
- checksum verification succeeds;
- corrupted artifact/checksum is rejected;
- archive extraction does not escape its destination;
- artifact contains only expected release material.

Do not claim release validation merely because the Rust source compiles.

Where a target cannot be executed on the current host, distinguish:

- build verified;
- artifact structurally verified;
- runtime behavior verified;
- not runtime verified.

---

## 13. Cross-platform matrix

Run the applicable tests on the platforms supported by the current branch.

At minimum, account for:

- Linux x86_64;
- Linux ARM64 where available;
- macOS x86_64;
- macOS ARM64;
- Windows x86_64;
- Android/Termux ARM64 where the current release contract supports it.

Do not fake another platform by changing `cfg` or environment variables.

If a platform cannot be executed in the current environment, record it explicitly as unverified.

Platform-specific assertions must be conditional and must not weaken the common contract.

---

## 14. Failure-injection tests

Deliberately create realistic failures:

- missing directory;
- missing file;
- malformed configuration;
- invalid numeric configuration;
- unwritable destination;
- unavailable dependency;
- corrupted persisted state;
- invalid release artifact;
- checksum mismatch;
- interrupted or incomplete installation where safely reproducible.

Verify that AWH fails safely, reports the failure, and does not silently claim success.

A failed operation must not leave a misleading success state.

---

## 15. Concurrency and repeatability

Where Foundation state can be touched concurrently, test:

- two independent projects concurrently;
- repeated `status`;
- repeated `init`;
- independent temporary HOME directories;
- concurrent reads of shared read-only metadata.

Do not invent a concurrency guarantee that the current product contract does not promise.

The goal is to detect cross-test contamination, races, global-state leakage, and accidental dependence on the developer environment.

---

## 16. Property and boundary testing

For pure parsers and validators, use existing `proptest` infrastructure where useful.

Cover boundaries such as:

- zero;
- minimum;
- maximum;
- maximum + 1;
- negative values where parsing is unsigned;
- empty strings;
- whitespace;
- very long strings;
- malformed Unicode where the Rust boundary permits it.

Property tests must assert meaningful invariants, not merely that a function does not panic.

---

## 17. Security regression requirements

Foundation tests must preserve the project's security invariants.

At minimum prove:

- secrets are not printed;
- invalid configuration does not silently become a permissive default;
- paths remain isolated to the intended test environment;
- installation does not execute arbitrary test-fixture content;
- checksum mismatch is not accepted;
- malformed input does not panic;
- test helpers do not disable authorization/security gates.

Never introduce a `cfg(test)` bypass that changes production security semantics.

---

## 18. Test implementation requirements

Use the repository's existing test architecture and conventions.

Prefer:

- Rust unit tests for pure deterministic functions;
- Rust integration tests for application boundaries;
- subprocess tests for actual CLI behavior;
- temporary directories for filesystem isolation;
- executable tests for the compiled `awh` binary;
- CI matrix tests for platform-specific behavior;
- shell-level tests for installer scripts when appropriate.

Create reusable helpers only when they reduce duplication without hiding important assertions.

Every important test should assert:

- command/function actually ran;
- exit/result status;
- expected observable output/state;
- absence of forbidden side effects.

Avoid tests that pass merely because a mock returned the expected value.

---

## 19. No test theater

The following are explicitly prohibited:

- testing only mocked services when a real boundary is available;
- asserting that a function was called instead of verifying the resulting behavior;
- using hard-coded paths from the developer machine;
- depending on pre-existing global state;
- swallowing command failures;
- converting failures into warnings just to make CI green;
- skipping tests without documenting why;
- adding production code solely to satisfy a test;
- changing security behavior to simplify tests;
- changing CI thresholds to hide failures;
- treating compilation as feature completion;
- treating a unit test as proof of end-to-end behavior;
- claiming a platform was tested when it was only cross-compiled.

---

## 20. Test execution gates

Run, as applicable:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
```

Also run the feature-specific integration/executable tests and installer/release tests.

If the repository has clippy/audit or other required CI gates, run the current documented gates rather than inventing replacements.

For shell scripts, run shell syntax/static checks available in the repository and execute them in isolated temporary directories.

---

## 21. Environment-variable evidence

The test report must include a safe table like:

| Variable/category | Default tested | Override tested | Invalid tested | Secret-safe |
|---|---:|---:|---:|---:|
| AWH_* relevant to foundation | yes/no | yes/no | yes/no | yes/no |

Never print actual secret values.

When a value comes from the user's GitHub Actions environment, consume it through the normal environment interface. Do not copy it into source, fixtures, documentation, logs, test names, snapshots, or command-line arguments.

If a required secret/environment value is unavailable, report the exact test that is blocked and continue all non-secret tests.

---

## 22. Human acceptance scenarios

In addition to automated tests, create executable scenarios corresponding to real usage:

### Scenario A — New project

```text
mkdir project
cd project
awh init
awh status
awh doctor
awh version
```

Assert the complete resulting state.

### Scenario B — Configuration override

```text
set AWH_* test value
run awh command
run same command with CLI override
```

Assert precedence.

### Scenario C — Restart

```text
initialize
exit process
start fresh process
inspect state
```

Assert persistence.

### Scenario D — Bad configuration

```text
set invalid AWH_* value
run awh
```

Assert deterministic failure and no unsafe fallback.

### Scenario E — Installation

```text
install into isolated prefix
run installed awh
verify version
```

Assert the installed artifact works independently of the source checkout.

These scenarios must be automated where practical.

---

## 23. Evidence and reporting

At completion, report separately:

### Passed

Tests with direct evidence.

### Failed

Tests that executed and exposed a defect.

### Blocked

Tests that could not execute because of missing environment/platform/dependency prerequisites.

### Unproven

Features for which the current implementation or environment does not provide sufficient evidence.

Do not convert blocked or unproven behavior into passed behavior.

For every failure include:

- test name;
- reproduction command;
- expected behavior;
- actual behavior;
- relevant error;
- likely subsystem;
- whether it is a production defect or test-environment issue.

Do not expose secrets in the report.

---

## 24. Completion criteria

This prompt is complete only when:

- current source and contracts were inspected;
- Foundation/Distribution behavior has unit, integration, and real-user coverage appropriate to the boundary;
- actual `awh` subprocess behavior is tested;
- relevant environment variables are tested safely;
- configuration precedence is proven;
- persistence across processes is proven where applicable;
- failure paths are tested;
- logs/errors are checked for secret leakage;
- installer/release behavior is tested where implemented;
- cross-platform evidence is honestly classified;
- existing regression tests remain green or failures are documented;
- no production security bypass was introduced;
- no unrelated feature implementation was added;
- the final report distinguishes passed, failed, blocked, and unproven evidence.

The objective is not the largest number of tests. The objective is trustworthy evidence that a human can actually use AWH's Foundation and Distribution features safely.

---

## 25. Scope boundary

This prompt owns only Foundation and Distribution testing.

Do not create full feature suites for:

- workspace lifecycle;
- editing;
- Git/worktrees;
- capability/policy;
- snapshots/rollback;
- context;
- memory;
- skills;
- agent profiles;
- sessions/tasks;
- MCP;
- audit;
- terminal;
- collaboration;
- Control API;
- TUI;
- connectors;
- advanced infrastructure.

Those feature families must receive their own independent master test prompts.

Do not modify other test master prompts while executing this prompt.

