from pathlib import Path
import importlib.util

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"
PIPELINE = ROOT / "scripts" / "awh_pipeline.py"


def _read(name: str) -> str:
    return (WORKFLOWS / name).read_text(encoding="utf-8")


def _load_pipeline():
    spec = importlib.util.spec_from_file_location("awh_pipeline", PIPELINE)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_stale_event_handlers_stop_without_recording_failure():
    """Stale notifications must be safe no-ops before any mutation step."""
    expectations = {
        "awh-builder.yml": "steps.validate_event.outputs.stale != 'true'",
        "awh-review-fix.yml": "steps.validate_event.outputs.stale != 'true'",
        "awh-reviewer.yml": "steps.validate_event.outputs.stale != 'true'",
        "awh-autonomous-loop.yml": "steps.validate_event.outputs.stale != 'true'",
    }

    for name, failure_guard in expectations.items():
        text = _read(name)
        assert "STALE_EVENT:" in text, name
        assert failure_guard in text, name

    reviewer = _read("awh-reviewer.yml")
    # Agent 3 explicitly turns stale validation into a successful no-op; every
    # mutation/LLM step is then gated on the stale output.
    assert "review event for PR $PR does not match the active checkpoint; safe no-op." in reviewer
    assert "exit 0" in reviewer
    assert "if: steps.validate_event.outputs.stale != 'true'" in reviewer
    assert "if: steps.pin.outputs.valid == 'true' && steps.pin.outputs.stale != 'true'" in reviewer


def test_planning_unrelated_pr_is_rejected_as_stale_without_state_mutation():
    """Regression: PLANNING + unrelated PR must not enter Agent 3."""
    pipeline = _load_pipeline()
    state = {
        "status": "PLANNING",
        "active_feature": "AWH-PLAN-001",
        "active_pr": 41,
    }

    ok, reason = pipeline.validate_event("reviewer", state, "AWH-OTHER-999", 40)

    assert ok is False
    assert "does not admit a reviewer run" in reason
    assert "PR_OPEN or REVIEWING" in reason
    assert state == {
        "status": "PLANNING",
        "active_feature": "AWH-PLAN-001",
        "active_pr": 41,
    }


def test_reviewer_trigger_and_concurrency_contract_prevent_cancellation_and_duplicate_sync_reviews():
    text = _read("awh-reviewer.yml")

    assert "types: [opened, reopened]" in text
    assert "types: [awh.review]" in text
    assert "synchronize" not in text
    assert "cancel-in-progress: false" in text
    assert "queue: max" in text

    # repository_dispatch is the authoritative autonomous review trigger and
    # must carry the exact SHA being reviewed.
    assert 'github.event_name == "repository_dispatch"' in text or "github.event_name == 'repository_dispatch'" in text
    assert "no reviewed_sha; safe no-op" in text
    assert "AWH_EVENT_REVIEWED_SHA" in text


def test_reviewer_stale_sha_is_rejected_before_review_mutation():
    text = _read("awh-reviewer.yml")

    assert "AWH_EVENT_REVIEWED_SHA" in text
    assert "STALE_EVENT: event reviewed_sha=" in text
    assert "current_sha=" in text

    # Dispatch SHA checks happen before the reviewer checkpoint begin-stage.
    assert text.index("STALE_EVENT: event reviewed_sha=") < text.index("begin-stage")

    assert "steps.validate_event.outputs.current_sha" in text
    assert "Pin and verify exact review head" in text
    assert text.index("Pin and verify exact review head") < text.index("Run Agent 3")


def test_merge_stale_guards_happen_before_merging_state_mutation():
    text = _read("awh-autonomous-loop.yml")

    assert "EVENT_OPERATION_ID" in text
    assert "REVIEWED_SHA" in text
    assert "STALE_EVENT: event operation_id=" in text
    assert "STALE_EVENT: reviewed_sha=" in text
    assert "REVIEW_INVALIDATED: PR head changed" in text

    validation = text.index("Validate merge event against the authoritative checkpoint")
    begin_merge = text.index("Checkpoint merge stage with CAS and operation identity")
    assert text.index("STALE_EVENT: event operation_id=") < begin_merge
    assert text.index("STALE_EVENT: reviewed_sha=") < begin_merge
    assert validation < begin_merge


def test_no_stale_path_uses_continue_on_error():
    for name in (
        "awh-builder.yml",
        "awh-review-fix.yml",
        "awh-reviewer.yml",
        "awh-autonomous-loop.yml",
    ):
        text = _read(name)
        assert "continue-on-error" not in text, name


def test_builder_and_fixer_validate_operation_before_checkpoint_mutation():
    builder = _read("awh-builder.yml")
    fixer = _read("awh-review-fix.yml")

    assert "AWH_EVENT_OPERATION_ID" in builder
    assert "CHECKPOINT_OP" in builder
    assert "STALE_EVENT: builder event operation_id=" in builder

    assert "EVENT_OPERATION_ID" in fixer
    assert "CHECKPOINT_OP" in fixer
    assert "STALE_EVENT: fix event operation_id=" in fixer

    assert builder.index("STALE_EVENT: builder event operation_id=") < builder.index("begin-stage")
    assert fixer.index("STALE_EVENT: fix event operation_id=") < fixer.index("begin-stage")
