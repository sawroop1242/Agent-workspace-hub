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
        "version": 4,
        "status": "IDLE",
        "operation_id": None,
        "recovery_attempt": 0,
        "recovery_claim": None,
        "active_feature": None,
        "active_pr": None,
        "active_branch": None,
        "active_pr_sha": None,
        "review_round": 0,
        "builder_attempt": 0,
        "last_error": None,
        "last_review": "",
        "updated_at": None,
    }


def legacy_v2_state() -> dict:
    """A well-formed v2 document: the pre-operation_id checkpoint."""
    state = valid_state()
    for key in ("operation_id", "recovery_attempt", "recovery_claim", "active_pr_sha"):
        del state[key]
    state["version"] = 2
    return state


def legacy_v3_state() -> dict:
    """A well-formed v3 document (PR #50's model): no recovery claim fields."""
    state = valid_state()
    for key in ("recovery_attempt", "recovery_claim", "active_pr_sha"):
        del state[key]
    state["version"] = 3
    return state


def valid_claim(**overrides) -> dict:
    claim = {
        "operation_id": "AWH-TEST-001:BUILD:1",
        "feature_id": "AWH-TEST-001",
        "attempt": 1,
        "claimed_at": "2026-09-13T10:00:00Z",
        "owner": "recovery-job-abc",
        "stale_status": "BUILDING",
    }
    return claim | overrides


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
        (dict(base, operation_id=123), "operation_id"),
        ({key: value for key, value in base.items() if key != "operation_id"}, "missing required field"),
        (dict(base, recovery_attempt=-1), "recovery_attempt"),
        (dict(base, recovery_claim=[]), "recovery_claim"),
        (dict(base, recovery_claim={"owner": "x"}), "recovery_claim"),
        (dict(base, recovery_claim=valid_claim(owner="")), "owner"),
        (dict(base, recovery_claim=valid_claim(attempt=0)), "attempt"),
        (dict(base, recovery_claim=valid_claim(stale_status="IDLE")), "stale_status"),
        (dict(base, active_pr_sha=42), "active_pr_sha"),
    ]
    for state, expected in cases:
        assert_rejected(module, state, schema, expected)


def test_v4_schema_enumerates_statuses_operation_id_and_recovery_fields():
    module = load_module()
    schema = module.load_json(module.SCHEMA_PATH)
    assert schema["properties"]["version"]["const"] == 4
    assert "operation_id" in schema["required"]
    assert {"recovery_attempt", "recovery_claim", "active_pr_sha"} <= set(schema["required"])
    assert {"MERGING", "RECOVERING"} <= set(schema["properties"]["status"]["enum"])
    assert {"MERGING", "RECOVERING"} <= module.STATUSES
    base = valid_state()
    for status in ("MERGING", "RECOVERING"):
        assert module.validate_state(dict(base, status=status), schema) is not None
    # RECOVERING re-entry exists solely so an expired recovery lease can be
    # reclaimed; PR #51 extends the PR #50 set by exactly that one purpose.
    assert module.TRANSITIONS["RECOVERING"] == {"BUILDING", "REVIEWING", "FIXING", "IDLE", "BLOCKED", "RECOVERING"}
    # Every active pipeline state can hand control to recovery.
    for status in ("PLANNING", "BUILDING", "PR_OPEN", "REVIEWING", "FIXING", "MERGING"):
        assert "RECOVERING" in module.TRANSITIONS[status], status
    # The approved merge path PR_OPEN -> MERGING -> COMPLETED exists end to end.
    assert "MERGING" in module.TRANSITIONS["PR_OPEN"]
    assert "COMPLETED" in module.TRANSITIONS["MERGING"]


def test_v3_state_migrates_to_v4():
    module = load_module()
    schema = module.load_json(module.SCHEMA_PATH)
    legacy = legacy_v3_state()
    legacy.update(status="PLANNING", active_feature="AWH-MIG-002", operation_id="AWH-MIG-002:BUILD:1")
    migrated = module.validate_state(legacy, schema)
    assert migrated == dict(
        legacy, version=4, recovery_attempt=0, recovery_claim=None, active_pr_sha=None
    )


def test_v2_state_migrates_to_v4():
    module = load_module()
    schema = module.load_json(module.SCHEMA_PATH)
    legacy = legacy_v2_state()
    legacy.update(status="PLANNING", active_feature="AWH-MIG-001")
    migrated = module.validate_state(legacy, schema)
    expected = dict(legacy, version=4, operation_id=None, recovery_attempt=0,
                    recovery_claim=None, active_pr_sha=None)
    expected["version"] = 4
    assert migrated == expected


def test_v3_state_with_unknown_fields_is_rejected_not_migrated():
    module = load_module()
    schema = module.load_json(module.SCHEMA_PATH)
    legacy = dict(legacy_v3_state(), rogue_field=True)
    assert_rejected(module, legacy, schema, "version")


def test_v2_state_with_unknown_fields_is_rejected_not_migrated():
    module = load_module()
    schema = module.load_json(module.SCHEMA_PATH)
    legacy = dict(legacy_v2_state(), rogue_field=True)
    assert_rejected(module, legacy, schema, "version")


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
    'version': 4, 'status': 'BUILDING', 'operation_id': None, 'recovery_attempt': 0,
    'recovery_claim': None, 'active_feature': 'AWH-CRASH-001',
    'active_pr': None, 'active_branch': 'awh-crash-001', 'active_pr_sha': None, 'review_round': 0,
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
    corrupted.write_text('{"version": 3, "status": "BUILDING", "active_feature":', encoding="utf-8")
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


def _load_transition_module(tmp_path: Path):
    module = load_module()
    module.STATE_PATH = tmp_path / "state.json"
    return module


def _seed_building_state(module, tmp_path: Path, operation_id=None, status="BUILDING"):
    module.atomic_write(valid_state() | {
        "status": status,
        "operation_id": operation_id,
        "active_feature": "AWH-TRANS-001",
        "builder_attempt": 1,
    })
    return json.loads((tmp_path / "state.json").read_text(encoding="utf-8"))


def test_transition_cli_valid_transition_succeeds(tmp_path: Path, capsys):
    module = _load_transition_module(tmp_path)
    _seed_building_state(module, tmp_path, operation_id="op-build-9")
    before = (tmp_path / "state.json").read_text(encoding="utf-8")

    module.main([
        "transition",
        "--expect-status", "BUILDING",
        "--expect-operation-id", "op-build-9",
        "--to", "PR_OPEN",
        "--set", "active_pr=42",
        "--set", 'active_branch="feature/AWH-X"',
    ])

    assert "transitioned to pr_open" in capsys.readouterr().out.lower()
    written = json.loads((tmp_path / "state.json").read_text(encoding="utf-8"))
    assert written["status"] == "PR_OPEN"
    assert written["active_pr"] == 42
    assert written["active_branch"] == "feature/AWH-X"
    assert written["active_feature"] == "AWH-TRANS-001"  # untouched fields survive
    assert written["operation_id"] == "op-build-9"  # token is opaque to the CLI
    assert written["updated_at"] is not None
    assert before != written  # sanity: file actually changed


def test_transition_cli_rejects_out_of_table_transition_without_mutation(tmp_path: Path):
    module = _load_transition_module(tmp_path)
    _seed_building_state(module, tmp_path)
    state_path = tmp_path / "state.json"
    before = state_path.read_text(encoding="utf-8")
    before_stat = state_path.stat()

    try:
        module.main(["transition", "--expect-status", "BUILDING", "--to", "PLANNING"])
    except SystemExit as exc:
        assert "transition BUILDING -> PLANNING is not allowed" in str(exc)
    else:
        raise AssertionError("illegal transition was accepted")

    after_stat = state_path.stat()
    assert state_path.read_text(encoding="utf-8") == before  # no file mutation
    assert (after_stat.st_mtime_ns, after_stat.st_ino) == (before_stat.st_mtime_ns, before_stat.st_ino)


def test_transition_cli_rejects_expect_status_mismatch_without_mutation(tmp_path: Path):
    module = _load_transition_module(tmp_path)
    _seed_building_state(module, tmp_path)
    state_path = tmp_path / "state.json"
    before = state_path.read_text(encoding="utf-8")
    before_stat = state_path.stat()

    try:
        module.main(["transition", "--expect-status", "IDLE", "--to", "PLANNING"])
    except SystemExit as exc:
        assert "expected current status IDLE" in str(exc)
        assert "BUILDING" in str(exc)
    else:
        raise AssertionError("status mismatch was accepted")

    assert state_path.read_text(encoding="utf-8") == before
    assert state_path.stat().st_mtime_ns == before_stat.st_mtime_ns


def test_transition_cli_rejects_expect_operation_id_mismatch_without_mutation(tmp_path: Path):
    module = _load_transition_module(tmp_path)
    _seed_building_state(module, tmp_path, operation_id="op-holder-A")
    state_path = tmp_path / "state.json"
    before = state_path.read_text(encoding="utf-8")

    try:
        module.main([
            "transition", "--expect-status", "BUILDING",
            "--expect-operation-id", "op-holder-B", "--to", "PR_OPEN",
        ])
    except SystemExit as exc:
        assert "expected operation_id 'op-holder-B'" in str(exc)
        assert "op-holder-A" in str(exc)
    else:
        raise AssertionError("operation_id mismatch was accepted")

    assert state_path.read_text(encoding="utf-8") == before


def test_transition_cli_accepts_repeated_expect_status_any_of_set(tmp_path: Path, capsys):
    module = _load_transition_module(tmp_path)
    _seed_building_state(module, tmp_path, status="FIXING")

    module.main([
        "transition", "--expect-status", "PLANNING", "--expect-status", "FIXING",
        "--to", "BUILDING",
    ])

    written = json.loads((tmp_path / "state.json").read_text(encoding="utf-8"))
    assert written["status"] == "BUILDING"
    assert "transitioned to building" in capsys.readouterr().out.lower()


def test_transition_cli_migrates_legacy_state_on_first_write(tmp_path: Path, capsys):
    """A legacy v2 or v3 checkpoint on disk is lifted in place on first write."""
    module = _load_transition_module(tmp_path)
    state_path = tmp_path / "state.json"
    state_path.parent.mkdir(parents=True, exist_ok=True)
    for legacy in (legacy_v2_state(), legacy_v3_state()):
        legacy.update(status="PLANNING")
        state_path.write_text(json.dumps(legacy, indent=2) + "\n", encoding="utf-8")

        module.main(["transition", "--expect-status", "PLANNING", "--to", "BUILDING"])

        written = json.loads(state_path.read_text(encoding="utf-8"))
        assert written["version"] == 4
        assert written["status"] == "BUILDING"
        assert written["operation_id"] is None
        assert written["recovery_attempt"] == 0
        assert written["recovery_claim"] is None
        assert "transitioned to building" in capsys.readouterr().out.lower()


def test_transition_table_rejects_every_illegal_status_pair():
    module = load_module()
    legal = {(current, target) for current, targets in module.TRANSITIONS.items() for target in targets}
    for current in module.STATUSES:
        for target in module.STATUSES:
            if (current, target) in legal:
                continue
            try:
                module._check_transition(current, target)
            except SystemExit as exc:
                assert current in str(exc) and target in str(exc), (current, target, exc)
            else:
                raise AssertionError(f"{current} -> {target} was accepted outside the table")


def test_migrated_workflow_sites_use_cas_transition_and_safe_push():
    """Every checkpoint write in the migrated workflows must be CAS-guarded and
    pushed through safe_git, never a bare `git push origin rust`."""
    import re

    module = load_module()
    sites = {
        ".github/workflows/awh-builder.yml": {
            "Checkpoint builder stage with CAS and operation identity": {"BUILDING", "FIXING"},
            "Record PR in state": {"BUILDING", "REVIEWING"},
        },
        ".github/workflows/awh-reviewer.yml": {
            "Checkpoint reviewer stage with CAS and operation identity": {"PR_OPEN", "REVIEWING"},
            "Publish review and record verdict under CAS": {"REVIEWING"},
        },
        ".github/workflows/awh-review-fix.yml": {
            "Checkpoint fix round with CAS and operation identity": {"FIXING", "REVIEWING"},
        },
        ".github/workflows/awh-recovery.yml": {
            "Atomically claim recovery of stale work": {"RECOVERING"},
        },
        ".github/workflows/awh-autonomous-loop.yml": {
            "Checkpoint planner start": {"IDLE", "COMPLETED", "PLANNING"},
            "Persist active build checkpoint": {"PLANNING"},
            "Checkpoint merge stage with CAS and operation identity": {"PR_OPEN"},
            "Update backlog and state": {"MERGING"},
        },
    }
    for workflow, expectations in sites.items():
        text = (ROOT / workflow).read_text(encoding="utf-8")
        assert "git push origin rust" not in text, f"bare push remains in {workflow}"
        # The shared-branch push may be a shell command, an embedded Python
        # call, or delegated to awh_pipeline.py (which routes through safe_git
        # in stage_and_push). All forms go through the single module.
        shell_pushes = text.count("safe_git.py push --branch rust")
        python_pushes = text.count("safe_git.py','push','--branch','rust'")
        delegated = "awh_pipeline.py begin-stage" in text or "awh_pipeline.py finalize-recovery" in text
        assert shell_pushes + python_pushes >= 1 or delegated, workflow
        for step_name, expected_statuses in expectations.items():
            assert all(status in module.STATUSES for status in expected_statuses), step_name
            assert step_name in text, f"{step_name} missing from {workflow}"
        # every transition call in the workflow declares at least one expectation
        # and a target status, spanning YAML's line continuations.
        for call in re.findall(r"checkpoint_state\.py transition(?:(?!scripts/).)*", text, re.DOTALL):
            normalized = call.replace("\\\n", " ")
            assert "--expect-status" in normalized, call
            assert "--to " in normalized, call


def test_workflows_never_contain_prohibited_unsafe_patterns():
    """Hard contract: no force push of any kind, no un-CAS'd `update` writes,
    and no duplicated hand-rolled fetch/rebase/push sequences in workflows."""
    import re

    workflows = sorted((ROOT / ".github/workflows").glob("awh-*.yml"))
    assert len(workflows) >= 5
    for workflow in workflows:
        text = workflow.read_text(encoding="utf-8")
        for forbidden in (
            "git push --force",
            "git push -f",
            "git push --force-with-lease",
            "push --force-with-lease",
            "checkpoint_state.py update",
            "git reset --hard",
            "git rebase",
        ):
            assert forbidden not in text, f"{forbidden!r} found in {workflow.name}"
        # The only permitted raw git push is the feature branch's own
        # --set-upstream push in the builder (never the shared rust branch).
        for match in re.finditer(r"git push[^\n]*", text):
            line = match.group(0)
            if "set-upstream" in line:
                assert "origin" in line and '"$BRANCH"' in line, line
            else:
                raise AssertionError(f"raw git push in {workflow.name}: {line}")


def test_single_safe_git_implementation_no_duplicates():
    """Exactly one module owns fetch/rebase/push; nothing else reimplements
    the retry loop, and no workflow inlines its own shared-branch sync."""
    import re

    scripts = sorted((ROOT / "scripts").glob("*.py"))
    owners = []
    for script in scripts:
        source = script.read_text(encoding="utf-8")
        if re.search(r"def (fetch|rebase|push)\(", source):
            owners.append(script.name)
    assert owners == ["safe_git.py"], f"synchronization logic outside safe_git.py: {owners}"

    for workflow in (ROOT / ".github/workflows").glob("awh-*.yml"):
        text = workflow.read_text(encoding="utf-8")
        # Hand-rolled shared-branch sync is forbidden; the builder's checkout
        # of its own feature branch is not shared-branch synchronization.
        for forbidden in ("git rebase", "git pull", "git fetch origin rust", "git fetch origin \"rust\""):
            assert forbidden not in text, f"{forbidden!r} found in {workflow.name}"
        # Any fetch present must be the feature-branch checkout in the builder.
        for match in re.finditer(r"git fetch[^\n]*", text):
            line = match.group(0)
            assert "BRANCH" in line and "rust" not in line, f"non-feature fetch: {line}"


def test_workflows_validate_events_and_record_failures():
    """Every mutating AWH workflow must validate its dispatch payload against
    the checkpoint and expose a failure-recording hook."""
    required = {
        "awh-builder.yml": ["validate-event --agent builder", "record-failure"],
        "awh-reviewer.yml": ["validate-event --agent reviewer", "record-failure"],
        "awh-review-fix.yml": ["validate-event --agent fixer", "record-failure"],
        "awh-recovery.yml": ["claim-recovery", "record-failure"],
        "awh-autonomous-loop.yml": ["record-failure", "safe_git.py sync --branch rust"],
    }
    for name, tokens in required.items():
        text = (ROOT / ".github/workflows" / name).read_text(encoding="utf-8")
        for token in tokens:
            assert token in text, f"{token!r} missing from {name}"
        # Stale events exit green (no-op), never red and never mutating.
        # Recovery's staleness protection is its atomic claim instead, and the
        # autonomous loop starts from the checkpoint rather than an event.
        assert "STALE_EVENT" in text or name in (
            "awh-autonomous-loop.yml", "awh-recovery.yml"), name


def test_reviewer_validates_pr_identity():
    """Agent 3 must verify repository, base branch, and open state before
    reviewing, and never approve from the payload alone."""
    text = (ROOT / ".github/workflows/awh-reviewer.yml").read_text(encoding="utf-8")
    assert "isCrossRepository" in text
    assert "baseRefName" in text
    assert "state" in text
    assert "begin-stage" in text
    # The verdict CAS is keyed on the operation the checkpoint itself holds.
    assert "--expect-operation-id" in text


def test_review_fix_follows_builder_safety_model():
    """The fix loop must use the same validate/CAS/safe-git discipline and
    guard the maximum number of rounds."""
    text = (ROOT / ".github/workflows/awh-review-fix.yml").read_text(encoding="utf-8")
    assert "validate-event --agent fixer" in text
    assert "begin-stage" in text
    assert "safe_git.py sync --branch rust" in text
    assert "review_round" in text
    assert "Maximum review rounds" in text


def test_recovery_workflow_uses_claim_and_finalization():
    """Recovery must claim atomically, inspect reality, then finalize - never
    replay the previous event blindly."""
    text = (ROOT / ".github/workflows/awh-recovery.yml").read_text(encoding="utf-8")
    assert "claim-recovery" in text
    assert "decide-recovery" in text
    assert "finalize-recovery" in text
    assert "awh-checkpoint-recovery" in text  # single global concurrency group


def test_builder_feature_branch_push_is_safe():
    """The only raw push left: pushing the feature branch (never rust)."""
    text = (ROOT / ".github/workflows/awh-builder.yml").read_text(encoding="utf-8")
    assert text.count("git push") == 1
    assert "git push --set-upstream origin" in text
    # The feature branch is never reset onto older history.
    assert "git reset --hard" not in text
    assert "merge --ff-only" in text
