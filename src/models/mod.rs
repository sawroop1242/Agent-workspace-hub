//! Data models shared across the workspace runtime: agents, capability
//! grants, projects, and sessions. These types are the persisted domain
//! objects serialized as JSON under each project's `.agent` directory.

/// Agent identity models.
pub mod agent;
/// Capability grant models.
pub mod capability_grant;
/// Workspace policy rule models.
pub mod policy_rule;
/// Project models.
pub mod project;
/// Session models (TW-002 runtime sessions).
pub mod session;

pub use agent::{Agent, AgentStatus};
pub use capability_grant::CapabilityGrant;
pub use policy_rule::PolicyRule;
pub use project::Project;
pub use session::{AgentSessionRecord, SessionStatus};
