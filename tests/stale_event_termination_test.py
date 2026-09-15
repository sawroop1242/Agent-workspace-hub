from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"


def _read(name: str) -> str:
    return (WORKFLOWS / name).read_text(encoding="utf-8")


def test_stale_event_handlers_fail_the_job_and_do_not_record_failure():
    """A stale notification must fail its validation step so GitHub's default
    success() gating skips every later mutation step. The failure recorder must
    explicitly exclude the stale case so it cannot mutate the checkpoint either.
    """
    expectations = {
        "awh-builder.yml": "steps.validate_event.outputs.stale != 'true'",
        "awh-review-fix.yml": "steps.validate_event.outputs.stale != 'true'",
        "awh-reviewer.yml": "steps.validate_event.outputs.stale != 'true'",
        "awh-autonomous-loop.yml": "steps.validate_event.outputs.stale != 'true'",
    }

    for name, failure_guard in expectations.items():
        text = _read(name)
        assert "STALE_EVENT:" in text, name
        assert "exit 1" in text, name
        assert failure_guard in text, name

    # No stale-event handler is allowed to terminate successfully. A successful
    # step would let later mutation steps run under GitHub Actions' default
    # success() gating.
    for name in expectations:
        text = _read(name)
        stale_blocks = re.findall(
            r"STALE_EVENT:.*?(?=\n\s*(?:fi|else|exit|echo|test|if|CHECKPOINT|CURRENT|PRDATA)|\Z)",
            text,
            flags=re.DOTALL,
        )
        assert stale_blocks, f"no stale-event blocks found in {name}"
        for block in stale_blocks:
            assert "exit 0" not in block, f"stale path can continue in {name}: {block}"


def test_builder_and_fixer_validate_operation_before_checkpoint_mutation():
    builder = _read("awh-builder.yml")
    fixer = _read("awh-review-fix.yml")

    assert "AWH_EVENT_OPERATION_ID" in builder
    assert "CHECKPOINT_OP" in builder
    assert "STALE_EVENT: builder event operation_id=" in builder

    assert "EVENT_OPERATION_ID" in fixer
    assert "CHECKPOINT_OP" in fixer
    assert "STALE_EVENT: fix event operation_id=" in fixer

    # The operation check belongs to the validation step, before begin-stage.
    assert builder.index("STALE_EVENT: builder event operation_id=") < builder.index("begin-stage")
    assert fixer.index("STALE_EVENT: fix event operation_id=") < fixer.index("begin-stage")


def test_reviewer_stale_sha_is_rejected_before_review_mutation():
    text = _read("awh-reviewer.yml")

    assert "AWH_EVENT_REVIEWED_SHA" in text
    assert "STALE_EVENT: event reviewed_sha=" in text
    assert "REVIEW_INVALIDATED: event reviewed_sha=" in text
    assert "current_sha=" in text

    # The dispatch SHA checks happen before the reviewer checkpoint begin-stage.
    assert text.index("STALE_EVENT: event reviewed_sha=") < text.index("begin-stage")
    assert text.index("REVIEW_INVALIDATED: event reviewed_sha=") < text.index("begin-stage")

    # A synchronize event may legitimately carry a new head; the reviewer pins
    # that fresh head into the checkpoint, then rechecks it before Agent 3.
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
