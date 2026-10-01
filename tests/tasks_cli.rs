//! End-to-end CLI tests for the TSK-001 task surface: `awh task
//! list|show|create|update|cancel|assign|delete` against the real
//! compiled binary. Every `run` is a fresh process, so
//! `.agent/tasks.json` durability across invocations is exercised
//! implicitly; the state machine, assignment validation, fail-closed
//! conflicts, and durable audit events are pinned here.

use std::process::Command;
use tempfile::tempdir;

fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn awh binary");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn task_create_list_show_update_lifecycle() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // create with auto-minted canonical id
    let (ok, out, err) = run(
        root,
        &[
            "task",
            "create",
            "--title",
            "Ship the release",
            "--tag",
            "release",
        ],
    );
    assert!(ok, "task create failed: {err}");
    let id = out
        .lines()
        .find_map(|l| l.strip_prefix("created task "))
        .and_then(|l| l.split(' ').next())
        .expect("id in create output")
        .to_owned();
    assert!(id.starts_with("task-"), "canonical TaskId expected: {id}");

    // explicit id + stdin description
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args([
            "task",
            "create",
            "--id",
            "review-1",
            "--title",
            "Review PR",
            "--priority",
            "High",
        ])
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Check the review checklist")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("review-1"));

    // list shows both rows
    let (ok, out, err) = run(root, &["task", "list"]);
    assert!(ok, "task list failed: {err}");
    assert!(out.contains(&id), "got: {out}");
    assert!(out.contains("review-1"), "got: {out}");

    // status filter
    let (ok, out, err) = run(root, &["task", "list", "--status", "Todo"]);
    assert!(ok, "task list --status failed: {err}");
    assert!(out.contains("review-1"), "got: {out}");

    // show returns the record
    let (ok, out, err) = run(root, &["task", "show", "--id", "review-1"]);
    assert!(ok, "task show failed: {err}");
    assert!(out.contains("\"priority\": \"High\""), "got: {out}");

    // legal transition Todo -> InProgress
    let (ok, out, err) = run(
        root,
        &[
            "task",
            "update",
            "--id",
            "review-1",
            "--status",
            "InProgress",
        ],
    );
    assert!(ok, "task update failed: {err}");
    assert!(
        out.contains("updated task review-1 (status InProgress)"),
        "got: {out}"
    );

    // illegal transition Done -> InProgress fails closed
    let (ok, _, err) = run(
        root,
        &["task", "update", "--id", "review-1", "--status", "Done"],
    );
    assert!(ok, "task update to Done failed: {err}");
    let (ok, _, err) = run(
        root,
        &[
            "task",
            "update",
            "--id",
            "review-1",
            "--status",
            "InProgress",
        ],
    );
    assert!(!ok, "Done -> InProgress must be rejected");
    assert!(
        err.contains("invalid task transition Done -> InProgress"),
        "got: {err}"
    );

    // unknown task fails closed
    let (ok, _, err) = run(
        root,
        &["task", "update", "--id", "ghost", "--status", "Todo"],
    );
    assert!(!ok, "update of unknown task must fail");
    assert!(err.contains("task not found: ghost"), "got: {err}");
}

#[test]
fn task_cancel_is_terminal_and_fails_closed_on_repeat() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, _, err) = run(
        root,
        &["task", "create", "--id", "t1", "--title", "Abandon me"],
    );
    assert!(ok, "create failed: {err}");

    let (ok, out, err) = run(root, &["task", "cancel", "--id", "t1"]);
    assert!(ok, "cancel failed: {err}");
    assert!(out.contains("cancelled task t1 (terminal)"), "got: {out}");

    // replayed cancel: stable conflict error, never silent success
    let (ok, _, err) = run(root, &["task", "cancel", "--id", "t1"]);
    assert!(!ok, "repeated cancel must fail closed");
    assert!(
        err.contains("invalid task transition Cancelled -> Cancelled"),
        "got: {err}"
    );

    // cancelled tasks are terminal for updates too
    let (ok, _, _) = run(root, &["task", "update", "--id", "t1", "--status", "Todo"]);
    assert!(!ok, "Cancelled -> Todo must be rejected");

    // list still shows it under the Cancelled filter
    let (ok, out, err) = run(root, &["task", "list", "--status", "Cancelled"]);
    assert!(ok, "list failed: {err}");
    assert!(out.contains("t1"), "got: {out}");
}

#[test]
fn task_lifecycle_is_distinct_from_session_lifecycle() {
    // §9 Tasks × sessions: assignment binds a task to a REAL session id,
    // but the task plane is independent of the execution lifecycle —
    // stopping the session neither touches the task nor blocks task
    // mutations, and the durable session record keeps the assignment
    // reference honest long after the session is terminal.
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    run(root, &["init"]);
    run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    run(root, &["agent", "start", "writer"]);

    let (ok, _, err) = run(
        root,
        &["task", "create", "--id", "t1", "--title", "Survives"],
    );
    assert!(ok, "task create failed: {err}");

    // assign to the live session
    let (ok, out, err) = run(root, &["agent", "session", "open", "writer"]);
    assert!(ok, "session open failed: {err}");
    let session_id = out
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("opened session "))
        .and_then(|l| l.split(' ').next())
        .expect("session id in output")
        .to_owned();
    let (ok, out, err) = run(root, &["task", "assign", "--id", "t1", "--to", &session_id]);
    assert!(ok, "assign to live session failed: {err}");
    assert!(
        out.contains(&format!("assigned task t1 to {session_id}")),
        "got: {out}"
    );

    // stop is TERMINAL on the execution plane…
    let (ok, out, err) = run(root, &["agent", "session", "stop", "writer", &session_id]);
    assert!(ok, "session stop failed: {err}");
    assert!(out.contains("terminal"), "got: {out}");

    // …but the task is untouched: same state, same assignee reference.
    let (ok, out, err) = run(root, &["task", "show", "--id", "t1"]);
    assert!(ok, "task show failed: {err}");
    assert!(out.contains("\"status\": \"Todo\""), "got: {out}");
    assert!(
        out.contains(&format!("\"assignee\": \"{session_id}\"")),
        "assignee reference survives the session's terminal state: {out}"
    );

    // the task plane does not consult session liveness: mutations and
    // even NEW references to the durable (terminal) session record work.
    let (ok, out, err) = run(
        root,
        &["task", "update", "--id", "t1", "--status", "InProgress"],
    );
    assert!(ok, "task update after session stop failed: {err}");
    assert!(
        out.contains("updated task t1 (status InProgress)"),
        "got: {out}"
    );
    let (ok, _, err) = run(
        root,
        &["task", "create", "--id", "t2", "--title", "Follow-up"],
    );
    assert!(ok, "second task create failed: {err}");
    let (ok, _, err) = run(root, &["task", "assign", "--id", "t2", "--to", &session_id]);
    assert!(
        ok,
        "assignment to the durable session record must not depend on \
         the session's execution state: {err}"
    );

    // list keeps showing both tasks after the session is long gone.
    let (ok, out, err) = run(root, &["task", "list"]);
    assert!(ok, "task list failed: {err}");
    assert!(out.contains("t1") && out.contains("t2"), "got: {out}");
}

#[test]
fn task_assign_requires_existing_target() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, _out, err) = run(
        root,
        &[
            "agent",
            "create",
            "writer",
            "Writer Agent",
            "--role",
            "writer",
        ],
    );
    assert!(ok, "agent create failed: {err}");

    let (ok, _, err) = run(
        root,
        &["task", "create", "--id", "t1", "--title", "Write docs"],
    );
    assert!(ok, "task create failed: {err}");

    // assigning to a real agent profile works
    let (ok, out, err) = run(root, &["task", "assign", "--id", "t1", "--to", "writer"]);
    assert!(ok, "assign failed: {err}");
    assert!(out.contains("assigned task t1 to writer"), "got: {out}");

    // assigning to a non-existent target fails closed
    let (ok, _, err) = run(root, &["task", "assign", "--id", "t1", "--to", "nobody"]);
    assert!(!ok, "assign to unknown target must fail");
    assert!(err.contains("assignee not found: nobody"), "got: {err}");

    // session-shaped ids are existence-checked, not prefix-trusted
    let (ok, _, err) = run(
        root,
        &["task", "assign", "--id", "t1", "--to", "sess-ghost"],
    );
    assert!(!ok, "assign to unknown session must fail");
    assert!(err.contains("assignee not found: sess-ghost"), "got: {err}");

    // unassign clears
    let (ok, _, err) = run(root, &["task", "update", "--id", "t1", "--unassign"]);
    assert!(ok, "unassign failed: {err}");
    let (ok, out, err) = run(root, &["task", "show", "--id", "t1"]);
    assert!(ok, "show failed: {err}");
    assert!(out.contains("\"assignee\": null"), "got: {out}");
}

#[test]
fn task_mutations_are_durable_and_audited() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, _, err) = run(
        root,
        &["task", "create", "--id", "t1", "--title", "Durable task"],
    );
    assert!(ok, "create failed: {err}");

    // persistence: .agent/tasks.json holds the record across processes
    let raw = std::fs::read_to_string(root.join(".agent/tasks.json")).unwrap();
    assert!(raw.contains("Durable task"), "got: {raw}");

    let (ok, _, err) = run(root, &["task", "cancel", "--id", "t1"]);
    assert!(ok, "cancel failed: {err}");
    let raw = std::fs::read_to_string(root.join(".agent/tasks.json")).unwrap();
    assert!(raw.contains("Cancelled"), "got: {raw}");

    // audit: durable log records the mutations without task text
    let audit = std::fs::read_to_string(root.join(".agent/audit/audit.log")).unwrap();
    assert!(audit.contains("cli_task_create"), "got: {audit}");
    assert!(audit.contains("cli_task_cancel"), "got: {audit}");
    assert!(
        !audit.contains("Durable task"),
        "audit must not leak task text: {audit}"
    );

    // delete then fail closed
    let (ok, _, err) = run(root, &["task", "delete", "--id", "t1"]);
    assert!(ok, "delete failed: {err}");
    let (ok, _, err) = run(root, &["task", "show", "--id", "t1"]);
    assert!(!ok, "show after delete must fail");
    assert!(err.contains("task not found: t1"), "got: {err}");
}
