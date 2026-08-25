use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use super::permissions::McpPermissions;

/// Linux sandbox policy for stdio MCP processes.
///
/// When enabled, execution is fail-closed if bubblewrap is unavailable.
/// Network permission currently means access to the host network namespace;
/// fine-grained egress control is intentionally left for a later phase.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub project_root: PathBuf,
    pub permissions: McpPermissions,
}

impl SandboxConfig {
    pub fn new(project_root: impl Into<PathBuf>, permissions: McpPermissions) -> Result<Self> {
        let cfg = Self { enabled: true, project_root: project_root.into(), permissions };
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if !self.project_root.is_absolute() {
            bail!("sandbox project root must be an absolute path");
        }
        self.permissions.validate()
    }
}

#[cfg(target_os = "linux")]
pub fn wrap_command(cfg: &SandboxConfig, command: &str, args: &[String]) -> Result<(String, Vec<String>)> {
    cfg.validate()?;
    if !cfg.enabled {
        return Ok((command.to_string(), args.to_vec()));
    }

    let bwrap = std::env::var("AWH_BWRAP").unwrap_or_else(|_| "bwrap".to_string());
    let available = std::process::Command::new(&bwrap).arg("--version").output()
        .map(|output| output.status.success()).unwrap_or(false);
    if !available {
        bail!("MCP sandbox is enabled but bubblewrap was not found; refusing unsandboxed execution");
    }

    let mut wrapped: Vec<String> = vec![
        "--die-with-parent".into(),
        "--new-session".into(),
        "--unshare-user".into(),
        "--unshare-pid".into(),
        "--unshare-ipc".into(),
        "--unshare-uts".into(),
        "--ro-bind".into(), "/usr".into(), "/usr".into(),
        "--proc".into(), "/proc".into(),
        "--dev".into(), "/dev".into(),
        "--tmpfs".into(), "/tmp".into(),
    ];

    if cfg.permissions.network {
        wrapped.push("--share-net".into());
    } else {
        wrapped.push("--unshare-net".into());
    }

    if !cfg.project_root.exists() {
        bail!("sandbox project root does not exist: {:?}", cfg.project_root);
    }
    wrapped.extend([
        "--bind".into(),
        cfg.project_root.to_string_lossy().into_owned(),
        cfg.project_root.to_string_lossy().into_owned(),
    ]);

    for path in &cfg.permissions.filesystem {
        let path = Path::new(path);
        if !path.is_absolute() {
            bail!("sandbox filesystem paths must be absolute: {path:?}");
        }
        if !path.exists() {
            bail!("sandbox filesystem path does not exist: {path:?}");
        }
        wrapped.extend([
            "--bind".into(),
            path.to_string_lossy().into_owned(),
            path.to_string_lossy().into_owned(),
        ]);
    }

    wrapped.push("--".into());
    wrapped.push(command.to_string());
    wrapped.extend(args.iter().cloned());
    Ok((bwrap, wrapped))
}

#[cfg(not(target_os = "linux"))]
pub fn wrap_command(cfg: &SandboxConfig, command: &str, args: &[String]) -> Result<(String, Vec<String>)> {
    if cfg.enabled {
        bail!("OS-level MCP sandbox is currently supported only on Linux");
    }
    Ok((command.to_string(), args.to_vec()))
}

pub fn sandbox_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        let bwrap = std::env::var("AWH_BWRAP").unwrap_or_else(|_| "bwrap".into());
        std::process::Command::new(bwrap).arg("--version").output()
            .map(|output| output.status.success()).unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    { false }
}
