# ARCH-001 (Prompt 15) — Before-state ownership map

Derived from current-source forensics on branch `arch-001-service-store-convergence`
(base: origin/rust @ 2a00394). "Canonical" = the single owner after convergence.

| Domain | Current implementation | Persistence | Writers | Readers | Interfaces | Status |
|---|---|---|---|---|---|---|
| Filesystem | `services::files::FilesService` (resolve_checked, 8 MiB cap, list/search/meta/rename/create_dir) | project tree | API, TUI, EditService | all four | CLI(no fs cmds)/API/TUI/Edit | **canonical** |
| Filesystem | `mcp::workspace::WorkspaceMcp` (own `safe_path`/`safe_new_path`, 2 MiB read / 5 MiB write caps) | project tree | MCP tools | MCP tools | MCP | **duplicate → thin adapter over FilesService** |
| Memory | `mcp::memory::MemoryMcp` (.agent/memory.json; id/scope/tags/timestamps; upsert/update/search/get/delete; StoreLock+atomic; fail-closed) | .agent/memory.json | MCP tools, ContextEngine | MCP tools, ContextEngine | MCP only | **duplicate → becomes canonical (relocated to core)** |
| Memory | `core::memory::MemoryStore` (.agent/memory.jsonl; timestamp+content; append-only; no locking) | .agent/memory.jsonl | Control API `/api/v1/memory`, TUI | API, TUI | API/TUI | **duplicate (divergent visibility!) → rewired onto canonical + legacy JSONL migration** |
| Tasks | `core::tasks::TaskStore` (one-file-per-task; models::Task Pending/InProgress/Completed/Cancelled) | .agent/tasks/<id>.json | **none** (zero production callers) | its own tests only | none | **dead duplicate → remove** |
| Tasks | `mcp::tasks::TasksMcp` (.agent/tasks.json; rich: description/priority/assignee/tags; Todo/InProgress/Blocked/Done) | .agent/tasks.json | MCP tools | MCP tools | MCP | **duplicate → becomes canonical (relocated to core)** |
| Context (persistent project notes) | `core::context::ContextStore` (.agent/context.md) | .agent/context.md | API, TUI | API, TUI | API/TUI | **canonical** (concept: mutable project context notes) |
| Context (instruction discovery) | `WorkspaceMcp::context()` reads AGENTS.md/AGENT.md/README.md | none (read-only discovery) | none | MCP `workspace.context` | MCP | **protocol-specific read-only adapter — allowed to stay** (distinct concept, not persisted state) |
| Project/workspace identity | `services::init` manifest + `core::workspace::Workspace` + `core::project::ProjectStore` | .agent/workspace.json | init service (single) | services, TUI, API | all | canonical |
| Projects (lifecycle) | `services::projects::ProjectsService` (list/create/get/delete) | project tree | CLI, API | TUI(via backend), API | CLI/API | canonical |
| Projects (delete) | `tui::backend::delete_project` re-implements name validation + remove_dir_all | project tree | TUI | — | TUI | **duplicate business mutation → rewire to ProjectsService::delete** |
| Editing | `services::edit::EditService` | project tree + edit records | MCP edit tools, CLI edit cmds | all | all | canonical (dispatcher line 1672+ confirms delegation) |
| Snapshots | `services::snapshot` / `context::snapshot` store | .agent/snapshots | edit service | edit/rollback | MCP/CLI | canonical |
| Provenance/refs | `services::edit` EditRefs/records | .agent/edits | edit service | edit/rollback | MCP/CLI | canonical |
| Audit | `services::audit` (global ring + record_outcome/audit_allow/audit_deny) | .agent/audit (init) + stderr tracing | services + API/MCP via record helpers | API /api/v1/audit, /api/v1/logs | all | canonical |
| Agents | `core::agents::AgentStore` (one-file-per-record, fail-closed on unsafe ids) | .agent/agents/ | AgentRuntimeService (all interfaces route through it per TW-002) | runtime service | all | canonical |
| Sessions | `core::sessions::SessionStore` | .agent/sessions/ | AgentRuntimeService | runtime service | all | canonical |
| Capabilities | `core::capability_grants::CapabilityGrantStore` | .agent/capabilities/ | CLI grants cmds, runtime service | authorization service | CLI/MCP | canonical (direct CLI reads = allowed per §39, no shadow state) |
| Policy | `core::policy::PolicyStore` (DENY rules, single array file) | .agent/policy.json | CLI policy cmds, dispatcher authorize_policy, authorization service | dispatcher, authz, init | all | canonical |
| Connectors | `mcp::connectors::ConnectorStore` | .agent/connectors.json | MCP connectors tools / CLI? (single module) | MCP | MCP | MCP-owned single-writer — **verify no second writer** (CLI string only mentions key handling) |
| Custom MCP registry | `mcp::custom_mcp::CustomMcpRegistry` | .agent/mcps.json | `awh mcp add/remove` via this module only | dispatcher | CLI/MCP | MCP-owned, single writer |
| Global MCP catalog | `mcp::global_mcp::GlobalMcpRegistry` | data_dir/mcps.json + project .agent/mcp_refs.json | registry module only | dispatcher | CLI/MCP | intentionally layered (global catalog + per-project refs) — distinct scopes, not duplicates |
| Trust | `mcp::trust_store::PersistentTrustStore` | data_dir/trust.json | MCP trust plane only | execution gate | MCP | **distinct security store — must stay separate (§12)** |
| Skills | `skills::*` (project skills.json + lockfile + user registries.json) | .agent/skills.json, data dir | skills module only | skills registry/API | CLI/MCP/API | canonical (single module family) |
| StoreLock | `mcp::store_lock::StoreLock` (cross-process lock) | lock files adjacent to stores | memory/tasks/connectors/custom_mcp/global_mcp/skills | — | — | shared primitive — §16: keep; owner = persistence owner |

## Interface bypass audit (§36)

- CLI (src/main.rs): NO direct fs:: writes at all; uses services + core stores' public APIs (allowed).
- TUI (src/tui/backend.rs): services for files/git/terminal; core stores for context/memory (rewire memory after convergence); **`delete_project` re-implements ProjectsService::delete → migrate**.
- Control API (src/api/control.rs): FilesService + core stores; no direct domain fs writes (hits are test fixtures).
- MCP dispatcher: EditService/snapshot/audit are canonical already; memory/tasks/workspace are the divergent ones.

## Convergence decisions

1. **Filesystem**: `WorkspaceMcp` methods become thin adapters delegating to `FilesService`
   (read → `read`, write → `write_atomic`, delete → `delete` + existence pre-check preserving the
   bool contract, list → `list` filtered to files, `context()` stays read-only discovery).
   Authorization ordering in the dispatcher is untouched (upstream of the store call).
2. **Memory**: the rich store is promoted to `core::memory::MemoryStore` at `.agent/memory.json`
   (same format, ids, limits, StoreLock semantics). Legacy `.agent/memory.jsonl` gets an explicit,
   deterministic, idempotent, atomic migration (deterministic `legacy-jsonl-<sha8>` ids; dedup on
   re-run; `.jsonl` renamed to `.jsonl.migrated` only after the JSON store publishes). Control API
   and TUI rewire onto the canonical store (API POST gains a record; GET lists richer records —
   additive, backward-compatible fields). `models::MemoryEntry` survives only in the migration
   reader. `mcp::memory` reduces to re-exports for the tool plane; ContextEngine/planner keep the
   same data source via the new canonical path.
3. **Tasks**: dead `core::tasks::TaskStore` + `models::Task` usage removed (zero production
   callers — grep evidence in PR). The live rich store moves to `core::tasks::TaskStore`
   (`.agent/tasks.json` unchanged on disk). `mcp::tasks` reduces to re-exports.
4. **TUI project delete** delegates to `ProjectsService::delete` (keeps TUI audit event).
5. Architecture regression tests pin: single `MemoryStore`/`TaskStore` definitions (core only),
   no `.agent/memory*`/`.agent/tasks*` path literals outside core, no mutation fs:: calls in
   `mcp::workspace.rs`, and no persistence-path literals in interface modules for memory/tasks.

## Post-convergence target truth table (§31)

| Operation | MCP | CLI | TUI | Control API | Authoritative owner |
|---|---|---|---|---|---|
| read file | `workspace.read_file` → FilesService | (n/a) | backend → FilesService | handler → FilesService | FilesService |
| write file | `workspace.write_file` → FilesService.write_atomic | edit cmds → EditService | backend → FilesService | handler → FilesService | FilesService |
| delete file | `workspace.delete_file` → FilesService.delete | (n/a) | backend → FilesService.delete | handler → FilesService.delete | FilesService |
| list files | `workspace.list_files` → FilesService.list | (n/a) | backend → FilesService.list | handler → FilesService.list | FilesService |
| memory store/update/delete | `memory.*` → core::MemoryStore | (n/a) | backend append → core::MemoryStore | POST/GET → core::MemoryStore | core::MemoryStore |
| tasks create/update/delete | `tasks.*` → core::TaskStore | (n/a) | (n/a) | (n/a) | core::TaskStore |
| project delete | (n/a) | (n/a) | backend → ProjectsService | handler → ProjectsService | ProjectsService |
