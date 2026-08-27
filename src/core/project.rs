use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::models::Project;

pub struct ProjectStore;

impl ProjectStore {
    pub fn create(root: &Path, name: &str) -> Result<Project> {
        let path = root.join(name);
        fs::create_dir_all(path.join(".agent"))
            .with_context(|| format!("failed to create project: {name}"))?;
        Ok(Project::new(name, path))
    }

    pub fn exists(root: &Path, name: &str) -> bool {
        root.join(name).is_dir()
    }

    pub fn list(root: &Path) -> Result<Vec<Project>> {
        let mut projects = Vec::new();
        if !root.exists() {
            return Ok(projects);
        }
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && path.join(".agent").is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).map(str::to_owned);
                if let Some(name) = name {
                    projects.push(Project::new(&name, path));
                }
            }
        }
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(projects)
    }
}
