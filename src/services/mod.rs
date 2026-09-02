//! Application service layer: the single owner of business logic shared by
//! CLI, TUI, Control API, and MCP interfaces. Interfaces call services;
//! services call core stores and domain engines. No interface duplicates
//! business logic.

pub mod files;
pub mod git;
pub mod projects;
pub mod terminal;
