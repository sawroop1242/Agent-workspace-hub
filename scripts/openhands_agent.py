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

MODEL = "moonshotai/kimi-k3"
BASE_URL = "https://integrate.api.nvidia.com/v1"

base_rules = """
You are operating inside the Agent Workspace Hub Rust repository.
Read README.md and relevant docs before changing anything. Follow the existing
architecture and security model. Never weaken tests, security controls, CI gates,
or error handling. Make the smallest coherent change.
"""

if ROLE == "planner":
    prompt = f"""
{base_rules}
You are Agent 1, the Orchestrator. Planning only: do not edit Rust source.
Read .openhands/backlog.json and repository docs. Select exactly one `ready`
feature whose dependencies are satisfied. If an override is provided, use it
only if that feature is ready.

Write the exact implementation contract to `.openhands/generated-task.md`.
The FIRST non-empty line MUST be `Feature ID: AWH-...` using the exact backlog ID.
Then use exactly these headings:
# Feature
# Goal
# Existing Architecture
# Files Likely Affected
# Required Implementation
# Acceptance Criteria
# Verification Commands
# Non-Goals

Acceptance criteria must be concrete and testable. Do not invent requirements.
If no feature is ready, write `NO_READY_FEATURE` to the file.

Requested feature override: {TASK or '(none)'}
"""
elif ROLE == "reviewer":
    prompt = f"""
{base_rules}
You are Agent 3, the independent PR Reviewer and QA/security gate.
Review PR #{PR} completely. Use the terminal to inspect the PR diff and repository
context. Do not modify source code. Evaluate:
- functional correctness and acceptance criteria
- Rust architecture and API compatibility
- error handling and edge cases
- concurrency/state correctness
- security, especially MCP, agent isolation, filesystem access, command execution,
  credentials, workflow permissions and prompt-injection risks
- performance and resource usage
- test quality and coverage
- documentation and maintainability

Produce a concise but technically rigorous review in `.openhands/review.md`.
The final non-empty line MUST be exactly one of:
VERDICT: APPROVE
VERDICT: CHANGES_REQUIRED
VERDICT: BLOCKED

APPROVE only when there are no critical/high defects, the acceptance criteria are
met, and the implementation is safe to merge. If anything important is missing,
use CHANGES_REQUIRED. Use BLOCKED for infrastructure/security conditions that
make a reliable review impossible.
"""
else:
    prompt = f"""
{base_rules}
You are Agent 2, the Builder. Implement the assigned feature in the current
working tree. Inspect existing code before editing. Add focused tests where
behavior changes. Do NOT create a PR, push, or modify workflow files unless the
contract explicitly requires it; the workflow owns Git operations.

TASK CONTRACT:
{TASK}

REVIEW/FIX FEEDBACK:
{REVIEW or '(none)'}

For normal Rust changes, run:
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings

If a verification command fails, investigate it and leave the tree in a state
that makes the failure clear. Never hide failures by disabling tests or linting.
"""

api_key = os.environ.get("AWH_LLM_API_KEY")
if not api_key:
    raise SystemExit("AWH_LLM_API_KEY is required")

llm = LLM(model=MODEL, api_key=api_key, base_url=BASE_URL)
agent = Agent(
    llm=llm,
    tools=[
        Tool(name=TerminalTool.name),
        Tool(name=FileEditorTool.name),
        Tool(name=TaskTrackerTool.name),
    ],
)
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
    verdict = [x.split(":", 1)[1].strip() for x in text.splitlines() if x.startswith("VERDICT:")][-1]
    if verdict not in {"APPROVE", "CHANGES_REQUIRED", "BLOCKED"}:
        raise SystemExit(f"Invalid reviewer verdict: {verdict}")
    Path(".openhands/review-result.json").write_text(
        json.dumps({"verdict": verdict, "review": text}, indent=2) + "\n",
        encoding="utf-8",
    )
    print(text)
else:
    print("Builder finished; deterministic workflow verification follows.")
