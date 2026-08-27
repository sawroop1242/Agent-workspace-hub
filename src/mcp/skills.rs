use crate::skills::{GlobalSkillRegistry, ProjectSkillReferences, Skill};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A lightweight skill summary exposed over MCP (name, description, version).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
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

    /// MCP-facing discovery: only skills referenced by this project are visible.
    pub fn list(&self) -> Result<Vec<SkillSummary>> {
        Ok(self
            .project
            .resolve(&self.registry)?
            .into_iter()
            .map(summary)
            .collect())
    }

    /// MCP-facing read: resolve a project reference before exposing skill content.
    pub fn read(&self, name: &str) -> Result<Skill> {
        if !self.project.load()?.skills.iter().any(|s| s == name) {
            bail!("skill is not referenced by the current project: {name}");
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
            .map(summary)
            .collect())
    }
}

fn summary(skill: Skill) -> SkillSummary {
    SkillSummary {
        name: skill.name,
        description: skill.description,
        version: skill.version,
    }
}
