use crate::mcp::store_lock::StoreLock;
use crate::models::PolicyRule;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// The three built-in tool names for which Phase 3 policy rules are
/// supported. Any other `tool` value is rejected at rule-creation time.
const SUPPORTED_POLICY_TOOLS: [&str; 3] = [
    "workspace.write_file",
    "workspace.delete_file",
    "terminal.run",
];

/// Persistent workspace-local policy storage under `.agent/policy.json`.
///
/// Unlike [`crate::core::agents::AgentStore`] and
/// [`crate::core::capability_grants::CapabilityGrantStore`] (which persist
/// one JSON file per record), a [`PolicyStore`] persists *all* rules as a
/// single JSON array in one file. This is the one deliberate deviation from
/// that pattern: the dispatcher consults the whole rule list on every gated
/// call through [`PolicyStore::matching`], so a single read per call (rather
/// than a directory scan) is the hot-path shape that matters.
#[derive(Clone)]
pub struct PolicyStore {
    root: PathBuf,
}

impl PolicyStore {
    /// Creates a `PolicyStore` rooted at `root` (rules live under
    /// `<root>/.agent/policy.json`).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self) -> PathBuf {
        self.root.join(".agent").join("policy.json")
    }

    /// Creates the `.agent` parent directory if it is missing, so a
    /// [`StoreLock`] and a first write can proceed on a fresh workspace.
    fn ensure_dir(&self) -> Result<()> {
        let path = self.path();
        let parent = path
            .parent()
            .context("policy store has no parent directory")?;
        fs::create_dir_all(parent).context("failed to create policy store directory")?;
        Ok(())
    }

    fn load(&self) -> Result<Vec<PolicyRule>> {
        let path = self.path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path).context("failed to read policy store")?;
        serde_json::from_str(&content).context("policy store contains invalid JSON")
    }

    /// Atomically writes the full rule list, never exposing a torn file.
    fn save(&self, rules: &[PolicyRule]) -> Result<()> {
        let path = self.path();
        let parent = path
            .parent()
            .context("policy store has no parent directory")?;
        fs::create_dir_all(parent)?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).context("failed to create policy temp file")?;
        std::io::Write::write_all(&mut temp, serde_json::to_vec_pretty(rules)?.as_slice())?;
        temp.as_file().sync_all()?;
        temp.persist(&path)
            .map_err(|error| error.error)
            .context("failed to atomically write policy store")?;
        Ok(())
    }

    /// Lists all stored rules in insertion order (the order they appear in
    /// `policy.json`). [`PolicyStore::matching`] returns the *first* match,
    /// so order is meaningful and deliberately preserved.
    pub fn list(&self) -> Result<Vec<PolicyRule>> {
        self.load()
    }

    /// Appends a rule, rejecting a duplicate `id` with a clear error.
    pub fn add(&self, rule: &PolicyRule) -> Result<()> {
        validate_rule(rule)?;
        self.ensure_dir()?;
        let _lock = StoreLock::acquire(&self.path())?;
        let mut rules = self.load()?;
        if rules.iter().any(|r| r.id == rule.id) {
            bail!("policy rule id already exists: {}", rule.id);
        }
        rules.push(rule.clone());
        self.save(&rules)?;
        Ok(())
    }

    /// Removes the rule with the given `id`, returning `false` if it did not
    /// exist.
    pub fn remove(&self, id: &str) -> Result<bool> {
        self.ensure_dir()?;
        let _lock = StoreLock::acquire(&self.path())?;
        let mut rules = self.load()?;
        let before = rules.len();
        rules.retain(|r| r.id != id);
        if rules.len() == before {
            return Ok(false);
        }
        self.save(&rules)?;
        Ok(true)
    }

    /// Returns the first rule (if any) for the exact `tool` name whose
    /// pattern matches `resource` under that tool's matching rule:
    ///
    /// * `workspace.write_file` / `workspace.delete_file`: a forward-slash,
    ///   relative-path *prefix* match against the tool's `path` argument
    ///   (both sides normalized so `a//b` == `a/b`, no globs);
    /// * `terminal.run`: an *exact*, case-sensitive match against the tool's
    ///   `program` argument.
    ///
    /// This is the single function the dispatcher consults to decide whether
    /// a resource-scoped deny applies.
    pub fn matching(&self, tool: &str, resource: &str) -> Result<Option<PolicyRule>> {
        for rule in self.load()? {
            if rule.tool != tool {
                continue;
            }
            let matched = match tool {
                "workspace.write_file" | "workspace.delete_file" => {
                    path_prefix_matches(&rule.pattern, resource)
                }
                "terminal.run" => rule.pattern == resource,
                _ => false,
            };
            if matched {
                return Ok(Some(rule));
            }
        }
        Ok(None)
    }
}

/// Returns whether `id` is safe to use as a policy rule id (no path
/// separators or traversal), mirroring `is_safe_agent_id`.
pub fn is_safe_policy_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains('/')
        && !id.contains('\\')
        && Path::new(id).file_name().and_then(|x| x.to_str()) == Some(id)
}

/// Validates a rule before it is persisted: the id must be safe and the tool
/// must be one of the three supported names.
fn validate_rule(rule: &PolicyRule) -> Result<()> {
    if !is_safe_policy_id(&rule.id) {
        bail!(
            "invalid policy rule id: {:?} (must not be empty or contain path separators)",
            rule.id
        );
    }
    if !SUPPORTED_POLICY_TOOLS.contains(&rule.tool.as_str()) {
        bail!(
            "unsupported policy tool: {:?} (expected one of {})",
            rule.tool,
            SUPPORTED_POLICY_TOOLS.join(", ")
        );
    }
    Ok(())
}

/// Forward-slash, relative-path prefix match used by the two `workspace.*`
/// tools. Both sides are normalized (repeated separators collapsed, trailing
/// slash trimmed) so a `/`-based comparison is meaningful without any glob
/// or glob-free-path resolution.
fn path_prefix_matches(pattern: &str, resource: &str) -> bool {
    let pattern = normalize_forward_slash(pattern);
    let resource = normalize_forward_slash(resource);
    // The empty pattern is a degenerate prefix of everything, but a rule
    // whose pattern is empty is treated as non-matching (a rule should name
    // a real prefix); this is also what keeps a malformed rule from denying
    // every path.
    if pattern.is_empty() {
        return false;
    }
    resource == pattern || resource.starts_with(&format!("{pattern}/"))
}

/// Normalizes a forward-slash relative path by collapsing repeated slashes
/// and trimming leading/trailing slashes (so `a//b` == `/a/b/` == `a/b`).
fn normalize_forward_slash(path: &str) -> String {
    let mut out = String::new();
    let mut last_was_slash = false;
    for ch in path.chars() {
        if ch == '/' {
            last_was_slash = true;
        } else {
            if last_was_slash && !out.is_empty() {
                out.push('/');
            }
            last_was_slash = false;
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, tool: &str, pattern: &str) -> PolicyRule {
        PolicyRule {
            id: id.into(),
            tool: tool.into(),
            pattern: pattern.into(),
            reason: None,
            created_at: "2026-09-12T00:00:00+00:00".into(),
        }
    }

    #[test]
    fn add_then_list_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        let r = rule("r1", "workspace.write_file", "src/");
        store.add(&r).unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed, vec![r]);
    }

    #[test]
    fn add_rejects_duplicate_id() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        store
            .add(&rule("r1", "workspace.write_file", "src/"))
            .unwrap();
        assert!(
            store.add(&rule("r1", "terminal.run", "git")).is_err(),
            "duplicate id must be rejected"
        );
    }

    #[test]
    fn remove_returns_false_for_missing_id() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        assert!(!store.remove("missing").unwrap());
        store
            .add(&rule("r1", "workspace.write_file", "src/"))
            .unwrap();
        assert!(store.remove("r1").unwrap());
        assert!(!store.remove("r1").unwrap());
    }

    #[test]
    fn matching_finds_path_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        store
            .add(&rule("r1", "workspace.write_file", "src/"))
            .unwrap();
        assert_eq!(
            store
                .matching("workspace.write_file", "src/main.rs")
                .unwrap()
                .map(|r| r.id),
            Some("r1".to_string())
        );
        // exact prefix also matches
        assert_eq!(
            store
                .matching("workspace.write_file", "src")
                .unwrap()
                .map(|r| r.id),
            Some("r1".to_string())
        );
    }

    #[test]
    fn matching_finds_exact_command() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        store.add(&rule("r1", "terminal.run", "git")).unwrap();
        assert_eq!(
            store.matching("terminal.run", "git").unwrap().map(|r| r.id),
            Some("r1".to_string())
        );
        // exact match only: "git-annex" must not match "git".
        assert!(store
            .matching("terminal.run", "git-annex")
            .unwrap()
            .is_none());
        // case-sensitive: "Git" must not match "git".
        assert!(store.matching("terminal.run", "Git").unwrap().is_none());
    }

    #[test]
    fn matching_returns_none_for_non_matching_or_other_tool() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        store
            .add(&rule("r1", "workspace.write_file", "src/"))
            .unwrap();
        // non-matching path
        assert!(store
            .matching("workspace.write_file", "docs/other")
            .unwrap()
            .is_none());
        // same resource, different tool
        assert!(store
            .matching("workspace.delete_file", "src/main.rs")
            .unwrap()
            .is_none());
        assert!(store
            .matching("terminal.run", "src/main.rs")
            .unwrap()
            .is_none());
    }

    #[test]
    fn add_rejects_unsupported_tool() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        for bad in ["memory.store", "git.commit", "workspace.read_file"] {
            assert!(
                store.add(&rule("r1", bad, "x")).is_err(),
                "tool {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn add_rejects_unsafe_id() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        for bad in ["../escape", "a/b", "a\\b", ""] {
            assert!(
                store.add(&rule(bad, "workspace.write_file", "x")).is_err(),
                "id {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn persists_as_single_json_array() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        store
            .add(&rule("r1", "workspace.write_file", "src/"))
            .unwrap();
        store.add(&rule("r2", "terminal.run", "git")).unwrap();
        let path = temp.path().join(".agent").join("policy.json");
        let content = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed.is_array(), "policy.json must be a JSON array");
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn matching_is_first_match_in_insertion_order() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        store
            .add(&rule("r1", "workspace.write_file", "src/"))
            .unwrap();
        store
            .add(&rule("r2", "workspace.write_file", "src/main"))
            .unwrap();
        let matched = store
            .matching("workspace.write_file", "src/main.rs")
            .unwrap();
        assert_eq!(matched.map(|r| r.id), Some("r1".to_string()));
    }

    #[test]
    fn path_prefix_normalization_collapses_slashes() {
        let temp = tempfile::tempdir().unwrap();
        let store = PolicyStore::new(temp.path());
        store
            .add(&rule("r1", "workspace.write_file", "src//"))
            .unwrap();
        assert!(store
            .matching("workspace.write_file", "src/main.rs")
            .unwrap()
            .is_some());
        // resource with doubled slashes also matches
        assert!(store
            .matching("workspace.write_file", "src//main.rs")
            .unwrap()
            .is_some());
        // empty pattern never matches
        store.add(&rule("r2", "workspace.delete_file", "")).unwrap();
        assert!(store
            .matching("workspace.delete_file", "anything")
            .unwrap()
            .is_none());
    }
}
