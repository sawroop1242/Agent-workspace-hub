#!/usr/bin/env python3
"""AWH pipeline safety layer: event validation, CAS stage entry with
deterministic operation identity, atomic recovery claims, verified recovery
continuation, and sanitized failure recording.

This module is the single orchestration entry point the AWH workflows use
for checkpoint mutation. It is deliberately thin: every state change goes
through scripts/checkpoint_state.py's compare-and-swap `transition`
implementation, and every shared-branch push goes through scripts/safe_git.py.
Nothing here duplicates those mechanisms; it composes them so all workflows
share exactly one tested path.

Exit codes:
  0  action succeeded, or was a safe no-op (stale event, claim held by
     another job, nothing recoverable);
  3  stale event / lost claim race: the caller must stop without doing work;
  1  hard failure: fail closed, never guess.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
# The checkpoint always lives in the repository root the workflow operates on.
# Tests point this at fixture clones; in real runs it equals ROOT.
REPO_ROOT = Path(os.environ.get("AWH_REPO_ROOT", ROOT))
SHARED_BRANCH = "rust"
STATE_FILE = ".openhands/state.json"

STALE_EXIT = 3

GIT_ENV = {**os.environ, "GIT_TERMINAL_PROMPT": "0"}

# Token shapes that must never reach the checkpoint's last_error field.
SECRET_PATTERNS = [
    (re.compile(r"ghp_[A-Za-z0-9]{16,}"), "ghp_[REDACTED]"),
    (re.compile(r"gho_[A-Za-z0-9]{16,}"), "gho_[REDACTED]"),
    (re.compile(r"ghu_[A-Za-z0-9]{16,}"), "ghu_[REDACTED]"),
    (re.compile(r"ghs_[A-Za-z0-9]{16,}"), "ghs_[REDACTED]"),
    (re.compile(r"ghr_[A-Za-z0-9]{16,}"), "ghr_[REDACTED]"),
    (re.compile(r"github_pat_[A-Za-z0-9_]{16,}"), "github_pat_[REDACTED]"),
    (re.compile(r"nvapi-[A-Za-z0-9_-]{16,}"), "nvapi-[REDACTED]"),
    (re.compile(r"sk-[A-Za-z0-9]{16,}"), "sk-[REDACTED]"),
    (re.compile(r"(?i)bearer\s+[A-Za-z0-9._~+/-]{8,}"), "Bearer [REDACTED]"),
]
MAX_DETAIL_CHARS = 2000

# One place describing every pipeline stage: which statuses admit entry, the
# target status of the entry transition, the counter that enumerates its
# attempts (None for stages without one), and the event-validation agent.
# The builder workflow serves both the initial build (arriving at BUILDING)
# and the fix loop (arriving at FIXING); both legally continue into BUILDING.
STAGES = {
    "builder": {"agent": "builder", "target": "BUILDING", "counter": "builder_attempt", "op": "BUILD"},
    "fixer": {"agent": "fixer", "target": "FIXING", "counter": "review_round", "op": "FIX"},
    "reviewer": {"agent": "reviewer", "target": "REVIEWING", "counter": "review_round", "op": "REVIEW"},
    "merger": {"agent": "merger", "target": "MERGING", "counter": None, "op": "MERGE"},
}


def load_script_module(name: str):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sanitize_detail(text: str) -> str:
    """Redact credential-shaped substrings and bound the length."""
    cleaned = text
    for pattern, replacement in SECRET_PATTERNS:
        cleaned = pattern.sub(replacement, cleaned)
    cleaned = re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", cleaned)
    if len(cleaned) > MAX_DETAIL_CHARS:
        cleaned = cleaned[:MAX_DETAIL_CHARS] + "...[truncated]"
    return cleaned.strip()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def minutes_since(value: str | None) -> float | None:
    if not value:
        return None
    try:
        stamp = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    return (datetime.now(timezone.utc) - stamp).total_seconds() / 60.0


def split_operation_id(operation_id: str | None) -> tuple[str, int] | None:
    """Split 'FEATURE:STAGE:N' into (prefix, N); None when malformed."""
    if not operation_id:
        return None
    prefix, _, counter = operation_id.rpartition(":")
    if not counter.isdigit():
        return None
    return prefix, int(counter)


def run_checkpoint(argv: list[str]) -> dict[str, Any]:
    """Invoke the single checkpoint implementation (compare-and-swap).

    SystemExit propagates on any CAS mismatch so a stale workflow can never
    overwrite newer state.
    """
    module = load_script_module("checkpoint_state")
    module.main(argv)
    return module.load_state()


def load_state() -> dict[str, Any]:
    return load_script_module("checkpoint_state").load_state()


def stage_and_push(checkpoint_module, message: str) -> None:
    """Stage the checkpoint file and push through the single safe-Git path."""
    result = subprocess.run(["git", "add", STATE_FILE], capture_output=True, text=True, env=GIT_ENV)
    if result.returncode != 0:
        raise SystemExit(f"awh_pipeline: git add {STATE_FILE} failed: {sanitize_detail(result.stderr)}")
    load_script_module("safe_git").push(SHARED_BRANCH, message)


# --------------------------------------------------------------------------
# Event validation: dispatch payloads are notifications, never authority.
# --------------------------------------------------------------------------

AGENT_ADMITTED_STATUSES = {
    "builder": {"BUILDING", "FIXING"},
    "fixer": {"FIXING"},
    "reviewer": {"PR_OPEN", "REVIEWING"},
    "merger": {"PR_OPEN", "MERGING"},
    "starter": {"IDLE", "COMPLETED", "PLANNING"},
}


def validate_event(agent: str, state: dict[str, Any], feature: str | None, pr: int | None) -> tuple[bool, str]:
    status = state["status"]
    active_feature = state["active_feature"]
    active_pr = state["active_pr"]

    admitted = AGENT_ADMITTED_STATUSES.get(agent)
    if admitted is None:
        return False, f"unknown agent {agent!r}"
    if status not in admitted:
        return False, f"status {status!r} does not admit a {agent} run (expected {' or '.join(sorted(admitted))})"

    if agent in {"builder", "fixer"}:
        if not feature:
            return False, "event carries no feature_id"
        if active_feature is not None and feature != active_feature:
            return False, f"event feature {feature!r} does not match active_feature {active_feature!r}"
        return True, "event agrees with checkpoint"

    if agent in {"reviewer", "merger"}:
        if pr is None:
            return False, "event carries no pr_number"
        if active_pr is not None and pr != active_pr:
            return False, f"event pr {pr} does not match active_pr {active_pr}"
        if active_feature is not None and feature is not None and feature != active_feature:
            return False, f"event feature {feature!r} does not match active_feature {active_feature!r}"
        return True, "event agrees with checkpoint"

    # starter
    if status == "PLANNING" and active_feature is not None and feature and feature != active_feature:
        return False, f"resume requested {feature!r} but active_feature is {active_feature!r}"
    return True, "event agrees with checkpoint"


# --------------------------------------------------------------------------
# begin-stage: deterministic operation identity + CAS transition + safe push.
# --------------------------------------------------------------------------

def begin_stage(
    stage: str,
    feature: str | None,
    extra_assignments: dict[str, Any],
    message: str,
) -> dict[str, Any]:
    """Enter a pipeline stage under CAS with a deterministic operation id.

    Idempotency contract: operation ids are 'FEATURE:STAGE:N' where N is the
    stage's own attempt counter read from the authoritative checkpoint. A
    retry of the current operation re-enters the stage WITHOUT incrementing
    any counter, so a re-dispatch of the same logical operation can never
    create a second one. Only a genuinely new operation (N == current+1)
    advances the counter. Anything else fails closed as a stale event.
    """
    spec = STAGES[stage]
    agent = spec["agent"]
    target = spec["target"]
    counter_field = spec["counter"]

    state = load_state()
    ok, reason = validate_event(agent, state, feature, extra_assignments.get("active_pr"))
    if not ok:
        print(f"STALE_EVENT: {reason}; doing nothing.")
        raise SystemExit(STALE_EXIT)

    if not feature:
        raise SystemExit("awh_pipeline: begin-stage requires --feature")

    assignments: dict[str, Any] = dict(extra_assignments)
    assignments["active_feature"] = feature
    assignments.setdefault("last_error", None)

    if counter_field is None:
        # Counterless stages (merge) have exactly one logical operation per
        # feature; a retry of the same id re-enters idempotently.
        operation_id = f"{feature}:{spec['op']}"
        if state["operation_id"] == operation_id:
            outcome = "idempotent retry of the same operation"
        else:
            outcome = "new operation"
    else:
        prefix = f"{feature}:{spec['op']}"
        current_counter = int(state[counter_field])
        parsed = split_operation_id(state["operation_id"])

        if parsed is not None and parsed[0] == prefix and parsed[1] == current_counter:
            # Same logical operation as the checkpoint currently holds: a retry.
            operation_id = state["operation_id"]
            outcome = "idempotent retry of the same operation"
        elif parsed is None or parsed[0] != prefix:
            # First (or next genuinely new) operation for this stage.
            next_counter = current_counter + 1
            operation_id = f"{prefix}:{next_counter}"
            assignments[counter_field] = next_counter
            outcome = f"new operation (attempt {next_counter})"
        else:
            # Same stage but a stale or forged counter: fail closed.
            print(
                f"STALE_EVENT: checkpoint operation_id {state['operation_id']!r} is for the same "
                f"stage but not the expected counter {current_counter}; refusing to guess."
            )
            raise SystemExit(STALE_EXIT)

    assignments["operation_id"] = operation_id

    transition_args = ["transition"]
    for status in sorted(AGENT_ADMITTED_STATUSES[agent]):
        transition_args += ["--expect-status", status]
    transition_args += ["--to", target]
    for key, value in assignments.items():
        transition_args += ["--set", f"{key}={json.dumps(value)}"]

    run_checkpoint(transition_args)
    stage_and_push(load_script_module("checkpoint_state"), message)
    print(f"operation_id={operation_id}")
    print(f"awh_pipeline: began {stage} for {feature} ({outcome}); operation_id={operation_id}")
    return {"operation_id": operation_id, "status": target}


# --------------------------------------------------------------------------
# Recovery: atomic claim/lease against the checkpoint, no external services.
# --------------------------------------------------------------------------

def claim_recovery(owner: str, stale_minutes: int, lease_minutes: int) -> dict[str, Any]:
    """Atomically claim recovery of a stale checkpoint, or safely stand down.

    The claim is a CAS transition <stale> -> RECOVERING carrying a claim
    object (operation_id, feature_id, attempt, claimed_at, owner,
    stale_status). Two recovery jobs racing on the same stale checkpoint
    cannot both win: the first push lands, and the loser either fails its
    CAS read, hits a non-fast-forward push that conflicts on state.json
    (never force-resolved), or observes the winner's claim on re-fetch and
    exits without doing work.
    """
    if not owner or not owner.strip():
        raise SystemExit("awh_pipeline: claim-recovery requires a non-empty --owner")

    checkpoint = load_script_module("checkpoint_state")
    state = checkpoint.load_state()
    status = state["status"]

    if status not in checkpoint.STALEABLE_STATUSES:
        print(f"No recoverable checkpoint: status={status}")
        return {"claimed": False, "reason": f"status={status}"}

    age = minutes_since(state["updated_at"])
    claim = state.get("recovery_claim")

    if status == "RECOVERING" and claim:
        if claim.get("owner") == owner:
            print("Recovery claim already held by this owner; continuing.")
            return {"claimed": True, "reentry": True, "claim": claim}
        claim_age = minutes_since(claim.get("claimed_at"))
        if claim_age is not None and claim_age < lease_minutes:
            print(
                f"Recovery claim already held by {claim.get('owner')!r} "
                f"(age {claim_age:.1f}m < lease {lease_minutes}m); standing down safely."
            )
            return {"claimed": False, "reason": "claim held by another job"}
        print(f"Recovery lease expired (age {claim_age:.1f}m); taking over the claim.")
    elif age is not None and age < stale_minutes:
        print(f"Checkpoint is not stale (age {age:.1f}m < {stale_minutes}m); nothing to recover.")
        return {"claimed": False, "reason": "not stale"}

    attempt = int(state["recovery_attempt"]) + 1
    recovery_operation = f"{state['active_feature'] or 'PIPELINE'}:RECOVERY:{attempt}"
    claim_object = {
        "operation_id": state["operation_id"],
        "feature_id": state["active_feature"],
        "attempt": attempt,
        "claimed_at": utc_now(),
        "owner": owner,
        "stale_status": status,
    }
    transition_args = [
        "transition",
        "--expect-status", status,
        "--to", "RECOVERING",
        "--set", f"operation_id={json.dumps(recovery_operation)}",
        "--set", f"recovery_attempt={attempt}",
        "--set", f"recovery_claim={json.dumps(claim_object)}",
        "--set", "last_error=null",
    ]
    if state["operation_id"]:
        transition_args += ["--expect-operation-id", state["operation_id"]]

    try:
        state = run_checkpoint(transition_args)
    except SystemExit as exc:
        message = str(exc)
        if "expected current status" in message or "expected operation_id" in message:
            # The checkpoint moved between the read and the CAS: another job
            # (recovery or pipeline) won the race. Do nothing.
            print(f"Claim lost to a concurrent checkpoint writer: {sanitize_detail(message)}")
            return {"claimed": False, "reason": "lost CAS race"}
        raise

    try:
        stage_and_push(checkpoint, f"chore(automation): recovery claim attempt {attempt} by {owner}")
    except SystemExit:
        # The push was rejected or conflicted. Inspect the remote authority:
        # if another job already holds the claim, stand down; otherwise this
        # is a real failure and must fail closed.
        remote = read_remote_state()
        if remote and remote.get("status") == "RECOVERING":
            remote_claim = remote.get("recovery_claim") or {}
            if remote_claim.get("owner") != owner:
                print(
                    f"Claim already held on the remote by {remote_claim.get('owner')!r}; "
                    "standing down safely without dispatching anything."
                )
                return {"claimed": False, "reason": "remote claim held by another job"}
        raise

    print(f"awh_pipeline: recovery claim acquired (attempt {attempt}, owner {owner}).")
    return {"claimed": True, "claim": claim_object, "state": state}


def read_remote_state() -> dict[str, Any] | None:
    """Read the authoritative checkpoint from origin/rust without merging."""
    result = subprocess.run(
        ["git", "show", f"origin/{SHARED_BRANCH}:{STATE_FILE}"],
        capture_output=True, text=True, env=GIT_ENV,
    )
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def decide_recovery(state: dict[str, Any], pr_open: bool, pr_merged: bool, branch_exists: bool) -> dict[str, Any]:
    """Inspect reality (GitHub PR/branch) plus the checkpoint and choose a
    safe continuation. Recovery never blindly replays the previous event."""
    claim = state.get("recovery_claim") or {}
    stale_status = claim.get("stale_status") or state["status"]
    feature = state["active_feature"]
    pr = state["active_pr"]

    if stale_status == "PLANNING":
        return {"target": "PLANNING", "dispatch": {"event": "awh.start", "feature_id": feature},
                "reason": "planning never completed; resume feature selection"}
    if stale_status == "BUILDING":
        if pr_merged:
            return {"target": "COMPLETED", "dispatch": None,
                    "reason": "PR was merged while the builder checkpoint stalled"}
        if pr_open:
            return {"target": "PR_OPEN",
                    "dispatch": {"event": "awh.review", "pr_number": pr, "feature_id": feature},
                    "reason": "builder produced a PR but crashed before recording it; resume at review"}
        return {"target": "BUILDING", "dispatch": {"event": "awh.build", "feature_id": feature},
                "reason": "no feature branch or PR exists; resume the build"}
    if stale_status in {"PR_OPEN", "REVIEWING"}:
        if pr_merged:
            return {"target": "COMPLETED", "dispatch": None,
                    "reason": "PR was merged while the review checkpoint stalled"}
        if pr_open:
            return {"target": "REVIEWING",
                    "dispatch": {"event": "awh.review", "pr_number": pr, "feature_id": feature},
                    "reason": "PR is open; (re)run the review"}
        return {"target": "BLOCKED", "dispatch": None,
                "reason": f"checkpoint references PR {pr} which no longer exists"}
    if stale_status == "FIXING":
        if pr_merged:
            return {"target": "COMPLETED", "dispatch": None,
                    "reason": "PR was merged while the fix checkpoint stalled"}
        if pr_open:
            return {"target": "FIXING",
                    "dispatch": {"event": "awh.fix", "feature_id": feature, "review": state.get("last_review")},
                    "reason": "PR is open; resume the recorded fix"}
        return {"target": "BLOCKED", "dispatch": None,
                "reason": f"checkpoint references PR {pr} which no longer exists"}
    if stale_status == "MERGING":
        if pr_merged:
            return {"target": "COMPLETED", "dispatch": {"event": "awh.start"},
                    "reason": "PR already merged; record completion and continue the loop"}
        if pr_open:
            return {"target": "MERGING",
                    "dispatch": {"event": "awh.review-complete", "pr_number": pr,
                                 "feature_id": feature, "review_state": "APPROVE"},
                    "reason": "approved PR never merged; retry the merge (the merge job re-validates everything)"}
        return {"target": "BLOCKED", "dispatch": None,
                "reason": f"checkpoint references PR {pr} which no longer exists"}
    return {"target": "BLOCKED", "dispatch": None, "reason": f"no safe continuation from {stale_status!r}"}


def finalize_recovery(owner: str, target: str, assignments: dict[str, Any], message: str) -> dict[str, Any]:
    """Release the claim and move RECOVERING -> target under CAS."""
    state = load_state()
    if state["status"] != "RECOVERING":
        print(f"STALE_EVENT: checkpoint is {state['status']!r}, not RECOVERING; nothing to finalize.")
        raise SystemExit(STALE_EXIT)
    claim = state.get("recovery_claim") or {}
    if claim.get("owner") not in (None, owner):
        # Another job took over the expired lease; do not stomp its work.
        print(f"STALE_EVENT: recovery lease is now held by {claim.get('owner')!r}; standing down.")
        raise SystemExit(STALE_EXIT)

    payload: dict[str, Any] = dict(assignments)
    payload["recovery_claim"] = None
    payload.setdefault("last_error", None)
    transition_args = ["transition", "--expect-status", "RECOVERING", "--to", target]
    for key, value in payload.items():
        transition_args += ["--set", f"{key}={json.dumps(value)}"]
    state = run_checkpoint(transition_args)
    stage_and_push(load_script_module("checkpoint_state"), message)
    print(f"awh_pipeline: recovery finalized to {target}.")
    return state


# --------------------------------------------------------------------------
# Failure recording: safe diagnostics, never success-on-failure.
# --------------------------------------------------------------------------

def record_failure(
    agent: str,
    stage: str,
    detail: str,
    feature: str | None,
    pr: int | None,
    branch: str | None,
    commit: str | None,
) -> bool:
    """Persist a sanitized diagnostic and transition to BLOCKED when legal.

    Returns True when the failure was recorded on the shared branch, False
    when the checkpoint was already terminal and was left untouched.
    """
    checkpoint = load_script_module("checkpoint_state")
    state = checkpoint.load_state()
    status = state["status"]

    diagnostic = {
        "agent": agent,
        "stage": stage,
        "operation_id": state["operation_id"],
        "feature_id": feature or state["active_feature"],
        "pr": pr or state["active_pr"],
        "branch": branch or state["active_branch"],
        "commit": commit,
        "attempt": state["builder_attempt"],
        "review_round": state["review_round"],
        "status_at_failure": status,
        "detail": sanitize_detail(detail),
    }
    rendered = json.dumps(diagnostic, sort_keys=True)
    print(f"awh_pipeline failure diagnostic: {rendered}")

    if status not in checkpoint.STALEABLE_STATUSES:
        print(f"Checkpoint is terminal ({status}); checkpoint left untouched, diagnostic kept in run logs.")
        return False

    transition_args = [
        "transition",
        "--expect-status", status,
        "--to", "BLOCKED",
        "--set", f"last_error={json.dumps(rendered)}",
    ]
    if state["operation_id"]:
        transition_args += ["--expect-operation-id", state["operation_id"]]
    run_checkpoint(transition_args)
    stage_and_push(checkpoint, f"chore(automation): record {agent} {stage} failure")
    print("awh_pipeline: failure recorded and checkpoint BLOCKED.")
    return True


# --------------------------------------------------------------------------
# GitHub inspection (used only by decide-recovery inside a real runner).
# --------------------------------------------------------------------------

def gh_json(*arguments: str) -> Any:
    result = subprocess.run(["gh", *arguments], capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(f"awh_pipeline: gh {' '.join(arguments)} failed: {sanitize_detail(result.stderr)}")
    return json.loads(result.stdout) if result.stdout.strip() else None


def remote_branch_exists(branch: str) -> bool:
    result = subprocess.run(
        ["git", "ls-remote", "--exit-code", "--heads", "origin", branch],
        capture_output=True, text=True, env=GIT_ENV,
    )
    return result.returncode == 0


def inspect_github(pr: int | None, state: dict[str, Any]) -> tuple[bool, bool, bool]:
    """Query GitHub for PR/branch reality. Any gh failure fails closed."""
    pr_open = False
    pr_merged = False
    if pr:
        try:
            data = gh_json("pr", "view", str(pr), "--json", "state,isCrossRepository,baseRefName")
            pr_open = (
                data.get("state") == "OPEN"
                and not data.get("isCrossRepository", False)
                and data.get("baseRefName") == SHARED_BRANCH
            )
            pr_merged = data.get("state") == "MERGED"
        except SystemExit as exc:
            if "not found" in str(exc).lower():
                pr_open, pr_merged = False, False
            else:
                raise
    branch = state.get("active_branch") or (f"feature/{state['active_feature']}" if state.get("active_feature") else None)
    branch_exists = remote_branch_exists(branch) if branch else False
    return pr_open, pr_merged, branch_exists


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------

def _parse_assignments(pairs: list[str]) -> dict[str, Any]:
    assignments: dict[str, Any] = {}
    for assignment in pairs:
        if "=" not in assignment:
            raise SystemExit(f"awh_pipeline: --set must be KEY=JSON, got {assignment!r}")
        key, raw = assignment.split("=", 1)
        assignments[key] = json.loads(raw)
    return assignments


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="action", required=True)

    validate = subparsers.add_parser("validate-event", help="check a dispatch payload against the checkpoint")
    validate.add_argument("--agent", required=True, choices=("builder", "fixer", "reviewer", "merger", "starter"))
    validate.add_argument("--feature", default=None)
    validate.add_argument("--pr", type=int, default=None)

    begin = subparsers.add_parser("begin-stage", help="enter a pipeline stage under CAS with a deterministic operation id")
    begin.add_argument("--stage", required=True, choices=("builder", "fixer", "reviewer", "merger"))
    begin.add_argument("--feature", default=None)
    begin.add_argument("--pr", type=int, default=None)
    begin.add_argument("--branch", default=None)
    begin.add_argument("--sha", default=None, help="PR head SHA to pin for immutable review tracking")
    begin.add_argument("--message", required=True)
    begin.add_argument("--set", action="append", default=[], metavar="KEY=JSON")

    claim = subparsers.add_parser("claim-recovery", help="atomically claim recovery of a stale checkpoint")
    claim.add_argument("--owner", required=True)
    claim.add_argument("--stale-minutes", type=int, default=90)
    claim.add_argument("--lease-minutes", type=int, default=120)

    decide = subparsers.add_parser("decide-recovery", help="inspect GitHub + checkpoint and choose a safe continuation")
    decide.add_argument("--pr", type=int, default=None)

    finalize = subparsers.add_parser("finalize-recovery", help="release the claim and resume a verified continuation")
    finalize.add_argument("--owner", required=True)
    finalize.add_argument("--to", required=True)
    finalize.add_argument("--message", required=True)
    finalize.add_argument("--set", action="append", default=[], metavar="KEY=JSON")

    record = subparsers.add_parser("record-failure", help="record a sanitized failure diagnostic")
    record.add_argument("--agent", required=True)
    record.add_argument("--stage", required=True)
    record.add_argument("--detail", required=True)
    record.add_argument("--feature", default=None)
    record.add_argument("--pr", type=int, default=None)
    record.add_argument("--branch", default=None)
    record.add_argument("--commit", default=None)

    args = parser.parse_args(argv)

    if args.action == "validate-event":
        state = load_state()
        ok, reason = validate_event(args.agent, state, args.feature, args.pr)
        if ok:
            print(f"Event validated: {reason}.")
            return
        print(f"STALE_EVENT: {reason}; doing nothing.")
        raise SystemExit(STALE_EXIT)

    if args.action == "begin-stage":
        extra = _parse_assignments(args.set)
        if args.pr is not None:
            extra["active_pr"] = args.pr
        if args.branch:
            extra["active_branch"] = args.branch
        if args.sha:
            extra["active_pr_sha"] = args.sha
        begin_stage(args.stage, args.feature, extra, args.message)
        return

    if args.action == "claim-recovery":
        claim_recovery(args.owner, args.stale_minutes, args.lease_minutes)
        return

    if args.action == "decide-recovery":
        state = load_state()
        pr = args.pr or state["active_pr"]
        pr_open, pr_merged, branch_exists = inspect_github(pr, state)
        decision = decide_recovery(state, pr_open, pr_merged, branch_exists)
        print(f"TARGET={decision['target']}")
        print(f"REASON={decision['reason']}")
        dispatch = decision.get("dispatch") or {}
        print(f"DISPATCH_EVENT={dispatch.get('event', '')}")
        print(f"DISPATCH_PAYLOAD={json.dumps(dispatch)}")
        return

    if args.action == "finalize-recovery":
        finalize_recovery(args.owner, args.to, _parse_assignments(args.set), args.message)
        return

    if args.action == "record-failure":
        record_failure(args.agent, args.stage, args.detail, args.feature, args.pr, args.branch, args.commit)
        return


if __name__ == "__main__":
    main()
