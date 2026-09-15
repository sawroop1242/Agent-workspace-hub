from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "awh-reviewer.yml"
ENGINE = ROOT / "scripts" / "reviewer_deterministic.py"
AGENT = ROOT / "scripts" / "openhands_agent.py"
DOC = ROOT / "docs" / "pr-reviews" / "agent3-v2.md"


def test_deterministic_engine_is_integrated_before_llm():
    workflow = WORKFLOW.read_text(encoding="utf-8")
    engine = ENGINE.read_text(encoding="utf-8")
    assert "scripts/reviewer_deterministic.py" in workflow
    assert "Run deterministic Agent 3 v2 review engine" in workflow
    assert workflow.index("Run deterministic Agent 3 v2 review engine") < workflow.index("Run semantic Agent 3 with key rotation")
    assert "executed_pr_code" in engine
    assert "NOT_RUN_UNTRUSTED_PR_CODE" in engine


def test_deterministic_engine_has_high_risk_rules():
    text = ENGINE.read_text(encoding="utf-8")
    for rule in (
        "RULE-WF-001", "RULE-WF-002", "RULE-GIT-001", "RULE-SEC-001",
        "RULE-RUST-001", "RULE-RUST-002", "RULE-EXEC-001", "RULE-NET-001",
        "RULE-STATE-001", "RULE-WF-003",
    ):
        assert rule in text
    assert '"verification": {"status": "CONFIRMED"' in text


def test_semantic_reviewer_must_consume_evidence_and_verify_findings():
    text = AGENT.read_text(encoding="utf-8")
    assert ".openhands/deterministic-review.json" in text
    assert "CONFIRMED, LIKELY, POSSIBLE, or SPECULATION" in text
    assert "Unsupported speculation must not become a blocking finding." in text
    assert "Agent 3 v2" in text


def test_analysis_mode_never_enters_checkpoint_and_publishes_evidence():
    text = WORKFLOW.read_text(encoding="utf-8")
    begin = text.index("Enter checkpoint reviewer stage with CAS")
    deterministic = text.index("Run deterministic Agent 3 v2 review engine")
    assert "analysis_only != 'true'" in text[begin:deterministic]
    publish = text.index("Publish analysis-only evidence to docs and PR")
    assert "Pipeline state: **unchanged**" in text[publish:]
    assert "deterministic-review.json" in text[publish:]
    assert "Agent 2 Working Prompt" in AGENT.read_text(encoding="utf-8")


def test_review_architecture_document_exists():
    text = DOC.read_text(encoding="utf-8")
    assert "Deterministic checks establish facts" in text
    assert "LLM performs contextual reasoning" in text
    assert "finding" in text.lower()
    assert "Agent 1" in text and "Agent 2" in text
