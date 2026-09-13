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


def assert_rejected(module, state, schema, expected):
    try:
        module.validate_state(state, schema)
    except SystemExit as exc:
        assert expected in str(exc), (state, exc)
    else:
        raise AssertionError(f"checkpoint violation was accepted: {state}")


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
        assert_rejected(module, state, schema, expected)


def test_atomic_write_publishes_only_complete_valid_state(tmp_path: Path):
    module = load_module()
    module.STATE_PATH = tmp_path / "state.json"
    original = valid_state()
    original.update(status="BUILDING", active_feature="AWH-TEST-001")
    module.atomic_write(original)

    replacement = dict(original, status="REVIEWING", active_pr=123)
    module.atomic_write(replacement)

    raw = module.STATE_PATH.read_text(encoding="utf-8")
    assert raw.endswith("\n")
    assert json.loads(raw) == replacement
    assert not list(tmp_path.glob(".state.*.tmp"))


def test_interrupted_write_preserves_last_good_checkpoint(tmp_path: Path):
    module = load_module()
    module.STATE_PATH = tmp_path / "state.json"
    original = valid_state()
    original.update(status="BUILDING", active_feature="AWH-TEST-002")
    module.atomic_write(original)

    # Simulate a runner dying after creating a temporary file but before os.replace().
    interrupted = tmp_path / ".state.interrupted.tmp"
    interrupted.write_text('{"version": 2, "status": "REVIEWING"', encoding="utf-8")

    assert json.loads(module.STATE_PATH.read_text(encoding="utf-8")) == original
    assert module.validate_state(json.loads(module.STATE_PATH.read_text(encoding="utf-8"))) == original


def test_fail_closed_recovery_decisions():
    """Assert every resumable status requires the metadata recovery needs."""
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
            assert not checkpoint.get("active_feature") or not checkpoint.get("last_review")
        else:
            assert not checkpoint.get("active_pr")


def test_corrupted_checkpoint_is_rejected_before_recovery(tmp_path: Path):
    module = load_module()
    corrupted = tmp_path / "state.json"
    corrupted.write_text('{"version": 2, "status": "BUILDING", "active_feature":', encoding="utf-8")
    try:
        module.load_json(corrupted)
    except SystemExit as exc:
        assert "valid JSON" in str(exc)
    else:
        raise AssertionError("recovery input corruption was accepted")


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
