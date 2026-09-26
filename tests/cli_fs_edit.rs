//! AWE-014 real-binary tests: the `awh fs` editing/recovery family run
//! against the compiled executable in isolated temporary workspaces,
//! asserting exit status, stdout/stderr separation, exact filesystem
//! bytes, and durable downstream state (provenance/audit) — never just
//! the command's own output.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

struct Workspace {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

impl Workspace {
    fn new() -> Self {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        run_in(&root, &["init", "--path", "."]);
        Self { root, _dir: dir }
    }

    fn write(&self, path: &str, content: &str) {
        std::fs::write(self.root.join(path), content).expect("write fixture");
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path)).expect("read result")
    }

    /// Runs the command and asserts + returns its stdout text.
    fn ok(&self, args: &[&str]) -> String {
        let output = run_in(&self.root, args);
        assert!(
            output.status.success(),
            "expected success, exit {:?}: stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf-8 stdout")
    }

    /// Runs the command and asserts it fails with exactly this exit code.
    fn fails(&self, args: &[&str], code: i32) -> Output {
        let output = run_in(&self.root, args);
        assert_eq!(
            output.status.code(),
            Some(code),
            "expected exit {code}, got {:?}: stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    /// Extracts the edit id from a JSON-mode result line.
    fn edit_id_of(&self, stdout: &str) -> String {
        let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json line");
        value["edit_id"].as_str().expect("edit_id field").to_owned()
    }
}

fn run_in(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("run awh binary")
}

// ---------------------------------------------------------------------------
// Editing correctness through the real binary (§55)
// ---------------------------------------------------------------------------

#[test]
fn fs_replace_exact_bytes_and_unicode() {
    let ws = Workspace::new();
    ws.write("edit.txt", "alpha\nbeta\n");
    let stdout = ws.ok(&["fs", "replace", "edit.txt", "alpha", "ALPHA"]);
    assert!(stdout.contains("committed"), "{stdout}");
    assert_eq!(ws.read("edit.txt"), "ALPHA\nbeta\n");

    // Unicode / Devanagari / emoji round-trip byte-exactly.
    ws.write("uni.txt", "नमस्ते 🌍\nsecond ✅\n");
    ws.ok(&["fs", "replace", "uni.txt", "नमस्ते", "नमस्ते दुनिया"]);
    assert_eq!(ws.read("uni.txt"), "नमस्ते दुनिया 🌍\nsecond ✅\n");
}

#[test]
fn fs_replace_occurrence_semantics() {
    let ws = Workspace::new();
    ws.write("multi.txt", "x\nx\nx\n");
    ws.ok(&["fs", "replace", "multi.txt", "x", "y", "--occurrence", "2"]);
    assert_eq!(ws.read("multi.txt"), "x\ny\nx\n");

    // Out-of-range occurrence → structured failure, zero mutation.
    ws.fails(
        &["fs", "replace", "multi.txt", "x", "z", "--occurrence", "9"],
        1,
    );
    assert_eq!(ws.read("multi.txt"), "x\ny\nx\n");
}

#[test]
fn fs_insert_and_delete_range_line_semantics() {
    let ws = Workspace::new();
    ws.write("lines.txt", "one\ntwo\n");
    ws.ok(&["fs", "insert", "lines.txt", "0", "top"]);
    assert_eq!(ws.read("lines.txt"), "top\none\ntwo\n");
    ws.ok(&[
        "fs",
        "delete-range",
        "lines.txt",
        "--from",
        "1",
        "--to",
        "2",
    ]);
    assert_eq!(ws.read("lines.txt"), "two\n");

    // Invalid range (start > end) → usage failure, zero mutation.
    ws.fails(
        &[
            "fs",
            "delete-range",
            "lines.txt",
            "--from",
            "2",
            "--to",
            "1",
        ],
        1,
    );
    assert_eq!(ws.read("lines.txt"), "two\n");
}

#[test]
fn fs_patch_and_apply_diff() {
    let ws = Workspace::new();
    ws.write("a.txt", "alpha\n");
    ws.write("b.txt", "beta\n");
    std::fs::write(
        ws.root.join("ops.json"),
        r#"{"operations":[
            {"path":"a.txt","old":"alpha","new":"ALPHA"},
            {"path":"b.txt","old":"beta","new":"BETA"}
        ]}"#,
    )
    .unwrap();
    ws.ok(&["fs", "patch", "ops.json"]);
    assert_eq!(ws.read("a.txt"), "ALPHA\n");
    assert_eq!(ws.read("b.txt"), "BETA\n");

    ws.write("diff.txt", "x\n");
    std::fs::write(
        ws.root.join("patch.diff"),
        "--- a/diff.txt\n+++ b/diff.txt\n@@ -1 +1 @@\n-x\n+X\n",
    )
    .unwrap();
    ws.ok(&["fs", "apply-diff", "patch.diff"]);
    assert_eq!(ws.read("diff.txt"), "X\n");

    // Malformed patch JSON → usage failure, zero mutation.
    std::fs::write(
        ws.root.join("bad.json"),
        "{\"operations\": [{\"path\":\"a.txt\"}]}",
    )
    .unwrap();
    ws.fails(&["fs", "patch", "bad.json"], 2);
    assert_eq!(ws.read("a.txt"), "ALPHA\n");
}

#[test]
fn fs_expected_state_conflict_blocks_stale_overwrite() {
    let ws = Workspace::new();
    ws.write("stale.txt", "original\n");
    // Stale hash → conflict (exit 4), no stale overwrite, bytes intact.
    ws.fails(
        &[
            "fs",
            "replace",
            "stale.txt",
            "original",
            "changed",
            "--expected-hash",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ],
        4,
    );
    assert_eq!(ws.read("stale.txt"), "original\n");

    // Correct expected size + lines → succeeds.
    let output = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args([
            "fs",
            "replace",
            "stale.txt",
            "original",
            "changed",
            "--expected-size",
            "9",
            "--expected-lines",
            "1",
        ])
        .current_dir(&ws.root)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(ws.read("stale.txt"), "changed\n");
}

// ---------------------------------------------------------------------------
// Rollback / verify / history through the real binary (§56)
// ---------------------------------------------------------------------------

#[test]
fn fs_rollback_restores_exact_bytes_and_is_idempotent() {
    let ws = Workspace::new();
    ws.write("edit.txt", "alpha\nbeta\n");
    let stdout = ws.ok(&["fs", "replace", "edit.txt", "alpha", "ALPHA", "--json"]);
    let edit_id = ws.edit_id_of(&stdout);
    assert_eq!(ws.read("edit.txt"), "ALPHA\nbeta\n");

    // fs verify confirms the durable recovery chain, read-only.
    ws.ok(&["fs", "verify", &edit_id]);

    // Rollback restores exact bytes.
    ws.ok(&["fs", "rollback", &edit_id]);
    assert_eq!(ws.read("edit.txt"), "alpha\nbeta\n");

    // Repeated rollback → already_rolled_back, exit 0.
    let stdout = ws.ok(&["fs", "rollback", &edit_id]);
    assert!(stdout.contains("already_rolled_back"), "{stdout}");
}

#[test]
fn fs_rollback_conflict_preserves_newer_bytes_and_exits_nonzero() {
    let ws = Workspace::new();
    ws.write("edit.txt", "original\n");
    let stdout = ws.ok(&["fs", "replace", "edit.txt", "original", "edited", "--json"]);
    let edit_id = ws.edit_id_of(&stdout);
    // External actor overwrites after the edit.
    ws.write("edit.txt", "external newer state\n");

    let output = ws.fails(&["fs", "rollback", &edit_id], 4);
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("conflict"),
        "canonical conflict status on stdout"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("did not restore"),
        "diagnostic on stderr"
    );
    assert_eq!(ws.read("edit.txt"), "external newer state\n");
}

#[test]
fn fs_history_and_verify_failures() {
    let ws = Workspace::new();
    ws.write("h.txt", "one\n");
    let stdout = ws.ok(&["fs", "replace", "h.txt", "one", "two", "--json"]);
    let edit_id = ws.edit_id_of(&stdout);

    // History lists the edit, newest first, no file contents.
    let stdout = ws.ok(&["fs", "history"]);
    assert!(stdout.contains(&edit_id), "{stdout}");
    assert!(
        !stdout.contains("two\none") && !stdout.contains("file contents"),
        "history must not dump file content"
    );

    // Unknown edit id → recovery failure (5), no mutation. A missing
    // provenance record IS a recovery-material failure by the canonical
    // error taxonomy.
    ws.fails(&["fs", "verify", "edit-does-not-exist"], 5);
    ws.fails(&["fs", "rollback", "edit-does-not-exist"], 5);
    assert_eq!(ws.read("h.txt"), "two\n");

    // Corrupting the durable provenance record fails closed.
    let provenance = ws.root.join(".agent").join("provenance");
    let path = provenance.join(format!("{edit_id}.json"));
    assert!(path.exists(), "durable provenance exists after edit");
    std::fs::write(&path, b"{corrupt").unwrap();
    ws.fails(&["fs", "verify", &edit_id], 5);
    ws.fails(&["fs", "history"], 5);
    // The live file was never touched by the corruption.
    assert_eq!(ws.read("h.txt"), "two\n");
}

// ---------------------------------------------------------------------------
// Security / adversarial through the real binary (§54, §53)
// ---------------------------------------------------------------------------

#[test]
fn fs_traversal_and_unsafe_paths_fail_closed() {
    let ws = Workspace::new();
    ws.write("inside.txt", "safe\n");
    ws.fails(&["fs", "replace", "../escape.txt", "a", "b"], 3);
    ws.fails(&["fs", "replace", "/abs/path.txt", "a", "b"], 3);
    assert_eq!(ws.read("inside.txt"), "safe\n");
    // No file was created outside the workspace.
    assert!(!ws.root.parent().unwrap().join("escape.txt").exists());
}

#[test]
fn fs_usage_failures_never_mutate() {
    let ws = Workspace::new();
    ws.write("guard.txt", "keep\n");

    // Missing arguments → clap usage failure (2).
    ws.fails(&["fs", "replace", "guard.txt", "keep"], 2);
    // Wrong types → usage failure (2).
    ws.fails(
        &[
            "fs",
            "delete-range",
            "guard.txt",
            "--from",
            "one",
            "--to",
            "2",
        ],
        2,
    );
    // Nonexistent patch file → usage failure (2).
    ws.fails(&["fs", "patch", "no-such-file.json"], 2);

    assert_eq!(ws.read("guard.txt"), "keep\n");
}

#[test]
fn fs_requires_initialized_workspace() {
    let dir = tempdir().unwrap();
    let output = run_in(dir.path(), &["fs", "replace", "x.txt", "a", "b"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("workspace"),
        "uninitialized workspace fails closed"
    );
}

// ---------------------------------------------------------------------------
// Workspace isolation (§37) + machine output (§22) + restart (§58)
// ---------------------------------------------------------------------------

#[test]
fn fs_workspaces_are_isolated() {
    let ws_a = Workspace::new();
    let ws_b = Workspace::new();
    ws_a.write("edit.txt", "A-content\n");
    ws_b.write("edit.txt", "B-content\n");

    let stdout = ws_a.ok(&[
        "fs",
        "replace",
        "edit.txt",
        "A-content",
        "A-changed",
        "--json",
    ]);
    let edit_a = ws_a.edit_id_of(&stdout);

    // A's edit id cannot roll back B's file: the durable provenance is
    // workspace-rooted, so B's store has no record — a recovery failure.
    let output = ws_b.fails(&["fs", "rollback", &edit_a], 5);
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("restored"),
        "A's edit must not restore in workspace B"
    );
    assert_eq!(ws_b.read("edit.txt"), "B-content\n");
    // A's file stays as A left it; B untouched.
    assert_eq!(ws_a.read("edit.txt"), "A-changed\n");

    // A's history never exposes B's edits.
    let history = ws_a.ok(&["fs", "history"]);
    assert!(history.contains(&edit_a));
    assert_eq!(history.lines().count(), 1, "only A's edit is listed");
}

#[test]
fn fs_json_output_is_parseable_and_streams_stay_clean() {
    let ws = Workspace::new();
    ws.write("j.txt", "data\n");
    let stdout = ws.ok(&["fs", "replace", "j.txt", "data", "DATA", "--json"]);
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("machine output is valid JSON");
    assert_eq!(value["status"], "committed");
    assert!(value["edit_id"].is_string());
    assert_eq!(value["path"], "j.txt");

    // Machine-readable stdout contains no diagnostics and no secrets.
    assert!(!stdout.contains("INFO"), "no tracing on stdout");
    assert!(!stdout.contains("awh fs:"), "no human header in json mode");
}

#[test]
fn fs_state_survives_process_restart() {
    // "Restart" = a brand-new process invocation after the edit process
    // exited; the provenance store is the durable authority.
    let ws = Workspace::new();
    ws.write("r.txt", "before\n");
    let stdout = ws.ok(&["fs", "replace", "r.txt", "before", "after", "--json"]);
    let edit_id = ws.edit_id_of(&stdout);

    // New process: verify + history + rollback all see the durable state.
    ws.ok(&["fs", "verify", &edit_id]);
    let history = ws.ok(&["fs", "history"]);
    assert!(history.contains(&edit_id));
    ws.ok(&["fs", "rollback", &edit_id]);
    assert_eq!(ws.read("r.txt"), "before\n");
}
