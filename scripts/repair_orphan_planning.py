#!/usr/bin/env python3
"""Repair a stale PLANNING checkpoint that never acquired feature identity.

A planner-start checkpoint is only healthy when both active_feature and
operation_id are present before the planner begins. An old PLANNING document
with neither field is an orphan left by a crashed/partial starter and cannot
be resumed by an agent safely. This script repairs only that exact shape,
using the normal checkpoint transition rules, then publishes the repair with
an ordinary non-force push. A concurrent writer wins; this script never
rebases over a competing checkpoint update.
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

    if state["status"] != "PLANNING":
        print(f"NO_OP: status={state['status']}")
        return 0

    if state.get("active_feature") or state.get("operation_id"):
        print("NO_OP: PLANNING checkpoint has identity; active planner owns it")
        return 0

    age = minutes_old(state.get("updated_at"))
    if age is None or age < args.stale_minutes:
        print("NO_OP: orphan PLANNING checkpoint is not safely stale")
        return 0

    attempt = int(state["recovery_attempt"]) + 1
    owner = f"awh-orphan-planning-{attempt}"
    claim = {
        "operation_id": None,
        "feature_id": None,
        "attempt": attempt,
        "claimed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "owner": owner,
        "stale_status": "PLANNING",
    }

    checkpoint.main([
        "transition",
        "--expect-status", "PLANNING",
        "--to", "RECOVERING",
        "--set", f"recovery_attempt={attempt}",
        "--set", f"recovery_claim={json.dumps(claim)}",
        "--set", "last_error=" + json.dumps("recovered stale orphaned PLANNING checkpoint"),
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
        "--set", "last_error=" + json.dumps("recovered stale orphaned PLANNING checkpoint; safe to start a new operation"),
    ])

    subprocess.run(["git", "config", "user.name", "AWH Recovery"], check=True)
    subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], check=True)
    subprocess.run(["git", "add", ".openhands/state.json"], cwd=ROOT, check=True)
    subprocess.run(["git", "commit", "-m", "chore(automation): recover orphaned planning checkpoint"], cwd=ROOT, check=True)
    result = subprocess.run(["git", "push", "origin", "rust"], cwd=ROOT, capture_output=True, text=True)
    if result.returncode != 0:
        print("LOST_CLAIM: remote checkpoint changed before orphan repair could be published", file=sys.stderr)
        print(result.stderr.strip(), file=sys.stderr)
        return 3

    print(f"REPAIRED: orphaned PLANNING checkpoint -> IDLE (recovery attempt {attempt})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
