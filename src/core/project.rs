use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::models::Project;

/// Static helper for creating and listing projects under a workspace root.
pub struct ProjectStore;

impl ProjectStore {
    /// Creates a project directory (with its `.agent` subdirectory) and returns it.
    pub fn create(root: &Path, name: &str) -> Result<Project> {
        let path = root.join(name);
        fs::create_dir_all(path.join(".agent"))
            .with_context(|| format!("failed to create project: {name}"))?;
        Ok(Project::new(name, path))
    }

    /// Returns whether a project directory with `name` exists under `root`.
    pub fn exists(root: &Path, name: &str) -> bool {
        root.join(name).is_dir()
    }

    /// Lists projects under `root` (directories containing a `.agent` subdirectory).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_returns_project_with_agent_dir() {
        let temp = tempfile::tempdir().unwrap();
        let project = ProjectStore::create(temp.path(), "alpha").unwrap();
        assert_eq!(project.name, "alpha");
        assert_eq!(project.path, temp.path().join("alpha"));
        assert!(project.path.join(".agent").is_dir());
    }

    #[test]
    fn exists_reflects_directory_presence() {
        let temp = tempfile::tempdir().unwrap();
        assert!(!ProjectStore::exists(temp.path(), "alpha"));
        ProjectStore::create(temp.path(), "alpha").unwrap();
        assert!(ProjectStore::exists(temp.path(), "alpha"));
    }

    #[test]
    fn list_on_missing_root_is_empty() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("no-such-root");
        assert!(ProjectStore::list(&missing).unwrap().is_empty());
    }

    #[test]
    fn list_returns_only_projects_with_agent_dir_sorted_by_name() {
        let temp = tempfile::tempdir().unwrap();
        ProjectStore::create(temp.path(), "beta").unwrap();
        ProjectStore::create(temp.path(), "alpha").unwrap();
        // a plain directory without `.agent` is not a project
        fs::create_dir_all(temp.path().join("not-a-project")).unwrap();
        // a file (not a directory) is not a project either
        fs::write(temp.path().join("some-file.txt"), "hi").unwrap();

        let projects = ProjectStore::list(temp.path()).unwrap();
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }
}
