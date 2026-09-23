//! Workspace initialization service (`awh init`, TW-001).
//!
//! [`initialize_workspace`] turns a directory into an AWH runtime boundary:
//! it records a durable [`WorkspaceId`] in `.agent/workspace.json` and
//! initializes the required stores in their valid empty state. The contract
//! is idempotent and fails closed:
//!
//! - fresh root → manifest created, `Created` returned;
//! - re-init → existing manifest is loaded and returned unchanged
//!   (`AlreadyInitialized`); the workspace id and all user configuration
//!   are preserved;
//! - malformed or unsupported-version manifest, or a manifest whose
//!   recorded root does not match the resolved directory, is an explicit
//!   error — never silently re-initialized;
//! - no agents and no capability grants are ever created here: init grants
//!   no implicit authority.

use crate::core::identity::WorkspaceId;
use crate::mcp::store_lock::StoreLock;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Schema version of the on-disk workspace manifest. Unknown future
/// versions fail closed on read rather than being silently accepted.
pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub version: u32,
    pub workspace_id: WorkspaceId,
    /// Canonical absolute path of the workspace root. Stored so that
    /// running `init` against a *different* directory containing this
    /// manifest is detected as a mistake rather than a fresh workspace.
    pub workspace_root: String,
    /// RFC3339 (UTC) timestamp of first initialization.
    pub created_at: String,
}

/// Result of [`initialize_workspace`].
#[derive(Debug, Clone)]
pub enum InitOutcome {
    /// A fresh workspace was initialized with this new manifest.
    Created(WorkspaceManifest),
    /// The workspace was already initialized; the existing manifest was
    /// loaded without modification.
    AlreadyInitialized(WorkspaceManifest),
}

impl InitOutcome {
    pub fn manifest(&self) -> &WorkspaceManifest {
        match self {
            Self::Created(m) | Self::AlreadyInitialized(m) => m,
        }
    }
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join(".agent").join("workspace.json")
}

/// Reads and validates the manifest at `path`, requiring it to describe the
/// canonical `root`. Corrupt JSON, an unsupported schema version, an
/// invalid workspace id, or a recorded root pointing elsewhere are all
/// explicit errors — a bad manifest is never treated as "no workspace".
fn read_manifest(path: &Path, root: &Path) -> Result<WorkspaceManifest> {
    let content = fs::read_to_string(path).context("failed to read workspace manifest")?;
    let manifest: WorkspaceManifest = serde_json::from_str(&content)
        .context("workspace manifest contains invalid JSON (corrupt state)")?;
    if manifest.version != MANIFEST_VERSION {
        bail!(
            "unsupported workspace manifest version {} (supported: {MANIFEST_VERSION})",
            manifest.version
        );
    }
    manifest
        .workspace_id
        .validate()
        .map_err(|e| anyhow::anyhow!("workspace manifest has an invalid workspace id: {e}"))?;
    let recorded = fs::canonicalize(&manifest.workspace_root).with_context(|| {
        format!(
            "recorded workspace root does not resolve: {}",
            manifest.workspace_root
        )
    })?;
    if recorded != root {
        bail!(
            "workspace manifest belongs to a different root ({}) than the requested directory ({})",
            recorded.display(),
            root.display()
        );
    }
    Ok(manifest)
}

/// Loads the persisted identity of the AWH workspace rooted at `root`,
/// reconstructing the same [`WorkspaceId`] across process restarts.
///
/// This is the reload entry point for any subsystem that needs to prove it
/// is operating inside an initialized workspace. It fails closed: an
/// uninitialized directory, a corrupt manifest, an unsupported schema
/// version, or a copied-in manifest describing another root are all errors.
pub fn load_workspace_manifest(root: &Path) -> Result<WorkspaceManifest> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve workspace root: {}", root.display()))?;
    let path = manifest_path(&root);
    if !path.exists() {
        bail!(
            "workspace is not initialized: {} (run `awh init` first)",
            root.display()
        );
    }
    read_manifest(&path, &root)
}

/// Initializes `root` (or confirms it is already initialized). See the
/// module docs for the contract.
pub fn initialize_workspace(root: &Path) -> Result<InitOutcome> {
    if root.exists() && !root.is_dir() {
        bail!("workspace root is not a directory: {}", root.display());
    }
    fs::create_dir_all(root)
        .with_context(|| format!("failed to create workspace: {}", root.display()))?;
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve workspace root: {}", root.display()))?;

    let agent_dir = root.join(".agent");
    fs::create_dir_all(&agent_dir).context("failed to create workspace state directory")?;

    let path = manifest_path(&root);
    // The manifest lock serializes the whole check-then-create init sequence
    // across processes; the policy store takes its own lock below so a
    // concurrent `awh policy deny` can never be lost to an init that is
    // (re)creating the empty store.
    let _lock = StoreLock::acquire(&path)?;
    ensure_policy_store(&root)?;

    if path.exists() {
        let manifest = read_manifest(&path, &root)?;
        return Ok(InitOutcome::AlreadyInitialized(manifest));
    }

    Ok(InitOutcome::Created(create_manifest(&root)?))
}

fn create_manifest(root: &Path) -> Result<WorkspaceManifest> {
    let manifest = WorkspaceManifest {
        version: MANIFEST_VERSION,
        workspace_id: WorkspaceId::new(),
        workspace_root: root.to_string_lossy().into_owned(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let path = manifest_path(root);
    let parent = path
        .parent()
        .context("workspace manifest has no parent directory")?;
    // Atomic write, same pattern as PolicyStore::save: tempfile in the
    // target directory, fsync, persist — never a torn manifest on disk.
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .context("failed to create workspace manifest temp file")?;
    std::io::Write::write_all(&mut temp, serde_json::to_vec_pretty(&manifest)?.as_slice())?;
    temp.as_file().sync_all()?;
    temp.persist(&path)
        .map_err(|error| error.error)
        .context("failed to atomically write workspace manifest")?;
    Ok(manifest)
}

/// Ensures `.agent/policy.json` exists in its valid empty state. An
/// existing file is loaded (any corruption surfaces as an explicit error)
/// and preserved byte-for-byte; a missing file gets the same empty store
/// [`crate::core::policy::PolicyStore`] would report for a fresh layout.
///
/// The store takes its own [`StoreLock`] (the same one `awh policy deny`
/// acquires) around the create-if-missing step, so a concurrent policy
/// write is never lost to a simultaneous init.
fn ensure_policy_store(root: &Path) -> Result<()> {
    let path = root.join(".agent").join("policy.json");
    let _lock = StoreLock::acquire(&path)?;
    if path.exists() {
        let content = fs::read_to_string(&path).context("failed to read policy store")?;
        serde_json::from_str::<serde_json::Value>(&content)
            .context("policy store contains invalid JSON")?;
        return Ok(());
    }
    let mut temp = tempfile::NamedTempFile::new_in(
        path.parent()
            .context("policy store has no parent directory")?,
    )
    .context("failed to create policy store temp file")?;
    std::io::Write::write_all(&mut temp, b"[\n]\n")?;
    temp.as_file().sync_all()?;
    temp.persist(&path)
        .map_err(|error| error.error)
        .context("failed to atomically write policy store")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_serde_round_trip() {
        let manifest = WorkspaceManifest {
            version: MANIFEST_VERSION,
            workspace_id: WorkspaceId::new(),
            workspace_root: "/tmp/ws".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let back: WorkspaceManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, back);
    }

    #[test]
    fn fresh_init_creates_manifest_and_empty_policy() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        let outcome = initialize_workspace(&root).unwrap();
        let InitOutcome::Created(manifest) = outcome else {
            panic!("expected Created");
        };
        assert!(manifest.workspace_id.as_str().starts_with("ws-"));
        assert!(manifest_path(root.canonicalize().unwrap().as_path()).exists());
        let policy = fs::read_to_string(root.join(".agent/policy.json")).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&policy).unwrap(),
            serde_json::json!([])
        );
    }

    #[test]
    fn reinit_is_idempotent_and_preserves_identity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        let first = initialize_workspace(&root).unwrap();
        let bytes_before = fs::read(manifest_path(root.canonicalize().unwrap().as_path())).unwrap();
        let second = initialize_workspace(&root).unwrap();
        let InitOutcome::AlreadyInitialized(m) = second else {
            panic!("expected AlreadyInitialized");
        };
        assert_eq!(m.workspace_id, first.manifest().workspace_id);
        let bytes_after = fs::read(manifest_path(root.canonicalize().unwrap().as_path())).unwrap();
        assert_eq!(bytes_before, bytes_after, "manifest must not be rewritten");
    }

    #[test]
    fn malformed_manifest_is_an_error_never_a_reset() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        initialize_workspace(&root).unwrap();
        let path = manifest_path(root.canonicalize().unwrap().as_path());
        fs::write(&path, "{ not valid json").unwrap();
        let err = initialize_workspace(&root).unwrap_err();
        assert!(format!("{err:#}").contains("invalid JSON"), "{err:#}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not valid json");
    }

    #[test]
    fn unsupported_manifest_version_is_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        initialize_workspace(&root).unwrap();
        let path = manifest_path(root.canonicalize().unwrap().as_path());
        let manifest = fs::read_to_string(&path)
            .unwrap()
            .replace("\"version\": 1", "\"version\": 99");
        fs::write(&path, manifest).unwrap();
        let err = initialize_workspace(&root).unwrap_err();
        assert!(
            format!("{err:#}").contains("unsupported workspace manifest version"),
            "{err:#}"
        );
    }

    #[test]
    fn manifest_root_mismatch_is_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        initialize_workspace(&a).unwrap();
        // Simulate an init invocation against a directory that contains a
        // manifest recorded for another root.
        fs::create_dir_all(b.join(".agent")).unwrap();
        fs::copy(
            manifest_path(a.canonicalize().unwrap().as_path()),
            manifest_path(b.canonicalize().unwrap().as_path()),
        )
        .unwrap();
        let err = initialize_workspace(&b).unwrap_err();
        assert!(format!("{err:#}").contains("different root"), "{err:#}");
    }

    #[test]
    fn root_that_is_a_file_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("not-a-dir");
        fs::write(&file, "x").unwrap();
        let err = initialize_workspace(&file).unwrap_err();
        assert!(format!("{err:#}").contains("not a directory"), "{err:#}");
    }

    #[test]
    fn corrupt_policy_store_blocks_init_without_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        fs::create_dir_all(root.join(".agent")).unwrap();
        fs::write(root.join(".agent/policy.json"), "broken").unwrap();
        let err = initialize_workspace(&root).unwrap_err();
        assert!(
            format!("{err:#}").contains("policy store contains invalid JSON"),
            "{err:#}"
        );
        assert!(!manifest_path(root.canonicalize().unwrap().as_path()).exists());
        assert_eq!(
            fs::read_to_string(root.join(".agent/policy.json")).unwrap(),
            "broken"
        );
    }

    #[test]
    fn manifest_with_invalid_workspace_id_is_rejected_without_mutation() {
        // A structurally valid manifest whose workspace_id fails typed
        // validation (traversal) must fail closed, never be adopted.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        initialize_workspace(&root).unwrap();
        let path = manifest_path(root.canonicalize().unwrap().as_path());
        let manifest = fs::read_to_string(&path).unwrap();
        let poisoned = manifest
            .split_once("\"workspace_id\": \"")
            .map(|(prefix, rest)| {
                let suffix = rest.split_once('"').map(|(_, s)| s).unwrap_or("");
                format!("{prefix}\"workspace_id\": \"../evil\"{suffix}")
            })
            .unwrap();
        assert_ne!(poisoned, manifest, "fixture must actually change the id");
        fs::write(&path, poisoned.clone()).unwrap();
        let err = initialize_workspace(&root).unwrap_err();
        assert!(
            format!("{err:#}").contains("invalid workspace id"),
            "{err:#}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), poisoned);
    }

    #[test]
    fn load_manifest_on_uninitialized_root_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ws");
        fs::create_dir_all(&root).unwrap();
        let err = load_workspace_manifest(&root).unwrap_err();
        assert!(format!("{err:#}").contains("not initialized"), "{err:#}");
        assert!(
            format!("{err:#}").contains("run `awh init` first"),
            "{err:#}"
        );
        // A nonexistent root also fails closed (it cannot contain a manifest).
        assert!(load_workspace_manifest(&temp.path().join("missing")).is_err());
        // A later real init still succeeds — the failed load mutated nothing.
        assert!(matches!(
            initialize_workspace(&root).unwrap(),
            InitOutcome::Created(_)
        ));
    }

    #[test]
    fn root_under_a_regular_file_is_rejected() {
        // `file/sub` can never become a directory; init must fail with a
        // deterministic error and leave no bootstrap state anywhere.
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file");
        fs::write(&file, "x").unwrap();
        let root = file.join("sub");
        let err = initialize_workspace(&root).unwrap_err();
        assert!(
            format!("{err:#}").contains("failed to create workspace"),
            "{err:#}"
        );
        assert!(!temp.path().join("sub").join(".agent").exists());
    }
}
