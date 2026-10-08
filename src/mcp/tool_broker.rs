//! Declarative authorization metadata for built-in MCP tools.
//!
//! This tiny module is the single source of truth for the pairing between a
//! built-in tool and the "which argument carries the resource to check" facts
//! that the dispatcher's [`authorize_tool`](crate::mcp::dispatcher::McpDispatcher::authorize_tool)
//! entry point consults. Keeping it here (rather than as a convention a human
//! or agent must remember at each call site) means a future resource-scoped
//! tool gets its resource check wired up by adding one table row, not by
//! remembering to place a second call in the right arm.

/// Built-in tools that require a resource-scoped policy check in addition to
/// the coarse category-level gate, and which argument name in the tool call
/// carries the resource to check.
///
/// This is the single source of truth for that pairing — see Phase 3 (PR #20)
/// for why these three specifically.
pub(crate) const RESOURCE_SCOPED_TOOLS: &[(&str, &str)] = &[
    ("workspace.write_file", "path"),
    ("workspace.delete_file", "path"),
    ("terminal.run", "program"),
];

#[cfg(test)]
mod tests {
    use super::RESOURCE_SCOPED_TOOLS;

    /// Guards against silent scope creep in either direction: the table is
    /// exactly the three tools whose call carries a resource worth a
    /// policy-scoped check, each mapping to the correct argument name.
    #[test]
    fn resource_scoped_tools_are_exactly_the_three_known_pairs() {
        assert_eq!(
            RESOURCE_SCOPED_TOOLS,
            &[
                ("workspace.write_file", "path"),
                ("workspace.delete_file", "path"),
                ("terminal.run", "program"),
            ],
            "RESOURCE_SCOPED_TOOLS must contain exactly the three resource-scoped tools"
        );
    }
}
