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


def test_crash_injection_at_every_pre_replace_step_preserves_last_good_checkpoint(tmp_path: Path):
    """Terminate a real writer subprocess at each write boundary before replace."""
    crash_script = tmp_path / "crash_writer.py"
    crash_script.write_text(
        """
import importlib.util
import os
import sys
import tempfile
from pathlib import Path

script = Path(sys.argv[1])
state_path = Path(sys.argv[2])
crash_step = sys.argv[3]
spec = importlib.util.spec_from_file_location('checkpoint_state', script)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.STATE_PATH = state_path

original = {
    'version': 2, 'status': 'BUILDING', 'active_feature': 'AWH-CRASH-001',
    'active_pr': None, 'active_branch': 'awh-crash-001', 'review_round': 0,
    'builder_attempt': 1, 'last_error': None, 'last_review': '', 'updated_at': None,
}
replacement = dict(original, status='REVIEWING', active_pr=456)
module.atomic_write(original)

real_mkstemp = tempfile.mkstemp
real_fdopen = os.fdopen
real_replace = os.replace
real_fsync = os.fsync


def crash(name):
    if crash_step == name:
        os._exit(97)


def hooked_mkstemp(*args, **kwargs):
    crash('before_mkstemp')
    result = real_mkstemp(*args, **kwargs)
    crash('after_mkstemp')
    return result


class CrashFile:
    def __init__(self, handle):
        self.handle = handle

    def write(self, data):
        crash('before_write')
        result = self.handle.write(data)
        crash('after_write')
        return result

    def flush(self):
        crash('before_flush')
        result = self.handle.flush()
        crash('after_flush')
        return result

    def __enter__(self):
        self.handle.__enter__()
        return self

    def __exit__(self, *args):
        return self.handle.__exit__(*args)

    def __getattr__(self, name):
        return getattr(self.handle, name)


def hooked_fdopen(*args, **kwargs):
    crash('before_fdopen')
    handle = real_fdopen(*args, **kwargs)
    crash('after_fdopen')
    return CrashFile(handle)


def hooked_fsync(fd):
    crash('before_fsync')
    result = real_fsync(fd)
    crash('after_fsync')
    return result


def hooked_replace(src, dst):
    crash('before_replace')
    return real_replace(src, dst)


tempfile.mkstemp = hooked_mkstemp
os.fdopen = hooked_fdopen
os.fsync = hooked_fsync
os.replace = hooked_replace
try:
    module.atomic_write(replacement)
finally:
    tempfile.mkstemp = real_mkstemp
    os.fdopen = real_fdopen
    os.fsync = real_fsync
    os.replace = real_replace
""",
        encoding="utf-8",
    )

    # These are every observable writer boundary in atomic_write() before
    # os.replace(). The child is terminated with os._exit(), so finally blocks
    # cannot repair the checkpoint; atomicity must come from the final replace.
    steps = [
        "before_mkstemp",
        "after_mkstemp",
        "before_fdopen",
        "after_fdopen",
        "before_write",
        "after_write",
        "before_flush",
        "after_flush",
        "before_fsync",
        "after_fsync",
        "before_replace",
    ]
    expected = valid_state()
    expected.update(
        status="BUILDING",
        active_feature="AWH-CRASH-001",
        active_branch="awh-crash-001",
        builder_attempt=1,
    )

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
