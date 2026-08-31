//! Skill discovery, installation, and reference management.
//!
//! Skills are validated packages installed from local paths or remote
//! registries, then referenced per-project. Installation writes are atomic and
//! integrity-checked (SHA-256).

/// Skill installer.
pub mod installer;
/// Skill lockfile persistence.
pub mod lockfile;
/// Skill data model.
pub mod model;
/// Skill package helpers.
pub mod package;
/// SKILL.md parsing.
pub mod parser;
/// Project skill references.
pub mod project;
/// Skill references.
pub mod references;
/// Skill registries.
pub mod registries;
/// Global skill registry.
pub mod registry;
/// Registry HTTP client.
pub mod registry_client;
/// Registry manifest models.
pub mod registry_manifest;
/// Remote (Git/community) skill sources.
pub mod remote;
/// Project skill store.
pub mod store;
/// Skill trust levels.
pub mod trust;

pub use installer::SkillInstaller;
pub use lockfile::{LockedSkill, LockfileStore, SkillLockfile};
pub use model::Skill;
pub use package::{safe_package_path, sha256_file, validate_skill_package};
pub use parser::parse_skill;
pub use project::ProjectSkillReferences;
pub use references::SkillReferences;
pub use registries::{RegistryConfig, RegistryStore};
pub use registry::GlobalSkillRegistry;
pub use registry_client::RegistryClient;
pub use registry_manifest::{RegistryManifest, RegistrySkill};
pub use remote::{RemoteSkillRegistry, SkillRegistrySource};
pub use store::SkillStore;
pub use trust::{validate_sha256, TrustLevel};
