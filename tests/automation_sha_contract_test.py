from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_reviewer_pins_and_rechecks_exact_head_sha():
    text = (ROOT / ".github/workflows/awh-reviewer.yml").read_text(encoding="utf-8")
    assert "--sha \"$SHA\"" in text
    assert "Pin and verify exact review head" in text
    assert "active_pr_sha" in text
    assert "current PR head" in text
    assert "REVIEW_INVALIDATED" in text
    assert "reviewed_sha':expected" in text
    assert "Reviewed SHA: `{expected}`" in text


def test_merge_workflow_requires_event_sha_checkpoint_sha_and_current_head():
    text = (ROOT / ".github/workflows/awh-autonomous-loop.yml").read_text(encoding="utf-8")
    assert "REVIEWED_SHA:" in text
    assert "Verify immutable reviewed head before entering MERGING" in text
    assert '"$REVIEWED_SHA" = "$CHECKPOINT_SHA"' in text
    assert '"$CURRENT_SHA" != "$REVIEWED_SHA"' in text
    assert "awh_merge_guard.py --pr \"$PR\" --reviewed-sha \"$REVIEWED_SHA\"" in text
    assert "--match-head-commit" in (ROOT / "scripts/awh_merge_guard.py").read_text(encoding="utf-8")


def test_recovery_repairs_merge_sha_invalidation_into_review():
    text = (ROOT / ".github/workflows/awh-recovery.yml").read_text(encoding="utf-8")
    assert "Repair an invalidated merge approval" in text
    assert "STALE_STATUS" in text
    assert '"$STALE_STATUS" != "MERGING"' in text
    assert "--to REVIEWING" in text
    assert "active_pr_sha" in text
    assert "awh.review" in text
