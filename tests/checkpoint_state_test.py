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


def test_crash_injection_before_replace_preserves_last_good_checkpoint(tmp_path: Path):
    """Kill the writer at every pre-replace step and verify the old checkpoint survives."""
    crash_script = tmp_path / "crash_writer.py"
    crash_script.write_text(
        """\nimport importlib.util\nimport os\nimport sys\nfrom pathlib import Path\n\nscript = Path(sys.argv[1])\nstate_path = Path(sys.argv[2])\ncrash_step = sys.argv[3]\nspec = importlib.util.spec_from_file_location('checkpoint_state', script)\nmodule = importlib.util.module_from_spec(spec)\nspec.loader.exec_module(module)\nmodule.STATE_PATH = state_path\n\noriginal = {\n    'version': 2, 'status': 'BUILDING', 'active_feature': 'AWH-CRASH-001',\n    'active_pr': None, 'active_branch': 'awh-crash-001', 'review_round': 0,\n    'builder_attempt': 1, 'last_error': None, 'last_review': '', 'updated_at': None,\n}\nreplacement = dict(original, status='REVIEWING', active_pr=456)\nmodule.atomic_write(original)\n\noriginal_replace = os.replace\noriginal_fsync = os.fsync\noriginal_fdopen = os.fdopen\n\ndef crash(name):\n    if crash_step == name:\n        os._exit(97)\n\ndef hooked_fsync(fd):\n    crash('before_fsync')\n    result = original_fsync(fd)\n    crash('after_fsync')\n    return result\n\ndef hooked_fdopen(fd, *args, **kwargs):\n    crash('before_fdopen')\n    result = original_fdopen(fd, *args, **kwargs)\n    crash('after_fdopen')\n    return result\n\ndef hooked_replace(src, dst):\n    crash('before_replace')\n    return original_replace(src, dst)\n\nos.fsync = hooked_fsync\nos.fdopen = hooked_fdopen\nos.replace = hooked_replace\ntry:\n    module.atomic_write(replacement)\nfinally:\n    os.fsync = original_fsync\n    os.fdopen = original_fdopen\n    os.replace = original_replace\n""",
        encoding="utf-8",
    )

    # These are the observable pre-replace boundaries in atomic_write(): temp
    # file creation, opening the temp file, writing/flush completion, file
    # fsync, and the final os.replace boundary. The injected process exits
    # before each selected boundary, leaving the committed checkpoint untouched.
    steps = ["before_fdopen", "after_fdopen", "before_fsync", "after_fsync", "before_replace"]
    expected = valid_state()
    expected.update(status="BUILDING", active_feature="AWH-CRASH-001", active_branch="awh-crash-001", builder_attempt=1)

    for step in steps:
        state_path = tmp_path / f"state-{step}.json"
        result = subprocess.run(
            [sys.executable, str(crash_script), str(SCRIPT), str(state_path), step],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
            env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
        )
        assert result.returncode == 97, (step, result.stdout, result.stderr)
        assert state_path.exists(), step
        raw = state_path.read_text(encoding="utf-8")
        assert json.loads(raw) == expected, step
        module = load_module()
        module.STATE_PATH = state_path
        assert module.load_state() == expected, step


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
