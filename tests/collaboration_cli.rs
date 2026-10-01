//! End-to-end CLI tests for COL-001 collaboration: `awh collaboration *`
//! against the real compiled binary, covering the ownership lifecycle
//! (assign → activate → handoff → release → reactivate), revision
//! safety, workspace-bound validation (unknown agents/tasks/worktrees
//! fail closed), session binding, evidence-based conflict reporting,
//! and the durable events query.

use std::process::Command;
use tempfile::tempdir;

/// Runs `awh <args>` inside `dir` and returns (exit-success, stdout, stderr).
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

/// Extracts `revision=N` from the first output line.
fn revision_from(out: &str) -> u64 {
    let line = out.lines().next().expect("record line");
    line.split_whitespace()
        .find(|t| t.starts_with("revision="))
        .and_then(|t| t.strip_prefix("revision="))
        .expect("revision in output")
        .parse()
        .expect("numeric revision")
}

/// Initializes a workspace with two started agents and one task.
fn seeded(dir: &std::path::Path) {
    let (ok, _, err) = run(dir, &["init"]);
    assert!(ok, "awh init failed: {err}");
    for (id, name) in [("agent-a", "Alice"), ("agent-b", "Bob")] {
        let (ok, _, err) = run(dir, &["agent", "create", id, name, "--role", "worker"]);
        assert!(ok, "agent create failed: {err}");
        let (ok, _, err) = run(dir, &["agent", "start", id]);
        assert!(ok, "agent start failed: {err}");
    }
    let (ok, _, err) = run(
        dir,
        &[
            "task",
            "create",
            "--id",
            "task-1",
            "--title",
            "Collab target",
            "--description",
            "d",
        ],
    );
    assert!(ok, "task create failed: {err}");
}

#[test]
fn assign_handoff_release_reactivate_lifecycle() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    seeded(root);

    // assign to agent-a
    let (ok, out, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
        ],
    );
    assert!(ok, "assign failed: {err}");
    assert!(
        out.contains("task:task-1 owner=agent-a state=Assigned"),
        "got: {out}"
    );

    // duplicate assign fails closed with the stable category text
    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-b",
        ],
    );
    assert!(!ok, "duplicate assign must fail");
    assert!(err.contains("already has an active owner"), "got: {err}");

    // activate (owner only)
    let (ok, out, err) = run(
        root,
        &[
            "collaboration",
            "activate",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
        ],
    );
    assert!(ok, "activate failed: {err}");
    assert!(out.contains("state=Active"), "got: {out}");

    // non-owner cannot hand off
    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "handoff",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--from",
            "agent-b",
            "--to",
            "agent-b",
            "--request",
        ],
    );
    assert!(!ok, "non-owner handoff must fail");
    assert!(err.contains("current owner"), "got: {err}");

    // one-shot handoff: request + accept in one invocation
    let (ok, out, err) = run(
        root,
        &[
            "collaboration",
            "handoff",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--from",
            "agent-a",
            "--to",
            "agent-b",
            "--note",
            "passing the baton",
        ],
    );
    assert!(ok, "handoff failed: {err}");
    assert!(out.contains("owner=agent-b"), "got: {out}");
    assert!(out.contains("state=HandedOff"), "got: {out}");

    // status shows the new owner
    let (ok, out, _) = run(root, &["collaboration", "status"]);
    assert!(ok, "status failed");
    assert!(out.contains("task:task-1 owner=agent-b"), "got: {out}");

    // release, then blind reactivation is rejected…
    let (ok, out, err) = run(
        root,
        &[
            "collaboration",
            "release",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-b",
        ],
    );
    assert!(ok, "release failed: {err}");
    let revision = revision_from(&out);
    assert!(out.contains("state=Released"), "got: {out}");

    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
        ],
    );
    assert!(!ok, "blind reactivation must fail");
    assert!(err.contains("stale revision"), "got: {err}");

    // …while echoing the observed revision reactivates it
    let (ok, out, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
            "--expected-revision",
            &revision.to_string(),
        ],
    );
    assert!(ok, "reactivation failed: {err}");
    assert!(out.contains("owner=agent-a"), "got: {out}");
}

#[test]
fn assignment_fails_closed_on_unknown_identities() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    seeded(root);

    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "ghost",
        ],
    );
    assert!(!ok, "unknown agent must fail");
    assert!(err.contains("agent not found"), "got: {err}");

    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-404",
            "--agent",
            "agent-a",
        ],
    );
    assert!(!ok, "unknown task must fail");
    assert!(err.contains("task not found"), "got: {err}");

    // no worktrees exist in this workspace: possession of an id is not
    // authority
    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "worktree",
            "--id",
            "wt-1",
            "--agent",
            "agent-a",
        ],
    );
    assert!(!ok, "unknown worktree must fail");
    assert!(err.contains("worktree not found"), "got: {err}");

    // unknown resource kinds are rejected at parse time
    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "bogus",
            "--id",
            "x",
            "--agent",
            "agent-a",
        ],
    );
    assert!(!ok, "bogus kind must fail");
    assert!(err.contains("unknown resource kind"), "got: {err}");
}

#[test]
fn session_binding_is_workspace_and_owner_checked() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    seeded(root);

    // open a session for agent-b and try to bind it to agent-a's
    // assignment — identity resolution must fail closed
    let (ok, out, err) = run(root, &["agent", "session", "open", "agent-b"]);
    assert!(ok, "session open failed: {err}");
    let session = out
        .lines()
        .find_map(|l| l.strip_prefix("opened session "))
        .and_then(|l| l.split(' ').next())
        .expect("session id")
        .to_owned();

    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
            "--session",
            &session,
        ],
    );
    assert!(!ok, "foreign session must fail");
    assert!(
        err.contains("does not belong") || err.contains("cannot bind session"),
        "got: {err}"
    );

    // the owner's own session binds and is recorded
    let (ok, out, _) = run(root, &["agent", "session", "open", "agent-a"]);
    assert!(ok, "session open failed");
    let own = out
        .lines()
        .find_map(|l| l.strip_prefix("opened session "))
        .and_then(|l| l.split(' ').next())
        .expect("session id")
        .to_owned();
    let (ok, out, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
            "--session",
            &own,
            "--note",
            "sprint 42",
        ],
    );
    assert!(ok, "own-session assign failed: {err}");
    assert!(out.contains(&format!("session={own}")), "got: {out}");
    assert!(out.contains("note=sprint 42"), "got: {out}");
}

#[test]
fn conflicts_reports_evidence_and_stays_clean_while_healthy() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    seeded(root);

    // healthy ownership: no findings
    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
        ],
    );
    assert!(ok, "assign failed: {err}");
    let (ok, out, _) = run(root, &["collaboration", "conflicts"]);
    assert!(ok, "conflicts failed");
    assert!(out.contains("no conflicts detected"), "got: {out}");

    // stop the owner → observable ownership/lifecycle mismatch
    let (ok, _, err) = run(root, &["agent", "stop", "agent-a"]);
    assert!(ok, "agent stop failed: {err}");
    let (ok, out, _) = run(root, &["collaboration", "conflicts"]);
    assert!(ok, "conflicts failed");
    assert!(out.contains("evidence=owner_agent_stopped"), "got: {out}");
    assert!(out.contains("task:task-1"), "got: {out}");

    // terminal task while held → task lifecycle evidence
    let (ok, _, err) = run(root, &["agent", "start", "agent-a"]);
    assert!(ok, "agent restart failed: {err}");
    let (ok, _, err) = run(
        root,
        &["task", "update", "--id", "task-1", "--status", "Done"],
    );
    assert!(ok, "task update failed: {err}");
    let (ok, out, _) = run(root, &["collaboration", "conflicts"]);
    assert!(ok, "conflicts failed");
    assert!(out.contains("evidence=task_terminal"), "got: {out}");

    // released records are out of scope for the scan
    let (ok, out, _) = run(
        root,
        &[
            "collaboration",
            "release",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
        ],
    );
    assert!(
        ok && out.contains("state=Released"),
        "release failed: {out}"
    );
    let (ok, out, _) = run(root, &["collaboration", "conflicts"]);
    assert!(ok, "conflicts failed");
    assert!(out.contains("no conflicts detected"), "got: {out}");
}

#[test]
fn events_list_durable_collaboration_transitions() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    seeded(root);

    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
        ],
    );
    assert!(ok, "assign failed: {err}");
    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "handoff",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--from",
            "agent-a",
            "--to",
            "agent-b",
        ],
    );
    assert!(ok, "handoff failed: {err}");

    // durable across processes: a fresh invocation must list both
    // halves of the handoff plus the assignment
    let (ok, out, err) = run(root, &["collaboration", "events"]);
    assert!(ok, "events failed: {err}");
    assert!(out.contains("assign"), "got: {out}");
    assert!(out.contains("handoff_request"), "got: {out}");
    assert!(out.contains("handoff_accept"), "got: {out}");
    assert!(out.contains("task:task-1"), "got: {out}");

    // action filter narrows to exactly the requested action
    let (ok, out, err) = run(root, &["collaboration", "events", "--action", "assign"]);
    assert!(ok, "events --action failed: {err}");
    assert!(out.contains("assign"), "got: {out}");
    assert!(!out.contains("handoff_request"), "got: {out}");
}

#[test]
fn agents_and_status_surfaces_report_state() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    seeded(root);

    let (ok, out, err) = run(root, &["collaboration", "agents"]);
    assert!(ok, "agents failed: {err}");
    assert!(out.contains("agent-a name=Alice"), "got: {out}");
    assert!(out.contains("enabled=true"), "got: {out}");

    let (ok, out, _) = run(root, &["collaboration", "status"]);
    assert!(ok, "status failed");
    assert!(out.contains("no ownership records"), "got: {out}");

    run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
        ],
    );
    let (ok, out, _) = run(root, &["collaboration", "status", "--kind", "task"]);
    assert!(ok, "status --kind failed");
    assert!(out.contains("task:task-1 owner=agent-a"), "got: {out}");
}
