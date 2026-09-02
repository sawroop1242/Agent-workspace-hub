//! Projects service: workspace-scoped project management shared by all
//! interfaces. Wraps [`crate::core::ProjectStore`] and enforces that
//! project names cannot escape the workspace root.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::core::project::ProjectStore;
use crate::core::workspace::Workspace;
use crate::models::Project;

/// Application service for project management under one workspace root.
pub struct ProjectsService {
    workspace: Workspace,
}

impl ProjectsService {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            workspace: Workspace::new(root),
        }
    }

    pub fn root(&self) -> &Path {
        self.workspace.root()
    }

    /// Lists projects (directories containing `.agent`).
    pub fn list(&self) -> Result<Vec<Project>> {
        ProjectStore::list(self.workspace.root())
    }

    /// Returns a project by name, or `None` when not found.
    pub fn get(&self, name: &str) -> Result<Option<Project>> {
        Ok(self.list()?.into_iter().find(|p| p.name == name))
    }

    /// Creates a project, rejecting names that could escape the root.
    pub fn create(&self, name: &str) -> Result<Project> {
        validate_project_name(name)?;
        if ProjectStore::exists(self.workspace.root(), name) {
            bail!("project already exists: {name}");
        }
        self.workspace.ensure()?;
        ProjectStore::create(self.workspace.root(), name)
            .with_context(|| format!("failed to create project {name}"))
    }

    /// Deletes a project after validating the name; the caller's UI/CLI
    /// must have confirmed the destructive action before calling.
    pub fn delete(&self, name: &str) -> Result<bool> {
        validate_project_name(name)?;
        let Some(project) = self.get(name)? else {
            return Ok(false);
        };
        let path = &project.path;
        // Defense in depth: canonicalize and verify containment before rm.
        let canonical = path
            .canonicalize()
            .with_context(|| format!("cannot resolve project path {}", path.display()))?;
        let root_canonical = self
            .workspace
            .root()
            .canonicalize()
            .context("cannot resolve workspace root")?;
        if !canonical.starts_with(&root_canonical) || canonical == root_canonical {
            bail!("refusing to delete a path outside the workspace root");
        }
        std::fs::remove_dir_all(&canonical)
            .with_context(|| format!("failed to delete project {name}"))?;
        Ok(true)
    }

    /// Returns the absolute path for `name` after validation.
    pub fn path_of(&self, name: &str) -> Result<PathBuf> {
        validate_project_name(name)?;
        if !ProjectStore::exists(self.workspace.root(), name) {
            bail!("project not found: {name}");
        }
        Ok(self.workspace.project_path(name))
    }
}

/// Project names are single path components: no separators, no `.`/`..`,
/// no absolute prefixes, no whitespace.
pub fn validate_project_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.chars().any(|c| c.is_whitespace() || c == '\\')
        || name.starts_with('.')
        || Path::new(name).file_name().and_then(|n| n.to_str()) != Some(name)
    {
        bail!("invalid project name: {name:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_project_names() {
        for bad in [
            "", ".", "..", "a/b", "/abs", "C:\\x", " lead", "trail ", ".hidden",
        ] {
            assert!(validate_project_name(bad).is_err(), "{bad:?}");
        }
        assert!(validate_project_name("good-name").is_ok());
        assert!(validate_project_name("proj_1").is_ok());
    }

    #[test]
    fn create_list_get_delete_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = ProjectsService::new(tmp.path());
        let p = svc.create("demo").unwrap();
        assert!(p.path.ends_with("demo"));
        assert_eq!(svc.list().unwrap().len(), 1);
        assert!(svc.get("demo").unwrap().is_some());
        assert!(svc.get("missing").unwrap().is_none());
        assert!(svc.delete("demo").unwrap());
        assert!(svc.list().unwrap().is_empty());
    }

    #[test]
    fn delete_refuses_unknown_project() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = ProjectsService::new(tmp.path());
        assert!(!svc.delete("nope").unwrap());
    }

    #[test]
    fn create_rejects_duplicates_and_bad_names() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = ProjectsService::new(tmp.path());
        svc.create("demo").unwrap();
        assert!(svc.create("demo").is_err());
        assert!(svc.create("../escape").is_err());
        assert!(svc.create("").is_err());
    }
}
