#!/usr/bin/env python3
"""PR #51 pipeline-safety regression tests: event validation, deterministic
operation identity and idempotency, atomic recovery claims (including the
two-racing-jobs scenario), verified recovery continuation, and sanitized
failure recording - all against a real bare git origin, with gh calls stubbed
by fixture files (the GitHub network is never touched)."""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
AWH = SCRIPTS / "awh_pipeline.py"
IDENTITY = ("-c", "user.name=AWH Test", "-c", "user.email=awh-test@example.invalid")
GIT_ENV = {**os.environ, "GIT_TERMINAL_PROMPT": "0", "GIT_AUTHOR_NAME": "AWH Test",
           "GIT_AUTHOR_EMAIL": "awh-test@example.invalid",
           "GIT_COMMITTER_NAME": "AWH Test", "GIT_COMMITTER_EMAIL": "awh-test@example.invalid"}


def git(cwd: Path, *arguments: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(["git", "-C", str(cwd), *arguments], capture_output=True, text=True, env=GIT_ENV)
    if check and result.returncode != 0:
        raise AssertionError(f"git {' '.join(arguments)} failed: {result.stderr}")
    return result


def load_pipeline():
    spec = importlib.util.spec_from_file_location("awh_pipeline", AWH)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_checkpoint():
    spec = importlib.util.spec_from_file_location("checkpoint_state", SCRIPTS / "checkpoint_state.py")
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def state_payload(**overrides) -> str:
    payload = {
        "version": 4, "status": "BUILDING", "operation_id": "AWH-AUTO-002:BUILD:1",
        "recovery_attempt": 0, "recovery_claim": None,
        "active_feature": "AWH-AUTO-002", "active_pr": None, "active_branch": "feature/AWH-AUTO-002",
        "active_pr_sha": None, "review_round": 0, "builder_attempt": 1,
        "last_error": None, "last_review": "", "updated_at": None,
    }
    payload.update(overrides)
    return json.dumps(payload, indent=2) + "\n"


class Runner:
    """A clone standing in for one GitHub Actions job, sharing a bare origin."""

    def __init__(self, origin: Path, name: str, initial: bool = False):
        self.path = origin.parent / name
        git(origin.parent, "clone", str(origin), str(self.path))
        if not initial:
            # The branch exists after the seed push; a fresh clone tracks it.
            git(self.path, "checkout", "rust")
        git(self.path, "config", "user.name", "AWH Automation")
        git(self.path, "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")

    def write_state(self, payload: str) -> None:
        (self.path / ".openhands").mkdir(exist_ok=True)
        (self.path / ".openhands" / "state.json").write_text(payload, encoding="utf-8")

    def read_state(self) -> dict:
        return json.loads((self.path / ".openhands" / "state.json").read_text(encoding="utf-8"))

    def run(self, *args: str, env_extra: dict | None = None) -> subprocess.CompletedProcess[str]:
        env = {
            **GIT_ENV, "PYTHONDONTWRITEBYTECODE": "1",
            "AWH_REPO_ROOT": str(self.path),
            **(env_extra or {}),
        }
        return subprocess.run(
            [sys.executable, str(AWH), *args],
            cwd=self.path, text=True, capture_output=True, check=False, env=env,
        )

    def stage(self) -> None:
        git(self.path, "add", ".openhands/state.json")


@pytest.fixture()
def repo(tmp_path: Path):
    """Bare origin seeded with one commit, plus a 'primary' runner clone."""
    origin = tmp_path / "origin.git"
    origin.mkdir()
    git(origin, "init", "--bare", "-b", "rust")
    primary = Runner(origin, "primary", initial=True)
    git(primary.path, "checkout", "-b", "rust")
    (primary.path / "seed.txt").write_text("seed\n", encoding="utf-8")
    schema_src = ROOT / ".openhands" / "state.schema.json"
    (primary.path / ".openhands").mkdir(exist_ok=True)
    (primary.path / ".openhands" / "state.schema.json").write_text(
        schema_src.read_text(encoding="utf-8"), encoding="utf-8")
    git(primary.path, "add", "-A")
    git(primary.path, *IDENTITY, "commit", "-m", "seed")
    git(primary.path, "push", "-u", "origin", "rust")
    return type("Repo", (), {"origin": origin, "primary": primary, "tmp": tmp_path})()


def stale_hours(hours: float) -> str:
    return (datetime.now(timezone.utc) - timedelta(hours=hours)).isoformat().replace("+00:00", "Z")


# --------------------------------------------------------------------------
# Event validation: payloads are notifications, never authority.
# --------------------------------------------------------------------------

def test_validate_event_rejects_feature_mismatch(repo):
    repo.primary.write_state(state_payload(status="BUILDING", active_feature="AWH-AUTO-002"))
    result = repo.primary.run("validate-event", "--agent", "builder", "--feature", "AWH-AUTO-999")
    assert result.returncode == 3
    assert "does not match active_feature" in result.stdout


def test_validate_event_rejects_wrong_status_for_agent(repo):
    repo.primary.write_state(state_payload(status="REVIEWING", active_pr=51, active_branch="feature/AWH-AUTO-002"))
    result = repo.primary.run("validate-event", "--agent", "builder", "--feature", "AWH-AUTO-002")
    assert result.returncode == 3
    assert "does not admit a builder run" in result.stdout


def test_validate_event_rejects_pr_mismatch_for_reviewer(repo):
    repo.primary.write_state(state_payload(status="PR_OPEN", active_pr=51, operation_id=None))
    result = repo.primary.run("validate-event", "--agent", "reviewer", "--pr", "99")
    assert result.returncode == 3
    assert "does not match active_pr" in result.stdout


def test_validate_event_accepts_current_event(repo):
    repo.primary.write_state(state_payload(status="PR_OPEN", active_pr=51))
    result = repo.primary.run("validate-event", "--agent", "reviewer", "--pr", "51", "--feature", "AWH-AUTO-002")
    assert result.returncode == 0
    assert "Event validated" in result.stdout


def test_validate_event_builder_requires_feature(repo):
    repo.primary.write_state(state_payload(status="BUILDING"))
    result = repo.primary.run("validate-event", "--agent", "builder")
    assert result.returncode == 3
    assert "no feature_id" in result.stdout


# --------------------------------------------------------------------------
# begin-stage: deterministic operation identity and true idempotency.
# --------------------------------------------------------------------------

def test_begin_stage_assigns_deterministic_operation_id(repo):
    repo.primary.write_state(state_payload(status="BUILDING", operation_id="AWH-AUTO-002:BUILD:1", builder_attempt=1))
    repo.primary.stage()
    git(repo.primary.path, "commit", "-m", "seed state")

    result = repo.primary.run(
        "begin-stage", "--stage", "builder", "--feature", "AWH-AUTO-002",
        "--message", "test: builder re-entry",
    )

    assert result.returncode == 0, (result.stdout, result.stderr)
    # Retrying operation ...:BUILD:1 keeps the counter and the id.
    assert "AWH-AUTO-002:BUILD:1" in result.stdout
    assert "idempotent retry" in result.stdout
    state = json.loads(subprocess.run(
        ["git", "-C", str(repo.primary.path), "show", "HEAD:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert state["operation_id"] == "AWH-AUTO-002:BUILD:1"
    assert state["builder_attempt"] == 1


def test_begin_stage_new_operation_advances_counter_exactly_once(repo):
    repo.primary.write_state(state_payload(status="FIXING", operation_id="AWH-AUTO-002:REVIEW:2",
                                           review_round=2, active_pr=51))
    repo.primary.stage()
    git(repo.primary.path, "commit", "-m", "seed state")

    result = repo.primary.run(
        "begin-stage", "--stage", "builder", "--feature", "AWH-AUTO-002",
        "--message", "test: fix enters build",
    )

    assert result.returncode == 0, (result.stdout, result.stderr)
    assert "AWH-AUTO-002:BUILD:2" in result.stdout
    assert "new operation (attempt 2)" in result.stdout
    state = json.loads(subprocess.run(
        ["git", "-C", str(repo.primary.path), "show", "HEAD:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert state["operation_id"] == "AWH-AUTO-002:BUILD:2"
    assert state["builder_attempt"] == 2
    assert state["status"] == "BUILDING"


def test_begin_stage_rejects_stale_counter_without_mutation(repo):
    # Checkpoint holds BUILD:3 but builder_attempt is 5: the event identity
    # cannot be trusted to be the same or next operation; fail closed.
    repo.primary.write_state(state_payload(status="BUILDING", operation_id="AWH-AUTO-002:BUILD:3", builder_attempt=5))
    before = repo.primary.read_state()

    result = repo.primary.run(
        "begin-stage", "--stage", "builder", "--feature", "AWH-AUTO-002",
        "--message", "test: stale counter",
    )

    assert result.returncode == 3
    assert "refusing to guess" in result.stdout
    assert repo.primary.read_state() == before  # no mutation at all


def test_begin_stage_stale_event_leaves_state_untouched(repo):
    repo.primary.write_state(state_payload(status="REVIEWING", active_pr=51, operation_id="AWH-AUTO-002:REVIEW:1"))
    before = repo.primary.read_state()

    result = repo.primary.run(
        "begin-stage", "--stage", "builder", "--feature", "AWH-AUTO-002",
        "--message", "test: stale builder event",
    )

    assert result.returncode == 3
    assert "STALE_EVENT" in result.stdout
    assert repo.primary.read_state() == before


def test_begin_stage_merger_has_no_counter_suffix(repo):
    repo.primary.write_state(state_payload(status="PR_OPEN", active_pr=51, operation_id="AWH-AUTO-002:REVIEW:1"))
    repo.primary.stage()
    git(repo.primary.path, "commit", "-m", "seed state")

    result = repo.primary.run(
        "begin-stage", "--stage", "merger", "--feature", "AWH-AUTO-002", "--pr", "51",
        "--message", "test: merge begins",
    )

    assert result.returncode == 0, (result.stdout, result.stderr)
    state = json.loads(subprocess.run(
        ["git", "-C", str(repo.primary.path), "show", "HEAD:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert state["operation_id"] == "AWH-AUTO-002:MERGE"
    assert state["status"] == "MERGING"

    # A retry of the same merge re-enters the same logical operation.
    result = repo.primary.run(
        "begin-stage", "--stage", "merger", "--feature", "AWH-AUTO-002", "--pr", "51",
        "--message", "test: merge retry",
    )
    assert result.returncode == 0, (result.stdout, result.stderr)
    assert "idempotent retry" in result.stdout


# --------------------------------------------------------------------------
# Recovery: atomic claim/lease, the two-racing-jobs scenario, verified
# continuation, and finalize discipline.
# --------------------------------------------------------------------------

def seed_stale_build(repo):
    payload = state_payload(status="BUILDING", operation_id="AWH-AUTO-002:BUILD:2",
                            builder_attempt=2, updated_at=stale_hours(3))
    repo.primary.write_state(payload)
    repo.primary.stage()
    git(repo.primary.path, "commit", "-m", "stale build state")
    git(repo.primary.path, "push", "origin", "rust")


def test_claim_recovery_single_job_acquires_claim(repo):
    seed_stale_build(repo)
    job = Runner(repo.origin, "recovery-a")

    result = job.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:BUILD:2",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    )

    assert result.returncode == 0, (result.stdout, result.stderr)
    assert result.stdout.strip().endswith("CLAIMED")
    assert "recovery claim acquired" in result.stdout
    state = json.loads(subprocess.run(
        ["git", "-C", str(job.path), "show", "origin/rust:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert state["status"] == "RECOVERING"
    assert state["recovery_attempt"] == 1
    assert state["recovery_claim"]["owner"] == "job-a"
    assert state["recovery_claim"]["stale_status"] == "BUILDING"
    assert state["recovery_claim"]["operation_id"] == "AWH-AUTO-002:BUILD:2"
    assert state["operation_id"] == "AWH-AUTO-002:RECOVERY:1"
    # The winner published its claim through the fail-closed path.
    log = git(job.path, "log", "--oneline", "origin/rust").stdout
    assert "claim recovery AWH-AUTO-002" in log


def test_claim_rejects_when_remote_identity_differs(repo):
    """The claim is only valid against the exact state the caller observed."""
    seed_stale_build(repo)
    job = Runner(repo.origin, "recovery-a")

    wrong_feature = job.run(
        "claim-recovery", "--feature", "AWH-OTHER-9",
        "--operation-id", "AWH-AUTO-002:BUILD:2",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    )
    assert wrong_feature.returncode == 0
    assert wrong_feature.stdout.strip().endswith("LOST_CLAIM")

    wrong_operation = job.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:BUILD:1",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    )
    assert wrong_operation.returncode == 0
    assert wrong_operation.stdout.strip().endswith("LOST_CLAIM")

    # Neither attempt touched the remote.
    state = json.loads(subprocess.run(
        ["git", "-C", str(job.path), "show", "origin/rust:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert state["status"] == "BUILDING"  # untouched, still stale


def test_two_workers_race_same_remote_state_exactly_one_claim(repo, monkeypatch):
    """The dangerous sequence: two recovery workers observe the SAME remote
    state, both build a claim locally, and both push. Exactly one publishes;
    the loser's claim push is rejected as non-fast-forward and it reports
    LOST_CLAIM without ever rebasing or retrying."""
    seed_stale_build(repo)
    base = git(repo.primary.path, "rev-parse", "HEAD").stdout.strip()

    owner_a, owner_b = "recovery-job-a", "recovery-job-b"
    runner_a = Runner(repo.origin, "recovery-a")
    runner_b = Runner(repo.origin, "recovery-b")
    assert git(runner_a.path, "rev-parse", "HEAD").stdout.strip() == base
    assert git(runner_b.path, "rev-parse", "HEAD").stdout.strip() == base

    # Worker B claims in-process; its observation of the remote is patched so
    # that worker A publishes ITS claim immediately after B reads the shared
    # stale state - the exact window where both believe they can win.
    monkeypatch.setenv("AWH_REPO_ROOT", str(runner_b.path))
    monkeypatch.chdir(runner_b.path)
    module = load_pipeline()

    original_read = module.read_remote_state

    def read_then_let_a_win():
        observed = original_read()
        assert observed["status"] == "BUILDING", "B must observe the shared stale state"
        # A races ahead and publishes its claim before B can.
        result_a = runner_a.run(
            "claim-recovery", "--feature", "AWH-AUTO-002",
            "--operation-id", "AWH-AUTO-002:BUILD:2",
            "--claim-owner", owner_a, "--stale-minutes", "90",
        )
        assert result_a.returncode == 0, (result_a.stdout, result_a.stderr)
        assert result_a.stdout.strip().endswith("CLAIMED")
        return observed

    monkeypatch.setattr(module, "read_remote_state", read_then_let_a_win)
    result_b = module.claim_recovery(
        feature_id="AWH-AUTO-002",
        expected_operation_id="AWH-AUTO-002:BUILD:2",
        claim_owner=owner_b,
        stale_minutes=90,
    )
    result_a_value = module.ClaimResult.CLAIMED  # A's subprocess reported CLAIMED

    assert result_a_value != result_b
    assert {result_a_value, result_b} == {module.ClaimResult.CLAIMED, module.ClaimResult.LOST_CLAIM}

    # The remote shows exactly one claim, from exactly one owner.
    remote = json.loads(subprocess.run(
        ["git", "-C", str(runner_a.path), "show", "origin/rust:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert remote["status"] == "RECOVERING"
    assert remote["recovery_claim"]["owner"] in {owner_a, owner_b}
    assert remote["recovery_claim"]["owner"] == owner_a

    # MOST IMPORTANT: the losing claim's commit does NOT appear on origin/rust.
    loser_head = git(runner_b.path, "rev-parse", "HEAD").stdout.strip()
    origin_commits = git(runner_a.path, "rev-list", "origin/rust").stdout.split()
    assert loser_head not in origin_commits, "the loser's claim commit reached the remote"

    # The loser stands down cleanly: its local commit survives unrebased (its
    # parent is still the shared base), and no rebase was left dangling.
    assert git(runner_b.path, "rev-parse", f"{loser_head}^").stdout.strip() == base
    assert not (runner_b.path / ".git" / "rebase-merge").exists()
    assert not (runner_b.path / ".git" / "rebase-apply").exists()


def test_recovery_claim_never_rebases_after_non_ff(repo, monkeypatch):
    """Regression: after a rejected claim push there must be exactly ONE push
    attempt and ZERO rebase calls - a second push or any rebase here would
    recreate the race the claim is meant to close."""
    seed_stale_build(repo)
    base = git(repo.primary.path, "rev-parse", "HEAD").stdout.strip()

    owner_a, owner_b = "recovery-job-a", "recovery-job-b"
    runner_a = Runner(repo.origin, "recovery-a")
    runner_b = Runner(repo.origin, "recovery-b")

    monkeypatch.setenv("AWH_REPO_ROOT", str(runner_b.path))
    monkeypatch.chdir(runner_b.path)
    module = load_pipeline()

    # A publishes its claim right after B commits its own claim locally, so
    # B's push is guaranteed to be rejected as non-fast-forward.
    original_commit = module.stage_and_commit

    def commit_then_let_a_win(message):
        original_commit(message)
        result_a = runner_a.run(
            "claim-recovery", "--feature", "AWH-AUTO-002",
            "--operation-id", "AWH-AUTO-002:BUILD:2",
            "--claim-owner", owner_a, "--stale-minutes", "90",
        )
        assert result_a.returncode == 0, (result_a.stdout, result_a.stderr)

    monkeypatch.setattr(module, "stage_and_commit", commit_then_let_a_win)

    # Instrument every safe_git call worker B makes so the test fails if the
    # claim path ever re-fetches, rebases, or pushes a second time.
    calls: list[tuple[str, ...]] = []
    real_loader = module.load_script_module

    def recording_loader(name):
        loaded = real_loader(name)
        if name == "safe_git":
            class RecordingSafeGit:
                def __getattr__(self, item):
                    return getattr(loaded, item)

                def sync(self, branch):
                    calls.append(("sync", branch))
                    return loaded.sync(branch)

                def push_claim(self, **kwargs):
                    calls.append(("push_claim", kwargs["branch"]))
                    return loaded.push_claim(**kwargs)

            return RecordingSafeGit()
        return loaded

    monkeypatch.setattr(module, "load_script_module", recording_loader)

    result = module.claim_recovery(
        feature_id="AWH-AUTO-002",
        expected_operation_id="AWH-AUTO-002:BUILD:2",
        claim_owner=owner_b,
        stale_minutes=90,
    )
    assert result is module.ClaimResult.LOST_CLAIM

    # Exactly one claim push was attempted, and nothing else was tried after
    # the rejection: no re-fetch, no rebase, no second push.
    assert calls.count(("push_claim", "rust")) == 1
    rebases = [c for c in calls if "rebase" in c[0]]
    assert rebases == []
    extra_syncs = [c for c in calls if c[0] == "sync"]
    assert len(extra_syncs) == 1  # only the initial observation sync

    # B's local claim commit was never rebased onto A's history.
    loser_head = git(runner_b.path, "rev-parse", "HEAD").stdout.strip()
    assert git(runner_b.path, "rev-parse", f"{loser_head}^").stdout.strip() == base
    origin_commits = git(runner_a.path, "rev-list", "origin/rust").stdout.split()
    assert loser_head not in origin_commits


def test_second_claim_same_owner_is_reentry_not_duplicate(repo):
    seed_stale_build(repo)
    job = Runner(repo.origin, "recovery-a")
    assert job.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:BUILD:2",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    ).returncode == 0

    # The same job re-running (retry) re-enters its own claim, no second claim.
    result = job.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:RECOVERY:1",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    )
    assert result.returncode == 0, (result.stdout, result.stderr)
    assert "already held by this owner" in result.stdout
    assert result.stdout.strip().endswith("CLAIMED")

    # The re-entry published nothing new: the remote is unchanged.
    remote = json.loads(subprocess.run(
        ["git", "-C", str(job.path), "show", "origin/rust:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert remote["recovery_attempt"] == 1
    assert remote["recovery_claim"]["owner"] == "job-a"


def test_claim_recovery_ignores_fresh_checkpoint(repo):
    payload = state_payload(status="BUILDING", updated_at=stale_hours(0.1))
    repo.primary.write_state(payload)
    repo.primary.stage()
    git(repo.primary.path, "commit", "-m", "fresh build state")
    git(repo.primary.path, "push", "origin", "rust")
    job = Runner(repo.origin, "recovery-a")

    result = job.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:BUILD:1",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    )

    assert result.returncode == 0
    assert "not stale" in result.stdout
    assert result.stdout.strip().endswith("NO_OP")
    remote = json.loads(subprocess.run(
        ["git", "-C", str(job.path), "show", "origin/rust:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert remote["status"] == "BUILDING"  # untouched


def test_claim_recovery_ignores_terminal_checkpoint(repo):
    payload = state_payload(status="BLOCKED", last_error="previous failure", updated_at=stale_hours(5))
    repo.primary.write_state(payload)
    repo.primary.stage()
    git(repo.primary.path, "commit", "-m", "blocked state")
    git(repo.primary.path, "push", "origin", "rust")
    job = Runner(repo.origin, "recovery-a")

    result = job.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:BUILD:1",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    )

    assert result.returncode == 0
    assert "No recoverable checkpoint: status=BLOCKED" in result.stdout
    assert result.stdout.strip().endswith("NO_OP")


def test_claim_recovery_requires_owner(repo):
    job = Runner(repo.origin, "recovery-a")
    result = job.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:BUILD:1",
        "--claim-owner", "",
    )
    assert result.returncode == 1
    assert "non-empty --claim-owner" in result.stdout + result.stderr


def test_finalize_recovery_releases_claim_and_moves_to_verified_target(repo):
    seed_stale_build(repo)
    job = Runner(repo.origin, "recovery-a")
    assert job.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:BUILD:2",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    ).returncode == 0

    result = job.run(
        "finalize-recovery", "--owner", "job-a", "--to", "BUILDING",
        "--message", "test: resume build",
        "--set", "operation_id=\"AWH-AUTO-002:BUILD:2\"",
    )

    assert result.returncode == 0, (result.stdout, result.stderr)
    state = json.loads(subprocess.run(
        ["git", "-C", str(job.path), "show", "origin/rust:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert state["status"] == "BUILDING"
    assert state["recovery_claim"] is None
    assert state["operation_id"] == "AWH-AUTO-002:BUILD:2"


def test_finalize_recovery_refuses_when_another_owner_holds_the_lease(repo):
    seed_stale_build(repo)
    job_a = Runner(repo.origin, "recovery-a")
    job_b = Runner(repo.origin, "recovery-b")
    assert job_a.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:BUILD:2",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    ).returncode == 0

    # job_b's stale clone still says BUILDING: its finalize must stand down,
    # never stomp job-a's claim.
    result = job_b.run(
        "finalize-recovery", "--owner", "job-b", "--to", "BUILDING",
        "--message", "test: b must not finalize",
    )
    assert result.returncode == 3
    assert "STALE_EVENT" in result.stdout

    remote = json.loads(subprocess.run(
        ["git", "-C", str(job_a.path), "show", "origin/rust:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert remote["recovery_claim"]["owner"] == "job-a"


def test_finalize_recovery_illegal_target_is_rejected_without_mutation(repo):
    seed_stale_build(repo)
    job = Runner(repo.origin, "recovery-a")
    assert job.run(
        "claim-recovery", "--feature", "AWH-AUTO-002",
        "--operation-id", "AWH-AUTO-002:BUILD:2",
        "--claim-owner", "job-a", "--stale-minutes", "90",
    ).returncode == 0

    result = job.run(
        "finalize-recovery", "--owner", "job-a", "--to", "PLANNING",
        "--message", "test: illegal target",
    )
    assert result.returncode == 1
    assert "not allowed" in result.stdout + result.stderr


# --------------------------------------------------------------------------
# decide-recovery: inspect reality, never blindly replay.
# --------------------------------------------------------------------------

def test_decide_recovery_table_covers_every_stale_status():
    module = load_pipeline()
    base = {"active_feature": "F", "active_pr": 7, "last_review": "fix it"}
    # PLANNING resumes planning; BUILDING depends on PR reality; the review
    # states resume the review; FIXING resumes the fix; MERGING retries merge.
    assert module.decide_recovery({**base, "status": "PLANNING"}, False, False, False)["target"] == "PLANNING"
    assert module.decide_recovery({**base, "status": "BUILDING"}, False, False, False)["dispatch"]["event"] == "awh.build"
    assert module.decide_recovery({**base, "status": "BUILDING"}, True, False, False)["target"] == "PR_OPEN"
    assert module.decide_recovery({**base, "status": "BUILDING"}, False, True, True)["target"] == "COMPLETED"
    assert module.decide_recovery({**base, "status": "REVIEWING"}, True, False, True)["dispatch"]["event"] == "awh.review"
    assert module.decide_recovery({**base, "status": "FIXING"}, True, False, True)["dispatch"]["event"] == "awh.fix"
    assert module.decide_recovery({**base, "status": "MERGING"}, False, True, False)["target"] == "COMPLETED"
    assert module.decide_recovery({**base, "status": "MERGING"}, True, False, False)["dispatch"]["event"] == "awh.review-complete"
    # A missing PR blocks instead of replaying anything.
    assert module.decide_recovery({**base, "status": "REVIEWING"}, False, False, False)["target"] == "BLOCKED"


def test_decide_recovery_never_replays_blindly_when_pr_is_gone():
    module = load_pipeline()
    decision = module.decide_recovery(
        {"status": "REVIEWING", "active_feature": "F", "active_pr": 404, "last_review": ""},
        pr_open=False, pr_merged=False, branch_exists=True,
    )
    assert decision["target"] == "BLOCKED"
    assert decision["dispatch"] is None
    assert "no longer exists" in decision["reason"]


# --------------------------------------------------------------------------
# Failure recording: sanitized diagnostics, terminal-state respect.
# --------------------------------------------------------------------------

def test_record_failure_sanitizes_secrets_and_blocks(repo):
    repo.primary.write_state(state_payload(status="BUILDING"))
    repo.primary.stage()
    git(repo.primary.path, "commit", "-m", "seed state")

    detail = "git push failed: remote returned 403 token ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ12 and nvapi-DeadBeefCafe123456789"
    result = repo.primary.run(
        "record-failure", "--agent", "builder", "--stage", "verification",
        "--detail", detail, "--feature", "AWH-AUTO-002", "--pr", "51",
        "--branch", "feature/AWH-AUTO-002", "--commit", "abc1234",
    )

    assert result.returncode == 0, (result.stdout, result.stderr)
    state = json.loads(subprocess.run(
        ["git", "-C", str(repo.primary.path), "show", "HEAD:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert state["status"] == "BLOCKED"
    diagnostic = json.loads(state["last_error"])
    assert diagnostic["agent"] == "builder"
    assert diagnostic["stage"] == "verification"
    assert diagnostic["operation_id"] == "AWH-AUTO-002:BUILD:1"
    assert diagnostic["feature_id"] == "AWH-AUTO-002"
    assert diagnostic["pr"] == 51
    assert diagnostic["branch"] == "feature/AWH-AUTO-002"
    assert diagnostic["commit"] == "abc1234"
    assert "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ12" not in state["last_error"]
    assert "nvapi-DeadBeefCafe123456789" not in state["last_error"]
    assert "ghp_[REDACTED]" in state["last_error"]
    assert "nvapi-[REDACTED]" in state["last_error"]


def test_record_failure_respects_terminal_state(repo):
    payload = state_payload(status="COMPLETED", active_feature=None, active_pr=None,
                            active_branch=None, operation_id=None, updated_at=stale_hours(1))
    repo.primary.write_state(payload)
    repo.primary.stage()
    git(repo.primary.path, "commit", "-m", "seed state")

    result = repo.primary.run("record-failure", "--agent", "merger", "--stage", "late",
                               "--detail", "failure after completion")

    assert result.returncode == 0
    assert "terminal" in result.stdout
    state = json.loads(subprocess.run(
        ["git", "-C", str(repo.primary.path), "show", "HEAD:.openhands/state.json"],
        capture_output=True, text=True, env=GIT_ENV,
    ).stdout)
    assert state["status"] == "COMPLETED"  # untouched
    assert state["last_error"] is None


def test_record_failure_is_cas_guarded_on_operation_id(repo, monkeypatch):
    # Simulate a concurrent writer advancing the operation between the read
    # and the CAS: the failure recorder must refuse instead of blocking new work.
    repo.primary.write_state(state_payload(status="BUILDING", operation_id="AWH-AUTO-002:BUILD:1"))
    repo.primary.stage()
    git(repo.primary.path, "commit", "-m", "seed state")

    # Point the scripts at the fixture clone BEFORE any module loads: the
    # modules resolve STATE_PATH at import time from this variable.
    monkeypatch.setenv("AWH_REPO_ROOT", str(repo.primary.path))
    monkeypatch.chdir(repo.primary.path)
    module = load_pipeline()
    checkpoint = load_checkpoint()

    original = module.run_checkpoint

    def racing_checkpoint(argv):
        # Another writer moves the state right before our CAS lands.
        state = checkpoint.load_state()
        state["status"] = "PR_OPEN"
        state["active_pr"] = 51
        checkpoint.atomic_write(state)
        return original(argv)

    monkeypatch.setattr(module, "run_checkpoint", racing_checkpoint)

    with pytest.raises(SystemExit) as caught:
        module.record_failure("builder", "verification", "boom", "AWH-AUTO-002", None, None, None)
    assert "expected current status BUILDING" in str(caught.value)
