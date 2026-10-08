use crate::mcp::store_lock::StoreLock;
use crate::skills::{GlobalSkillRegistry, Skill};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Names of skills a project references from the global registry.
///
/// `disabled` records references that exist but are switched off for
/// runtime exposure — disabling keeps the reference (and anything
/// pointing at it) intact while removing it from the agent-facing
/// surface.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillReferences {
    /// Referenced skill names.
    pub skills: Vec<String>,
    /// Referenced but disabled skill names.
    #[serde(default)]
    pub disabled: Vec<String>,
}

/// One reference's runtime state: the name plus whether runtime
/// surfaces (MCP `skills.list`, TUI, Control API) expose it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRefState {
    /// Referenced skill name.
    pub name: String,
    /// Whether the reference is enabled for runtime exposure.
    pub enabled: bool,
}

/// Per-project skill references persisted under `.agent/skills.json`.
///
/// The ONE canonical project-skill reference store: CLI, MCP tools,
/// Control API, and TUI all read and write through it. Mutations are
/// serialized with the shared store lock and published atomically
/// (temp file + rename), so no surface observes a torn write.
pub struct ProjectSkillReferences {
    project_root: PathBuf,
}

impl ProjectSkillReferences {
    /// Creates reference storage for a project root.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: root.into(),
        }
    }

    /// Returns the on-disk path of the references file.
    pub fn path(&self) -> PathBuf {
        self.project_root.join(".agent").join("skills.json")
    }

    /// Loads the project's skill references, defaulting to empty.
    pub fn load(&self) -> Result<SkillReferences> {
        let path = self.path();
        if !path.exists() {
            return Ok(SkillReferences::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    fn validate_name(name: &str) -> Result<()> {
        if name.is_empty()
            || name.len() > 100
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            bail!("invalid skill name: {name}");
        }
        Ok(())
    }

    /// Adds a skill reference, returning whether it was newly added.
    /// Exposure state is owned by `enable`/`disable` alone: re-adding a
    /// disabled reference keeps it disabled — no silent state flips.
    pub fn add(&self, name: &str, registry: &GlobalSkillRegistry) -> Result<bool> {
        Self::validate_name(name)?;
        if registry.get(name)?.is_none() {
            bail!("skill is not installed globally: {name}");
        }
        let _guard = self.lock()?;
        let mut refs = self.load()?;
        if refs.skills.iter().any(|s| s == name) {
            return Ok(false);
        }
        refs.skills.push(name.to_owned());
        refs.skills.sort();
        refs.skills.dedup();
        refs.disabled.retain(|s| s != name);
        self.save(&refs)?;
        Ok(true)
    }

    /// Removes a skill reference (and any disable state), returning
    /// whether it was present.
    pub fn remove(&self, name: &str) -> Result<bool> {
        let _guard = self.lock()?;
        let mut refs = self.load()?;
        let old_len = refs.skills.len();
        refs.skills.retain(|s| s != name);
        refs.disabled.retain(|s| s != name);
        if refs.skills.len() == old_len {
            return Ok(false);
        }
        self.save(&refs)?;
        Ok(true)
    }

    /// Resolves referenced skill names to installed skills, skipping missing ones.
    pub fn resolve(&self, registry: &GlobalSkillRegistry) -> Result<Vec<Skill>> {
        let refs = self.load()?;
        let mut resolved = Vec::with_capacity(refs.skills.len());
        for name in refs.skills {
            if let Some(skill) = registry.get(&name)? {
                resolved.push(skill);
            }
        }
        Ok(resolved)
    }

    /// Returns every reference with its runtime state, in reference order.
    ///
    /// Names absent from `disabled` default to enabled, so a references
    /// file written before disable states existed needs no migration.
    pub fn states(&self) -> Result<Vec<SkillRefState>> {
        let refs = self.load()?;
        Ok(refs
            .skills
            .iter()
            .map(|name| SkillRefState {
                name: name.clone(),
                enabled: !refs.disabled.iter().any(|d| d == name),
            })
            .collect())
    }

    /// Enables a referenced skill for runtime exposure. Fails closed on
    /// names that are not referenced — enabling a ghost invents state.
    pub fn enable(&self, name: &str) -> Result<()> {
        Self::validate_name(name)?;
        let _guard = self.lock()?;
        let mut refs = self.load()?;
        if !refs.skills.iter().any(|s| s == name) {
            bail!("skill is not referenced by the current project: {name}");
        }
        refs.disabled.retain(|s| s != name);
        self.save(&refs)?;
        Ok(())
    }

    /// Disables a referenced skill without dropping the reference.
    /// Fails closed on names that are not referenced.
    pub fn disable(&self, name: &str) -> Result<()> {
        Self::validate_name(name)?;
        let _guard = self.lock()?;
        let mut refs = self.load()?;
        if !refs.skills.iter().any(|s| s == name) {
            bail!("skill is not referenced by the current project: {name}");
        }
        if !refs.disabled.iter().any(|s| s == name) {
            refs.disabled.push(name.to_owned());
            refs.disabled.sort();
            refs.disabled.dedup();
        }
        self.save(&refs)?;
        Ok(())
    }

    /// Persists the project's skill references atomically.
    pub fn save(&self, refs: &SkillReferences) -> Result<()> {
        let path = self.path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_write(&path, &serde_json::to_string_pretty(refs)?)
    }

    fn lock(&self) -> Result<StoreLock> {
        // The lock file lives beside the store, so its parent must
        // exist before acquisition (ENOENT otherwise).
        let path = self.path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        StoreLock::acquire(&path)
    }
}

/// Writes a file atomically: serialize to a temp file in the same
/// directory, fsync, then rename over the target. Readers observe
/// either the old bytes or the new bytes, never a torn write.
fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(contents.as_bytes())?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("awh-skills-project-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn registry_with_skill(root: &Path, name: &str) -> GlobalSkillRegistry {
        let reg_dir = root.join("registry");
        std::fs::create_dir_all(reg_dir.join(name)).unwrap();
        std::fs::write(
            reg_dir.join(name).join("SKILL.md"),
            format!("---\nname: {name}\ndescription: test skill\n---\nbody"),
        )
        .unwrap();
        GlobalSkillRegistry::new(reg_dir)
    }

    #[test]
    fn add_remove_roundtrip_and_default_empty() {
        let root = tmp_root("roundtrip");
        let refs = ProjectSkillReferences::new(root.join("proj"));
        let reg = registry_with_skill(&root, "alpha");
        assert!(refs.load().unwrap().skills.is_empty());
        assert!(refs.add("alpha", &reg).unwrap());
        assert!(!refs.add("alpha", &reg).unwrap(), "second add is not new");
        assert_eq!(refs.load().unwrap().skills, vec!["alpha".to_string()]);
        assert!(refs.remove("alpha").unwrap());
        assert!(!refs.remove("alpha").unwrap());
        assert!(refs.load().unwrap().skills.is_empty());
    }

    #[test]
    fn add_rejects_uninstalled_and_invalid_names() {
        let root = tmp_root("reject");
        let refs = ProjectSkillReferences::new(root.join("proj"));
        let reg = registry_with_skill(&root, "alpha");
        assert!(refs.add("ghost", &reg).is_err());
        assert!(refs.add("../evil", &reg).is_err());
        assert!(refs.add("UPPER", &reg).is_err());
        assert!(refs.add("", &reg).is_err());
    }

    #[test]
    fn resolve_skips_missing_globals() {
        let root = tmp_root("resolve-skip");
        let refs = ProjectSkillReferences::new(root.join("proj"));
        let reg = registry_with_skill(&root, "alpha");
        refs.add("alpha", &reg).unwrap();
        let resolved = refs.resolve(&reg).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "alpha");
        // A reference whose global install vanished is skipped, not fatal.
        std::fs::remove_dir_all(root.join("registry").join("alpha")).unwrap();
        assert!(refs.resolve(&reg).unwrap().is_empty());
    }

    #[test]
    fn enable_disable_toggle_keeps_reference() {
        let root = tmp_root("toggle");
        let refs = ProjectSkillReferences::new(root.join("proj"));
        let reg = registry_with_skill(&root, "alpha");
        refs.add("alpha", &reg).unwrap();
        assert!(refs.states().unwrap()[0].enabled);
        refs.disable("alpha").unwrap();
        let states = refs.states().unwrap();
        assert_eq!(states.len(), 1);
        assert!(!states[0].enabled, "disabled references stay referenced");
        refs.enable("alpha").unwrap();
        assert!(refs.states().unwrap()[0].enabled);
    }

    #[test]
    fn enable_disable_fail_closed_on_unreferenced() {
        let root = tmp_root("fail-closed");
        let refs = ProjectSkillReferences::new(root.join("proj"));
        let reg = registry_with_skill(&root, "alpha");
        refs.add("alpha", &reg).unwrap();
        assert!(refs.enable("ghost").is_err());
        assert!(refs.disable("ghost").is_err());
        // Failed calls must not have mutated anything.
        assert_eq!(refs.states().unwrap().len(), 1);
    }

    #[test]
    fn disabled_state_survives_reload_and_remove_cleans_it() {
        let root = tmp_root("persist");
        let refs = ProjectSkillReferences::new(root.join("proj"));
        let reg = registry_with_skill(&root, "alpha");
        refs.add("alpha", &reg).unwrap();
        refs.disable("alpha").unwrap();
        // A fresh handle reads the same persisted state.
        let reopened = ProjectSkillReferences::new(root.join("proj"));
        assert!(!reopened.states().unwrap()[0].enabled);
        reopened.remove("alpha").unwrap();
        assert!(reopened.states().unwrap().is_empty());
    }

    #[test]
    fn legacy_references_file_without_disabled_field_enables_all() {
        let root = tmp_root("legacy");
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join(".agent")).unwrap();
        std::fs::write(
            proj.join(".agent").join("skills.json"),
            r#"{"skills":["alpha"]}"#,
        )
        .unwrap();
        let refs = ProjectSkillReferences::new(proj.clone());
        let states = refs.states().unwrap();
        assert_eq!(
            states,
            vec![SkillRefState {
                name: "alpha".into(),
                enabled: true
            }]
        );
    }

    #[test]
    fn re_adding_a_disabled_skill_keeps_it_disabled() {
        let root = tmp_root("re-add");
        let refs = ProjectSkillReferences::new(root.join("proj"));
        let reg = registry_with_skill(&root, "alpha");
        refs.add("alpha", &reg).unwrap();
        refs.disable("alpha").unwrap();
        assert!(!refs.add("alpha", &reg).unwrap(), "still referenced");
        assert!(
            !refs.states().unwrap()[0].enabled,
            "exposure state changes only through enable/disable"
        );
    }

    #[test]
    fn concurrent_mutations_serialize_through_the_lock() {
        let root = tmp_root("concurrent");
        let shared = std::sync::Arc::new(ProjectSkillReferences::new(root.join("proj")));
        let reg_dir = root.join("registry");
        for name in ["s1", "s2", "s3", "s4"] {
            std::fs::create_dir_all(reg_dir.join(name)).unwrap();
            std::fs::write(
                reg_dir.join(name).join("SKILL.md"),
                format!("---\nname: {name}\ndescription: d\n---\n"),
            )
            .unwrap();
        }
        let reg = std::sync::Arc::new(GlobalSkillRegistry::new(reg_dir));
        let mut handles = Vec::new();
        for name in ["s1", "s2", "s3", "s4"] {
            let refs = shared.clone();
            let reg = reg.clone();
            handles.push(std::thread::spawn(move || {
                refs.add(name, &reg).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // Every writer's add survived — no lost update.
        assert_eq!(shared.load().unwrap().skills.len(), 4);
    }

    #[test]
    fn save_is_atomic_via_rename() {
        let root = tmp_root("atomic");
        let refs = ProjectSkillReferences::new(root.join("proj"));
        let payload = SkillReferences {
            skills: vec!["alpha".into()],
            disabled: vec![],
        };
        refs.save(&payload).unwrap();
        let raw = std::fs::read_to_string(refs.path()).unwrap();
        assert!(raw.contains("alpha"));
        // No temp leftovers next to the target.
        let entries = std::fs::read_dir(root.join("proj").join(".agent")).unwrap();
        assert_eq!(entries.count(), 1, "no temp files remain after atomic save");
    }

    #[test]
    fn lock_creates_missing_agent_dir() {
        let root = tmp_root("lockdir");
        let refs = ProjectSkillReferences::new(root.join("proj").join("nested"));
        let guard = refs.lock().unwrap();
        // The lock is a `<store>.lock` sibling; the store file itself is
        // only written on save.
        let lock_path = {
            let mut name = refs
                .path()
                .file_name()
                .map(|n| n.to_os_string())
                .unwrap_or_default();
            name.push(".lock");
            refs.path().with_file_name(name)
        };
        assert!(lock_path.exists());
        drop(guard);
    }
}
