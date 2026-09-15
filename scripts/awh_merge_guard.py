#!/usr/bin/env python3
"""Fail-closed merge guard for the AWH autonomous pipeline.

The guard binds an APPROVE notification to the exact PR head SHA that Agent 3
reviewed. It re-reads both the authoritative checkpoint and GitHub immediately
before merge, then performs the merge with GitHub's expected-head-SHA guard.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
from pathlib import Path

ROOT = Path(os.environ.get("AWH_REPO_ROOT", Path(__file__).resolve().parents[1]))
STATE_PATH = ROOT / ".openhands" / "state.json"
BASE_BRANCH = "rust"


class MergeGuardError(RuntimeError):
    """A merge invariant failed; the caller must not merge."""


def gh_json(pr: int) -> dict:
    result = subprocess.run(
        [
            "gh", "pr", "view", str(pr), "--repo", os.environ["GITHUB_REPOSITORY"],
            "--json", "state,mergedAt,headRefOid,baseRefName,isCrossRepository,headRefName",
        ],
        capture_output=True,
        text=True,
        env={**os.environ, "GH_PAGER": "cat"},
    )
    if result.returncode != 0:
        raise MergeGuardError(f"GitHub PR lookup failed: {result.stderr.strip()}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise MergeGuardError(f"GitHub PR lookup returned invalid JSON: {exc}") from exc


def load_state() -> dict:
    try:
        return json.loads(STATE_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise MergeGuardError(f"cannot read authoritative checkpoint: {exc}") from exc


def validate(pr: int, reviewed_sha: str, feature_id: str | None = None) -> str:
    """Return the immutable reviewed SHA only when every merge invariant holds."""
    if not reviewed_sha or not reviewed_sha.strip():
        raise MergeGuardError("reviewed_sha is required; refusing to merge without a SHA-bound approval")

    state = load_state()
    if state.get("status") != "MERGING":
        raise MergeGuardError(f"checkpoint status is {state.get('status')!r}, not MERGING")
    if state.get("active_pr") != pr:
        raise MergeGuardError(
            f"checkpoint active_pr is {state.get('active_pr')!r}, not PR {pr}"
        )
    if feature_id and state.get("active_feature") != feature_id:
        raise MergeGuardError(
            f"checkpoint active_feature is {state.get('active_feature')!r}, not {feature_id!r}"
        )

    checkpoint_sha = state.get("active_pr_sha")
    if not checkpoint_sha:
        raise MergeGuardError("checkpoint has no active_pr_sha; refusing to merge")
    if checkpoint_sha != reviewed_sha:
        raise MergeGuardError(
            f"reviewed SHA {reviewed_sha} does not match checkpoint active_pr_sha {checkpoint_sha}"
        )

    pr_data = gh_json(pr)
    if pr_data.get("isCrossRepository"):
        raise MergeGuardError("PR is cross-repository; refusing to merge")
    if pr_data.get("baseRefName") != BASE_BRANCH:
        raise MergeGuardError(
            f"PR base is {pr_data.get('baseRefName')!r}, not {BASE_BRANCH!r}"
        )
    if pr_data.get("state") != "OPEN":
        raise MergeGuardError(f"PR state is {pr_data.get('state')!r}, not OPEN")
    if pr_data.get("mergedAt"):
        raise MergeGuardError("PR is already merged; refusing duplicate merge")

    current_sha = pr_data.get("headRefOid")
    if current_sha != reviewed_sha:
        raise MergeGuardError(
            f"PR head changed after review: reviewed {reviewed_sha}, current {current_sha}"
        )

    return reviewed_sha


def merge(pr: int, reviewed_sha: str, feature_id: str | None = None) -> None:
    """Validate immediately before merging and use GitHub's head-SHA guard."""
    expected = validate(pr, reviewed_sha, feature_id)
    repo = os.environ["GITHUB_REPOSITORY"]
    result = subprocess.run(
        [
            "gh", "pr", "merge", str(pr), "--repo", repo,
            "--squash", "--delete-branch", "--match-head-commit", expected,
        ],
        capture_output=True,
        text=True,
        env={**os.environ, "GH_PAGER": "cat"},
    )
    if result.returncode != 0:
        raise MergeGuardError(
            f"GitHub rejected the SHA-guarded merge: {(result.stderr or result.stdout).strip()}"
        )


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pr", required=True, type=int)
    parser.add_argument("--reviewed-sha", required=True)
    parser.add_argument("--feature", default=None)
    args = parser.parse_args(argv)
    try:
        if os.environ.get("AWH_MERGE_GUARD_VALIDATE_ONLY") == "1":
            print(validate(args.pr, args.reviewed_sha, args.feature))
        else:
            merge(args.pr, args.reviewed_sha, args.feature)
            print(f"SHA-guarded merge succeeded for PR {args.pr} at {args.reviewed_sha}")
    except MergeGuardError as exc:
        raise SystemExit(f"MERGE_GUARD_BLOCKED: {exc}") from exc


if __name__ == "__main__":
    main()
