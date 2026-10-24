//! CLI-to-edit-service policy parity.
//!
//! The canonical `PolicyStore` supports every `filesystem.*` mutation. This
//! regression suite proves the operator CLI can create those same rules and
//! that a resulting denial happens before edit or recovery state is written.

use agent_workspace_hub::core::policy::PolicyStore;
use std::fs;
use std::path::Path;
use std::process::Output;
use tempfile::tempdir;

#[path = "common/mod.rs"]
mod common;

fn run(root: &Path, args: &[&str]) -> Output {
    common::sanitized_command(root, args)
        .output()
        .expect("spawn awh")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn record_count(path: &Path) -> usize {
    fs::read_dir(path)
        .map(|entries| entries.count())
        .unwrap_or(0)
}

#[test]
fn policy_cli_accepts_edit_tools_and_denied_edit_has_zero_side_effects() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let init = run(root, &["init"]);
    assert!(init.status.success(), "init failed: {}", stderr(&init));

    // Keep this list independent from the production constant: the test is
    // the cross-check that the CLI accepts every edit action supported by
    // EditAction/PolicyStore, rather than merely mirroring one list.
    let edit_tools = [
        "filesystem.replace",
        "filesystem.insert",
        "filesystem.delete_range",
        "filesystem.patch",
        "filesystem.apply_diff",
        "filesystem.rollback",
    ];
    for (index, tool) in edit_tools.iter().enumerate() {
        let id = format!("deny-edit-{index}");
        let pattern = if *tool == "filesystem.replace" {
            "src/"
        } else {
            "unused/"
        };
        let output = common::sanitized_command(root, &["policy", "deny", tool, pattern, "--id"])
            .arg(&id)
            .output()
            .expect("spawn policy deny");
        assert!(
            output.status.success(),
            "CLI rejected canonical edit tool {tool}: {}",
            stderr(&output)
        );
    }

    let rules = PolicyStore::new(root).list().expect("persisted rules");
    assert_eq!(rules.len(), edit_tools.len());
    for tool in edit_tools {
        assert!(rules.iter().any(|rule| rule.tool == tool), "missing {tool}");
    }

    fs::create_dir_all(root.join("src")).expect("src directory");
    fs::write(root.join("src/edit.txt"), "alpha\n").expect("edit fixture");
    let denied = run(root, &["fs", "replace", "src/edit.txt", "alpha", "ALPHA"]);

    assert_eq!(
        denied.status.code(),
        Some(3),
        "policy-denied edit should use the authorization exit code: {}",
        stderr(&denied)
    );
    assert!(
        stderr(&denied).contains("policy rule deny-edit-0"),
        "denial should identify the matching rule: {}",
        stderr(&denied)
    );
    assert_eq!(
        fs::read_to_string(root.join("src/edit.txt")).expect("target bytes"),
        "alpha\n",
        "a denied edit must preserve the target bytes"
    );
    for state in ["snapshots", "snapshot-contents", "provenance"] {
        assert_eq!(
            record_count(&root.join(".agent").join(state)),
            0,
            "a denied edit must not write {state} records"
        );
    }

    // Positive control: removing only the matching rule makes the identical
    // edit succeed, proving the denial above came from the CLI-created rule.
    let removed = run(root, &["policy", "remove", "deny-edit-0"]);
    assert!(
        removed.status.success(),
        "remove failed: {}",
        stderr(&removed)
    );
    let allowed = run(root, &["fs", "replace", "src/edit.txt", "alpha", "ALPHA"]);
    assert!(
        allowed.status.success(),
        "allow failed: {}",
        stderr(&allowed)
    );
    assert_eq!(
        fs::read_to_string(root.join("src/edit.txt")).expect("edited bytes"),
        "ALPHA\n"
    );
}
