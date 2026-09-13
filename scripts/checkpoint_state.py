#!/usr/bin/env python3
"""Read, validate, and atomically update the AWH checkpoint state."""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(os.environ.get("AWH_REPO_ROOT", Path(__file__).resolve().parents[1]))
STATE_PATH = ROOT / ".openhands" / "state.json"
SCHEMA_PATH = ROOT / ".openhands" / "state.schema.json"

STATE_VERSION = 4
LEGACY_STATE_VERSIONS = (2, 3)

STATUSES = {
    "IDLE",
    "PLANNING",
    "BUILDING",
    "PR_OPEN",
    "REVIEWING",
    "FIXING",
    "MERGING",
    "COMPLETED",
    "BLOCKED",
    "RECOVERING",
}

# Legal status changes. Re-entrant entries (BUILDING -> BUILDING on a fix
# re-dispatch, REVIEWING -> REVIEWING on a re-review, ...) let a workflow
# refresh checkpoint metadata under the same compare-and-swap discipline;
# they never skip or bypass a stage. RECOVERING -> RECOVERING exists solely
# so a crashed recovery lease can be reclaimed or refreshed once it has
# itself gone stale; it is not a way to loop recovery forever.
TRANSITIONS: dict[str, set[str]] = {
    "IDLE": {"PLANNING"},
    "PLANNING": {"BUILDING", "BLOCKED", "PLANNING", "RECOVERING"},
    "BUILDING": {"PR_OPEN", "BLOCKED", "BUILDING", "REVIEWING", "RECOVERING"},
    "PR_OPEN": {"REVIEWING", "BLOCKED", "COMPLETED", "MERGING", "RECOVERING"},
    "REVIEWING": {"FIXING", "MERGING", "BLOCKED", "REVIEWING", "PR_OPEN", "RECOVERING"},
    "FIXING": {"REVIEWING", "BLOCKED", "FIXING", "BUILDING", "RECOVERING"},
    # Idempotent re-entry of the same stage: a retried dispatch of the same
    # operation re-enters its own stage rather than being rejected as illegal.
    # MERGING re-entry exists so a crashed merge (state left in MERGING) can be
    # retried/recovered into MERGING again; it must still exit only via
    # COMPLETED or BLOCKED.
    "MERGING": {"COMPLETED", "BLOCKED", "MERGING", "RECOVERING"},
    "COMPLETED": {"IDLE", "PLANNING"},
    "BLOCKED": {"RECOVERING", "IDLE"},
    "RECOVERING": {"BUILDING", "REVIEWING", "FIXING", "IDLE", "BLOCKED", "RECOVERING"},
}

# Claim owners are free-form strings recorded in the checkpoint for humans
# and tooling; the safety property comes from the CAS transition itself.
RECOVERY_CLAIM_FIELDS = ("operation_id", "feature_id", "attempt", "claimed_at", "owner", "stale_status")
STALEABLE_STATUSES = ("PLANNING", "BUILDING", "PR_OPEN", "REVIEWING", "FIXING", "MERGING", "RECOVERING")


def fail(message: str) -> None:
    raise SystemExit(f"Invalid checkpoint state: {message}")


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read valid JSON from {path}: {exc}")


def _v4_template_from_legacy(state: dict[str, Any]) -> dict[str, Any]:
    """Field-ordered v4 document built from a validated v2/v3 payload."""
    return {
        "version": STATE_VERSION,
        "status": state["status"],
        "operation_id": state.get("operation_id"),
        "recovery_attempt": 0,
        "recovery_claim": None,
        "active_feature": state["active_feature"],
        "active_pr": state["active_pr"],
        "active_branch": state["active_branch"],
        "active_pr_sha": None,
        "review_round": state["review_round"],
        "builder_attempt": state["builder_attempt"],
        "last_error": state["last_error"],
        "last_review": state["last_review"],
        "updated_at": state["updated_at"],
    }


def migrate_state(state: Any) -> dict[str, Any] | None:
    """Lift a well-formed legacy state (v2 or v3) to v4; return None when not applicable.

    v2 -> v3 added only `operation_id` (caller-supplied, no v2 equivalent).
    v3 -> v4 adds `recovery_attempt`, `recovery_claim`, and `active_pr_sha`
    (the staging field for immutable PR-head-SHA review tracking). Every lift
    is injective: defaults are null/zero and no information is lost. Anything
    that is not a valid legacy document falls through and is rejected.
    """
    if not isinstance(state, dict) or state.get("version") not in LEGACY_STATE_VERSIONS:
        return None
    version = state["version"]
    base_required = [
        "version",
        "status",
        "active_feature",
        "active_pr",
        "active_branch",
        "review_round",
        "builder_attempt",
        "last_error",
        "last_review",
        "updated_at",
    ]
    expected = list(base_required)
    if version >= 3:
        expected.insert(2, "operation_id")
    if set(state) != set(expected):
        return None
    # Rebuild in schema order so a migrated state.json reads like a native v4 one.
    return _v4_template_from_legacy(state)


def validate_state(state: Any, schema: Any | None = None) -> dict[str, Any]:
    # Keep the schema in-repo and validate the exact contract here without
    # requiring a third-party package on GitHub-hosted runners.
    if schema is None:
        schema = load_json(SCHEMA_PATH)
    if not isinstance(state, dict):
        fail("root must be an object")
    version = state.get("version")
    if version in LEGACY_STATE_VERSIONS:
        migrated = migrate_state(state)
        if migrated is None:
            fail(f"version must be {STATE_VERSION}")
        return validate_state(migrated, schema)
    if version != schema["properties"]["version"]["const"]:
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

    if state["operation_id"] is not None and not isinstance(state["operation_id"], str):
        fail("operation_id must be a string or null")
    nullable_strings = {"active_feature", "active_branch", "last_error", "active_pr_sha"}
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
    for key in ("review_round", "builder_attempt", "recovery_attempt"):
        value = state[key]
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            fail(f"{key} must be a non-negative integer")
    claim = state["recovery_claim"]
    if claim is not None:
        if not isinstance(claim, dict):
            fail("recovery_claim must be an object or null")
        if set(claim) != set(RECOVERY_CLAIM_FIELDS):
            fail(
                "recovery_claim must contain exactly "
                f"{', '.join(RECOVERY_CLAIM_FIELDS)}"
            )
        if not isinstance(claim["owner"], str) or not claim["owner"].strip():
            fail("recovery_claim.owner must be a non-empty string")
        if not isinstance(claim["claimed_at"], str) or not claim["claimed_at"].strip():
            fail("recovery_claim.claimed_at must be a non-empty ISO-8601 string")
        if claim["stale_status"] not in STALEABLE_STATUSES:
            fail(f"recovery_claim.stale_status must be one of {', '.join(STALEABLE_STATUSES)}")
        for key in ("operation_id", "feature_id"):
            value = claim[key]
            if value is not None and not isinstance(value, str):
                fail(f"recovery_claim.{key} must be a string or null")
        if isinstance(claim["attempt"], bool) or not isinstance(claim["attempt"], int) or claim["attempt"] < 1:
            fail("recovery_claim.attempt must be a positive integer")
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


def _check_transition(current: str, requested: str) -> None:
    """Reject any status change that is not in TRANSITIONS. Fail closed."""
    allowed = TRANSITIONS.get(current)
    if allowed is None:
        fail(f"unknown current status {current!r}")
    if requested not in allowed:
        fail(
            f"transition {current} -> {requested} is not allowed "
            f"(permitted from {current}: {', '.join(sorted(allowed))})"
        )


def apply_assignments(state: dict[str, Any], assignments: list[str]) -> dict[str, Any]:
    for assignment in assignments:
        if "=" not in assignment:
            fail(f"--set must be KEY=JSON, got {assignment!r}")
        key, raw = assignment.split("=", 1)
        if key not in state:
            fail(f"unknown field {key!r}")
        try:
            state[key] = json.loads(raw)
        except json.JSONDecodeError as exc:
            fail(f"invalid JSON for {key}: {exc}")
    return state


def stamp_updated_at(state: dict[str, Any], assignments: list[str]) -> dict[str, Any]:
    if "updated_at" in state and not any(item.startswith("updated_at=") for item in assignments):
        state["updated_at"] = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    return state


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


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("validate", "update", "transition"))
    parser.add_argument("--set", action="append", default=[], metavar="KEY=JSON")
    parser.add_argument(
        "--expect-status",
        action="append",
        default=[],
        metavar="STATUS",
        help="current status required for a transition to proceed; repeatable",
    )
    parser.add_argument(
        "--expect-operation-id",
        default="",
        metavar="ID",
        help="current operation_id required for a transition; empty matches any",
    )
    parser.add_argument("--to", metavar="STATUS", help="requested status for a transition")
    args = parser.parse_args(argv)

    if args.action == "validate":
        load_state()
        print("Checkpoint state is valid.")
        return

    if args.action == "transition":
        state = load_state()
        expected = args.expect_status or [state["status"]]
        if state["status"] not in expected:
            fail(
                f"expected current status {' or '.join(expected)}, "
                f"but state has {state['status']!r}; refusing to transition"
            )
        expected_operation_id = args.expect_operation_id
        if expected_operation_id != "" and state["operation_id"] != expected_operation_id:
            fail(
                f"expected operation_id {expected_operation_id!r}, "
                f"but state has {state['operation_id']!r}; refusing to transition"
            )
        requested = args.to
        if requested is None:
            fail("transition requires --to STATUS")
        _check_transition(state["status"], requested)
        apply_assignments(state, args.set)
        state["status"] = requested
        stamp_updated_at(state, args.set)
        atomic_write(state)
        print(f"Checkpoint transitioned to {requested} atomically and validated.")
        return

    # `update` is retained only for compatibility with callers outside the AWH
    # automation workflows (for example a human repairing a broken checkpoint).
    # It performs no status change and no CAS guards; every automation workflow
    # must use `transition` instead. Contract tests enforce this.
    state = load_state()
    original_status = state["status"]
    apply_assignments(state, args.set)
    if state["status"] != original_status:
        fail("update must not change status; use 'transition' with --expect-status/--to")
    stamp_updated_at(state, args.set)
    atomic_write(state)
    print("Checkpoint state updated atomically and validated.")


if __name__ == "__main__":
    main()
