#!/usr/bin/env python3
"""Read, validate, and atomically update the AWH checkpoint state."""

from __future__ import annotations

import argparse
import json
import os
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
STATE_PATH = ROOT / ".openhands" / "state.json"
SCHEMA_PATH = ROOT / ".openhands" / "state.schema.json"

STATUSES = {"IDLE", "PLANNING", "BUILDING", "PR_OPEN", "REVIEWING", "FIXING", "COMPLETED", "BLOCKED"}


def fail(message: str) -> None:
    raise SystemExit(f"Invalid checkpoint state: {message}")


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read valid JSON from {path}: {exc}")


def validate_state(state: Any, schema: Any | None = None) -> dict[str, Any]:
    # Keep the schema in-repo and validate the exact contract here without
    # requiring a third-party package on GitHub-hosted runners.
    if schema is None:
        schema = load_json(SCHEMA_PATH)
    if not isinstance(state, dict):
        fail("root must be an object")
    if state.get("version") != schema["properties"]["version"]["const"]:
        fail(f"version must be {schema['properties']['version']['const']}")
    required = schema["required"]
    missing = [key for key in required if key not in state]
    if missing:
        fail(f"missing required field(s): {', '.join(missing)}")
    if set(state) != set(required):
        fail("unknown or missing top-level fields are not allowed")
    status = state["status"]
    if status not in STATUSES:
        fail(f"unsupported status {status!r}")

    nullable_strings = {"active_feature", "active_branch", "last_error"}
    for key in nullable_strings:
        value = state[key]
        if value is not None and not isinstance(value, str):
            fail(f"{key} must be a string or null")
    if not isinstance(state["last_review"], str):
        fail("last_review must be a string")
    if state["active_pr"] is not None and (
        isinstance(state["active_pr"], bool) or not isinstance(state["active_pr"], int) or state["active_pr"] <= 0
    ):
        fail("active_pr must be a positive integer or null")
    for key in ("review_round", "builder_attempt"):
        value = state[key]
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            fail(f"{key} must be a non-negative integer")
    updated = state["updated_at"]
    if updated is not None:
        if not isinstance(updated, str):
            fail("updated_at must be an ISO-8601 string or null")
        try:
            stamp = datetime.fromisoformat(updated.replace("Z", "+00:00"))
        except ValueError:
            fail("updated_at is not valid ISO-8601")
        if stamp.tzinfo is None:
            fail("updated_at must include a timezone")
    return state


def load_state() -> dict[str, Any]:
    return validate_state(load_json(STATE_PATH))


def _sync_directory(directory: Path) -> None:
    """Durably sync the parent directory where the platform supports it.

    POSIX filesystems can fsync a directory after os.replace(), which closes
    the durability window for the rename. Windows does not expose
    os.O_DIRECTORY, and attempting to use it raises AttributeError. The file
    itself is flushed before replacement on every platform; on Windows we
    therefore rely on the platform's atomic replace semantics rather than
    opening the directory with a POSIX-only flag.
    """
    directory_flag = getattr(os, "O_DIRECTORY", None)
    if directory_flag is None:
        return
    dir_fd = os.open(directory, directory_flag)
    try:
        os.fsync(dir_fd)
    finally:
        os.close(dir_fd)


def atomic_write(state: dict[str, Any]) -> None:
    validate_state(state)
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(prefix=".state.", suffix=".tmp", dir=STATE_PATH.parent)
    tmp_path = Path(tmp_name)
    try:
        payload = json.dumps(state, indent=2, sort_keys=False) + "\n"
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp_path, STATE_PATH)
        _sync_directory(STATE_PATH.parent)
    finally:
        if tmp_path.exists():
            tmp_path.unlink()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("validate", "update"))
    parser.add_argument("--set", action="append", default=[], metavar="KEY=JSON")
    args = parser.parse_args()

    state = load_state()
    if args.action == "validate":
        print("Checkpoint state is valid.")
        return

    for assignment in args.set:
        if "=" not in assignment:
            fail(f"--set must be KEY=JSON, got {assignment!r}")
        key, raw = assignment.split("=", 1)
        if key not in state:
            fail(f"unknown field {key!r}")
        try:
            state[key] = json.loads(raw)
        except json.JSONDecodeError as exc:
            fail(f"invalid JSON for {key}: {exc}")
    if "updated_at" in state and not any(item.startswith("updated_at=") for item in args.set):
        state["updated_at"] = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    atomic_write(state)
    print("Checkpoint state updated atomically and validated.")


if __name__ == "__main__":
    main()
