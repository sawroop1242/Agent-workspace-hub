import json
import os
from pathlib import Path

from openhands.sdk import LLM, Agent, Conversation, Tool
from openhands.tools.file_editor import FileEditorTool
from openhands.tools.task_tracker import TaskTrackerTool
from openhands.tools.terminal import TerminalTool

ROLE = os.environ.get("AWH_AGENT_ROLE", "builder")
TASK = os.environ.get("AWH_TASK", "")
REVIEW = os.environ.get("AWH_REVIEW", "")
PR = os.environ.get("AWH_PR", "")
REVIEW_MODE = os.environ.get("AWH_REVIEW_MODE", "pipeline")
MODEL = os.environ.get("AWH_LLM_MODEL", "moonshotai/kimi-k3")
BASE_URL = os.environ.get("AWH_LLM_BASE_URL", "https://integrate.api.nvidia.com/v1")

base_rules = """
You are operating inside the Agent Workspace Hub Rust repository.
You have complete read access to the repository's `docs/` tree. Treat it as a
first-class source of architecture, implementation-status, roadmap, security,
operational, and issue-resolution context. Reconcile docs with source, tests,
Git history, and CI. Never weaken tests, security controls, CI gates, or error
handling. Make the smallest coherent change.
"""

if ROLE == "planner":
    prompt = f"""
{base_rules}
You are Agent 1, the Orchestrator. Planning only: do not edit Rust source.
Read `.openhands/backlog.json`, `.openhands/state.json`, relevant `docs/`, all
relevant `docs/issue-resolving-prompts/`, and relevant `docs/pr-reviews/`.
Inspect source, tests, Git history, open/merged PRs and CI. Select exactly one
ready feature whose dependencies are satisfied. Treat PR reviews as evidence,
not authority over source, tests, security invariants, or checkpoint state.

Write `.openhands/generated-task.md`. First non-empty line MUST be
`Feature ID: AWH-...`. Use headings:
# Feature
# Goal
# Issue / Prompt Context
# Existing Architecture
# Files Likely Affected
# Required Implementation
# Acceptance Criteria
# Verification Commands
# Non-Goals

Requested feature override: {TASK or '(none)'}
"""
elif ROLE == "reviewer":
    mode_rules = """
This is an analysis-only review. Do NOT mutate `.openhands/state.json`, call
checkpoint transitions, or advance/recover the main Agent pipeline. Produce
repository knowledge for Agent 1 and an actionable Agent 2 Working Prompt.
""" if REVIEW_MODE == "analysis" else """
This is the active Agent 2 pipeline PR. Review it against the authoritative
checkpoint, task contract, source, tests, CI, and issue-resolution prompt.
"""
    prompt = f"""
{base_rules}
You are Agent 3 v2, an independent PR Reviewer and QA/security gate. Review PR
#{PR} completely. Do not modify source code.

FIRST read `.openhands/deterministic-review.json`. It is generated from the
exact PR head using non-executing deterministic rules. Treat confirmed findings
as evidence. Never claim a test or command ran unless evidence or CI proves it.

{mode_rules}

Use this hybrid process:
1. Deterministic evidence: validate applicability of each rule finding.
2. Semantic review: reason about intent, architecture, correctness,
   concurrency/state, security, API compatibility, error handling, performance,
   tests, documentation conflicts, and maintainability.
3. Evidence verification: every actionable finding needs concrete file/line or
   repository evidence and status CONFIRMED, LIKELY, POSSIBLE, or SPECULATION.
   Unsupported speculation must not become a blocking finding.
4. Deduplicate findings and assign severity and confidence.

For analysis-only mode include `Agent 2 Working Prompt` with ordered steps,
constraints, acceptance criteria, and verification commands.

Produce `.openhands/review.md` with:
# Summary
# Deterministic Evidence
# Findings
# Security
# Tests and Verification
# Documentation / Architecture Conflicts
# Agent 2 Working Prompt (analysis mode only)
# Verdict Rationale

Final non-empty line MUST be exactly:
VERDICT: APPROVE
or
VERDICT: CHANGES_REQUIRED
or
VERDICT: BLOCKED

APPROVE only when no critical/high defects remain and acceptance criteria are
met. Use CHANGES_REQUIRED for important defects/missing requirements and
BLOCKED when a reliable review is impossible.
"""
else:
    prompt = f"""
{base_rules}
You are Agent 2, the Builder. Implement the assigned feature in the current
working tree. Read relevant docs, matching issue-resolution prompts, source,
tests, and applicable `docs/pr-reviews/` reports. Reconcile all guidance with
current source, tests, CI and security invariants.

TASK CONTRACT:
{TASK}

REVIEW/FIX FEEDBACK:
{REVIEW or '(none)'}

Run for normal Rust changes:
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
"""

api_key = os.environ.get("AWH_LLM_API_KEY")
if not api_key:
    raise SystemExit("AWH_LLM_API_KEY is required")

llm = LLM(model=MODEL, api_key=api_key, base_url=BASE_URL)
agent = Agent(llm=llm, tools=[
    Tool(name=TerminalTool.name),
    Tool(name=FileEditorTool.name),
    Tool(name=TaskTrackerTool.name),
])
conversation = Conversation(agent=agent, workspace=os.getcwd())
conversation.send_message(prompt)
conversation.run()

if ROLE == "planner":
    task_file = Path(".openhands/generated-task.md")
    if not task_file.exists():
        raise SystemExit("Planner did not create .openhands/generated-task.md")
    text = task_file.read_text(encoding="utf-8").strip()
    if not text or text == "NO_READY_FEATURE":
        raise SystemExit("No ready feature is available")
    print(text)
elif ROLE == "reviewer":
    review_file = Path(".openhands/review.md")
    if not review_file.exists():
        raise SystemExit("Reviewer did not create .openhands/review.md")
    text = review_file.read_text(encoding="utf-8").strip()
    if not text or "VERDICT:" not in text:
        raise SystemExit("Reviewer output has no machine-readable verdict")
    verdicts = [x.split(":", 1)[1].strip() for x in text.splitlines() if x.startswith("VERDICT:")]
    if len(verdicts) != 1 or verdicts[0] not in {"APPROVE", "CHANGES_REQUIRED", "BLOCKED"}:
        raise SystemExit("Reviewer output must contain exactly one valid VERDICT")
    evidence = {}
    evidence_file = Path(".openhands/deterministic-review.json")
    if evidence_file.exists():
        evidence = json.loads(evidence_file.read_text(encoding="utf-8"))
    Path(".openhands/review-result.json").write_text(
        json.dumps({"schema_version": 1, "verdict": verdicts[0], "review": text,
                    "mode": REVIEW_MODE, "deterministic": evidence}, indent=2) + "\n",
        encoding="utf-8")
    print(text)
else:
    print("Builder finished; deterministic workflow verification follows.")
