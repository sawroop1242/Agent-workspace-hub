use crate::skills::{GlobalSkillRegistry, ProjectSkillReferences, Skill};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A lightweight skill summary exposed over MCP (name, description, version).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    /// Skill name.
    pub name: String,
    /// Short description of what the skill does.
    pub description: String,
    /// Optional version string.
    pub version: Option<String>,
    /// Runtime-exposure toggle for the project reference: `Some(true)` /
    /// `Some(false)` in project-scoped views, `None` for global views
    /// where the concept does not apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// MCP gateway over the global registry and per-project skill references.
pub struct SkillMcp {
    registry: GlobalSkillRegistry,
    project: ProjectSkillReferences,
}

impl SkillMcp {
    /// Creates the gateway, discovering the global registry and the project's references.
    pub fn new(project_root: PathBuf) -> Result<Self> {
        Ok(Self {
            registry: GlobalSkillRegistry::discover()?,
            project: ProjectSkillReferences::new(project_root),
        })
    }

    /// Creates the gateway over an explicit registry — the seam tests and
    /// embedders use instead of `discover()`.
    pub fn with_registry(project_root: PathBuf, registry: GlobalSkillRegistry) -> Self {
        Self {
            registry,
            project: ProjectSkillReferences::new(project_root),
        }
    }

    /// MCP-facing discovery: only skills referenced by this project are
    /// visible, each annotated with its enabled flag so management
    /// clients can see (and re-enable) disabled references.
    pub fn list(&self) -> Result<Vec<SkillSummary>> {
        let enabled_by_name: std::collections::HashMap<String, bool> = self
            .project
            .states()?
            .into_iter()
            .map(|state| (state.name, state.enabled))
            .collect();
        Ok(self
            .project
            .resolve(&self.registry)?
            .into_iter()
            .map(|skill| {
                let enabled = enabled_by_name.get(&skill.name).copied();
                summary(skill, enabled)
            })
            .collect())
    }

    /// MCP-facing read: resolve a project reference before exposing skill
    /// content. A disabled reference is not runtime-exposed — reading it
    /// fails closed with a stable message instead of silently serving
    /// content the operator turned off.
    pub fn read(&self, name: &str) -> Result<Skill> {
        if !self.project.load()?.skills.iter().any(|s| s == name) {
            bail!("skill is not referenced by the current project: {name}");
        }
        if self
            .project
            .states()?
            .iter()
            .any(|state| state.name == name && !state.enabled)
        {
            bail!("skill is disabled: {name}");
        }
        self.registry
            .get(name)?
            .ok_or_else(|| anyhow::anyhow!("skill not installed globally: {name}"))
    }

    /// Adds a skill to the project's references.
    pub fn add(&self, name: &str) -> Result<()> {
        self.project.add(name, &self.registry)?;
        Ok(())
    }

    /// Removes a skill from the project's references, returning whether it was present.
    pub fn remove(&self, name: &str) -> Result<bool> {
        self.project.remove(name)
    }

    /// Enables a referenced skill for runtime exposure (fails closed on
    /// unreferenced names — the canonical store owns the semantics).
    pub fn enable(&self, name: &str) -> Result<()> {
        self.project.enable(name)
    }

    /// Disables a referenced skill without dropping the reference.
    pub fn disable(&self, name: &str) -> Result<()> {
        self.project.disable(name)
    }

    /// Searches all globally installed skills by name or description.
    pub fn search_global(&self, query: &str) -> Result<Vec<SkillSummary>> {
        let query = query.to_ascii_lowercase();
        Ok(self
            .registry
            .list()?
            .into_iter()
            .filter(|s| {
                s.name.to_ascii_lowercase().contains(&query)
                    || s.description.to_ascii_lowercase().contains(&query)
            })
            .map(|skill| summary(skill, None))
            .collect())
    }
}

fn summary(skill: Skill, enabled: Option<bool>) -> SkillSummary {
    SkillSummary {
        name: skill.name,
        description: skill.description,
        version: skill.version,
        enabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway_with(temp: &tempfile::TempDir, names: &[&str]) -> SkillMcp {
        let registry = GlobalSkillRegistry::new(temp.path().join("global-skills"));
        for name in names {
            registry.create(name, "test skill").unwrap();
        }
        SkillMcp::with_registry(temp.path().join("proj"), registry)
    }

    #[test]
    fn list_annotates_enabled_state_and_read_gates_disabled_skills() {
        let temp = tempfile::tempdir().unwrap();
        let gateway = gateway_with(&temp, &["alpha", "beta"]);
        gateway.add("alpha").unwrap();
        gateway.add("beta").unwrap();

        // both referenced + enabled
        let listed = gateway.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|s| s.enabled == Some(true)));

        // disable one: still listed (management visibility) but read fails
        gateway.disable("alpha").unwrap();
        let listed = gateway.list().unwrap();
        let alpha = listed.iter().find(|s| s.name == "alpha").unwrap();
        assert_eq!(alpha.enabled, Some(false));

        let err = gateway.read("alpha").unwrap_err();
        assert!(err.to_string().contains("skill is disabled: alpha"));

        // enabled skills still read fine
        assert!(gateway.read("beta").is_ok());

        // re-enable restores runtime exposure
        gateway.enable("alpha").unwrap();
        assert!(gateway.read("alpha").is_ok());
        assert_eq!(
            gateway.list().unwrap()[0].enabled,
            Some(true),
            "all references re-enabled"
        );
    }

    #[test]
    fn enable_and_disable_fail_closed_on_unreferenced_names() {
        let temp = tempfile::tempdir().unwrap();
        let gateway = gateway_with(&temp, &["alpha"]);
        let err = gateway.enable("ghost").unwrap_err();
        assert!(err
            .to_string()
            .contains("skill is not referenced by the current project: ghost"));
        let err = gateway.disable("ghost").unwrap_err();
        assert!(err
            .to_string()
            .contains("skill is not referenced by the current project: ghost"));
    }

    #[test]
    fn global_search_does_not_carry_a_project_enabled_flag() {
        let temp = tempfile::tempdir().unwrap();
        let gateway = gateway_with(&temp, &["alpha"]);
        let hits = gateway.search_global("alpha").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].enabled, None);
    }
}
