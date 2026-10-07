# TP11 Report — Skills Verification

**Prompt:** `docs/testing-prompts/11-skills.md`
**Branch:** `tp11-skills-verification` (from `rust` @ 66a92f0)
**Date:** 2026-10-07
**Suite:** `tests/skills_boundaries.rs` — 18 tests, plus 2 new production unit tests in `src/skills/registry.rs`
**Result:** 2 production defects (one shared root cause) found and fixed (§3); 1547 passed / 0 failed workspace-wide; fmt + clippy clean.

---

## 1. Methodology

Forensics first: every skills module was read at source — the model
(`Skill`), parser (`parse_skill` / `parse_front_matter` /
`is_valid_name`), package helpers (`validate_skill_package` 1 MiB
`SKILL.md` cap, `safe_package_path`, `sha256_file`), the global registry
(`GlobalSkillRegistry` — `discover()` honors `AWH_GLOBAL_SKILLS_ROOT`,
`create`/`get`/`list` with `validate_name`), the canonical project
reference store (`ProjectSkillReferences` — `.agent/skills.json`, the
`skills`/`disabled` split, StoreLock + atomic rename, fail-closed
enable/disable), the installer (`SkillInstaller` —
`install_from_registry`/`install_from_local`), the remote GitHub path
(`RemoteSkillRegistry`, `SkillRegistrySource::parse`), the registry
config store, the lockfile store, trust/`validate_sha256`, and the
legacy `SkillStore`. Every adapter was traced at its real boundary: the
CLI (`src/main.rs` `SkillCommand` + `RegistryCommand`, `audit_skill_mutation`),
the MCP dispatcher arms + schemas (`src/mcp/dispatcher.rs` `skills.*`,
`src/mcp/skills.rs` `SkillMcp`), the Control API
(`src/api/control.rs` `GET /api/v1/skills`, `/api/v1/skills/project`
GET/POST/DELETE), and the TUI backend (`src/tui/backend.rs`).

Pre-existing coverage was inventoried so this suite adds only the
boundary gaps: the module unit tests cover parse/validate, add/remove/
toggle, fail-closed enable/disable, atomic save, legacy-file loading,
id-minting concurrency, `validate_sha256`, and the installer's package
checks; `tests/mcp_http.rs` covers `skills.*` resource round-trips.
**No test drove the CLI install/uninstall name boundary, the malicious
registry path, the disabled-read gate over the real MCP wire, remote
integrity at the binary, or the Control API delegation with an injected
skills root.**

## 2. Established implementation map (forensics)

| Concern | Owner (source of truth) |
| --- | --- |
| Model | `skills::model::Skill` — name/description/version/path |
| Parser | `skills::parser::parse_skill` — YAML front matter (line-scan), name ≤ 100 chars `[a-z0-9-_]`, description required |
| Package safety | `skills::package` — `validate_skill_package` (SKILL.md ≤ 1 MiB), `safe_package_path`, `sha256_file` |
| Global registry | `skills::registry::GlobalSkillRegistry` — `~/.agent-workspace-hub/skills` or `AWH_GLOBAL_SKILLS_ROOT` |
| Project references | `skills::project::ProjectSkillReferences` — `.agent/skills.json`, `{skills, disabled}`, StoreLock + atomic publish |
| Installer | `skills::installer::SkillInstaller` — local + registry install |
| Remote | `skills::remote` — GitHub clone; `SkillRegistrySource` parse |
| Integrity | `skills::trust::validate_sha256` (case-insensitive, 64-hex, fail-closed) |
| Registry config | `skills::registries::RegistryStore` — `registries.json` |
| Lockfile | `skills::lockfile::LockfileStore` — `.agent/skills.lock.json` |
| CLI | `main.rs` `SkillCommand::{Create,List,Show,Read,Add,Remove,Enable,Disable,Project,Search,Install,Uninstall}` |
| MCP | `dispatcher` `skills.{list,read,add,remove,enable,disable,search}`; `SkillMcp` gates disabled/unreferenced reads |
| Control API | `GET /api/v1/skills`, `GET/POST/DELETE /api/v1/skills/project` (injected `global_skills_root`) |
| Audit | `cli_skill_{create,add,remove,enable,disable,install,uninstall}`, `api_skill_{add,remove}` — name + scope, never content |

## 3. Defects found

### D1 — `awh skill uninstall <name>` deleted a directory outside the global skills root (HIGH, fixed)

**Probe** (real binary, `AWH_GLOBAL_SKILLS_ROOT` isolated): with a victim
directory `../victim` beside the skills root, `awh skill uninstall
../../victim` printed `uninstalled global skill: ../../victim`, exited
`0`, and **deleted the victim directory** (`ls` → not found).

**Root cause:** the `Uninstall` arm joined the raw CLI name to
`global.skills_dir()` with no validation:
`let path = global.skills_dir().join(&name); std::fs::remove_dir_all(path)`.
Unlike `get`/`create`, which validate the name inside the registry, the
uninstall path trusted its caller. Any name containing `..` or a path
separator could therefore address and `remove_dir_all` an arbitrary
directory the process can write.

**Impact:** arbitrary directory deletion (data destruction) driven by a
CLI argument; on the agent-tool-adjacent plane this is a filesystem-safety
violation of the prompt's core "destination remains within canonical
Skills storage" contract.

### D2 — `awh skill install <name>` (registry) deleted and wrote outside the global skills root (HIGH, fixed)

**Probe** (real binary + malicious local registry): a manifest whose entry
`name` was `../../escape-target` and `path` `pkg/SKILL.md` (benign), with
`awh skill install ../../escape-target --registry http://127.0.0.1:<port>`,
exited `0` and: deleted `<root>/escape-target/keep.txt` (the victim) and
wrote `<root>/escape-target/SKILL.md`. The install **escaped the skills
root** entirely.

**Root cause:** `SkillInstaller::install_from_registry` validated
`entry.path` (`starts_with('/') || contains("..")`) but never validated
`entry.name`, then did `registry.skills_dir().join(&entry.name)` and
`remove_dir_all`/`copy_dir` — the same unchecked-join pattern as D1, this
time fed by an **untrusted remote manifest**. `install_from_local` and
`RemoteSkillRegistry::install_github` had the identical unchecked join.

**Impact:** a malicious (or compromised) registry could make the installer
delete and overwrite files anywhere the process can write — remote
content dictating local destructive writes. `install_github` additionally
joined an unvalidated `reference` into the cache path.

**Fix (shared root cause):** `skills::registry::validate_name` is now
public and is the single name boundary, applied before every join that
addresses the skills root or cache:
- CLI `Uninstall` validates before the join;
- `SkillInstaller::install_from_registry` validates `entry.name` (the
  untrusted registry value) before any path use;
- `SkillInstaller::install_from_local` validates the destination name;
- `RemoteSkillRegistry::install_github` validates `skill_name` and rejects
  path/control characters in `reference`.
`GlobalSkillRegistry::get`/`create` already used the same rule, so the
boundary is now uniform. Verified live post-fix: both probes exit `1` with
`invalid skill name: ../../…`, victims intact, no file written outside the
root; a valid registry install still succeeds.

**Regressions:** `cli_uninstall_rejects_path_traversal_and_never_deletes_outside`,
`cli_install_rejects_traversal_names_from_caller_and_registry` (black-box,
with positive controls), `package_validation_rejects_unsafe_layouts_without_partial_install`,
and the `registry::tests` unit tests.

### Follow-up: a TP10 defect surfaced by TP11 CI (fixed)

The TP11 CI run exposed a **pre-existing TP10 defect**, not a TP11
regression: `tests/memory_boundaries.rs::concurrent_cross_process_writers_keep_store_valid`
failed on macOS with 40 of 41 adds persisted (a lost update). Root cause
is in `src/core/memory.rs::MemoryStore::generate_id`: ids were
`mem-<nanos>-<in-process-counter>`, so two sibling processes reading the
same coarse clock tick (macOS/Windows granularity) each minted
`mem-<nanos>-0000`, and the store's id-keyed upsert silently collapsed
them into one record. The id now embeds the process id
(`mem-<nanos>-<pid>-<seq>`), unique across concurrently-live processes,
while the atomic counter still separates threads. Fixed here because
TP11 must merge with green cross-platform CI; regression pinned by
`generated_ids_embed_the_process_id_for_cross_process_uniqueness`.

### Non-defects / gaps verified and classified honestly

- **Lockfile (`LockfileStore`) is not wired to any lifecycle path** —
  no install/enable/disable call site creates or reads
  `.agent/skills.lock.json`. Classified **Not implemented (pinning)**; the
  store's own load/save is unit-tested.
- **Legacy `SkillStore` (`.agent/skills/`)** has no consumer outside
  `mod.rs`'s re-export; the canonical project store is
  `ProjectSkillReferences`. Classified as a dormant compatibility type,
  not a second authority (nothing writes through it).
- **`install_from_local`, `RemoteSkillRegistry`, `TrustLevel`,
  `safe_package_path`, `SkillLockfile`** are public APIs with no CLI/MCP
  call site today (the CLI install path is registry-only). Their
  boundaries were still tested directly (D2 area) and classified.
- **Empty search query** matches every globally installed skill
  (substring semantics) — pinned as the current contract.
- **Legacy reference file without `disabled`** loads all-enabled and is
  not rewritten — pinned.

## 4. Test-by-test evidence (tests/skills_boundaries.rs, 18)

1. `installed_skill_parsing_rules_hold_at_the_binary` — valid parse via
   `skill show`; malformed installed `SKILL.md` fails `show`/`list`
   (never silently dropped); a directory without `SKILL.md` is ignored.
   **Passed** (§5, §7).
2. `cli_uninstall_rejects_path_traversal_and_never_deletes_outside` —
   **D1 regression**: traversal/absolute/separator/uppercase names
   rejected; victim preserved; legitimate skill intact. **Passed**.
3. `cli_install_rejects_traversal_names_from_caller_and_registry` —
   **D2 regression**: caller + registry traversal rejected, victim
   preserved, nothing written outside root; valid install positive
   control. **Passed**.
4. `package_validation_rejects_unsafe_layouts_without_partial_install` —
   missing/oversized `SKILL.md` rejected with no partial install;
   `install_from_local` validates the destination name. **Passed** (§6).
5. `global_registry_listing_order_missing_and_name_safety` — sorted
   deterministic list, missing → `None`, invalid names rejected, a
   global install does not activate in any project. **Passed** (§7).
6. `global_registry_create_rejects_duplicates_and_bad_names` — duplicate
   and invalid names rejected, nothing written outside root. **Passed** (§7).
7. `reference_lifecycle_and_enable_disable_distinct_from_removal` — the
   four states stay distinct; duplicate add idempotent; disable keeps the
   reference; re-add while disabled keeps it disabled; remove drops the
   reference (not the global); errors for ghost add/remove/disable.
   **Passed** (§8, §9).
8. `project_reference_rejects_invalid_names` — add/enable/disable all
   reject invalid names, no file written. **Passed** (§8).
9. `legacy_reference_file_defaults_all_enabled` — legacy `{skills:[…]}`
   loads all-enabled, file not rewritten. **Passed** (§8).
10. `mcp_skills_lifecycle_and_disabled_read_gating` — MCP add/list/read/
    disable/enable/remove; disabled read fails closed with `skill is
    disabled`; unreferenced read fails closed; malformed args → error, no
    mutation; cross-workspace isolation. **Passed** (§11, §9).
11. `mcp_and_cli_share_the_project_reference_authority` — CLI↔MCP parity;
    the only project store is `.agent/skills.json` (no MCP-local store).
    **Passed** (§11, §26).
12. `remote_install_verifies_integrity_before_publication` — matching
    digest installs (independently re-hashed); mismatch/malformed digest
    rejected with no install; missing digest allowed. **Passed** (§15, §16).
13. `remote_registry_errors_fail_closed_without_fallback` — bad manifest,
    missing skill, unsafe path, connection failure all fail closed with no
    fallback and no partial state. **Passed** (§15).
14. `reference_state_survives_restart_and_corruption_fails_closed` —
    reference + disabled state survive a fresh process; corrupt file
    fails closed and is not silently emptied. **Passed** (§18, §19).
15. `concurrent_reference_additions_keep_state_valid` — 6 concurrent
    processes, all adds survive, no duplicates, valid JSON. **Passed** (§20).
16. `replay_and_lifecycle_edge_cases` — idempotent add/disable/enable,
    remove-twice errors, enable/disable after remove fail closed, no
    resurrection, re-reference works. **Passed** (§25).
17. `control_api_skills_delegates_to_canonical_store` — API add/remove
    visible to CLI + raw file, CLI add visible to API, uninstalled add →
    400 no mutation, no bearer → 401. **Passed** (§13).
18. `skill_mutations_are_audited_without_leaking_content` — five mutation
    actions durably audited; a token-shaped canary description never
    reaches the audit log; failed mutation leaves no false-positive
    success. **Passed** (§22, §23).

Plus 2 unit tests in `src/skills/registry.rs`:
`validate_name_rejects_path_shapes_and_accepts_safe_names` and
`registry_get_and_create_share_the_same_validation`.

## 5. Classification (per prompt evidence classes)

| Requirement area | Class | Evidence |
| --- | --- | --- |
| §5 manifest parsing/validation | **Passed** | test 1; parser unit tests |
| §6 package/filesystem safety | **Passed** | tests 2-4 (D1/D2 fixed) |
| §7 global registry | **Passed** | tests 5, 6 |
| §8 project reference lifecycle | **Passed** | tests 7-9 |
| §9 enable/disable security semantics | **Passed** | tests 7, 10 |
| §10 CLI black-box | **Passed** | tests 1-9, 14, 15, 18 |
| §11 MCP boundary | **Passed** | tests 10, 11 |
| §12 TUI integration | **Not automated / Unproven** | TUI backend uses the same canonical store (verified at source, `src/tui/backend.rs`); no automated TUI interaction harness in this environment — not claimed as executed |
| §13 Control API | **Passed** | test 17 |
| §14 local installation | **Passed** | test 4 |
| §15 remote source behavior | **Passed** | tests 12, 13 |
| §16 integrity + lock/pinning | **Integrity Passed; pinning Not implemented** | tests 12; `LockfileStore` has no lifecycle call site |
| §17 transactionality | **Passed** | tests 4, 13, 14 |
| §18 persistence/restart | **Passed** | test 14 |
| §19 corruption/recovery | **Passed** | test 14 |
| §20/§25 concurrency/replay | **Passed** | tests 15, 16 |
| §21 authorization/capability boundary | **Passed** | declared capabilities are not modeled in the skill package (data only); enable ≠ authorize proven by tests 7/10 (installed ≠ referenced ≠ enabled); consequential actions remain behind the existing capability gate |
| §22 audit | **Passed** | test 18 |
| §23 sensitive data | **Passed** | test 18 |
| §24 resource limits | **Passed** | tests 4, 6 (1 MiB cap, name bound) |
| §28 cross-platform | **Passed (CI)** | runs on ubuntu/macos/windows in CI |

**Not implemented (by design, classified):** skill lock/pinning in the
lifecycle, local-install CLI verb, GitHub-source CLI verb, declared
capability/requirement metadata (the `Skill` model has none — a skill
declaration is data and never authority, which the current model enforces
by not carrying capabilities at all). **Unproven:** automated TUI
interaction (no harness available).

## 6. Gates

- `cargo fmt` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — **1547 passed / 0 failed** (1527 pre-TP11
  baseline + 18 boundary tests + 2 registry unit tests).

Environment note: the Rust toolchain was reinstalled once during this
round (AGENTS.md rustup recipe; cargo 1.99.0).

## 7. No theater checklist (§29)

- Every CLI/MCP test drives the compiled `awh` binary; the MCP tests
  speak real JSON-RPC on the real stdio transport.
- Integration tests use the canonical `GlobalSkillRegistry` /
  `ProjectSkillReferences` / `SkillInstaller` and the real Control API
  router — never a parallel registry, reference store, or package format.
- Raw `.agent/skills.json`, the installed `SKILL.md` bytes, and
  independently computed SHA-256 digests are the oracles; no test trusts
  a getter to verify the same getter's write.
- Both defects were reproduced by REAL probes against the compiled binary
  BEFORE the fix (D1 CLI, D2 CLI + malicious registry) and re-verified
  gone AFTER the fix at the same boundaries, each with a positive control.
- Unimplemented paths (lock/pinning, local/GitHub CLI verbs, TUI
  automation) are classified honestly rather than papered over; no
  test-only registry, reference store, policy engine, or audit authority
  was introduced.
