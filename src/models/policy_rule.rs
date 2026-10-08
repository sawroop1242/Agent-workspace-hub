use serde::{Deserialize, Serialize};

/// A workspace-local DENY-only policy rule that narrows which resource a
/// built-in tool may touch.
///
/// Phase 3 supports exactly three tools — [`crate::mcp::dispatcher`]'s
/// `workspace.write_file`, `workspace.delete_file`, and `terminal.run` — and
/// for each, a single resource argument that matters:
///
/// * for the two `workspace.*` tools, [`PolicyRule::pattern`] is a *path
///   prefix* matched against the tool's `path` argument, with both sides
///   treated as forward-slash relative paths (no globs — that is a documented
///   v1 limitation);
/// * for `terminal.run`, [`PolicyRule::pattern`] is an *exact, case-sensitive*
///   match against the `program` argument (not a substring match, not
///   path-aware — also a documented v1 limitation).
///
/// A rule's presence only ever *narrows* a call: the coarse
/// category-level trust gate decides "is this tool allowed at all", and a
/// matching policy rule additionally rejects a specific path/command. There
/// are no Allow rules and no "require approval" in this phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyRule {
    /// Unique rule id (a short slug or auto-generated); validated with an
    /// `is_safe_*_id`-style guard so it can never contain path separators.
    pub id: String,
    /// Exact tool name this rule applies to: one of `workspace.write_file`,
    /// `workspace.delete_file`, or `terminal.run`. Any other value is
    /// rejected at creation time (this phase supports policy on no other
    /// tool yet).
    pub tool: String,
    /// The deny pattern: a path prefix for the two `workspace.*` tools, or an
    /// exact program name for `terminal.run`.
    pub pattern: String,
    /// Optional human-readable reason for the denial.
    pub reason: Option<String>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}
