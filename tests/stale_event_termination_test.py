from pathlib import Path
import importlib.util

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"
PIPELINE = ROOT / "scripts" / "awh_pipeline.py"
OPENHANDS = ROOT / "scripts" / "openhands_agent.py"


def _read(name: str) -> str:
    return (WORKFLOWS / name).read_text(encoding="utf-8")


def _load_pipeline():
    spec = importlib.util.spec_from_file_location("awh_pipeline", PIPELINE)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_stale_event_handlers_stop_without_recording_failure():
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


def test_planning_unrelated_pr_is_not_a_pipeline_mutation_case():
    pipeline = _load_pipeline()
    state = {"status": "PLANNING", "active_feature": "AWH-PLAN-001", "active_pr": None}
    original = state.copy()
    ok, reason = pipeline.validate_event("reviewer", state, "AWH-OTHER-999", 40)
    assert ok is False
    assert "does not admit a reviewer run" in reason
    assert state == original


def test_reviewer_has_analysis_only_mode_and_never_enters_checkpoint_for_it():
    text = _read("awh-reviewer.yml")
    assert "analysis_only=true" in text
    assert "AWH_REVIEW_MODE" in text
    assert "AWH_REVIEW_MODE" in text
    assert "Publish analysis to docs and PR without changing pipeline state" in text
    assert "docs/pr-reviews/pr-${PR}-${SAFE_SHA}.md" in text
    begin = text.index("Checkpoint reviewer stage with CAS and operation identity")
    analysis = text.index("Publish analysis to docs and PR without changing pipeline state")
    assert "steps.validate_event.outputs.analysis_only != 'true'" in text[begin:analysis]


def test_analysis_review_publishes_same_report_to_docs_and_pr():
    text = _read("awh-reviewer.yml")
    section = text[text.index("Publish analysis to docs and PR without changing pipeline state"):text.index("Publish pipeline review and record verdict under CAS")]
    assert 'git add "$REPORT"' in section
    assert 'gh pr comment "$PR"' in section
    assert "Pipeline state: unchanged" in section


def test_review_docs_contract_is_visible_to_agent_1_and_agent_2():
    readme = (ROOT / "docs" / "pr-reviews" / "README.md").read_text(encoding="utf-8")
    agent = OPENHANDS.read_text(encoding="utf-8")
    assert "Agent 1 must read relevant reports" in readme
    assert "Agent 2 must read the report" in readme
    assert "docs/pr-reviews/" in agent
    assert "Agent 2 Working Prompt" in agent
    assert "analysis-only" in agent


def test_reviewer_trigger_and_concurrency_contract_prevent_cancellation_and_duplicate_sync_reviews():
    text = _read("awh-reviewer.yml")
    assert "types: [opened, reopened]" in text
    assert "types: [awh.review]" in text
    assert "synchronize" not in text
    assert "cancel-in-progress: false" in text
    assert "queue: max" in text
    assert '[ "$GITHUB_EVENT_NAME" = "repository_dispatch" ]' in text
    assert "no reviewed_sha; safe no-op" in text
    assert "AWH_EVENT_REVIEWED_SHA" in text


def test_reviewer_stale_sha_is_rejected_before_pipeline_mutation():
    text = _read("awh-reviewer.yml")
    assert "AWH_EVENT_REVIEWED_SHA" in text
    assert "STALE_EVENT: dispatch reviewed_sha" in text
    assert "Pin and verify exact review head" in text
    assert text.index("STALE_EVENT: dispatch reviewed_sha") < text.index("Checkpoint reviewer stage with CAS and operation identity")


def test_merge_stale_guards_happen_before_merging_state_mutation():
    text = _read("awh-autonomous-loop.yml")
    assert "EVENT_OPERATION_ID" in text
    assert "REVIEWED_SHA" in text
    assert "STALE_EVENT: event operation_id=" in text
    assert "STALE_EVENT: reviewed_sha=" in text
    assert "REVIEW_INVALIDATED: PR head changed" in text


def test_no_stale_path_uses_continue_on_error():
    for name in ("awh-builder.yml", "awh-review-fix.yml", "awh-reviewer.yml", "awh-autonomous-loop.yml"):
        assert "continue-on-error" not in _read(name), name


def test_builder_and_fixer_validate_operation_before_checkpoint_mutation():
    builder = _read("awh-builder.yml")
    fixer = _read("awh-review-fix.yml")
    assert "AWH_EVENT_OPERATION_ID" in builder
    assert "CHECKPOINT_OP" in builder
    assert "STALE_EVENT: builder event operation_id=" in builder
    assert "EVENT_OPERATION_ID" in fixer
    assert "CHECKPOINT_OP" in fixer
    assert "STALE_EVENT: fix event operation_id=" in fixer
