#!/usr/bin/env python3
"""Run Agent 3 with exactly one LLM secret in the child environment."""
from __future__ import annotations

import os
import re
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
for name in ("K1", "K2", "K3", "KF", "AWH_REVIEW_KEY_NAME"):
    child_env.pop(name, None)

result = subprocess.run(
    [sys.executable, "scripts/openhands_agent.py"],
    env=child_env,
    text=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
)
print(result.stdout, end="")
if result.returncode == 0:
    STATUS_FILE.write_text("success\n", encoding="utf-8")
    raise SystemExit(0)

if re.search(r"(^|[^0-9])429([^0-9]|$)|rate.?limit|too many requests", result.stdout, re.IGNORECASE):
    STATUS_FILE.write_text("rate_limit\n", encoding="utf-8")
    raise SystemExit(75)

STATUS_FILE.write_text("failed\n", encoding="utf-8")
raise SystemExit(result.returncode)
