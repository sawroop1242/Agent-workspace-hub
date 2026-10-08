use crate::mcp::permissions::Permission;
use serde::{Deserialize, Serialize};

/// A capability granted to an agent, recorded but not yet enforced.
///
/// Grants reuse the shared [`Permission`] vocabulary so a grant and a tool's
/// declared `required_permissions` can be compared directly. `scope` narrows
/// a grant to a resource prefix (e.g. a filesystem path prefix); `None`
/// means the grant is unscoped.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityGrant {
    /// Unique grant id.
    pub id: String,
    /// The agent the grant belongs to.
    pub agent_id: String,
    /// The capability category granted.
    pub permission: Permission,
    /// Optional resource scope (e.g. a filesystem path prefix).
    pub scope: Option<String>,
    /// RFC 3339 creation timestamp.
    pub granted_at: String,
    /// Optional RFC 3339 expiry timestamp; `None` never expires.
    pub expires_at: Option<String>,
}
