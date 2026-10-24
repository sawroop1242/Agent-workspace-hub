//! Executable acceptance coverage for the read-only `awh doctor` path.
//!
//! Every workspace-mutating setup step happens before the snapshot used by
//! the assertion. The doctor invocation itself must preserve every file,
//! directory, and symlink in the workspace (including `.agent` and `.git`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::tempdir;

#[path = "common/mod.rs"]
mod common;

fn run(root: &Path, args: &[&str]) -> Output {
    common::sanitized_command(root, args)
        .output()
        .expect("spawn awh doctor")
}

fn init(root: &Path) {
    let output = run(root, &["init"]);
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_git(root: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[derive(Debug, PartialEq, Eq)]
enum TreeEntry {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, TreeEntry> {
    fn visit(workspace: &Path, directory: &Path, entries: &mut BTreeMap<PathBuf, TreeEntry>) {
        for entry in fs::read_dir(directory).expect("read workspace directory") {
            let entry = entry.expect("workspace directory entry");
            let path = entry.path();
            let relative = path
                .strip_prefix(workspace)
                .expect("entry stays under workspace")
                .to_path_buf();
            let metadata = fs::symlink_metadata(&path).expect("workspace entry metadata");
            if metadata.file_type().is_symlink() {
                entries.insert(
                    relative,
                    TreeEntry::Symlink(fs::read_link(&path).expect("read symlink")),
                );
            } else if metadata.is_dir() {
                entries.insert(relative, TreeEntry::Directory);
                visit(workspace, &path, entries);
            } else if metadata.is_file() {
                entries.insert(
                    relative,
                    TreeEntry::File(fs::read(&path).expect("read file")),
                );
            }
        }
    }

    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

fn assert_read_only(root: &Path, before: BTreeMap<PathBuf, TreeEntry>) {
    assert_eq!(
        snapshot(root),
        before,
        "doctor must not create, remove, or modify workspace state"
    );
}

fn assert_failure(output: &Output, expected: &str) {
    assert_eq!(
        output.status.code(),
        Some(1),
        "doctor should exit 1: stdout={} stderr={}",
        stdout(output),
        stderr(output)
    );
    assert!(
        stdout(output).contains(expected),
        "missing {expected:?} in doctor output: {}",
        stdout(output)
    );
}

#[test]
fn doctor_is_advertised_by_the_real_cli() {
    let dir = tempdir().expect("tempdir");
    let before = snapshot(dir.path());
    let output = run(dir.path(), &["--help"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(output).contains("doctor"), "{}", stdout(&output));
    assert_read_only(dir.path(), before);
}

#[test]
fn uninitialized_workspace_fails_without_creating_state() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("keep.txt"), "unchanged").expect("fixture");
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["doctor"]);

    assert_failure(&output, "workspace is not initialized");
    assert!(stdout(&output).contains("awh init"), "{}", stdout(&output));
    assert!(!dir.path().join(".agent").exists());
    assert_read_only(dir.path(), before);
}

#[test]
fn valid_workspace_outside_git_passes_without_mutation() {
    let dir = tempdir().expect("tempdir");
    init(dir.path());
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["doctor", "--path", "."]);

    assert!(
        output.status.success(),
        "doctor failed: stdout={} stderr={}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stdout(&output).contains("[PASS] workspace manifest"));
    assert!(stdout(&output).contains("[PASS] policy store"));
    assert!(stdout(&output).contains("not a repository"));
    assert!(stdout(&output).contains("Result: workspace checks passed."));
    assert_read_only(dir.path(), before);
}

#[test]
fn invalid_manifest_fails_and_preserves_corrupt_bytes() {
    let dir = tempdir().expect("tempdir");
    init(dir.path());
    fs::write(dir.path().join(".agent/workspace.json"), "{ corrupt").expect("corrupt");
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["doctor"]);

    assert_failure(&output, "workspace manifest");
    assert!(
        stdout(&output).contains("invalid JSON"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(".agent/workspace.json")).expect("manifest"),
        "{ corrupt"
    );
    assert_read_only(dir.path(), before);
}

#[test]
fn invalid_policy_fails_and_preserves_corrupt_bytes() {
    let dir = tempdir().expect("tempdir");
    init(dir.path());
    fs::write(dir.path().join(".agent/policy.json"), "{ corrupt").expect("corrupt");
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["doctor"]);

    assert_failure(&output, "policy store");
    assert!(
        stdout(&output).contains("invalid JSON"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(".agent/policy.json")).expect("policy"),
        "{ corrupt"
    );
    assert_read_only(dir.path(), before);
}

#[test]
fn missing_git_is_a_warning_not_a_workspace_failure() {
    let dir = tempdir().expect("tempdir");
    let empty_path = tempdir().expect("empty PATH directory");
    init(dir.path());
    let before = snapshot(dir.path());

    let output = common::sanitized_command(dir.path(), &["doctor"])
        .env("PATH", empty_path.path())
        .output()
        .expect("spawn awh doctor without git on PATH");

    assert!(
        output.status.success(),
        "missing Git must be non-fatal: stdout={} stderr={}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stdout(&output).contains("[WARN] Git is not installed"));
    assert!(stdout(&output).contains("Result: workspace checks passed."));
    assert_read_only(dir.path(), before);
}

#[cfg(unix)]
#[test]
fn symlinked_agent_directory_is_rejected_without_being_followed() {
    let dir = tempdir().expect("tempdir");
    init(dir.path());
    fs::rename(dir.path().join(".agent"), dir.path().join("agent-state")).expect("move state");
    std::os::unix::fs::symlink("agent-state", dir.path().join(".agent")).expect("symlink");
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["doctor"]);

    assert_failure(&output, ".agent is a symlink");
    assert_read_only(dir.path(), before);
}

#[test]
fn tracked_agent_state_is_rejected_without_touching_the_index() {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init", "--quiet"]);
    init(dir.path());
    run_git(dir.path(), &["add", "-f", ".agent/workspace.json"]);
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["doctor"]);

    assert_failure(&output, "already tracked by Git");
    assert_read_only(dir.path(), before);
}

#[test]
fn missing_agent_ignore_rule_is_rejected_without_editing_gitignore() {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init", "--quiet"]);
    init(dir.path());
    fs::write(dir.path().join(".gitignore"), "target/\n").expect("remove AWH ignore rule");
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["doctor"]);

    assert_failure(&output, "runtime state is not ignored");
    assert!(
        stdout(&output).contains("Add `/.agent/`"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(".gitignore")).expect("gitignore"),
        "target/\n"
    );
    assert_read_only(dir.path(), before);
}

#[test]
fn valid_git_workspace_passes_ignore_and_tracking_checks() {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init", "--quiet"]);
    init(dir.path());
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["doctor"]);

    assert!(
        output.status.success(),
        "doctor failed: stdout={} stderr={}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stdout(&output).contains("ignored and untracked"));
    assert_read_only(dir.path(), before);
}
