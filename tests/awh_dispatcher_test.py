import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("awh_dispatcher", ROOT / "scripts/awh_dispatcher.py")
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def state(status, **extra):
    return {"status": status, **extra}


def test_idle_starts_planner():
    result = MODULE.plan_dispatch(state("IDLE"))
    assert result["event"] == "awh.start"
    assert result["payload"] == {}


def test_building_resumes_builder_with_operation_identity():
    result = MODULE.plan_dispatch(
        state("BUILDING", active_feature="AWH-1", operation_id="AWH-1:BUILD:1")
    )
    assert result["event"] == "awh.build"
    assert result["payload"] == {
        "feature_id": "AWH-1",
        "operation_id": "AWH-1:BUILD:1",
    }


def test_review_carries_pr_and_sha():
    result = MODULE.plan_dispatch(
        state(
            "REVIEWING",
            active_feature="AWH-2",
            operation_id="AWH-2:BUILD:1",
            active_pr=51,
            active_pr_sha="abc123",
        )
    )
    assert result["event"] == "awh.review"
    assert result["payload"]["pr_number"] == 51
    assert result["payload"]["reviewed_sha"] == "abc123"


def test_blocked_requests_recovery_without_reset():
    result = MODULE.plan_dispatch(
        state("BLOCKED", active_feature="AWH-3", operation_id="AWH-3:BUILD:1")
    )
    assert result["event"] == "awh.recover"
    assert result["payload"]["operation_id"] == "AWH-3:BUILD:1"


def test_active_planner_or_recovery_waits():
    assert MODULE.plan_dispatch(state("PLANNING", active_feature="AWH-4"))["action"] == "wait"
    assert MODULE.plan_dispatch(
        state("RECOVERING", active_feature="AWH-4", operation_id="AWH-4:BUILD:1")
    )["action"] == "wait"


def test_orphan_startup_states_request_repair():
    for status in ("PLANNING", "BLOCKED", "RECOVERING"):
        result = MODULE.plan_dispatch(
            state(status, active_feature=None, operation_id=None)
        )
        assert result["action"] == "repair"
        assert "orphan startup" in result["reason"]


def test_missing_identity_is_rejected():
    try:
        MODULE.plan_dispatch(state("BUILDING"))
    except ValueError as exc:
        assert "operation_id" in str(exc)
    else:
        raise AssertionError("missing BUILDING identity must be rejected")
