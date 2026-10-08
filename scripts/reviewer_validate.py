#!/usr/bin/env python3
"""Validate Agent 3 v2 output before any checkpoint mutation.

This is a fail-closed evidence gate. The semantic reviewer may add contextual
findings, but it cannot silently override confirmed deterministic HIGH/CRITICAL
findings or review a different PR head.
"""
from __future__ import annotations

import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REVIEW = ROOT / ".openhands" / "review.md"
RESULT = ROOT / ".openhands" / "review-result.json"
DETERMINISTIC = ROOT / ".openhands" / "deterministic-review.json"

ALLOWED = {"APPROVE", "CHANGES_REQUIRED", "BLOCKED"}


def main() -> int:
    pr = os.environ.get("AWH_PR", "").strip()
    expected_sha = os.environ.get("AWH_REVIEW_SHA", "").strip()
    if not pr or not expected_sha:
        raise SystemExit("AWH_PR and AWH_REVIEW_SHA are required")
    if not REVIEW.exists() or not DETERMINISTIC.exists():
        raise SystemExit("Agent 3 evidence files are missing")

    review = REVIEW.read_text(encoding="utf-8").strip()
    deterministic = json.loads(DETERMINISTIC.read_text(encoding="utf-8"))
    if str(deterministic.get("pr")) != pr:
        raise SystemExit("REVIEW_INVALIDATED: deterministic evidence PR mismatch")
    if deterministic.get("reviewed_sha") != expected_sha:
        raise SystemExit("REVIEW_INVALIDATED: deterministic evidence SHA mismatch")

    verdicts = [
        line.split(":", 1)[1].strip()
        for line in review.splitlines()
        if line.startswith("VERDICT:")
    ]
    if len(verdicts) != 1 or verdicts[0] not in ALLOWED:
        raise SystemExit("review.md must contain exactly one valid VERDICT")
    verdict = verdicts[0]

    findings = deterministic.get("findings", [])
    blocking = [f for f in findings if f.get("severity") in {"CRITICAL", "HIGH"}]
    missing = [f.get("id", "UNKNOWN") for f in blocking if f.get("id") not in review]
    if missing:
        raise SystemExit(
            "Agent 3 did not acknowledge confirmed deterministic finding(s): "
            + ", ".join(missing)
        )
    if verdict == "APPROVE" and blocking:
        raise SystemExit(
            "Agent 3 cannot APPROVE while confirmed deterministic CRITICAL/HIGH "
            "findings remain: " + ", ".join(f.get("id", "UNKNOWN") for f in blocking)
        )

    result = {
        "schema_version": 2,
        "reviewer_version": "agent3-v2",
        "pr": int(pr),
        "reviewed_sha": expected_sha,
        "mode": os.environ.get("AWH_REVIEW_MODE", "pipeline"),
        "verdict": verdict,
        "deterministic": {
            "finding_count": len(findings),
            "blocking_findings": [f.get("id") for f in blocking],
            "confirmed": True,
        },
        "review": review,
    }
    RESULT.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"verdict": verdict, "blocking_findings": result["deterministic"]["blocking_findings"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
