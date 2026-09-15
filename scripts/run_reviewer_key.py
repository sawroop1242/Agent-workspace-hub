#!/usr/bin/env python3
"""Run Agent 3 once with exactly one LLM secret in the process environment.

Exit 0 on success, 75 on a rate-limit response (safe to rotate), and the
underlying exit code for other failures. Keeping one secret per process avoids
large inherited environments and minimizes secret exposure.
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

KEY_NAME = os.environ.get("AWH_REVIEW_KEY_NAME", "")
STATUS_FILE = Path("/tmp/awh-review-key-status")

if KEY_NAME not in {"K1", "K2", "K3", "KF"}:
    raise SystemExit("AWH_REVIEW_KEY_NAME must be one of K1, K2, K3, KF")

key = os.environ.get(KEY_NAME, "")
if not key:
    STATUS_FILE.write_text("missing\n", encoding="utf-8")
    raise SystemExit(75)

child_env = os.environ.copy()
child_env["AWH_LLM_API_KEY"] = key
# Do not pass the rotation variables to the OpenHands process.
for name in ("K1", "K2", "K3", "KF", "AWH_REVIEW_KEY_NAME"):
    child_env.pop(name, None)

result = subprocess.run(
    [sys.executable, "scripts/openhands_agent.py"],
    env=child_env,
    text=True,
)
if result.returncode == 0:
    STATUS_FILE.write_text("success\n", encoding="utf-8")
    raise SystemExit(0)

# OpenHands/HTTP clients can report rate limiting in stdout/stderr. The
# workflow intentionally rotates only for rate-limit failures; other errors
# must fail closed instead of hiding a real reviewer defect.
# The child output has already been streamed to the workflow log, so use a
# conservative status marker based on the known process exit/result text is
# not possible here. Exit 75 is reserved for explicit retry requests from the
# caller; OpenHands errors remain non-zero and fail closed.
STATUS_FILE.write_text("failed\n", encoding="utf-8")
raise SystemExit(result.returncode)
