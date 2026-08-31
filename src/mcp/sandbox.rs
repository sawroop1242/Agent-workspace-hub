use super::permissions::McpPermissions;
#[cfg(windows)]
use anyhow::Context;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// Resource limits enforced on a sandboxed MCP process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxLimits {
    /// Maximum address space (virtual memory) in bytes.
    pub address_space_bytes: u64,
    /// Maximum CPU time in seconds.
    pub cpu_seconds: u64,
    /// Maximum number of processes/threads.
    pub processes: u64,
    /// Maximum number of open file descriptors.
    pub open_files: u64,
}
impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            address_space_bytes: 2 * 1024 * 1024 * 1024,
            cpu_seconds: 300,
            processes: 128,
            open_files: 1024,
        }
    }
}
impl SandboxLimits {
    fn validate(&self) -> Result<()> {
        if self.address_space_bytes == 0
            || self.cpu_seconds == 0
            || self.processes == 0
            || self.open_files == 0
        {
            bail!("sandbox resource limits must be greater than zero");
        }
        Ok(())
    }
}

/// Configuration for sandboxing an MCP process.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Whether OS-level sandboxing is enabled.
    pub enabled: bool,
    /// The project root directory exposed to the sandbox.
    pub project_root: PathBuf,
    /// The permissions granted to the sandboxed process.
    pub permissions: McpPermissions,
    /// Resource limits enforced on the sandboxed process.
    pub limits: SandboxLimits,
}
impl SandboxConfig {
    /// Creates a sandbox config with default limits, enabling sandboxing.
    pub fn new(project_root: impl Into<PathBuf>, permissions: McpPermissions) -> Result<Self> {
        let cfg = Self {
            enabled: true,
            project_root: project_root.into(),
            permissions,
            limits: SandboxLimits::default(),
        };
        cfg.validate()?;
        Ok(cfg)
    }
    /// Validates the config: absolute project root, valid permissions and limits.
    pub fn validate(&self) -> Result<()> {
        // A disabled sandbox applies no root restrictions, so only validate the
        // project root when sandboxing is actually enabled.
        if self.enabled {
            if !self.project_root.is_absolute() {
                bail!("sandbox project root must be an absolute path");
            }
            if !self.project_root.exists() {
                bail!(
                    "sandbox project root does not exist: {:?}",
                    self.project_root
                );
            }
            if !self.project_root.is_dir() {
                bail!(
                    "sandbox project root must be a directory: {:?}",
                    self.project_root
                );
            }
        }
        if let Some(path) = self
            .permissions
            .filesystem
            .iter()
            .find(|p| !Path::new(p).is_absolute())
        {
            bail!("sandbox filesystem paths must be absolute: {path:?}");
        }
        self.permissions.validate()?;
        self.limits.validate()
    }
}

/// Wraps a command with `bwrap` (Linux) to enforce sandbox limits and permissions.
#[cfg(target_os = "linux")]
pub fn wrap_command(
    cfg: &SandboxConfig,
    command: &str,
    args: &[String],
) -> Result<(String, Vec<String>)> {
    cfg.validate()?;
    if !cfg.enabled {
        return Ok((command.to_string(), args.to_vec()));
    }
    let bwrap = std::env::var("AWH_BWRAP").unwrap_or_else(|_| "bwrap".to_string());
    let available = std::process::Command::new(&bwrap)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !available {
        bail!(
            "MCP sandbox is enabled but bubblewrap was not found; refusing unsandboxed execution"
        );
    }
    if !cfg.project_root.exists() {
        bail!(
            "sandbox project root does not exist: {:?}",
            cfg.project_root
        );
    }
    let mut wrapped = vec![
        "--die-with-parent".into(),
        "--new-session".into(),
        "--unshare-user".into(),
        "--unshare-pid".into(),
        "--unshare-ipc".into(),
        "--unshare-uts".into(),
        "--cap-drop".into(),
        "ALL".into(),
        "--rlimit-as".into(),
        cfg.limits.address_space_bytes.to_string(),
        "--rlimit-cpu".into(),
        cfg.limits.cpu_seconds.to_string(),
        "--rlimit-nproc".into(),
        cfg.limits.processes.to_string(),
        "--rlimit-nofile".into(),
        cfg.limits.open_files.to_string(),
        "--ro-bind".into(),
        "/usr".into(),
        "/usr".into(),
        "--proc".into(),
        "/proc".into(),
        "--dev".into(),
        "/dev".into(),
        "--tmpfs".into(),
        "/tmp".into(),
    ];
    if cfg.permissions.network {
        wrapped.push("--share-net".into());
    } else {
        wrapped.push("--unshare-net".into());
    }
    wrapped.extend([
        "--bind".into(),
        cfg.project_root.to_string_lossy().into_owned(),
        cfg.project_root.to_string_lossy().into_owned(),
    ]);
    for path in &cfg.permissions.filesystem {
        let path = Path::new(path);
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

/// Wraps a command with `sandbox-exec` (macOS) to enforce sandbox restrictions.
#[cfg(target_os = "macos")]
pub fn wrap_command(
    cfg: &SandboxConfig,
    command: &str,
    args: &[String],
) -> Result<(String, Vec<String>)> {
    cfg.validate()?;
    if !cfg.enabled {
        return Ok((command.to_string(), args.to_vec()));
    }
    ensure_command_available("sandbox-exec")?;
    if !cfg.project_root.exists() {
        bail!(
            "sandbox project root does not exist: {:?}",
            cfg.project_root
        );
    }
    let root = profile_quote(&cfg.project_root.to_string_lossy());
    let command_path = profile_quote(command);
    let mut profile = format!("(version 1)\n(deny default)\n(allow process*)\n(allow file-read* (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/Library\") (literal \"{}\"))\n(allow file-read* (subpath \"{}\"))\n(allow file-write* (subpath \"{}\"))\n(allow file-write* (subpath \"/tmp\"))\n", command_path, root, root);
    if cfg.permissions.network {
        profile.push_str("(allow network-outbound)\n");
    }
    let mut wrapped = vec!["-p".into(), profile, command.to_string()];
    wrapped.extend(args.iter().cloned());
    Ok(("sandbox-exec".into(), wrapped))
}

/// Windows applies limits via a Job Object rather than a wrapper command.
#[cfg(windows)]
pub fn wrap_command(
    cfg: &SandboxConfig,
    command: &str,
    args: &[String],
) -> Result<(String, Vec<String>)> {
    cfg.validate()?;
    if !cfg.enabled {
        return Ok((command.to_string(), args.to_vec()));
    }
    Ok((command.to_string(), args.to_vec()))
}

/// Rejects sandboxing on unsupported platforms rather than running unsandboxed.
#[cfg(all(not(target_os = "linux"), not(target_os = "macos"), not(windows)))]
pub fn wrap_command(
    cfg: &SandboxConfig,
    command: &str,
    args: &[String],
) -> Result<(String, Vec<String>)> {
    if cfg.enabled {
        bail!(
            "OS-level MCP sandbox is unsupported on this platform; refusing unsandboxed execution"
        );
    }
    Ok((command.to_string(), args.to_vec()))
}

/// A Windows Job Object handle that limits a sandboxed process and releases it on drop.
#[cfg(windows)]
pub struct WindowsJob {
    handle: usize,
}
#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(
                self.handle as windows_sys::Win32::Foundation::HANDLE,
            );
        }
    }
}
/// Applies sandbox limits to a running Windows child process via a Job Object.
#[cfg(windows)]
pub fn apply_windows_job(
    child: &tokio::process::Child,
    limits: &SandboxLimits,
) -> Result<WindowsJob> {
    use std::mem::zeroed;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    let process = child
        .raw_handle()
        .context("Windows MCP process handle unavailable")?;
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() || job == INVALID_HANDLE_VALUE {
        bail!("failed to create Windows Job Object");
    }
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_JOB_MEMORY
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    info.ProcessMemoryLimit = limits.address_space_bytes as usize;
    info.JobMemoryLimit = limits.address_space_bytes as usize;
    info.BasicLimitInformation.ActiveProcessLimit = limits.processes as u32;
    let result = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if result == 0 {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        bail!("failed to configure Windows MCP Job Object");
    }
    if unsafe { AssignProcessToJobObject(job, process as _) } == 0 {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        bail!("failed to assign MCP process to Windows Job Object");
    }
    Ok(WindowsJob {
        handle: job as usize,
    })
}

#[cfg(target_os = "macos")]
fn ensure_command_available(command: &str) -> Result<()> {
    let status = std::process::Command::new("/usr/bin/which")
        .arg(command)
        .status();
    if !status.map(|s| s.success()).unwrap_or(false) {
        bail!("MCP sandbox is enabled but {command} was not found; refusing unsandboxed execution");
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn profile_quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Reports whether OS-level sandboxing is available on this platform.
pub fn sandbox_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        let bwrap = std::env::var("AWH_BWRAP").unwrap_or_else(|_| "bwrap".into());
        std::process::Command::new(bwrap)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/which")
            .arg("sandbox-exec")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        true
    }
    #[cfg(all(not(target_os = "linux"), not(target_os = "macos"), not(windows)))]
    {
        false
    }
}
