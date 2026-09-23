//! End-to-end CLI tests for the TW-002 agent runtime identity surface:
//! `awh agent *` lifecycle commands and `awh agent session *` against the
//! real compiled binary.
//!
//! These verify the full identity contract through the CLI: registration
//! (duplicate rejection), lifecycle (start/stop/restart/status), the
//! enabled switch, session lifecycle with a validated transition table,
//! caller resolution (substitution attacks fail closed), workspace
//! binding, and persistence across invocations (every `run` is a fresh
//! process — equivalent to a restart).

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

/// Extracts the session id from `agent session open` output.
fn session_id_from(out: &str) -> String {
    out.lines()
        .next()
        .and_then(|l| l.strip_prefix("opened session "))
        .and_then(|l| l.split(' ').next())
        .expect("session id in output")
        .to_owned()
}

#[test]
fn agent_lifecycle_create_show_start_stop_restart_status() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // register
    let (ok, out, err) = run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    assert!(ok, "agent create failed: {err}");
    assert!(out.contains("created agent writer (Writer)"), "got: {out}");

    // duplicate registration is rejected, original preserved
    let (ok, _, err) = run(
        root,
        &["agent", "create", "writer", "Impostor", "--role", "writer"],
    );
    assert!(!ok, "duplicate id must fail");
    assert!(err.contains("already registered"), "got: {err}");
    let (ok, out, _) = run(root, &["agent", "show", "writer"]);
    assert!(ok && out.contains("name: Writer"), "got: {out}");

    // show includes lifecycle + enabled state
    let (ok, out, err) = run(root, &["agent", "show", "writer"]);
    assert!(ok, "agent show failed: {err}");
    assert!(out.contains("id: writer"), "got: {out}");
    assert!(out.contains("status: created"), "got: {out}");
    assert!(out.contains("enabled: true"), "got: {out}");
    assert!(out.contains("grants: none"), "got: {out}");

    // start
    let (ok, out, err) = run(root, &["agent", "start", "writer"]);
    assert!(ok, "agent start failed: {err}");
    assert!(
        out.contains("started agent writer status: active"),
        "got: {out}"
    );

    // status reflects lifecycle
    let (ok, out, _) = run(root, &["agent", "status"]);
    assert!(ok, "agent status failed");
    assert!(out.contains("writer Writer status: active"), "got: {out}");
    assert!(out.contains("enabled: true"), "got: {out}");
    assert!(out.contains("usable-sessions: 0"), "got: {out}");

    // stop → restart
    let (ok, out, err) = run(root, &["agent", "stop", "writer"]);
    assert!(ok, "agent stop failed: {err}");
    assert!(out.contains("stopped agent writer"), "got: {out}");
    let (ok, out, err) = run(root, &["agent", "restart", "writer"]);
    assert!(ok, "agent restart failed: {err}");
    assert!(
        out.contains("restarted agent writer status: active"),
        "got: {out}"
    );

    // unknown agent ids fail closed everywhere
    for cmd in [
        vec!["agent", "start", "ghost"],
        vec!["agent", "stop", "ghost"],
        vec!["agent", "restart", "ghost"],
        vec!["agent", "show", "ghost"],
        vec!["agent", "enable", "ghost"],
        vec!["agent", "disable", "ghost"],
    ] {
        let (ok, out, err) = run(root, &cmd);
        assert!(!ok, "{cmd:?} must fail for unknown agent: {out}");
        assert!(err.contains("agent not found"), "got: {err}");
    }
}

#[test]
fn agent_start_all_starts_only_enabled_profiles() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    for (id, name) in [("alpha", "Alpha"), ("beta", "Beta"), ("gamma", "Gamma")] {
        run(root, &["agent", "create", id, name, "--role", "worker"]);
    }
    // gamma is disabled → `--all` must skip it
    run(root, &["agent", "disable", "gamma"]);

    let (ok, out, err) = run(root, &["agent", "start", "--all"]);
    assert!(ok, "agent start --all failed: {err}");
    assert!(out.contains("started agent alpha"), "got: {out}");
    assert!(out.contains("started agent beta"), "got: {out}");
    assert!(
        !out.contains("started agent gamma"),
        "disabled profile must not start: {out}"
    );

    // gamma cannot be started by name either while disabled
    let (ok, _, err) = run(root, &["agent", "start", "gamma"]);
    assert!(!ok, "disabled agent must not start");
    assert!(err.contains("disabled"), "got: {err}");

    // re-enable restores it
    let (ok, out, _) = run(root, &["agent", "enable", "gamma"]);
    assert!(ok && out.contains("enabled agent gamma"));
    let (ok, out, _) = run(root, &["agent", "start", "gamma"]);
    assert!(ok && out.contains("started agent gamma"), "got: {out}");
}

#[test]
fn session_lifecycle_open_pause_resume_stop_is_terminal() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    run(root, &["init"]);
    run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    run(root, &["agent", "start", "writer"]);

    // open: session id is emitted and persisted
    let (ok, out, err) = run(root, &["agent", "session", "open", "writer"]);
    assert!(ok, "session open failed: {err}");
    assert!(out.contains("opened session"), "got: {out}");
    assert!(out.contains("status: active"), "got: {out}");
    let session_id = session_id_from(&out);

    // show: workspace binding visible
    let (ok, out, err) = run(root, &["agent", "session", "show", &session_id]);
    assert!(ok, "session show failed: {err}");
    assert!(out.contains("agent: writer"), "got: {out}");
    assert!(out.contains("status: active"), "got: {out}");
    assert!(out.contains("workspace: "), "got: {out}");

    // resolve: legitimate caller
    let (ok, out, err) = run(
        root,
        &["agent", "session", "resolve", "writer", &session_id],
    );
    assert!(ok, "resolve failed: {err}");
    assert!(out.contains("resolved session"), "got: {out}");
    assert!(out.contains("agent: writer"), "got: {out}");

    // pause → resume
    let (ok, out, err) = run(root, &["agent", "session", "pause", "writer", &session_id]);
    assert!(ok, "pause failed: {err}");
    assert!(out.contains("paused session"), "got: {out}");
    let (ok, out, err) = run(root, &["agent", "session", "resume", "writer", &session_id]);
    assert!(ok, "resume failed: {err}");
    assert!(out.contains("resumed session"), "got: {out}");

    // stop is terminal: no pause, no resume, no resolve afterwards
    let (ok, out, err) = run(root, &["agent", "session", "stop", "writer", &session_id]);
    assert!(ok, "stop failed: {err}");
    assert!(out.contains("stopped session"), "got: {out}");
    assert!(out.contains("terminal"), "got: {out}");
    for cmd in [
        vec!["agent", "session", "pause", "writer", &session_id],
        vec!["agent", "session", "resume", "writer", &session_id],
    ] {
        let (ok, out, err) = run(root, &cmd);
        assert!(!ok, "{cmd:?} must fail after stop: {out}");
        // The service resolves ownership before any transition, so a
        // terminal session is rejected at resolution (fail closed either
        // way; the store's table rejects it independently).
        assert!(err.contains("cannot be used"), "got: {err}");
    }
    let (ok, _, err) = run(
        root,
        &["agent", "session", "resolve", "writer", &session_id],
    );
    assert!(!ok, "stopped session must not resolve");
    assert!(err.contains("cannot be used"), "got: {err}");

    // the record persists with terminal status (visible in list)
    let (ok, out, _) = run(root, &["agent", "session", "list"]);
    assert!(ok, "list failed");
    assert!(out.contains("status: stopped"), "got: {out}");
}

#[test]
fn session_open_requires_initialized_workspace_and_active_agent() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // no `awh init`: fail closed
    run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    let (ok, out, err) = run(root, &["agent", "session", "open", "writer"]);
    assert!(!ok, "session open must fail on uninitialized workspace");
    assert!(err.contains("not initialized"), "got: {err}{out}");

    // initialized but agent not started
    run(root, &["init"]);
    let (ok, _, err) = run(root, &["agent", "session", "open", "writer"]);
    assert!(
        !ok,
        "session open must fail for a created (not started) agent"
    );
    assert!(
        err.contains("start it before opening a session"),
        "got: {err}"
    );

    // started: works
    run(root, &["agent", "start", "writer"]);
    let (ok, out, err) = run(root, &["agent", "session", "open", "writer"]);
    assert!(ok, "session open failed: {err}");
    assert!(out.contains("opened session"), "got: {out}");

    // disabled agent cannot open new sessions even when active
    run(root, &["agent", "disable", "writer"]);
    let (ok, _, err) = run(root, &["agent", "session", "open", "writer"]);
    assert!(!ok, "disabled agent must not open sessions");
    assert!(err.contains("disabled"), "got: {err}");
}

#[test]
fn session_substitution_and_cross_agent_isolation_fail_closed() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    run(root, &["init"]);
    run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    run(
        root,
        &[
            "agent", "create", "reviewer", "Reviewer", "--role", "reviewer",
        ],
    );
    run(root, &["agent", "start", "writer"]);
    run(root, &["agent", "start", "reviewer"]);

    let (_, writer_out, _) = run(root, &["agent", "session", "open", "writer"]);
    let writer_session = session_id_from(&writer_out);
    let (_, reviewer_out, _) = run(root, &["agent", "session", "open", "reviewer"]);
    let reviewer_session = session_id_from(&reviewer_out);

    // each session resolves only for its own agent
    assert!(
        run(
            root,
            &["agent", "session", "resolve", "writer", &writer_session]
        )
        .0
    );
    assert!(
        run(
            root,
            &["agent", "session", "resolve", "reviewer", &reviewer_session]
        )
        .0
    );

    // cross-claims are rejected (substitution attack)
    let (ok, out, err) = run(
        root,
        &["agent", "session", "resolve", "reviewer", &writer_session],
    );
    assert!(!ok, "reviewer must not resolve writer's session: {out}");
    assert!(err.contains("does not belong"), "got: {err}");
    let (ok, out, err) = run(
        root,
        &["agent", "session", "resolve", "writer", &reviewer_session],
    );
    assert!(!ok, "writer must not resolve reviewer's session: {out}");
    assert!(err.contains("does not belong"), "got: {err}");

    // one agent cannot transition another agent's session
    let (ok, out, err) = run(
        root,
        &["agent", "session", "pause", "reviewer", &writer_session],
    );
    assert!(!ok, "cross-agent pause must fail: {out}");
    assert!(err.contains("does not belong"), "got: {err}");

    // writer's session is still active and usable after the attempt
    let (ok, out, _) = run(
        root,
        &["agent", "session", "resolve", "writer", &writer_session],
    );
    assert!(ok && out.contains("status: active"), "got: {out}");

    // unknown session ids fail closed
    let (ok, _, err) = run(
        root,
        &["agent", "session", "resolve", "writer", "sess-ghost"],
    );
    assert!(!ok, "unknown session must not resolve");
    assert!(err.contains("session not found"), "got: {err}");

    // list --agent filters correctly and shows only that agent's sessions
    let (ok, out, _) = run(root, &["agent", "session", "list", "--agent", "writer"]);
    assert!(ok, "list --agent failed");
    assert!(out.contains(&writer_session), "got: {out}");
    assert!(!out.contains(&reviewer_session), "got: {out}");
}

#[test]
fn stopping_agent_invalidates_but_does_not_mutate_sessions() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    run(root, &["init"]);
    run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    run(root, &["agent", "start", "writer"]);
    let (_, out, _) = run(root, &["agent", "session", "open", "writer"]);
    let session_id = session_id_from(&out);

    // stopping the agent makes the session unresolvable...
    run(root, &["agent", "stop", "writer"]);
    let (ok, _, err) = run(
        root,
        &["agent", "session", "resolve", "writer", &session_id],
    );
    assert!(!ok, "session must not resolve while agent is stopped");
    assert!(err.contains("not active"), "got: {err}");

    // ...without mutating the session record (still active on disk)
    let (ok, out, _) = run(root, &["agent", "session", "show", &session_id]);
    assert!(ok, "session show failed");
    assert!(
        out.contains("status: active"),
        "record must be untouched: {out}"
    );

    // restarting the agent restores resolution (explicit operator action)
    run(root, &["agent", "restart", "writer"]);
    let (ok, out, err) = run(
        root,
        &["agent", "session", "resolve", "writer", &session_id],
    );
    assert!(ok, "resolve after restart failed: {err}");
    assert!(out.contains("resolved session"), "got: {out}");
}

#[test]
fn session_ids_are_unique_per_open_and_globally_named() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    run(root, &["init"]);
    run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    run(root, &["agent", "start", "writer"]);

    let (_, a, _) = run(root, &["agent", "session", "open", "writer"]);
    let (_, b, _) = run(root, &["agent", "session", "open", "writer"]);
    let sa = session_id_from(&a);
    let sb = session_id_from(&b);
    assert_ne!(sa, sb, "consecutive opens must produce distinct ids");
    assert!(
        sa.starts_with("sess-"),
        "session ids use the sess- prefix: {sa}"
    );

    // both persist as records
    let (ok, out, _) = run(root, &["agent", "session", "list"]);
    assert!(ok, "list failed");
    assert!(out.contains(&sa) && out.contains(&sb), "got: {out}");
}

#[test]
fn status_reports_usable_session_counts_per_agent() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    run(root, &["init"]);
    run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    run(root, &["agent", "start", "writer"]);

    run(root, &["agent", "session", "open", "writer"]);
    run(root, &["agent", "session", "open", "writer"]);
    let (_, out, _) = run(root, &["agent", "session", "open", "writer"]);
    let third = session_id_from(&out);
    run(root, &["agent", "session", "stop", "writer", &third]);

    let (ok, out, _) = run(root, &["agent", "status"]);
    assert!(ok, "status failed");
    assert!(
        out.contains("usable-sessions: 2"),
        "2 usable (stopped excluded): {out}"
    );
}

#[test]
fn session_commands_on_uninitialized_workspace_fail_closed() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    run(root, &["agent", "start", "writer"]);
    // no init: open fails (needs workspace binding)
    let (ok, _, err) = run(root, &["agent", "session", "open", "writer"]);
    assert!(!ok, "open must fail without init");
    assert!(err.contains("not initialized"), "got: {err}");
}
