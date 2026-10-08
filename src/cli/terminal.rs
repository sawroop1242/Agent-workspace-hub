//! `awh terminal` (TRM-001): thin CLI adapter over the canonical
//! [`crate::services::terminal::TerminalService`] — the same authority
//! the MCP `terminal.run` tool, the Control API `/terminal/run` route,
//! and the TUI terminal screen use. The CLI owns argument parsing,
//! output formatting, and exit codes only: argv validation, the
//! wall-clock timeout, and the 256 KiB capture cap live in the service,
//! and every run is audited (program name and exit status only — never
//! arguments or captured output, which can carry secrets).
//!
//! Lifecycle is deliberately ephemeral (TRM-001 §6): `terminal run`
//! executes bounded, synchronous commands that complete within the
//! command itself, so AWH tracks no long-lived OS processes.
//! `terminal list` therefore reports an empty list and `terminal kill`
//! fails deterministically on any id — documented explicitly instead of
//! pretending an OS process is durable AWH state.

use crate::services::audit;
use crate::services::terminal::{TerminalService, DEFAULT_EXEC_TIMEOUT};
use anyhow::{bail, Context as _, Result};
use clap::Subcommand;
use std::time::Duration;

/// Hard upper bound for `--timeout`: a CLI run stays bounded even when
/// the operator overrides the 30 s service default.
const MAX_TIMEOUT_SECS: u64 = 600;
/// `timeout(1)` convention: the CLI exits 124 when the run timed out.
const TIMED_OUT_EXIT_CODE: i32 = 124;

#[derive(Debug, Subcommand)]
pub enum TerminalCommand {
    /// Run a bounded command in the current workspace (argv form, no shell).
    ///
    /// The program is the first entry after `--`; everything after it is
    /// passed as separate argv entries, so shell metacharacters stay
    /// data. The child's stdout/stderr go to this command's stdout/stderr;
    /// the exit status is the child's own (124 on timeout, 1 on spawn
    /// failure).
    Run {
        /// Wall-clock limit in seconds (1..=600; default 30).
        #[arg(long)]
        timeout: Option<u64>,
        /// Program and arguments, argv-style: `awh terminal run -- printf hi`.
        #[arg(trailing_var_arg = true, required = true)]
        argv: Vec<String>,
    },
    /// List tracked terminal executions.
    ///
    /// Lifecycle is ephemeral (TRM-001 §6): executions complete within
    /// `terminal run`, so a live `awh` process tracks none.
    List,
    /// Terminate a tracked execution by its AWH execution id.
    ///
    /// Lifecycle is ephemeral (TRM-001 §6), so any id resolves
    /// deterministically to "unknown execution id" — a kill never signals
    /// a raw PID.
    Kill {
        /// AWH execution id.
        id: String,
    },
}

pub fn handle_terminal_cli(root: &std::path::Path, command: TerminalCommand) -> Result<()> {
    match command {
        TerminalCommand::Run { timeout, argv } => run(root, timeout, argv),
        TerminalCommand::List => {
            println!("[]");
            eprintln!(
                "(no live executions — terminal lifecycle is ephemeral: \
                 `awh terminal run` completes within the command)"
            );
            Ok(())
        }
        TerminalCommand::Kill { id } => bail!(
            "unknown execution id: {id} (terminal lifecycle is ephemeral — \
             executions complete within `awh terminal run`, so there is nothing to kill)"
        ),
    }
}

fn run(root: &std::path::Path, timeout: Option<u64>, argv: Vec<String>) -> Result<()> {
    let Some((program, args)) = argv.split_first() else {
        bail!("no program given: `awh terminal run -- <program> [args…]`");
    };
    let timeout_secs = timeout.unwrap_or(DEFAULT_EXEC_TIMEOUT.as_secs());
    if timeout_secs == 0 || timeout_secs > MAX_TIMEOUT_SECS {
        bail!("--timeout must be between 1 and {MAX_TIMEOUT_SECS} seconds");
    }
    let service = TerminalService::new(root).with_timeout(Duration::from_secs(timeout_secs));
    // Same audit shape as the TUI and Control API operator planes:
    // program name as subject, exit status as detail, args never logged.
    audit::global().record(
        "allow",
        "cli_terminal_run",
        program,
        &format!("timeout {timeout_secs}s"),
    );
    let outcome = tokio::runtime::Runtime::new()
        .context("failed to start the execution runtime")?
        .block_on(service.run(program, args))?;

    // Child output goes to the CLI's stdout/stderr, unmixed.
    print!("{}", outcome.stdout);
    eprint!("{}", outcome.stderr);
    if outcome.truncated {
        eprintln!("(output truncated at the 256 KiB capture cap)");
    }
    // process::exit skips destructors, so flush the pipes explicitly or
    // the last buffered stdout bytes die with the process.
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    if outcome.timed_out {
        std::process::exit(TIMED_OUT_EXIT_CODE);
    }
    if let Some(code) = outcome.exit_code {
        if code != 0 {
            std::process::exit(code);
        }
    }
    Ok(())
}
