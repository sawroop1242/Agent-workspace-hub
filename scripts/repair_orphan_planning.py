#!/usr/bin/env python3
"""Repair a stale pre-identity autonomous checkpoint.

An autonomous run must have active_feature + operation_id before any agent
stage can be resumed. PLANNING, BLOCKED, or RECOVERING with neither identity
is an orphan left by a crashed/partial starter and cannot be continued safely.
This script moves only that exact shape through RECOVERING -> IDLE using normal
checkpoint CAS transitions, then publishes the repair without force-push.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / ".openhands" / "state.json"
SCRIPTS = ROOT / "scripts"


def load_module(name: str):
    import importlib.util

    spec = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {name}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def minutes_old(value: str | None) -> float | None:
    if not value:
        return None
    try:
        stamp = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    return (datetime.now(timezone.utc) - stamp).total_seconds() / 60


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--stale-minutes", type=int, default=15)
    args = parser.parse_args()

    checkpoint = load_module("checkpoint_state")
    state = checkpoint.load_state()
    status = state["status"]

    if status not in {"PLANNING", "BLOCKED", "RECOVERING"}:
        print(f"NO_OP: status={status}")
        return 0

    if state.get("active_feature") or state.get("operation_id"):
        print(f"NO_OP: {status} checkpoint has identity; normal pipeline/recovery owns it")
        return 0

    age = minutes_old(state.get("updated_at"))
    if age is None or age < args.stale_minutes:
        print(f"NO_OP: orphan {status} checkpoint is not safely stale")
        return 0

    attempt = int(state["recovery_attempt"]) + 1
    owner = f"awh-orphan-startup-{attempt}"
    claim_status = "RECOVERING" if status == "RECOVERING" else status
    claim = {
        "operation_id": None,
        "feature_id": None,
        "attempt": attempt,
        "claimed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "owner": owner,
        "stale_status": claim_status,
    }

    if status != "RECOVERING":
        checkpoint.main([
            "transition",
            "--expect-status", status,
            "--to", "RECOVERING",
            "--set", f"recovery_attempt={attempt}",
            "--set", f"recovery_claim={json.dumps(claim)}",
            "--set", "last_error=" + json.dumps(f"recovered stale orphaned {status} checkpoint"),
        ])
    else:
        checkpoint.main([
            "transition",
            "--expect-status", "RECOVERING",
            "--to", "RECOVERING",
            "--set", f"recovery_attempt={attempt}",
            "--set", f"recovery_claim={json.dumps(claim)}",
            "--set", "last_error=" + json.dumps("reclaimed stale orphaned RECOVERING checkpoint"),
        ])

    checkpoint.main([
        "transition",
        "--expect-status", "RECOVERING",
        "--to", "IDLE",
        "--set", "recovery_claim=null",
        "--set", "operation_id=null",
        "--set", "active_feature=null",
        "--set", "active_pr=null",
        "--set", "active_branch=null",
        "--set", "active_pr_sha=null",
        "--set", "review_round=0",
        "--set", "builder_attempt=0",
        "--set", "last_review=\"\"",
        "--set", "last_error=" + json.dumps("recovered stale orphaned autonomous startup checkpoint; safe to start a new operation"),
    ])

    subprocess.run(["git", "config", "user.name", "AWH Recovery"], cwd=ROOT, check=True)
    subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], cwd=ROOT, check=True)
    subprocess.run(["git", "add", ".openhands/state.json"], cwd=ROOT, check=True)
    subprocess.run(["git", "commit", "-m", "chore(automation): recover orphaned startup checkpoint"], cwd=ROOT, check=True)
    result = subprocess.run(["git", "push", "origin", "rust"], cwd=ROOT, capture_output=True, text=True)
    if result.returncode != 0:
        print("LOST_CLAIM: remote checkpoint changed before orphan repair could be published", file=sys.stderr)
        print(result.stderr.strip(), file=sys.stderr)
        return 3

    print(f"REPAIRED: orphaned {status} checkpoint -> IDLE (recovery attempt {attempt})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
