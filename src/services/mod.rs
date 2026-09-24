//! Application service layer: the single owner of business logic shared by
//! CLI, TUI, Control API, and MCP interfaces. Interfaces call services;
//! services call core stores and domain engines. No interface duplicates
//! business logic.

pub mod agent_runtime;
pub mod audit;
pub mod authorization;
pub mod edit;
pub use edit::{EditOperation, EditService, EditTransaction};
pub mod files;
pub mod git;
pub mod init;
pub mod projects;
pub mod rate_limit;
pub mod snapshot;
pub mod terminal;
