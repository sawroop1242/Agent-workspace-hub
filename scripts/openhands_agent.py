import os
from pathlib import Path

from openhands.sdk import LLM, Agent, Conversation, Tool
from openhands.tools.file_editor import FileEditorTool
from openhands.tools.task_tracker import TaskTrackerTool
from openhands.tools.terminal import TerminalTool

ROLE = os.environ.get("AWH_AGENT_ROLE", "builder")
TASK = os.environ.get("AWH_TASK", "")
REVIEW = os.environ.get("AWH_REVIEW", "")

base_rules = """
You are operating inside the Agent Workspace Hub Rust repository.
Read README.md and relevant docs before changing anything. Follow the existing
architecture and security model. Never weaken tests, security controls, CI gates,
or error handling. Make the smallest coherent change. Do not modify
.github/workflows unless the assigned task explicitly requires it.
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

llm = LLM(
    model=os.environ.get("AWH_LLM_MODEL", "gpt-5.5"),
    api_key=api_key,
    base_url=os.environ.get("AWH_LLM_BASE_URL") or None,
)

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
else:
    print("Builder finished; deterministic workflow verification follows.")
