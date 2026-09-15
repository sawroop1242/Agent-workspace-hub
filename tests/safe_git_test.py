#!/usr/bin/env python3
"""safe_git push scenarios against a real origin: clean push, rebase retry, conflict abort."""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "safe_git.py"
IDENTITY = ("-c", "user.name=AWH Test", "-c", "user.email=awh-test@example.invalid")


def git(cwd: Path, *arguments: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        ["git", "-C", str(cwd), *arguments],
        capture_output=True,
        text=True,
        env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
    )
    if check and result.returncode != 0:
        raise AssertionError(f"git {' '.join(arguments)} failed: {result.stderr}")
    return result


def commit_file(cwd: Path, name: str, content: str, message: str) -> None:
    (cwd / name).write_text(content, encoding="utf-8")
    git(cwd, "add", "-A")
    git(cwd, *IDENTITY, "commit", "-m", message)


def load_module():
    spec = importlib.util.spec_from_file_location("safe_git", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build_repository(tmp_path: Path, branch: str = "rust") -> tuple[Path, Path]:
    """Create a bare origin plus a primary clone on `branch` seeded with one commit."""
    origin = tmp_path / "origin.git"
    clone = tmp_path / "clone"
    origin.mkdir()
    git(origin, "init", "--bare", "-b", branch)
    git(tmp_path, "clone", str(origin), str(clone))
    git(clone, "checkout", "-b", branch)
    # The AWH workflows configure identity in the clone before committing.
    git(clone, "config", "user.name", "AWH Automation")
    git(clone, "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
    (clone / "seed.txt").write_text("seed\n", encoding="utf-8")
    git(clone, "add", "-A")
    git(clone, *IDENTITY, "commit", "-m", "seed")
    git(clone, "push", "-u", "origin", branch)
    return origin, clone


def clone_runner(tmp_path: Path, origin: Path) -> Path:
    """A second runner sharing the same origin, used to advance the remote."""
    other = tmp_path / "other"
    git(tmp_path, "clone", str(origin), str(other))
    git(other, "config", "user.name", "AWH Automation")
    git(other, "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
    git(other, "checkout", "rust")
    return other


def stage_state_checkpoint(clone: Path, status: str, operation_id: str = "op-1") -> None:
    """Stage exactly what the AWH workflows stage before a push."""
    (clone / ".openhands").mkdir(exist_ok=True)
    payload = (
        '{"version": 3, "status": "%s", "operation_id": "%s", "active_feature": null, '
        '"active_pr": null, "active_branch": null, "review_round": 0, '
        '"builder_attempt": 1, "last_error": null, "last_review": "", "updated_at": null}\n' % (status, operation_id)
    )
    (clone / ".openhands" / "state.json").write_text(payload, encoding="utf-8")
    git(clone, "add", ".openhands/state.json")


def run_push(clone: Path, branch: str, message: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), "push", "--branch", branch, "--message", message],
        cwd=clone,
        text=True,
        capture_output=True,
        check=False,
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )


def assert_status_clean(clone: Path) -> None:
    status = git(clone, "status", "--porcelain").stdout
    assert status == "", f"working tree is not clean after safe_git failure:\n{status}"
    assert not (clone / ".git" / "rebase-merge").exists(), "a rebase is left dangling mid-state"
    assert not (clone / ".git" / "rebase-apply").exists(), "a rebase is left dangling mid-state"


def test_normal_push_succeeds(tmp_path: Path):
    origin, clone = build_repository(tmp_path)
    stage_state_checkpoint(clone, "PLANNING")

    result = run_push(clone, "rust", "chore(automation): checkpoint planner start")

    assert result.returncode == 0, (result.stdout, result.stderr)
    assert "pushed to origin/rust on attempt 1" in result.stdout
    assert git(clone, "rev-parse", "origin/rust").stdout.strip() == git(clone, "rev-parse", "HEAD").stdout.strip()
    assert "PLANNING" in git(clone, "show", "HEAD:.openhands/state.json").stdout
    assert_status_clean(clone)


def test_advanced_remote_triggers_fetch_rebase_retry(tmp_path: Path):
    origin, clone = build_repository(tmp_path)
    other = clone_runner(tmp_path, origin)
    commit_file(other, "other.txt", "remote change\n", "remote advance")
    git(other, "push", "origin", "rust")

    stage_state_checkpoint(clone, "BUILDING", operation_id="op-build-2")
    message = "chore(automation): persist active build checkpoint"
    result = run_push(clone, "rust", message)

    assert result.returncode == 0, (result.stdout, result.stderr)
    assert "non-fast-forward" in result.stdout
    assert "rebased onto origin/rust" in result.stdout
    log = git(clone, "log", "--oneline", "origin/rust").stdout.splitlines()
    assert log[0].endswith(message), log
    assert any("remote advance" in line for line in log), log  # rebased on top, not clobbered
    assert "other.txt" in git(clone, "ls-tree", "--name-only", "origin/rust").stdout
    assert "state.json" in git(clone, "ls-tree", "--name-only", "origin/rust", ".openhands/").stdout
    assert_status_clean(clone)


def test_normal_safe_push_rebases_after_non_ff(tmp_path: Path):
    """Regression counterpart to the recovery-claim path: the NORMAL
    checkpoint write must keep reconciling - fetch, rebase, retry - when the
    remote advanced. Only the recovery claim is fail-closed."""
    origin, clone = build_repository(tmp_path)
    other = clone_runner(tmp_path, origin)
    commit_file(other, "advance.txt", "remote moved\n", "remote advance")
    git(other, "push", "origin", "rust")

    stage_state_checkpoint(clone, "PR_OPEN", operation_id="op-normal-1")
    result = run_push(clone, "rust", "chore(automation): normal checkpoint write")

    assert result.returncode == 0, (result.stdout, result.stderr)
    assert "non-fast-forward" in result.stdout
    assert "fetching and rebasing" in result.stdout
    assert "pushed to origin/rust" in result.stdout
    # The remote advance survived and the write landed on top of it.
    log = git(clone, "log", "--oneline", "origin/rust").stdout
    assert "remote advance" in log
    assert "normal checkpoint write" in log
    assert_status_clean(clone)


def test_genuine_conflict_aborts_cleanly_with_no_dangling_rebase(tmp_path: Path):
    origin, clone = build_repository(tmp_path)
    other = clone_runner(tmp_path, origin)
    (other / ".openhands").mkdir()
    commit_file(
        other,
        ".openhands/state.json",
        '{"version": 3, "status": "CONFLICT"}\n',
        "conflicting remote write",
    )
    git(other, "push", "origin", "rust")

    stage_state_checkpoint(clone, "PLANNING", operation_id="op-conflict")
    message = "chore(automation): checkpoint planner start"
    remote_before = git(other, "rev-parse", "origin/rust").stdout.strip()
    result = run_push(clone, "rust", message)

    assert result.returncode != 0
    assert "rebase onto origin/rust hit a conflict" in (result.stderr + result.stdout)
    assert "nothing was pushed" in (result.stderr + result.stdout)
    assert_status_clean(clone)
    # Our commit survives locally, ready for manual resolution; the remote is untouched.
    assert git(clone, "log", "-1", "--oneline").stdout.strip().endswith(message)
    assert git(clone, "rev-parse", "origin/rust").stdout.strip() == remote_before


def test_nothing_staged_fails_closed_without_pushing(tmp_path: Path):
    _, clone = build_repository(tmp_path)
    before = git(clone, "rev-parse", "origin/rust").stdout.strip()

    result = run_push(clone, "rust", "chore(automation): nothing to commit")

    assert result.returncode != 0
    assert "nothing is staged" in (result.stderr + result.stdout)
    assert git(clone, "rev-parse", "origin/rust").stdout.strip() == before
    assert_status_clean(clone)


def test_repeated_rejections_exhaust_retries_and_never_force(tmp_path: Path, monkeypatch):
    """A remote that advances before every push must exhaust retries, not loop or force."""
    origin, clone = build_repository(tmp_path)
    other = clone_runner(tmp_path, origin)
    module = load_module()
    assert module.MAX_PUSH_ATTEMPTS == 3

    stage_state_checkpoint(clone, "PLANNING", operation_id="op-race")

    original_git = module.git
    racing = {"pushes": 0}

    def racing_git(*arguments, check=True):
        if arguments and arguments[0] == "push":
            # A competing runner commits AND pushes between our rebase and push.
            racing["pushes"] += 1
            commit_file(other, f"race{racing['pushes']}.txt", "competing write\n", f"race {racing['pushes']}")
            git(other, "push", "origin", "rust")
        return original_git(*arguments, check=check)

    # safe_git's git calls inherit the process cwd: point them at the clone.
    monkeypatch.chdir(clone)
    monkeypatch.setattr(module, "git", racing_git)
    with pytest.raises(SystemExit, match="all 3 attempts"):
        module.main(["push", "--branch", "rust", "--message", "chore(automation): checkpoint planner start"])

    assert racing["pushes"] == 3  # bounded: one push attempt per retry, then fail closed
    assert_status_clean(clone)
    assert "race 3" in git(other, "log", "--oneline", "origin/rust").stdout


def test_script_source_never_uses_force_push_flags():
    """The one hard invariant of this module: no force flags, ever, in any form."""
    source = SCRIPT.read_text(encoding="utf-8")
    assert "--force-with-lease" not in source
    for token in ("--force", "-force", "=force"):
        assert token not in source, f"forbidden push flag {token!r} appears in safe_git.py"
    # Every push invocation in the module must be a plain push of the branch.
    module = load_module()
    import inspect
    body = inspect.getsource(module.push)
    assert 'git("push", "origin", branch' in body
    assert "force" not in body


def test_detached_head_refuses_to_push(tmp_path: Path):
    """A runner that somehow ended on a detached HEAD must never push rust."""
    origin, clone = build_repository(tmp_path)
    remote_before = git(clone, "rev-parse", "origin/rust").stdout.strip()
    head_before = git(clone, "rev-parse", "HEAD").stdout.strip()
    stage_state_checkpoint(clone, "PLANNING", operation_id="op-detached")

    head = git(clone, "rev-parse", "HEAD").stdout.strip()
    git(clone, "checkout", "--detach", head)

    result = run_push(clone, "rust", "chore(automation): detached head attempt")

    assert result.returncode != 0
    assert "detached HEAD" in (result.stdout + result.stderr)
    assert "refusing to push" in (result.stdout + result.stderr)
    # Nothing was committed or pushed: HEAD and the remote are untouched.
    assert git(clone, "rev-parse", "HEAD").stdout.strip() == head_before
    assert git(clone, "rev-parse", "origin/rust").stdout.strip() == remote_before
    # The staged checkpoint survives for the caller to handle; no rebase is left dangling.
    assert not (clone / ".git" / "rebase-merge").exists()
    assert not (clone / ".git" / "rebase-apply").exists()
    staged = git(clone, "diff", "--cached", "--name-only").stdout.split()
    assert staged == [".openhands/state.json"]


def test_sync_fast_forwards_to_advanced_remote(tmp_path: Path):
    """sync must fetch and fast-forward the local branch onto the remote."""
    origin, clone = build_repository(tmp_path)
    other = clone_runner(tmp_path, origin)
    commit_file(other, "advance.txt", "remote advance\n", "remote advance")
    git(other, "push", "origin", "rust")

    result = subprocess.run(
        [sys.executable, str(SCRIPT), "sync", "--branch", "rust"],
        cwd=clone, text=True, capture_output=True, check=False,
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )

    assert result.returncode == 0, (result.stdout, result.stderr)
    assert "synced rust to origin/rust" in result.stdout
    assert git(clone, "rev-parse", "HEAD").stdout.strip() == git(clone, "rev-parse", "origin/rust").stdout.strip()
    assert "advance.txt" in git(clone, "ls-tree", "--name-only", "HEAD").stdout


def test_sync_refuses_diverged_local_history_without_rewrite(tmp_path: Path):
    """A diverged local rust branch must be refused, never rebased or reset."""
    origin, clone = build_repository(tmp_path)
    other = clone_runner(tmp_path, origin)
    commit_file(other, "advance.txt", "remote advance\n", "remote advance")
    git(other, "push", "origin", "rust")
    commit_file(clone, "local.txt", "local change\n", "local divergence")

    result = subprocess.run(
        [sys.executable, str(SCRIPT), "sync", "--branch", "rust"],
        cwd=clone, text=True, capture_output=True, check=False,
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )

    assert result.returncode != 0
    assert "diverged" in (result.stdout + result.stderr)
    assert "fast-forward only" in (result.stdout + result.stderr)
    # Nothing was rewritten: the local commit still exists, remote untouched.
    assert git(clone, "log", "-1", "--oneline").stdout.strip().endswith("local divergence")
    assert_status_clean(clone)


def test_sync_rejects_message_argument():
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "sync", "--branch", "rust", "--message", "x"],
        cwd=ROOT, text=True, capture_output=True, check=False,
    )
    assert result.returncode != 0
    assert "--message is only valid for a push" in (result.stdout + result.stderr)


# --------------------------------------------------------------------------
# push_claim: the fail-closed recovery-claim publish path.
# --------------------------------------------------------------------------


def test_push_claim_publishes_when_remote_is_unchanged(tmp_path: Path):
    origin, clone = build_repository(tmp_path)
    stage_state_checkpoint(clone, "RECOVERING", operation_id="F:RECOVERY:1")
    git(clone, *IDENTITY, "commit", "-m", "chore(automation): claim recovery F")

    load_module().push_claim(repo=clone, remote="origin", branch="rust")

    assert git(clone, "rev-parse", "origin/rust").stdout.strip() == git(clone, "rev-parse", "HEAD").stdout.strip()
    assert "RECOVERING" in git(clone, "show", "origin/rust:.openhands/state.json").stdout


def test_push_claim_lost_when_remote_advanced(tmp_path: Path):
    """A non-fast-forward rejection is a lost claim - never a fetch/rebase/retry."""
    origin, clone = build_repository(tmp_path)
    other = clone_runner(tmp_path, origin)
    commit_file(other, "winner.txt", "winner's claim\n", "winner's claim")
    git(other, "push", "origin", "rust")

    stage_state_checkpoint(clone, "RECOVERING", operation_id="F:RECOVERY:1")
    git(clone, *IDENTITY, "commit", "-m", "chore(automation): claim recovery F")
    loser_head = git(clone, "rev-parse", "HEAD").stdout.strip()

    module = load_module()
    with pytest.raises(module.LostClaimError, match="remote branch advanced"):
        module.push_claim(repo=clone, remote="origin", branch="rust")

    # The loser's commit never reached the remote and was never rebased.
    origin_commits = git(clone, "rev-list", "origin/rust").stdout.split()
    assert loser_head not in origin_commits
    assert git(clone, "rev-parse", f"{loser_head}^").stdout.strip() in origin_commits
    assert not (clone / ".git" / "rebase-merge").exists()
    assert not (clone / ".git" / "rebase-apply").exists()


def test_push_claim_hard_fails_on_real_push_error(tmp_path: Path):
    """A non-rejection failure (e.g. unknown remote) is a hard GitError, not a
    silent lost claim."""
    _, clone = build_repository(tmp_path)
    stage_state_checkpoint(clone, "RECOVERING", operation_id="F:RECOVERY:1")
    git(clone, *IDENTITY, "commit", "-m", "chore(automation): claim recovery F")

    module = load_module()
    with pytest.raises(module.GitError, match="claim push failed"):
        module.push_claim(repo=clone, remote="no-such-remote", branch="rust")


def test_push_claim_refuses_detached_head(tmp_path: Path):
    _, clone = build_repository(tmp_path)
    stage_state_checkpoint(clone, "RECOVERING", operation_id="F:RECOVERY:1")
    head = git(clone, "rev-parse", "HEAD").stdout.strip()
    git(clone, "checkout", "--detach", head)

    module = load_module()
    with pytest.raises(module.GitError, match="detached HEAD"):
        module.push_claim(repo=clone, remote="origin", branch="rust")


def test_push_claim_never_calls_safe_push(tmp_path: Path):
    """The claim path must not route through the fetch/rebase/retry loop."""
    import inspect

    module = load_module()
    body = inspect.getsource(module.push_claim)
    # No invocation of the retrying push, and no reconciliation of any kind.
    for call_pattern in ("safe_push(", "push(branch", "rebase_onto(", "rebase(", "fetch("):
        assert call_pattern not in body, f"{call_pattern!r} called inside push_claim"
    for forbidden in ("MAX_PUSH_ATTEMPTS",):
        assert forbidden not in body, f"{forbidden!r} appears in push_claim"
    # Exactly one push invocation: a rejected push is terminal, never retried.
    assert body.count('"push"') == 1
    assert body.count('"HEAD:{branch}"') == 1
