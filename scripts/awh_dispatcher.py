#!/usr/bin/env python3
"""AWH autonomous dispatcher.

The durable checkpoint is authoritative. The dispatcher emits only the next
small repository_dispatch event; agents perform mutations under CAS. A stale
PLANNING checkpoint without feature identity is treated as an orphan repair,
not as a healthy active stage.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


EVENTS = {
    "IDLE": "awh.start",
    "COMPLETED": "awh.start",
    "BUILDING": "awh.build",
    "PR_OPEN": "awh.review",
    "REVIEWING": "awh.review",
    "FIXING": "awh.fix",
    "MERGING": "awh.review-complete",
    "BLOCKED": "awh.recover",
}


def load_state(path: Path) -> dict:
    with path.open(encoding="utf-8") as fh:
        state = json.load(fh)
    if not isinstance(state, dict):
        raise ValueError("checkpoint must be a JSON object")
    return state


def plan_dispatch(state: dict) -> dict:
    status = state.get("status")
    feature = state.get("active_feature")
    operation = state.get("operation_id")
    pr = state.get("active_pr")
    sha = state.get("active_pr_sha")
    review = state.get("last_review") or ""

    if status == "PLANNING":
        if not feature and not operation:
            return {
                "action": "repair",
                "status": status,
                "reason": "stale orphaned PLANNING checkpoint has no feature or operation identity",
            }
        return {"action": "wait", "status": status, "reason": "active planner stage owns continuation"}

    if status == "RECOVERING":
        return {"action": "wait", "status": status, "reason": "active recovery stage owns continuation"}

    if status not in EVENTS:
        raise ValueError(f"unsupported checkpoint status: {status!r}")

    event = EVENTS[status]
    payload: dict[str, object] = {}

    if status in {"BUILDING", "BLOCKED"}:
        if not feature or not operation:
            raise ValueError(f"{status} checkpoint requires active_feature and operation_id")
        payload.update(feature_id=feature, operation_id=operation)
    elif status in {"PR_OPEN", "REVIEWING", "FIXING", "MERGING"}:
        if not feature or not operation or not pr:
            raise ValueError(f"{status} checkpoint requires feature, operation and active_pr")
        payload.update(feature_id=feature, operation_id=operation, pr_number=int(pr))
        if sha:
            payload["reviewed_sha"] = sha
        if review:
            payload["review"] = review
        if status == "MERGING":
            payload["review_state"] = "APPROVE"

    return {"action": "dispatch", "event": event, "payload": payload, "status": status}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", default=".openhands/state.json")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    result = plan_dispatch(load_state(Path(args.state)))
    if args.json:
        print(json.dumps(result, separators=(",", ":")))
    else:
        print(f"ACTION={result['action']}")
        print(f"STATUS={result['status']}")
        if result["action"] == "dispatch":
            print(f"EVENT={result['event']}")
            print("PAYLOAD=" + json.dumps(result["payload"], separators=(",", ":")))
        else:
            print(f"REASON={result['reason']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
