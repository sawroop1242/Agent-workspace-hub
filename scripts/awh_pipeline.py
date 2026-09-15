#!/usr/bin/env python3
"""AWH pipeline safety layer: event validation, CAS stage entry with deterministic operation identity, atomic recovery claims, verified recovery continuation, and sanitized failure recording."""

from __future__ import annotations

import argparse
import enum
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
REPO_ROOT = Path(os.environ.get("AWH_REPO_ROOT", ROOT))
SHARED_BRANCH = "rust"
STATE_FILE = ".openhands/state.json"
STALE_EXIT = 3
GIT_ENV = {**os.environ, "GIT_TERMINAL_PROMPT": "0"}
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
STAGES = {
    "builder": {"agent": "builder", "target": "BUILDING", "counter": "builder_attempt", "op": "BUILD"},
    "fixer": {"agent": "fixer", "target": "FIXING", "counter": "review_round", "op": "FIX"},
    "reviewer": {"agent": "reviewer", "target": "REVIEWING", "counter": "review_round", "op": "REVIEW"},
    "merger": {"agent": "merger", "target": "MERGING", "counter": None, "op": "MERGE"},
}
AGENT_ADMITTED_STATUSES = {
    "builder": {"BUILDING", "FIXING"},
    "fixer": {"FIXING"},
    "reviewer": {"PR_OPEN", "REVIEWING"},
    "merger": {"PR_OPEN", "MERGING"},
    "starter": {"IDLE", "COMPLETED", "PLANNING"},
}


def load_script_module(name: str):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sanitize_detail(text: str) -> str:
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
    if not operation_id:
        return None
    prefix, _, counter = operation_id.rpartition(":")
    if not counter.isdigit():
        return None
    return prefix, int(counter)


def run_checkpoint(argv: list[str]) -> dict[str, Any]:
    module = load_script_module("checkpoint_state")
    module.main(argv)
    return module.load_state()


def load_state() -> dict[str, Any]:
    return load_script_module("checkpoint_state").load_state()


def stage_and_push(checkpoint_module, message: str) -> None:
    result = subprocess.run(["git", "add", STATE_FILE], capture_output=True, text=True, env=GIT_ENV)
    if result.returncode != 0:
        raise SystemExit(f"awh_pipeline: git add {STATE_FILE} failed: {sanitize_detail(result.stderr)}")
    load_script_module("safe_git").push(SHARED_BRANCH, message)


def stage_and_commit(message: str) -> None:
    result = subprocess.run(["git", "-C", str(REPO_ROOT), "add", STATE_FILE], capture_output=True, text=True, env=GIT_ENV)
    if result.returncode != 0:
        raise SystemExit(f"awh_pipeline: git add {STATE_FILE} failed: {sanitize_detail(result.stderr)}")
    result = subprocess.run(["git", "-C", str(REPO_ROOT), "commit", "-m", message], capture_output=True, text=True, env=GIT_ENV)
    if result.returncode != 0:
        detail = sanitize_detail((result.stderr or result.stdout).strip())
        raise SystemExit(f"awh_pipeline: commit of the claim failed (exit {result.returncode}):\n{detail}")


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
    if status == "PLANNING" and active_feature is not None and feature and feature != active_feature:
        return False, f"resume requested {feature!r} but active_feature is {active_feature!r}"
    return True, "event agrees with checkpoint"


def begin_stage(stage: str, feature: str | None, extra_assignments: dict[str, Any], message: str) -> dict[str, Any]:
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
        operation_id = f"{feature}:{spec['op']}"
    else:
        prefix = f"{feature}:{spec['op']}"
        current_counter = int(state[counter_field])
        parsed = split_operation_id(state["operation_id"])
        if parsed is not None and parsed[0] == prefix and parsed[1] == current_counter:
            operation_id = state["operation_id"]
        elif parsed is None or parsed[0] != prefix:
            next_counter = current_counter + 1
            operation_id = f"{prefix}:{next_counter}"
            assignments[counter_field] = next_counter
        else:
            print(f"STALE_EVENT: checkpoint operation_id {state['operation_id']!r} is for the same stage but not the expected counter {current_counter}; refusing to guess.")
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
    return {"operation_id": operation_id, "status": target}


class ClaimResult(enum.Enum):
    CLAIMED = "CLAIMED"
    LOST_CLAIM = "LOST_CLAIM"
    NO_OP = "NO_OP"


def sync_to_remote() -> None:
    load_script_module("safe_git").sync(SHARED_BRANCH)


def claim_recovery(*, feature_id: str, expected_operation_id: str, claim_owner: str, stale_minutes: int = 90, lease_minutes: int = 120) -> ClaimResult:
    if not claim_owner or not claim_owner.strip():
        raise SystemExit("awh_pipeline: claim-recovery requires a non-empty --claim-owner")
    checkpoint = load_script_module("checkpoint_state")
    recoverable_statuses = set(checkpoint.STALEABLE_STATUSES)
    sync_to_remote()
    state = read_remote_state()
    if state is None:
        raise SystemExit("awh_pipeline: could not read the checkpoint from origin/rust")
    status = state["status"]
    if status not in recoverable_statuses:
        print(f"No recoverable checkpoint: status={status}")
        return ClaimResult.NO_OP
    claim = state.get("recovery_claim")
    if status == "RECOVERING":
        return _handle_existing_claim(state, claim, claim_owner, feature_id, expected_operation_id, lease_minutes)
    age = minutes_since(state["updated_at"])
    if age is None:
        print("Checkpoint updated_at is missing or invalid; refusing recovery to fail closed.")
        return ClaimResult.NO_OP
    if age < stale_minutes:
        print(f"Checkpoint is not stale (age {age:.1f}m < {stale_minutes}m); nothing to recover.")
        return ClaimResult.NO_OP
    if state["active_feature"] != feature_id:
        print(f"STALE_EVENT: remote active_feature is {state['active_feature']!r}, not {feature_id!r}; the claim is lost.")
        return ClaimResult.LOST_CLAIM
    if state["operation_id"] != expected_operation_id:
        print(f"STALE_EVENT: remote operation_id is {state['operation_id']!r}, not {expected_operation_id!r}; the claim is lost.")
        return ClaimResult.LOST_CLAIM
    return _publish_claim(state, feature_id=feature_id, claim_owner=claim_owner, expect_status=status, expect_operation_id=state["operation_id"], stale_status=status)


def _handle_existing_claim(state: dict[str, Any], claim: dict[str, Any] | None, claim_owner: str, feature_id: str, expected_operation_id: str, lease_minutes: int) -> ClaimResult:
    if claim and claim.get("owner") == claim_owner:
        if claim.get("feature_id") != feature_id or claim.get("operation_id") != expected_operation_id:
            print("STALE_EVENT: recorded recovery claim identity does not match this recovery request; refusing takeover/continuation.")
            return ClaimResult.LOST_CLAIM
        print("Recovery claim already held by this owner; continuing.")
        return ClaimResult.CLAIMED
    if claim:
        claim_age = minutes_since(claim.get("claimed_at"))
        if claim_age is None:
            print("Recovery claim timestamp is missing or invalid; standing down safely.")
            return ClaimResult.LOST_CLAIM
        if claim_age < lease_minutes:
            print(f"Recovery claim already held by {claim.get('owner')!r} (age {claim_age:.1f}m < lease {lease_minutes}m); standing down safely.")
            return ClaimResult.LOST_CLAIM
    if state["operation_id"] != state.get("recovery_claim", {}).get("operation_id") if claim else expected_operation_id != expected_operation_id:
        pass
    if claim and claim.get("operation_id") != expected_operation_id:
        print(f"STALE_EVENT: recovery claim operation_id {claim.get('operation_id')!r} does not match expected {expected_operation_id!r}; refusing takeover.")
        return ClaimResult.LOST_CLAIM
    return _publish_claim(state, feature_id=feature_id, claim_owner=claim_owner, expect_status="RECOVERING", expect_operation_id=state["operation_id"], stale_status=(claim or {}).get("stale_status") or state["status"])


def _publish_claim(state: dict[str, Any], *, feature_id: str, claim_owner: str, expect_status: str, expect_operation_id: str | None, stale_status: str) -> ClaimResult:
    attempt = int(state["recovery_attempt"]) + 1
    claim_object = {
        "operation_id": state.get("recovery_claim", {}).get("operation_id") if state.get("recovery_claim") else state["operation_id"],
        "feature_id": state["active_feature"],
        "attempt": attempt,
        "claimed_at": utc_now(),
        "owner": claim_owner,
        "stale_status": stale_status,
    }
    recovery_operation = f"{state['active_feature'] or 'PIPELINE'}:RECOVERY:{attempt}"
    transition_args = ["transition", "--expect-status", expect_status, "--to", "RECOVERING", "--set", f"operation_id={json.dumps(recovery_operation)}", "--set", f"recovery_attempt={attempt}", "--set", f"recovery_claim={json.dumps(claim_object)}", "--set", "last_error=null"]
    if expect_operation_id:
        transition_args += ["--expect-operation-id", expect_operation_id]
    try:
        run_checkpoint(transition_args)
    except SystemExit as exc:
        message = str(exc)
        if "expected current status" in message or "expected operation_id" in message:
            print(f"Claim lost to a concurrent checkpoint writer: {sanitize_detail(message)}")
            return ClaimResult.LOST_CLAIM
        raise
    stage_and_commit(f"chore(automation): claim recovery {state['active_feature']}")
    safe_git = load_script_module("safe_git")
    try:
        safe_git.push_claim(repo=REPO_ROOT, remote="origin", branch=SHARED_BRANCH)
    except safe_git.LostClaimError:
        print("LOST_CLAIM: another writer advanced origin/rust before this claim was published; standing down.")
        return ClaimResult.LOST_CLAIM
    except safe_git.GitError as exc:
        raise SystemExit(f"awh_pipeline: {sanitize_detail(str(exc))}")
    print(f"awh_pipeline: recovery claim acquired (attempt {attempt}, owner {claim_owner}).")
    return ClaimResult.CLAIMED


def read_remote_state() -> dict[str, Any] | None:
    result = subprocess.run(["git", "show", f"origin/{SHARED_BRANCH}:{STATE_FILE}"], capture_output=True, text=True, env=GIT_ENV)
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def inspect_github(pr: int | None, state: dict[str, Any]) -> tuple[bool, bool, bool]:
    pr_open = False
    pr_merged = False
    if pr:
        try:
            data = gh_json("pr", "view", str(pr), "--json", "state,isCrossRepository,baseRefName")
            pr_open = data.get("state") == "OPEN" and not data.get("isCrossRepository", False) and data.get("baseRefName") == SHARED_BRANCH
            pr_merged = data.get("state") == "MERGED"
        except SystemExit as exc:
            if "not found" in str(exc).lower():
                pr_open, pr_merged = False, False
            else:
                raise
    branch = state.get("active_branch") or (f"feature/{state['active_feature']}" if state.get("active_feature") else None)
    branch_exists = remote_branch_exists(branch) if branch else False
    return pr_open, pr_merged, branch_exists


def gh_json(*arguments: str) -> Any:
    result = subprocess.run(["gh", *arguments], capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(f"awh_pipeline: gh {' '.join(arguments)} failed: {sanitize_detail(result.stderr)}")
    return json.loads(result.stdout) if result.stdout.strip() else None


def remote_branch_exists(branch: str) -> bool:
    result = subprocess.run(["git", "ls-remote", "--exit-code", "--heads", "origin", branch], capture_output=True, text=True, env=GIT_ENV)
    return result.returncode == 0


def decide_recovery(state: dict[str, Any], pr_open: bool, pr_merged: bool, branch_exists: bool) -> dict[str, Any]:
    claim = state.get("recovery_claim") or {}
    stale_status = claim.get("stale_status") or state["status"]
    feature = state["active_feature"]
    pr = state["active_pr"]
    if stale_status == "PLANNING":
        return {"target": "PLANNING", "dispatch": {"event": "awh.start", "feature_id": feature}, "reason": "planning never completed; resume feature selection"}
    if stale_status == "BUILDING":
        if pr_merged:
            return {"target": "COMPLETED", "dispatch": None, "reason": "PR was merged while the builder checkpoint stalled"}
        if pr_open:
            return {"target": "PR_OPEN", "dispatch": {"event": "awh.review", "pr_number": pr, "feature_id": feature}, "reason": "builder produced a PR but crashed before recording it; resume at review"}
        return {"target": "BUILDING", "dispatch": {"event": "awh.build", "feature_id": feature}, "reason": "no feature branch or PR exists; resume the build"}
    if stale_status in {"PR_OPEN", "REVIEWING"}:
        if pr_merged:
            return {"target": "COMPLETED", "dispatch": None, "reason": "PR was merged while the review checkpoint stalled"}
        if pr_open:
            return {"target": "REVIEWING", "dispatch": {"event": "awh.review", "pr_number": pr, "feature_id": feature}, "reason": "PR is open; (re)run the review"}
        return {"target": "BLOCKED", "dispatch": None, "reason": f"checkpoint references PR {pr} which no longer exists"}
    if stale_status == "FIXING":
        if pr_merged:
            return {"target": "COMPLETED", "dispatch": None, "reason": "PR was merged while the fix checkpoint stalled"}
        if pr_open:
            return {"target": "FIXING", "dispatch": {"event": "awh.fix", "feature_id": feature, "review": state.get("last_review")}, "reason": "PR is open; resume the recorded fix"}
        return {"target": "BLOCKED", "dispatch": None, "reason": f"checkpoint references PR {pr} which no longer exists"}
    if stale_status == "MERGING":
        if pr_merged:
            return {"target": "COMPLETED", "dispatch": {"event": "awh.start"}, "reason": "PR already merged; record completion and continue the loop"}
        if pr_open:
            return {"target": "MERGING", "dispatch": {"event": "awh.review-complete", "pr_number": pr, "feature_id": feature, "review_state": "APPROVE"}, "reason": "approved PR never merged; retry the merge (the merge job re-validates everything)"}
        return {"target": "BLOCKED", "dispatch": None, "reason": f"checkpoint references PR {pr} which no longer exists"}
    return {"target": "BLOCKED", "dispatch": None, "reason": f"no safe continuation from {stale_status!r}"}


def finalize_recovery(owner: str, target: str, assignments: dict[str, Any], message: str) -> dict[str, Any]:
    state = load_state()
    if state["status"] != "RECOVERING":
        print(f"STALE_EVENT: checkpoint is {state['status']!r}, not RECOVERING; nothing to finalize.")
        raise SystemExit(STALE_EXIT)
    claim = state.get("recovery_claim") or {}
    if claim.get("owner") not in (None, owner):
        print(f"STALE_EVENT: recovery lease is now held by {claim.get('owner')!r}; standing down.")
        raise SystemExit(STALE_EXIT)
    payload: dict[str, Any] = dict(assignments)
    payload["recovery_claim"] = None
    payload.setdefault("last_error", None)
    # The claim temporarily replaces operation_id with a recovery operation.
    # Before resuming the original stage, restore the original operation ID.
    if target in {"BUILDING", "PLANNING", "PR_OPEN", "REVIEWING", "FIXING", "MERGING"} and claim.get("operation_id"):
        payload["operation_id"] = claim["operation_id"]
    transition_args = ["transition", "--expect-status", "RECOVERING", "--to", target, "--expect-operation-id", state["operation_id"]]
    for key, value in payload.items():
        transition_args += ["--set", f"{key}={json.dumps(value)}"]
    state = run_checkpoint(transition_args)
    stage_and_push(load_script_module("checkpoint_state"), message)
    print(f"awh_pipeline: recovery finalized to {target}.")
    return state


def record_failure(agent: str, stage: str, detail: str, feature: str | None, pr: int | None, branch: str | None, commit: str | None) -> bool:
    checkpoint = load_script_module("checkpoint_state")
    state = checkpoint.load_state()
    status = state["status"]
    diagnostic = {"agent": agent, "stage": stage, "operation_id": state["operation_id"], "feature_id": feature or state["active_feature"], "pr": pr or state["active_pr"], "branch": branch or state["active_branch"], "commit": commit, "attempt": state["builder_attempt"], "review_round": state["review_round"], "status_at_failure": status, "detail": sanitize_detail(detail)}
    rendered = json.dumps(diagnostic, sort_keys=True)
    print(f"awh_pipeline failure diagnostic: {rendered}")
    if status not in checkpoint.STALEABLE_STATUSES:
        print(f"Checkpoint is terminal ({status}); checkpoint left untouched, diagnostic kept in run logs.")
        return False
    transition_args = ["transition", "--expect-status", status, "--to", "BLOCKED", "--set", f"last_error={json.dumps(rendered)}"]
    if state["operation_id"]:
        transition_args += ["--expect-operation-id", state["operation_id"]]
    run_checkpoint(transition_args)
    stage_and_push(checkpoint, f"chore(automation): record {agent} {stage} failure")
    print("awh_pipeline: failure recorded and checkpoint BLOCKED.")
    return True


def _parse_assignments(pairs: list[str]) -> dict[str, Any]:
    assignments: dict[str, Any] = {}
    for pair in pairs:
        key, sep, value = pair.partition("=")
        if not sep or not key:
            raise SystemExit(f"awh_pipeline: invalid --set assignment {pair!r}")
        try:
            assignments[key] = json.loads(value)
        except json.JSONDecodeError:
            assignments[key] = value
    return assignments


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("validate-event"); p.add_argument("--agent", required=True); p.add_argument("--feature"); p.add_argument("--pr", type=int)
    p = sub.add_parser("begin-stage"); p.add_argument("--stage", choices=STAGES); p.add_argument("--feature", required=True); p.add_argument("--message", required=True); p.add_argument("--set", action="append", default=[])
    p = sub.add_parser("claim-recovery"); p.add_argument("--feature", required=True); p.add_argument("--operation-id", required=True); p.add_argument("--claim-owner", required=True); p.add_argument("--stale-minutes", type=int, default=90); p.add_argument("--lease-minutes", type=int, default=120)
    p = sub.add_parser("decide-recovery")
    p = sub.add_parser("finalize-recovery"); p.add_argument("--owner", required=True); p.add_argument("--to", required=True); p.add_argument("--message", required=True); p.add_argument("--set", action="append", default=[])
    p = sub.add_parser("record-failure"); p.add_argument("--agent", required=True); p.add_argument("--stage", required=True); p.add_argument("--detail", required=True); p.add_argument("--feature"); p.add_argument("--pr", type=int); p.add_argument("--branch"); p.add_argument("--commit")
    args = parser.parse_args()
    if args.command == "validate-event":
        state = load_state(); ok, reason = validate_event(args.agent, state, args.feature, args.pr); print(reason); return 0 if ok else STALE_EXIT
    if args.command == "begin-stage":
        begin_stage(args.stage, args.feature, _parse_assignments(args.set), args.message); return 0
    if args.command == "claim-recovery":
        print(claim_recovery(feature_id=args.feature, expected_operation_id=args.operation_id, claim_owner=args.claim_owner, stale_minutes=args.stale_minutes, lease_minutes=args.lease_minutes).value); return 0
    if args.command == "decide-recovery":
        state = load_state(); pr_open, pr_merged, branch_exists = inspect_github(state.get("active_pr"), state); decision = decide_recovery(state, pr_open, pr_merged, branch_exists); print(f"TARGET={decision['target']}"); print(f"REASON={decision['reason']}"); dispatch = decision.get("dispatch"); print(f"DISPATCH_EVENT={dispatch.get('event','') if dispatch else ''}"); print(f"DISPATCH_PAYLOAD={json.dumps({k:v for k,v in (dispatch or {}).items() if k != 'event'})}"); return 0
    if args.command == "finalize-recovery":
        finalize_recovery(args.owner, args.to, _parse_assignments(args.set), args.message); return 0
    if args.command == "record-failure":
        return 0 if record_failure(args.agent, args.stage, args.detail, args.feature, args.pr, args.branch, args.commit) else 1
    return 1

if __name__ == "__main__":
    raise SystemExit(main())
