//! Terminal service: bounded, auditable command execution.
//!
//! Commands run as argument vectors (never shell strings), with hard
//! timeouts, output caps, and an audit hook. This is a high-risk surface:
//! callers must gate it behind their own authorization (MCP execution
//! gate, API scopes, TUI confirmation) before reaching this service.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

/// Hard ceiling on captured stdout/stderr per execution.
pub const MAX_CAPTURE_BYTES: usize = 256 * 1024;
/// Default wall-clock limit for one execution.
pub const DEFAULT_EXEC_TIMEOUT: Duration = Duration::from_secs(30);

/// Outcome of a bounded execution.
#[derive(Debug, Clone, Serialize)]
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
        let mut cmd = Command::new(program);
        cmd.args(args)
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

#[cfg(test)]
mod tests {
    use super::*;

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
