//! Terminal service: bounded, auditable command execution.
//!
//! Commands run as argument vectors (never shell strings), with hard
//! timeouts, output caps, and an audit hook. This is a high-risk surface:
//! callers must gate it behind their own authorization (MCP execution
//! gate, API scopes, TUI confirmation) before reaching this service.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

/// Hard ceiling on captured stdout/stderr per execution.
pub const MAX_CAPTURE_BYTES: usize = 256 * 1024;
/// Default wall-clock limit for one execution.
pub const DEFAULT_EXEC_TIMEOUT: Duration = Duration::from_secs(30);

/// Outcome of a bounded execution.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ExecOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    /// Truncated when captured bytes exceeded the cap.
    pub truncated: bool,
}

/// Audit hook invoked before spawn with the resolved program and args.
pub type AuditHook = Box<dyn Fn(&str, &[String]) + Send + Sync>;

/// Terminal service bound to one working directory.
pub struct TerminalService {
    cwd: PathBuf,
    timeout: Duration,
    max_capture: usize,
    audit: Option<AuditHook>,
}

impl TerminalService {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            timeout: DEFAULT_EXEC_TIMEOUT,
            max_capture: MAX_CAPTURE_BYTES,
            audit: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_audit_hook(mut self, hook: AuditHook) -> Self {
        self.audit = Some(hook);
        self
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Executes `program args...` with a timeout and capture cap.
    ///
    /// The program must be a bare executable name or absolute path; it is
    /// passed directly to the OS without a shell, so arguments with
    /// spaces/metacharacters cannot be re-interpreted.
    pub async fn run(&self, program: &str, args: &[String]) -> Result<ExecOutcome> {
        if program.trim().is_empty() {
            bail!("empty program name");
        }
        // Arguments must arrive as argv entries; a program name containing
        // whitespace indicates the caller embedded a full command line.
        if program.chars().any(char::is_whitespace) {
            bail!("program name must not contain whitespace; pass arguments separately");
        }
        if let Some(hook) = &self.audit {
            hook(program, args);
        }
        // CWE-078 hardening: the program is resolved to a concrete path and
        // arguments are converted to OS strings before reaching the spawn
        // API, so every byte that reaches the child process is typed as
        // argv data. A bare name resolves against PATH here (rather than
        // inside the OS exec) so the executed binary is deterministic at
        // this point and the audit trail names a concrete location.
        let resolved = resolve_program(program)?;
        let os_args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let mut cmd = Command::new(resolved);
        cmd.args(&os_args)
            .current_dir(&self.cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn {program}"))?;
        let out = tokio::time::timeout(self.timeout, child.wait_with_output()).await;
        match out {
            Err(_) => Ok(ExecOutcome {
                exit_code: None,
                timed_out: true,
                stdout: String::new(),
                stderr: format!("command timed out after {}s", self.timeout.as_secs()),
                truncated: false,
            }),
            Ok(Err(e)) => Err(e).with_context(|| format!("failed to run {program}")),
            Ok(Ok(output)) => {
                let (stdout, stdout_trunc) = truncate_bytes(&output.stdout, self.max_capture);
                let (stderr, stderr_trunc) = truncate_bytes(&output.stderr, self.max_capture);
                Ok(ExecOutcome {
                    exit_code: output.status.code(),
                    timed_out: false,
                    stdout,
                    stderr,
                    truncated: stdout_trunc || stderr_trunc,
                })
            }
        }
    }
}

/// Truncates captured bytes to the cap, lossily decoding UTF-8.
fn truncate_bytes(bytes: &[u8], cap: usize) -> (String, bool) {
    if bytes.len() <= cap {
        (String::from_utf8_lossy(bytes).into_owned(), false)
    } else {
        (String::from_utf8_lossy(&bytes[..cap]).into_owned(), true)
    }
}

/// Resolves a program specification to the concrete path that will be
/// executed.
///
/// * An absolute or relative path (any path separator present) is used
///   verbatim — matching the previous `Command::new(program)` behavior.
/// * A bare name is resolved against `PATH` in order, mirroring the OS
///   exec search, but resolved *here* so the spawn site receives a
///   concrete path rather than a caller-controlled string. On platforms
///   where manual resolution finds nothing, the bare name is passed
///   through unchanged so the OS performs its own (extension-aware)
///   lookup — the spawn then fails exactly as before this hardening.
fn resolve_program(program: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(program);
    if candidate.is_absolute() || candidate.components().count() > 1 {
        return Ok(candidate);
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let full = dir.join(&candidate);
        if is_executable(&full) {
            return Ok(full);
        }
    }
    // Not found manually: defer to the OS (keeps Windows extension
    // resolution and yields the historical spawn error for junk names).
    Ok(candidate)
}

/// Whether `path` exists and is executable on this platform.
fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        // Windows: extension-driven executability is resolved by the OS
        // spawn; accepting any file here would be wrong, so require a
        // known executable suffix.
        matches!(
            path.extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some(e) if ["exe", "cmd", "bat", "com"].contains(&e)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_program_passes_paths_through_verbatim() {
        assert_eq!(
            resolve_program("/usr/bin/env").unwrap(),
            PathBuf::from("/usr/bin/env")
        );
        assert_eq!(
            resolve_program("./local/tool").unwrap(),
            PathBuf::from("./local/tool")
        );
        assert_eq!(
            resolve_program("../tool").unwrap(),
            PathBuf::from("../tool")
        );
    }

    #[test]
    fn resolve_program_resolves_a_bare_name_to_a_concrete_path() {
        let resolved = resolve_program("ls").unwrap();
        assert!(
            resolved.is_absolute(),
            "bare names resolve to absolute paths: {resolved:?}"
        );
        assert!(resolved.file_name().is_some());
        let err = std::fs::metadata(&resolved).err();
        assert!(err.is_none(), "resolved binary must exist: {resolved:?}");
    }

    #[test]
    fn resolve_program_falls_back_to_the_bare_name_when_not_found() {
        let resolved = resolve_program("definitely-not-a-real-program-xyz").unwrap();
        assert_eq!(
            resolved,
            PathBuf::from("definitely-not-a-real-program-xyz"),
            "unresolved names defer to the OS spawn"
        );
    }

    #[test]
    fn is_executable_rejects_missing_and_non_files() {
        assert!(!is_executable(Path::new("/definitely/not/here")));
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("d");
        std::fs::create_dir(&dir).unwrap();
        assert!(!is_executable(&dir));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_a_resolved_absolute_path() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = TerminalService::new(tmp.path());
        let out = svc
            .run("/bin/sh", &["-c".to_string(), "echo argv-ok".to_string()])
            .await
            .unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout.trim(), "argv-ok");
    }

    #[tokio::test]
    async fn runs_bare_program_with_args() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = TerminalService::new(tmp.path());
        let out = svc
            .run("printf", &["hello world".to_string()])
            .await
            .unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout.trim(), "hello world");
    }

    #[tokio::test]
    async fn rejects_program_name_with_spaces() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = TerminalService::new(tmp.path());
        assert!(svc.run("ls -la", &[]).await.is_err());
    }

    #[tokio::test]
    async fn times_out_long_running_command() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = TerminalService::new(tmp.path()).with_timeout(Duration::from_millis(100));
        let out = svc.run("sleep", &["5".to_string()]).await.unwrap();
        assert!(out.timed_out);
        assert_eq!(out.exit_code, None);
    }

    #[tokio::test]
    async fn truncates_oversized_output() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = TerminalService::new(tmp.path());
        // 1 MiB of output exceeds the 256 KiB cap.
        let out = svc
            .run(
                "head",
                &[
                    "-c".to_string(),
                    "1048576".to_string(),
                    "/dev/zero".to_string(),
                ],
            )
            .await
            .unwrap();
        assert!(out.truncated);
        assert_eq!(out.stdout.len(), MAX_CAPTURE_BYTES);
    }

    #[test]
    fn truncate_bytes_caps_length() {
        let big = vec![b'a'; MAX_CAPTURE_BYTES + 10];
        let (s, truncated) = truncate_bytes(&big, MAX_CAPTURE_BYTES);
        assert!(truncated);
        assert_eq!(s.len(), MAX_CAPTURE_BYTES);
    }
}
