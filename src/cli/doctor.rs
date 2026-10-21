//! Read-only diagnostics for a local AWH workspace.

use crate::core::policy::PolicyStore;
use crate::services::init::load_workspace_manifest;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

/// Inspect the workspace without creating or changing AWH state.
///
/// Returns a process exit code so callers can use `awh doctor` in setup
/// scripts and CI. Missing Git is only a warning because the workspace and
/// MCP features do not require a Git repository.
pub fn run(path: &Path) -> i32 {
    println!("Agent Workspace Hub doctor");

    let root = match fs::canonicalize(path) {
        Ok(root) if root.is_dir() => root,
        Ok(root) => {
            println!(
                "[FAIL] workspace path: not a directory ({})",
                root.display()
            );
            println!("Next: pass an existing directory with `--path`.");
            return 1;
        }
        Err(error) => {
            println!("[FAIL] workspace path: {} ({error})", path.display());
            println!("Next: create the directory or pass an existing path with `--path`.");
            return 1;
        }
    };
    println!("Workspace: {}", root.display());

    let mut failures = 0;
    let agent_dir = root.join(".agent");
    let agent_dir_is_symlink = match fs::symlink_metadata(&agent_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            failures += 1;
            println!("[FAIL] AWH state directory: .agent is a symlink");
            println!("       Keep AWH runtime state inside the workspace before using it.");
            true
        }
        Ok(_) => false,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            failures += 1;
            println!("[FAIL] AWH state directory: could not inspect .agent ({error})");
            true
        }
    };

    let manifest_ok = if agent_dir_is_symlink {
        false
    } else {
        match load_workspace_manifest(&root) {
            Ok(manifest) => {
                println!("[PASS] workspace manifest: {}", manifest.workspace_id);
                true
            }
            Err(error) => {
                failures += 1;
                println!("[FAIL] workspace manifest: {error:#}");
                println!("       Next: run `awh init --path \"{}\"`.", root.display());
                false
            }
        }
    };

    if !agent_dir_is_symlink && agent_dir.is_dir() {
        match PolicyStore::new(root.clone()).list() {
            Ok(rules) => println!("[PASS] policy store: {} deny rule(s)", rules.len()),
            Err(error) => {
                failures += 1;
                println!("[FAIL] policy store: {error:#}");
                println!("       Repair the policy JSON before using this workspace.");
            }
        }
    } else {
        println!("[SKIP] policy store: no local AWH state directory");
    }

    if agent_dir.is_dir() && !agent_dir_is_symlink {
        failures += check_git_protection(&root);
    } else {
        println!("[SKIP] Git protection: initialize the workspace to create .agent state");
    }

    if manifest_ok {
        println!("[INFO] MCP client setup is not inspected. Configure your agent to launch `awh mcp serve` in this workspace.");
    }

    if failures == 0 {
        println!("Result: workspace checks passed.");
        0
    } else {
        println!("Result: {failures} problem(s) found.");
        1
    }
}

fn check_git_protection(root: &Path) -> usize {
    let git_check = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match git_check {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("[WARN] Git is not installed; Git and worktree features need Git.");
            return 0;
        }
        Err(error) => {
            println!("[FAIL] Git check: could not run Git ({error})");
            return 1;
        }
        Ok(status) if !status.success() => {
            println!("[INFO] Git: this directory is not a repository; workspace and MCP features can still be used.");
            return 0;
        }
        Ok(_) => {}
    }

    let tracked = Command::new("git")
        .args(["ls-files", "--error-unmatch", "--", ".agent"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match tracked {
        Ok(status) if status.success() => {
            println!("[FAIL] Git protection: AWH state is already tracked by Git");
            println!("       Remove .agent from the index and keep it ignored.");
            return 1;
        }
        Err(error) => {
            println!("[FAIL] Git protection: could not inspect tracked files ({error})");
            return 1;
        }
        Ok(_) => {}
    }

    let ignored = Command::new("git")
        .args([
            "check-ignore",
            "--quiet",
            "--no-index",
            "--",
            ".agent/workspace.json",
        ])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match ignored {
        Ok(status) if status.success() => {
            println!("[PASS] Git protection: .agent runtime state is ignored and untracked");
            0
        }
        Ok(_) => {
            println!("[FAIL] Git protection: .agent runtime state is not ignored");
            println!(
                "       Add `/.agent/` to the workspace .gitignore, then run `awh doctor` again."
            );
            1
        }
        Err(error) => {
            println!("[FAIL] Git protection: could not check ignore rules ({error})");
            1
        }
    }
}
