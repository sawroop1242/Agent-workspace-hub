use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use super::permissions::McpPermissions;

/// Resource limits applied to MCP processes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxLimits {
    /// Maximum address space in bytes.
    pub address_space_bytes: u64,
    /// Maximum CPU time in seconds.
    pub cpu_seconds: u64,
    /// Maximum number of processes/threads.
    pub processes: u64,
    /// Maximum number of open file descriptors where the host sandbox supports it.
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

/// Cross-platform sandbox policy for stdio MCP processes.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub project_root: PathBuf,
    pub permissions: McpPermissions,
    pub limits: SandboxLimits,
}

impl SandboxConfig {
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

    pub fn validate(&self) -> Result<()> {
        if !self.project_root.is_absolute() {
            bail!("sandbox project root must be an absolute path");
        }
        if self.project_root.exists() && !self.project_root.is_dir() {
            bail!("sandbox project root must be a directory: {:?}", self.project_root);
        }
        self.permissions.validate()?;
        self.limits.validate()
    }
}

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
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        bail!("MCP sandbox is enabled but bubblewrap was not found; refusing unsandboxed execution");
    }
    if !cfg.project_root.exists() {
        bail!("sandbox project root does not exist: {:?}", cfg.project_root);
    }

    let mut wrapped: Vec<String> = vec![
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
        bail!("sandbox project root does not exist: {:?}", cfg.project_root);
    }

    let root = profile_quote(&cfg.project_root.to_string_lossy());
    let command_path = profile_quote(command);
    let mut profile = format!(
        "(version 1)\n(deny default)\n(allow process*)\n(allow file-read* (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/Library\") (literal \"{}\"))\n(allow file-read* (subpath \"{}\"))\n(allow file-write* (subpath \"{}\"))\n(allow file-write* (subpath \"/tmp\"))\n",
        command_path, root, root
    );
    if cfg.permissions.network {
        profile.push_str("(allow network-outbound)\n");
    }

    let mut wrapped = vec!["-p".into(), profile, command.to_string()];
    wrapped.extend(args.iter().cloned());
    Ok(("sandbox-exec".into(), wrapped))
}

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

#[cfg(all(not(target_os = "linux"), not(target_os = "macos"), not(windows)))]
pub fn wrap_command(
    cfg: &SandboxConfig,
    command: &str,
    args: &[String],
) -> Result<(String, Vec<String>)> {
    if cfg.enabled {
        bail!("OS-level MCP sandbox is unsupported on this platform; refusing unsandboxed execution");
    }
    Ok((command.to_string(), args.to_vec()))
}

#[cfg(windows)]
pub struct WindowsJob {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
pub fn apply_windows_job(
    child: &tokio::process::Child,
    limits: &SandboxLimits,
) -> Result<WindowsJob> {
    use std::mem::{size_of, zeroed};
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
    if job == 0 || job == INVALID_HANDLE_VALUE as isize {
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
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
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

    Ok(WindowsJob { handle: job })
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

pub fn sandbox_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        let bwrap = std::env::var("AWH_BWRAP").unwrap_or_else(|_| "bwrap".into());
        std::process::Command::new(bwrap)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/which")
            .arg("sandbox-exec")
            .status()
            .map(|status| status.success())
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
