#!/usr/bin/env python3
"""Fail-closed CI gate for an exact PR head SHA."""
from __future__ import annotations

import json
import os
import subprocess
import sys

REQUIRED = {"fmt", "clippy", "Build / test (ubuntu-latest)", "Dependency vulnerability audit"}


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: merge_ci_gate.py <pr-number> <expected-sha>", file=sys.stderr)
        return 2
    pr, expected_sha = sys.argv[1:]
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not repo:
        print("CI gate failed: GITHUB_REPOSITORY is required", file=sys.stderr)
        return 2
    raw = subprocess.check_output(
        ["gh", "api", f"repos/{repo}/commits/{expected_sha}/check-runs", "--paginate", "--jq", ".check_runs[] | {name,status,conclusion,head_sha}"],
        text=True,
    )
    checks = [json.loads(line) for line in raw.splitlines() if line.strip()]
    by_name = {item["name"]: item for item in checks}
    missing = sorted(REQUIRED - by_name.keys())
    if missing:
        print(f"CI gate failed for PR #{pr}: missing required checks: {', '.join(missing)}", file=sys.stderr)
        return 1
    wrong_sha = [name for name in REQUIRED if by_name[name].get("head_sha") != expected_sha]
    if wrong_sha:
        print("CI gate failed: check SHA mismatch: " + ", ".join(sorted(wrong_sha)), file=sys.stderr)
        return 1
    bad = []
    for name in sorted(REQUIRED):
        item = by_name[name]
        if item.get("status") != "completed" or item.get("conclusion") != "success":
            bad.append(f"{name}={item.get('status')}/{item.get('conclusion')}")
    if bad:
        print("CI gate failed: required checks are not successful: " + ", ".join(bad), file=sys.stderr)
        return 1
    print(f"CI gate passed for PR #{pr} at {expected_sha}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
