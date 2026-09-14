# SEC-001 / #44 — High-Risk Built-in MCP Tools Default Deny — Issue-Resolving Master Prompt

## Mission

Implement and verify SEC-001 / GitHub issue #44: **High-risk built-in MCP tools must be denied by default and require explicit authorization**.

The repository's current sequential master prompt makes SEC-001 the next unresolved security gate. Do not begin AGENT-001 or any later architecture issue in this run. The issue is not considered resolved merely because the authorization code or tests are changed; resolution requires implementation evidence, the complete verification gate, CI evidence, and acceptance evidence.

GitHub issue #44 states that the existing built-in authorization gate is opt-in: without an `awh.builtin` trust record, Medium/High tools can proceed. The forensic finding classifies this as P0 because `terminal.run` can execute arbitrary host commands and other High-risk mutations may also be reachable. The issue explicitly requires default denial, explicit authorization, least privilege, unchanged external-MCP trust semantics, regression tests, and migration/onboarding documentation. fileciteturn250file0

The existing short prompt confirms the intended rule: reuse the existing authorization machinery, deny High-risk `terminal.run`, `connector.invoke`, and High-risk provider mutations unless explicitly authorized, keep unknown tools denied, produce actionable errors, and do not create a second policy engine. fileciteturn248file0

---

# 1. Hard sequencing rules

1. Work only on SEC-001 / #44.
2. Do not implement AGENT-001 / #46, ARCH-001 / #47, FS-001 / #48, GIT-001 / #49, or any other future issue in this run.
3. Do not turn SEC-001 into a general authorization rewrite.
4. Reuse the existing authorization/trust/capability/risk metadata machinery.
5. Do not create a second policy engine, parallel authorization path, or tool-specific bypass table.
6. Preserve the already-reconciled SEC-002 behavior; do not modify SEC-002 merely to make SEC-001 easier.
7. Do not weaken existing external-MCP trust semantics unless current source proves they violate an explicit SEC-001 acceptance criterion.
8. Make the smallest coherent implementation patch.
9. Modify only files necessary for SEC-001 implementation, tests, and required migration/onboarding documentation. Do not perform unrelated cleanup.
10. HARD STOP after SEC-001. Do not continue automatically.

---

# 2. Repository and evidence preflight

Before editing anything, inspect the current `rust` branch and establish the real baseline.

Inspect, where present:

```text
AGENTS.md
Cargo.toml
README.md
docs/PROJECT_ROADMAP.md
docs/PROJECT_STATUS.md
docs/CLI.md
docs/FEATURES.md
docs/RECONCILED_ROADMAP_V2.md
docs/issue-resolving-prompts/AWE-SEQUENTIAL-IMPLEMENTATION-MASTER-PROMPT.md
docs/issue-resolving-prompts/SEC-001-high-risk-default-deny.md
```

Then locate the actual implementation of:

```text
authorize_builtin_tool
builtin authorization
awh.builtin
Tool Registry / risk metadata
Trust / authorization machinery
terminal.run
connector.invoke
High-risk GitHub/provider mutations
external MCP authorization
authorization errors
existing authorization tests
```

Do not trust PR #43, issue text, or this prompt as proof of current implementation. Use them as forensic context only. Current source, tests, CI, and GitHub issue acceptance are authoritative.

Record whether the baseline is:

- still default-allow;
- partially migrated to default-deny;
- already default-deny but missing coverage;
- or otherwise different from the original forensic finding.

If SEC-001 is already implemented, verify every acceptance criterion instead of duplicating the implementation.

---

# 3. Security objective

The authoritative security invariant is:

```text
High-risk built-in tool
    + no explicit authorization
    → DENY

High-risk built-in tool
    + explicit matching authorization
    + valid trust/capability state
    + required scope
    + not expired/revoked
    → ALLOW

High-risk built-in tool
    + authorization for a different capability
    → DENY

unknown built-in tool
    → DENY
```

The default deployment must fail closed.

There must be no path where absence of an `awh.builtin` authorization/trust record implicitly grants a High-risk built-in capability.

---

# 4. Canonical authorization boundary

Identify the existing authoritative authorization function and make the default-deny change there or at the narrowest shared boundary that governs all relevant built-in invocations.

The implementation must not introduce:

- a second `PolicyEngine`;
- a separate High-risk checker outside the registry/risk model;
- special-case string matching scattered across tools;
- MCP-only authorization that can be bypassed by CLI/internal service calls;
- tool-local authorization that disagrees with the central authorization machinery.

Risk classification must remain data/registry driven wherever the existing architecture already supports it.

The intended decision flow is conceptually:

```text
caller/tool request
→ resolve canonical built-in tool identity
→ obtain canonical risk metadata
→ evaluate existing authorization/trust machinery
→ if High-risk, require explicit authorization
→ verify capability/permission/scope as applicable
→ allow or return structured denial
→ only then execute the tool
```

Do not authorize based solely on a tool name supplied by an untrusted caller.

---

# 5. High-risk coverage

At minimum verify the issue's explicitly named classes:

### `terminal.run`

Must be denied by default because it can execute arbitrary host commands.

Test both authorization decision and the actual invocation boundary where practical. A test that only calls a helper is insufficient if the production execution path can bypass that helper.

### `connector.invoke`

Must require explicit authorization when classified High-risk by the existing risk metadata.

Do not create a connector-specific policy engine.

### High-risk GitHub/provider mutations

Enumerate the current registry/risk metadata and identify all High-risk provider mutation tools. The implementation must apply the same default-deny rule to the entire High-risk class, not only to two named tools.

### Future High-risk tools

The rule must derive from the existing risk classification/registry mechanism so a newly registered High-risk built-in does not accidentally inherit default-allow behavior.

Do not hard-code today's list as the security mechanism.

---

# 6. Explicit authorization semantics

Define and test exactly what constitutes explicit authorization using the existing machinery.

At minimum establish:

- no authorization record → deny;
- matching authorization → allow;
- wrong capability → deny;
- insufficient scope → deny;
- expired authorization → deny if expiry exists in the current model;
- revoked/disabled authorization → deny where supported;
- malformed/corrupt authorization state → fail closed;
- unknown tool → deny;
- unrelated authorization must not grant the requested capability.

Do not silently broaden authorization because a caller possesses some other capability.

If the current repository has a different but canonical capability representation, adapt the tests to that representation rather than inventing a new schema.

---

# 7. Least privilege

Prove that explicit authorization grants only what it actually grants.

Examples of required behavior:

```text
allow terminal.run
→ does not implicitly allow connector.invoke

allow connector.invoke
→ does not implicitly allow terminal.run

allow one provider mutation
→ does not implicitly allow unrelated High-risk mutations

no authorization
→ no High-risk built-in execution
```

If capabilities are resource-scoped, verify resource boundaries as well.

Do not replace least privilege with a global `allow_high_risk=true` escape hatch.

---

# 8. Lower-risk compatibility

SEC-001 targets High-risk built-ins. Preserve deliberate compatibility for lower-risk tools where the existing design intentionally allows them.

Determine the current risk categories from the source rather than assuming a fixed list.

Verify at least:

- an intentionally permitted lower-risk built-in continues to work;
- changing the default for High-risk tools does not accidentally deny unrelated low-risk functionality;
- Medium-risk behavior is preserved unless the existing policy explicitly requires a change;
- unknown/unclassified tools remain fail-closed rather than being treated as safe by accident.

If the existing security model intentionally treats Medium-risk tools as requiring explicit authorization too, preserve that established behavior rather than weakening it in the name of SEC-001.

Document any compatibility boundary discovered during implementation.

---

# 9. External MCP trust semantics

Issue #44 explicitly requires that existing external-MCP trust semantics remain unchanged.

Separate these concepts carefully:

```text
built-in tool authorization
≠
external MCP server trust
```

Inspect the existing external-MCP authorization path and add regression coverage proving SEC-001 does not accidentally change it.

Do not route external MCP authorization through a new built-in-only special case.

Do not claim unchanged semantics without a regression test or existing authoritative test that demonstrates the boundary.

---

# 10. Actionable denial errors

A denied High-risk tool must return a structured, machine-readable authorization failure through the canonical error path.

The error should communicate enough information for an agent/operator to understand:

- authorization was denied;
- the requested capability/tool class;
- what explicit authorization is missing;
- how the caller can obtain the required authorization, where the repository has a documented mechanism.

Do not expose secrets, trust-store contents, tokens, credentials, or sensitive internal authorization records.

Do not turn denial into a generic internal error that makes remediation impossible.

Test the error shape at the relevant service/transport boundary if structured errors already exist.

---

# 11. Fail-closed behavior

Security failures must fail closed.

Verify behavior when:

- the authorization store is missing;
- the authorization store is unreadable;
- the authorization record is malformed;
- risk metadata is missing or invalid;
- the tool identity cannot be resolved;
- the requested capability cannot be resolved;
- authorization state is stale or otherwise unavailable;
- an internal authorization dependency returns an error.

The safe result for a High-risk built-in is denial, not an implicit allow.

Never implement:

```rust
unwrap_or(true)
```

or equivalent default-allow fallbacks for security decisions.

If a failure cannot be safely classified, reject execution.

---

# 12. Invocation-boundary proof

Do not stop at unit-testing `authorize_builtin_tool`.

Identify every production entry point that can invoke the affected built-ins, including where applicable:

- MCP `tools/call`;
- Control API;
- CLI/internal command path;
- service-layer direct invocation;
- connector/tool registry dispatch.

Verify that all relevant paths reach the same authoritative authorization decision before performing the High-risk action.

If a lower-level service is intentionally trusted/internal, document that boundary rather than adding an artificial duplicate authorization layer.

The acceptance test should prove that a default-configured runtime cannot actually execute `terminal.run` or other High-risk built-ins without explicit authorization.

---

# 13. Test plan

Add or update behavior-focused tests. Prefer real authorization state and production invocation paths over mocked copies of the policy algorithm.

## Mandatory security matrix

| Scenario | Expected result |
|---|---|
| High-risk tool, no authorization | DENY |
| High-risk tool, explicit matching authorization | ALLOW |
| High-risk tool, wrong capability | DENY |
| High-risk tool, insufficient scope | DENY |
| High-risk tool, expired authorization | DENY where expiry exists |
| High-risk tool, revoked/disabled authorization | DENY where supported |
| High-risk tool, corrupt/missing auth store | DENY / fail closed |
| Unknown built-in tool | DENY |
| Low-risk intentionally allowed tool | Existing behavior preserved |
| External MCP trusted path | Existing behavior preserved |
| One High-risk capability granted | Only that capability is enabled |
| New/representative High-risk registry entry | Same default-deny rule |

## Required tool coverage

At minimum cover:

- `terminal.run`;
- `connector.invoke`;
- every currently registered High-risk provider mutation discovered during preflight;
- an unknown tool;
- at least one intentionally lower-risk built-in;
- external MCP trust regression.

## Boundary coverage

Where architecture permits, include tests through the real production dispatch boundary rather than only testing the authorization helper.

A passing helper test alone is not sufficient acceptance evidence.

## No-execution proof

For denied High-risk actions, prove that the underlying action did not execute.

For `terminal.run`, use a safe test fixture/marker where the test can prove that the command was not reached. Do not execute arbitrary host commands as part of the test.

For mutation tools, verify the protected state remains unchanged after denial.

---

# 14. Migration and onboarding documentation

Update the appropriate existing documentation only if SEC-001 requires a user-visible migration/onboarding change.

Document:

- High-risk built-ins are deny-by-default;
- explicit authorization is required;
- how a legitimate operator/agent obtains the required authorization using the existing mechanism;
- how least privilege should be configured;
- how to diagnose a denial;
- that unknown tools remain denied;
- that external-MCP trust is a separate concept where applicable.

Do not invent configuration commands or schemas. Document only mechanisms that exist in the current implementation.

If documentation already accurately states the behavior, do not duplicate it; verify it and leave it unchanged.

---

# 15. Security regression review

Before considering the implementation complete, inspect the final diff specifically for accidental authorization broadening.

Search for:

```text
allow
bypass
skip authorization
unwrap_or
unwrap_or_else
unknown tool
builtin
risk
trust
authorize
terminal.run
connector.invoke
```

Confirm that no new path turns an authorization error into success.

Confirm that no test fixture accidentally establishes global authorization and therefore masks the default-deny behavior.

Confirm that tests distinguish:

```text
no authorization
```

from:

```text
explicit authorization
```

Do not make all test fixtures authorized merely to preserve existing tests.

---

# 16. Full verification gate

After implementation, run exactly the repository verification gate required by the sequential master prompt:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also inspect:

```bash
git status
git diff --stat
git diff
```

Never report a command as passed unless it actually ran successfully.

If a command fails:

1. stop;
2. diagnose the failure;
3. fix only SEC-001-related causes;
4. rerun the failed gate and the complete gate as appropriate;
5. do not declare resolution while any required gate remains failing.

If local Cargo execution is unavailable, explicitly state that and obtain actual GitHub CI evidence. Do not substitute an assumed or remembered CI result.

---

# 17. CI acceptance evidence — mandatory before resolution

SEC-001 must **not** be counted as resolved until current-branch CI has passed after the final implementation commit.

The evidence must identify:

- exact commit SHA tested;
- workflow/check name;
- successful status;
- timestamp or run identifier where available;
- relevant test suite/job result.

Minimum CI evidence must cover the repository's required Rust checks:

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

If CI uses a combined workflow, verify that all equivalent required jobs actually passed. A green unrelated workflow is not sufficient.

A PR being mergeable, a commit existing, or a test file being added is **not** CI acceptance evidence.

---

# 18. Acceptance checklist — resolution gate

Use this checklist literally. **Every item must be checked before saying “SEC-001 resolved.”**

### Implementation

- [ ] Current source confirms the real authorization boundary was identified.
- [ ] High-risk built-ins default to explicit deny.
- [ ] No authorization record cannot grant High-risk access.
- [ ] `terminal.run` is denied without explicit authorization.
- [ ] `connector.invoke` is denied without explicit authorization when classified High-risk.
- [ ] All current High-risk provider mutations inherit the same registry/risk rule.
- [ ] Future High-risk registry entries inherit the same default-deny behavior.
- [ ] Unknown tools remain denied.
- [ ] Explicit matching authorization permits only the intended capability.
- [ ] Wrong capability/scope is denied.
- [ ] Expired/revoked authorization is denied where supported.
- [ ] Missing/corrupt authorization state fails closed.
- [ ] Lower-risk compatibility is preserved intentionally.
- [ ] External MCP trust semantics remain unchanged and are regression-tested.
- [ ] No second policy/authorization engine was introduced.
- [ ] No tool-specific authorization bypass was introduced.
- [ ] Denial errors are actionable and secret-safe.

### Tests

- [ ] Default-deny test passes.
- [ ] Explicit-allow test passes.
- [ ] Least-privilege test passes.
- [ ] Wrong-capability test passes.
- [ ] Missing/corrupt-store fail-closed test passes.
- [ ] Unknown-tool denial test passes.
- [ ] Lower-risk regression test passes.
- [ ] External-MCP trust regression passes.
- [ ] High-risk invocation boundary is tested, not only the helper.
- [ ] Denied action is proven not to execute.

### Documentation

- [ ] Migration/onboarding behavior is documented if required.
- [ ] Documentation describes only implementation-backed behavior.
- [ ] No unsupported authorization commands/configuration were invented.

### Verification

- [ ] `cargo fmt --all -- --check` passed.
- [ ] `cargo check --all-targets` passed.
- [ ] `cargo test --all-targets` passed.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passed.
- [ ] Final `git status` was inspected.
- [ ] Final `git diff --stat` was inspected.
- [ ] Final `git diff` was inspected.
- [ ] No unrelated files changed.

### CI

- [ ] CI ran against the final SEC-001 implementation commit.
- [ ] Required CI checks all passed.
- [ ] CI run/check identifiers were recorded.
- [ ] No required check was skipped, ignored, or made non-blocking merely to obtain green status.

### Issue resolution

- [ ] All GitHub issue #44 acceptance criteria are satisfied.
- [ ] Implementation evidence and CI evidence agree.
- [ ] Acceptance tests demonstrate the production security boundary.
- [ ] No known SEC-001 blocker remains.
- [ ] Only now may the issue be reported as **resolved**.
- [ ] If any checkbox is unchecked, the issue remains **open / unresolved**.

---

# 19. Explicit anti-premature-resolution rule

The following are **NOT sufficient** to mark SEC-001 resolved:

- creating this prompt;
- writing the implementation;
- adding unit tests without running them;
- local tests passing while required CI is unavailable;
- a PR being opened;
- a PR being mergeable;
- a PR being merged without acceptance evidence;
- one CI job passing while another required gate fails;
- issue text being updated without implementation evidence;
- documentation claiming default-deny behavior;
- a security helper passing while an invocation path bypasses it;
- counting SEC-001 as complete because SEC-002 was completed;
- counting an earlier PR or competing implementation as a resolved issue.

The only acceptable resolution state is:

```text
implementation complete
→ targeted tests pass
→ full verification gate passes
→ final diff reviewed
→ CI passes on final commit
→ acceptance matrix passes
→ no blockers remain
→ issue #44 can be truthfully closed
```

---

# 20. Commit discipline

Create one focused SEC-001 implementation commit (or a minimal sequence of SEC-001-only commits if repository mechanics require it).

Do not mix:

- AGENT-001 work;
- ARCH-001 work;
- FS-001 work;
- GIT-001 work;
- unrelated refactors;
- formatting-only cleanup outside touched files;
- speculative policy redesign.

The commit message should clearly identify SEC-001/#44.

After committing, re-check the final commit diff and CI status.

---

# 21. Final implementation report

At the end of the run, report exactly:

1. **Issue:** SEC-001 / #44 — title.
2. **State:** resolved only if every resolution-gate checkbox is satisfied; otherwise open/blocked/partially implemented.
3. **Baseline:** what the preflight inspection found.
4. **Implementation:** exact authorization boundary changed and why.
5. **High-risk coverage:** tools/classes covered.
6. **Tests:** exact tests executed and their outcomes.
7. **Verification:** exact Cargo commands and outcomes.
8. **CI:** final commit SHA and successful required checks.
9. **Documentation:** files changed, if any.
10. **Diff scope:** exact files changed and confirmation that unrelated files were untouched.
11. **Known limitations:** anything still unresolved.
12. **GitHub issue status:** whether #44 is actually eligible to be closed.
13. **Next issue:** do not implement it; only identify it as AGENT-001/#46 if and only if SEC-001 and SEC-002 are both verified and closed.

If the evidence is incomplete, explicitly say:

> **SEC-001 is NOT resolved because the required CI/acceptance evidence is incomplete.**

Do not soften or reinterpret this requirement.

---

# HARD STOP

After SEC-001 implementation, testing, verification, and reporting, **STOP**.

Do not implement AGENT-001/#46 in the same run.

The updated sequential master prompt requires security gates and acceptance evidence before post-edit architecture work. Preserve that sequencing exactly.
