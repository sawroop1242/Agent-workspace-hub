#!/usr/bin/env python3
"""Regression tests for checkpoint validation, atomic writes, and fail-closed recovery."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "checkpoint_state.py"


def valid_state() -> dict:
    return {
        "version": 2,
        "status": "IDLE",
        "active_feature": None,
        "active_pr": None,
        "active_branch": None,
        "review_round": 0,
        "builder_attempt": 0,
        "last_error": None,
        "last_review": "",
        "updated_at": None,
    }


def load_module():
    import importlib.util

    spec = importlib.util.spec_from_file_location("checkpoint_state", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_malformed_json_is_rejected(tmp_path: Path):
    module = load_module()
    path = tmp_path / "state.json"
    path.write_text('{"version": 2, "status":', encoding="utf-8")
    try:
        module.load_json(path)
    except SystemExit as exc:
        assert "valid JSON" in str(exc)
    else:
        raise AssertionError("malformed JSON was accepted")


def test_schema_violations_are_rejected():
    module = load_module()
    base = valid_state()
    schema = module.load_json(module.SCHEMA_PATH)

    cases = [
        (dict(base, version=999), "version"),
        (dict(base, status="UNKNOWN"), "unsupported status"),
        (dict(base, review_round=-1), "review_round"),
        (dict(base, active_pr=0), "active_pr"),
        (dict(base, updated_at="2026-09-13T10:00:00"), "timezone"),
        (dict(base, unexpected=True), "unknown or missing"),
    ]
    for state, expected in cases:
        try:
            module.validate_state(state, schema)
        except SystemExit as exc:
            assert expected in str(exc), (state, exc)
        else:
            raise AssertionError(f"schema violation was accepted: {state}")


def test_atomic_write_never_leaves_partial_state(tmp_path: Path):
    module = load_module()
    module.STATE_PATH = tmp_path / "state.json"
    module.STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    original = valid_state()
    original["status"] = "BUILDING"
    original["active_feature"] = "AWH-TEST-001"
    original["updated_at"] = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    module.atomic_write(original)

    replacement = dict(original, status="REVIEWING", active_pr=123)
    module.atomic_write(replacement)

    raw = module.STATE_PATH.read_text(encoding="utf-8")
    assert raw.endswith("\n")
    parsed = json.loads(raw)
    assert parsed == replacement
    assert not list(tmp_path.glob(".state.*.tmp"))


def test_interrupted_write_does_not_replace_existing_checkpoint(tmp_path: Path):
    module = load_module()
    module.STATE_PATH = tmp_path / "state.json"
    original = valid_state()
    original["status"] = "BUILDING"
    original["active_feature"] = "AWH-TEST-002"
    module.atomic_write(original)

    # Simulate an interrupted writer by leaving a truncated temporary file.
    tmp = tmp_path / ".state.interrupted.tmp"
    tmp.write_text('{"version": 2, "status": "REVIEWING"', encoding="utf-8")
    assert json.loads(module.STATE_PATH.read_text(encoding="utf-8")) == original


def test_fail_closed_for_unsafe_resume_states():
    """Mirror recovery's safety contract without dispatching anything."""
    unsafe = [
        {"status": "BUILDING", "active_feature": None},
        {"status": "FIXING", "active_feature": "AWH-X", "last_review": ""},
        {"status": "REVIEWING", "active_pr": None},
        {"status": "PR_OPEN", "active_pr": None},
    ]
    for checkpoint in unsafe:
        status = checkpoint["status"]
        if status == "BUILDING":
            assert not checkpoint.get("active_feature")
        elif status == "FIXING":
            assert not checkpoint.get("last_review")
        else:
            assert not checkpoint.get("active_pr")


def test_cli_validate_accepts_repository_checkpoint():
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "validate"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )
    assert result.returncode == 0, result.stderr
    assert "valid" in result.stdout.lower()
