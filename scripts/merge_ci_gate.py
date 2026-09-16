#!/usr/bin/env python3
"""Fail-closed CI gate for an exact PR head SHA.

The LLM review verdict is intentionally not consulted here. This gate only
answers whether GitHub reports every required check as successful for the
exact commit that was reviewed.
"""
from __future__ import annotations

import json
import subprocess
import sys

REQUIRED = {"fmt", "clippy", "build", "test"}


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: merge_ci_gate.py <pr-number> <expected-sha>", file=sys.stderr)
        return 2
    pr, expected_sha = sys.argv[1:]
    raw = subprocess.check_output(
        ["gh", "pr", "checks", pr, "--json", "name,state,oid"], text=True
    )
    checks = json.loads(raw)
    by_name = {item["name"]: item for item in checks}
    missing = sorted(REQUIRED - by_name.keys())
    if missing:
        print(f"CI gate failed: missing required checks: {', '.join(missing)}", file=sys.stderr)
        return 1
    bad = []
    wrong_sha = []
    for name in sorted(REQUIRED):
        item = by_name[name]
        if item.get("oid") != expected_sha:
            wrong_sha.append(f"{name}={item.get('oid')}")
        if item.get("state") != "SUCCESS":
            bad.append(f"{name}={item.get('state')}")
    if wrong_sha:
        print("CI gate failed: check SHA mismatch: " + ", ".join(wrong_sha), file=sys.stderr)
        return 1
    if bad:
        print("CI gate failed: required checks are not successful: " + ", ".join(bad), file=sys.stderr)
        return 1
    print(f"CI gate passed for PR #{pr} at {expected_sha}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
