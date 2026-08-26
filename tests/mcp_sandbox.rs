use agent_workspace_hub::mcp::{wrap_command, McpPermissions, SandboxConfig, SandboxLimits};
use std::path::PathBuf;
use std::sync::Mutex;

static BWRAP_ENV_LOCK: Mutex<()> = Mutex::new(());

fn permissions() -> McpPermissions {
    McpPermissions::default()
}

#[test]
fn relative_project_root_is_rejected() {
    let result = SandboxConfig::new("relative/project", permissions());
    assert!(result.is_err());
}

#[test]
fn sandbox_requires_existing_absolute_project_root() {
    let root = std::env::temp_dir().join("awh-nonexistent-sandbox-root");
    let _ = std::fs::remove_dir_all(&root);
    let cfg = SandboxConfig {
        enabled: true,
        project_root: root,
        permissions: permissions(),
        limits: SandboxLimits::default(),
    };
    assert!(wrap_command(&cfg, "echo", &[]).is_err());
}

#[test]
fn relative_filesystem_path_is_rejected() {
    let root = std::env::temp_dir();
    let mut p = permissions();
    p.filesystem.push("relative/path".to_string());
    let cfg = SandboxConfig {
        enabled: true,
        project_root: root,
        permissions: p,
        limits: SandboxLimits::default(),
    };
    assert!(wrap_command(&cfg, "echo", &[]).is_err());
}

#[test]
fn disabled_sandbox_returns_original_command() {
    let cfg = SandboxConfig {
        enabled: false,
        project_root: PathBuf::from("/does/not/need/to/exist"),
        permissions: permissions(),
        limits: SandboxLimits::default(),
    };
    let args = vec!["hello".to_string()];
    let result = wrap_command(&cfg, "echo", &args).unwrap();
    assert_eq!(result.0, "echo");
    assert_eq!(result.1, args);
}

#[test]
fn default_limits_are_nonzero_and_bounded() {
    let limits = SandboxLimits::default();
    assert_eq!(limits.address_space_bytes, 2 * 1024 * 1024 * 1024);
    assert_eq!(limits.cpu_seconds, 300);
    assert_eq!(limits.processes, 128);
    assert_eq!(limits.open_files, 1024);
}

#[test]
fn zero_resource_limit_is_rejected() {
    let root = std::env::temp_dir();
    let cfg = SandboxConfig {
        enabled: true,
        project_root: root,
        permissions: permissions(),
        limits: SandboxLimits {
            address_space_bytes: 0,
            ..SandboxLimits::default()
        },
    };
    assert!(cfg.validate().is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn enabled_linux_sandbox_fails_closed_without_bwrap() {
    let _guard = BWRAP_ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir();
    let cfg = SandboxConfig {
        enabled: true,
        project_root: root,
        permissions: permissions(),
        limits: SandboxLimits::default(),
    };
    std::env::set_var("AWH_BWRAP", "/definitely/missing/bwrap");
    let result = wrap_command(&cfg, "echo", &[]);
    std::env::remove_var("AWH_BWRAP");
    assert!(result.is_err());
}
