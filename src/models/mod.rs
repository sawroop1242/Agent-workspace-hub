//! Data models shared across the workspace runtime: agents, capability
//! grants, projects, tasks, and memory entries. These types are the persisted
//! domain objects serialized as JSON/JSONL under each project's `.agent`
//! directory.

/// Agent identity models.
pub mod agent;
/// Capability grant models.
pub mod capability_grant;
/// Memory entry models.
pub mod memory;
/// Project models.
pub mod project;
/// Task models.
pub mod task;

pub use agent::{Agent, AgentStatus};
pub use capability_grant::CapabilityGrant;
pub use memory::MemoryEntry;
pub use project::Project;
pub use task::{Task, TaskStatus};
