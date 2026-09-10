//! Canonical Tool Registry: EXPLICIT, name-addressed metadata for every
//! tool AWH exposes, replacing name-prefix heuristics. The registry is
//! the single source of truth for a tool's category, versions, provider,
//! risk level, and required permissions.
//!
//! Invariants (enforced by tests):
//! * Every tool in the static catalog (including the optional `github.*`
//!   set) has a registry entry — a tool without metadata cannot ship.
//! * Categories, providers, risks, and permissions are DECLARED per tool,
//!   never inferred from name prefixes.
//! * Tool metadata is descriptive only: it powers discovery and
//!   observability for clients. It is NEVER a capability, permission
//!   check, or security boundary.
//!
//! Versions are split deliberately:
//! * [`AWH_TOOL_API_VERSION`] — the tool API version for tools that do not
//!   track an independent version history. Bumped centrally when a tool's
//!   contract evolves.
//! * [`SCHEMA_FORMAT_VERSION`] — the version of the `inputSchema` FORMAT
//!   (the JSON Schema subset the validator implements), independent of
//!   any tool's behavioral version.

use crate::mcp::permissions::Permission;

/// Central tool API version for tools without independent version history.
pub const AWH_TOOL_API_VERSION: &str = "1.0.0";

/// Version of the `inputSchema` format (JSON Schema subset) used by all
/// advertised schemas. Changes only when the schema language itself
/// changes (e.g. support for new keywords), never per tool.
pub const SCHEMA_FORMAT_VERSION: u32 = 1;

/// Descriptive risk level for a tool. Documentation for clients and
/// operators — it is not consulted by any authorization decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk {
    /// Read-only or otherwise side-effect free.
    Low,
    /// Mutates workspace-local state (files, memory, tasks, context).
    Medium,
    /// External side effects: processes, network mutations, or arbitrary
    /// connector dispatch.
    High,
}

impl ToolRisk {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolRisk::Low => "low",
            ToolRisk::Medium => "medium",
            ToolRisk::High => "high",
        }
    }
}

/// Explicit metadata for one tool in the AWH catalog.
#[derive(Debug)]
pub struct ToolDefinition {
    /// The exact tool name (no prefix matching, ever).
    pub name: &'static str,
    /// Short canonical role summary (long descriptions live with the tool
    /// schemas in `tools_list_static`).
    pub description: &'static str,
    /// Discovery/observability category.
    pub category: &'static str,
    /// The tool's own evolution version.
    pub tool_version: &'static str,
    /// The format version of the tool's `inputSchema`.
    pub schema_version: u32,
    /// `"awh"` for built-ins, a provider id for dynamic tools.
    pub provider: &'static str,
    /// Descriptive risk level (never a security decision).
    pub risk: ToolRisk,
    /// Capability vocabulary this tool's execution involves. Reuses the
    /// [`Permission`] enum so tool metadata and execution policy share one
    /// vocabulary.
    pub required_permissions: &'static [Permission],
    /// Whether the tool is exposed in `tools/list`.
    pub enabled: bool,
}

macro_rules! awh_tool {
    ($name:literal, $description:literal, $category:literal, $risk:ident,
     [$($perm:ident),*]) => {
        ToolDefinition {
            name: $name,
            description: $description,
            category: $category,
            tool_version: AWH_TOOL_API_VERSION,
            schema_version: SCHEMA_FORMAT_VERSION,
            provider: "awh",
            risk: ToolRisk::$risk,
            required_permissions: &[$(Permission::$perm),*],
            enabled: true,
        }
    };
}

/// The registry: every static tool, sorted by name (required by the
/// binary-search lookup). Kept EXHAUSTIVE by
/// `every_static_tool_has_registry_metadata`.
static REGISTRY: &[ToolDefinition] = &[
    // ---- connector.* (dynamic provider plane) ----
    awh_tool!(
        "connector.composio_accounts",
        "List connected Composio accounts",
        "connector",
        Low,
        [Network]
    ),
    awh_tool!(
        "connector.composio_link",
        "Create a Composio OAuth link",
        "connector",
        Medium,
        [Network]
    ),
    awh_tool!(
        "connector.composio_register",
        "Register a Composio API key",
        "connector",
        Medium,
        [Network]
    ),
    awh_tool!(
        "connector.composio_remove",
        "Remove the registered Composio provider",
        "connector",
        Medium,
        []
    ),
    awh_tool!(
        "connector.invoke",
        "Invoke a tool exposed by a provider",
        "connector",
        High,
        [Network]
    ),
    awh_tool!(
        "connector.providers",
        "List registered providers",
        "connector",
        Low,
        []
    ),
    awh_tool!(
        "connector.tools",
        "List tools exposed by a provider",
        "connector",
        Low,
        []
    ),
    // ---- connectors.* (project connector metadata) ----
    awh_tool!(
        "connectors.add",
        "Register connector metadata",
        "connector",
        Medium,
        []
    ),
    awh_tool!(
        "connectors.disable",
        "Disable a connector",
        "connector",
        Medium,
        []
    ),
    awh_tool!(
        "connectors.enable",
        "Enable a connector",
        "connector",
        Medium,
        []
    ),
    awh_tool!(
        "connectors.get",
        "Get connector metadata by id",
        "connector",
        Low,
        []
    ),
    awh_tool!(
        "connectors.list",
        "List connector metadata",
        "connector",
        Low,
        []
    ),
    awh_tool!(
        "connectors.remove",
        "Remove connector metadata",
        "connector",
        Medium,
        []
    ),
    // ---- context.* ----
    awh_tool!(
        "context.assemble",
        "Assemble the context window",
        "context",
        Low,
        []
    ),
    awh_tool!("context.get", "Get a context item", "context", Low, []),
    awh_tool!(
        "context.insert",
        "Insert a context item",
        "context",
        Medium,
        []
    ),
    awh_tool!(
        "context.offload",
        "Offload a context item",
        "context",
        Medium,
        []
    ),
    awh_tool!(
        "context.optimize",
        "Optimize the context window",
        "context",
        Medium,
        []
    ),
    awh_tool!(
        "context.protect",
        "Protect a context item",
        "context",
        Medium,
        []
    ),
    awh_tool!(
        "context.remove",
        "Remove a context item",
        "context",
        Medium,
        []
    ),
    awh_tool!(
        "context.restore",
        "Restore an offloaded context item",
        "context",
        Medium,
        []
    ),
    awh_tool!("context.search", "Search context items", "context", Low, []),
    awh_tool!(
        "context.status",
        "Context engine status",
        "context",
        Low,
        []
    ),
    awh_tool!(
        "context.unprotect",
        "Unprotect a context item",
        "context",
        Medium,
        []
    ),
    // ---- git.* ----
    awh_tool!(
        "git.branch",
        "Create or switch a git branch",
        "git",
        Medium,
        [Filesystem]
    ),
    awh_tool!(
        "git.commit",
        "Commit staged changes",
        "git",
        Medium,
        [Filesystem]
    ),
    awh_tool!(
        "git.diff",
        "Show working-tree or staged changes",
        "git",
        Low,
        [Filesystem]
    ),
    awh_tool!("git.log", "Show commit history", "git", Low, [Filesystem]),
    awh_tool!("git.stage", "Stage files", "git", Medium, [Filesystem]),
    awh_tool!(
        "git.status",
        "Show repository status",
        "git",
        Low,
        [Filesystem]
    ),
    awh_tool!("git.unstage", "Unstage files", "git", Medium, [Filesystem]),
    // ---- github.* (optional; present when the provider is configured) ----
    awh_tool!(
        "github.checks_status",
        "CI/check status for a ref",
        "github",
        Low,
        [Network]
    ),
    awh_tool!(
        "github.issue_comment",
        "Comment on an issue or PR",
        "github",
        High,
        [Network]
    ),
    awh_tool!(
        "github.issue_create",
        "Create an issue",
        "github",
        High,
        [Network]
    ),
    awh_tool!(
        "github.issue_get",
        "Get an issue or PR",
        "github",
        Low,
        [Network]
    ),
    awh_tool!("github.issue_list", "List issues", "github", Low, [Network]),
    awh_tool!(
        "github.pr_create",
        "Create a pull request",
        "github",
        High,
        [Network]
    ),
    awh_tool!(
        "github.pr_get",
        "Get a pull request",
        "github",
        Low,
        [Network]
    ),
    awh_tool!(
        "github.pr_list",
        "List pull requests",
        "github",
        Low,
        [Network]
    ),
    awh_tool!(
        "github.pr_merge",
        "Merge a pull request",
        "github",
        High,
        [Network]
    ),
    awh_tool!(
        "github.pr_review",
        "Submit a pull request review",
        "github",
        High,
        [Network]
    ),
    awh_tool!(
        "github.release_create",
        "Create a release",
        "github",
        High,
        [Network]
    ),
    awh_tool!(
        "github.workflow_dispatch",
        "Trigger a workflow run",
        "github",
        High,
        [Network]
    ),
    // ---- system ----
    awh_tool!("mcp.status", "Server status", "system", Low, []),
    // ---- memory.* ----
    awh_tool!(
        "memory.delete",
        "Delete a memory entry",
        "memory",
        Medium,
        []
    ),
    awh_tool!("memory.get", "Get a memory entry", "memory", Low, []),
    awh_tool!("memory.search", "Search memory", "memory", Low, []),
    awh_tool!("memory.store", "Store a memory entry", "memory", Medium, []),
    awh_tool!(
        "memory.update",
        "Update a memory entry",
        "memory",
        Medium,
        []
    ),
    // ---- skills.* ----
    awh_tool!(
        "skills.add",
        "Reference an installed skill",
        "skills",
        Medium,
        []
    ),
    awh_tool!("skills.list", "List project skills", "skills", Low, []),
    awh_tool!("skills.read", "Read a skill", "skills", Low, []),
    awh_tool!(
        "skills.remove",
        "Remove a skill reference",
        "skills",
        Medium,
        []
    ),
    awh_tool!(
        "skills.search",
        "Search installed skills",
        "skills",
        Low,
        []
    ),
    // ---- tasks.* ----
    awh_tool!("tasks.create", "Create a task", "tasks", Medium, []),
    awh_tool!("tasks.delete", "Delete a task", "tasks", Medium, []),
    awh_tool!("tasks.get", "Get a task", "tasks", Low, []),
    awh_tool!("tasks.list", "List tasks", "tasks", Low, []),
    awh_tool!("tasks.update", "Update a task", "tasks", Medium, []),
    // ---- terminal ----
    awh_tool!(
        "terminal.run",
        "Run a shell command",
        "terminal",
        High,
        [Process]
    ),
    // ---- workspace.* ----
    awh_tool!(
        "workspace.context",
        "Read project agent instructions",
        "workspace",
        Low,
        [Filesystem]
    ),
    awh_tool!(
        "workspace.delete_file",
        "Delete a workspace file",
        "workspace",
        Medium,
        [Filesystem]
    ),
    awh_tool!(
        "workspace.list_files",
        "List workspace files",
        "workspace",
        Low,
        [Filesystem]
    ),
    awh_tool!(
        "workspace.read_file",
        "Read a workspace file",
        "workspace",
        Low,
        [Filesystem]
    ),
    awh_tool!(
        "workspace.write_file",
        "Write a workspace file",
        "workspace",
        Medium,
        [Filesystem]
    ),
];

/// Exact-name registry lookup. No prefix inference: an unknown name has
/// no metadata, full stop — callers must handle `None` explicitly
/// (dynamic tools carry provider-declared metadata instead).
pub fn registry_lookup(name: &str) -> Option<&'static ToolDefinition> {
    REGISTRY
        .binary_search_by(|tool| tool.name.cmp(name))
        .ok()
        .map(|index| &REGISTRY[index])
}

/// All registry entries, name-sorted. Used by the exhaustiveness test to
/// cross-check against the static tool catalog.
pub fn registry_entries() -> &'static [ToolDefinition] {
    REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_sorted_for_binary_search() {
        let names: Vec<&str> = REGISTRY.iter().map(|t| t.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "REGISTRY must stay name-sorted");
        let deduped: Vec<&&str> = sorted
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(
            deduped.len(),
            sorted.len(),
            "duplicate tool names in REGISTRY"
        );
    }

    #[test]
    fn lookup_is_exact_not_prefix_based() {
        assert!(registry_lookup("terminal.run").is_some());
        // A prefix that is not a real tool must NOT resolve (no inference).
        assert!(registry_lookup("terminal").is_none());
        assert!(registry_lookup("terminal.run.extra").is_none());
        assert!(registry_lookup("").is_none());
        assert!(registry_lookup("no.such.tool").is_none());
        // Case sensitivity is exact.
        assert!(registry_lookup("Terminal.Run").is_none());
    }

    #[test]
    fn versions_are_split_and_explicit() {
        let tool = registry_lookup("workspace.write_file").expect("registered");
        assert_eq!(tool.tool_version, AWH_TOOL_API_VERSION);
        assert_eq!(tool.schema_version, SCHEMA_FORMAT_VERSION);
        // The two versions are distinct concepts (str vs format level)
        // and the schema format is stable at 1 until the schema language
        // itself changes.
        assert_eq!(tool.schema_version, 1);
        assert_eq!(tool.provider, "awh");
    }

    #[test]
    fn risk_levels_are_declared_not_derived() {
        // Read-only stays low, external side effects high, workspace-local
        // mutations medium — spot-check each declared level.
        assert_eq!(
            registry_lookup("workspace.read_file").unwrap().risk,
            ToolRisk::Low
        );
        assert_eq!(
            registry_lookup("workspace.write_file").unwrap().risk,
            ToolRisk::Medium
        );
        assert_eq!(
            registry_lookup("terminal.run").unwrap().risk,
            ToolRisk::High
        );
        assert_eq!(
            registry_lookup("github.pr_merge").unwrap().risk,
            ToolRisk::High
        );
        assert_eq!(
            registry_lookup("github.pr_list").unwrap().risk,
            ToolRisk::Low
        );
        assert_eq!(
            registry_lookup("connector.invoke").unwrap().risk,
            ToolRisk::High
        );
    }

    #[test]
    fn permissions_use_the_shared_vocabulary() {
        assert_eq!(
            registry_lookup("terminal.run")
                .unwrap()
                .required_permissions,
            &[Permission::Process]
        );
        assert_eq!(
            registry_lookup("git.status").unwrap().required_permissions,
            &[Permission::Filesystem]
        );
        assert_eq!(
            registry_lookup("github.pr_list")
                .unwrap()
                .required_permissions,
            &[Permission::Network]
        );
        assert!(registry_lookup("memory.store")
            .unwrap()
            .required_permissions
            .is_empty());
    }

    #[test]
    fn every_entry_is_enabled_and_categorized() {
        for tool in REGISTRY {
            assert!(
                tool.enabled,
                "{}: registry entries must be enabled",
                tool.name
            );
            assert!(
                !tool.category.is_empty() && tool.category != "uncategorized",
                "{}: registry entries must declare a real category",
                tool.name
            );
        }
    }
}
