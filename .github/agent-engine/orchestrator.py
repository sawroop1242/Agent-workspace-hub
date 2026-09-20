#!/usr/bin/env python3
"""AWH external long-running agent orchestrator.

This is development infrastructure only; it is not an AWH runtime dependency.
It reads the feature registry, processes one feature at a time, invokes
mini-SWE-agent through FreeLLMAPI's Anthropic-compatible API, runs deterministic
AWH verification, permits exactly one repair attempt, and stores JSON state
and evidence artifacts.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:
    print("Missing dependency: PyYAML")
    print("Install with: python -m pip install pyyaml")
    raise SystemExit(2)

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / ".github/agent-engine/feature-registry.yml"
STATE_DIR = ROOT / ".github/agent-engine/state"
ARTIFACT_DIR = ROOT / ".github/agent-engine/artifacts"
MAX_REPAIR_ATTEMPTS = 1
DEFAULT_ANTHROPIC_MODEL = "anthropic/auto"
DEFAULT_ANTHROPIC_API_BASE = "https://freellmapi-production-905c.up.railway.app/v1"


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def save_json(path: Path, data: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run(command: list[str], *, cwd: Path = ROOT, output_file: Path | None = None,
        env: dict[str, str] | None = None) -> tuple[int, str]:
    print(f"\n$ {' '.join(command)}")
    process = subprocess.run(command, cwd=cwd, text=True,
                             stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                             env=env)
    output = process.stdout
    print(output)
    if output_file:
        output_file.parent.mkdir(parents=True, exist_ok=True)
        output_file.write_text(output, encoding="utf-8")
    return process.returncode, output


def load_anthropic_configuration() -> dict[str, str]:
    api_key = os.environ.get("FREELLMAPI_API_KEY") or os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        raise RuntimeError(
            "FreeLLMAPI credential is missing. Configure the GitHub Actions "
            "secret FREELLMAPI_API_KEY."
        )
    return {
        "api_key": api_key,
        "model": os.environ.get("AWH_ANTHROPIC_MODEL", DEFAULT_ANTHROPIC_MODEL),
        "api_base": os.environ.get("ANTHROPIC_API_BASE", DEFAULT_ANTHROPIC_API_BASE),
    }


def load_registry() -> dict[str, Any]:
    if not REGISTRY.exists():
        raise FileNotFoundError(f"Feature registry not found: {REGISTRY}")
    with REGISTRY.open("r", encoding="utf-8") as f:
        data = yaml.safe_load(f) or {}
    if not isinstance(data, dict):
        raise ValueError("feature-registry.yml must contain a YAML object")
    return data


def get_features(registry: dict[str, Any]) -> list[dict[str, Any]]:
    features = registry.get("features", [])
    if not isinstance(features, list):
        raise ValueError("'features' must be a YAML list")
    return [f for f in features if isinstance(f, dict) and f.get("id")]


def select_feature(features: list[dict[str, Any]], requested_id: str | None,
                   completed: list[str]) -> dict[str, Any] | None:
    if requested_id:
        for feature in features:
            if feature["id"] == requested_id:
                return feature
        raise ValueError(f"Feature not found: {requested_id}")
    priority = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}
    candidates = [f for f in features if f["id"] not in completed]
    candidates.sort(key=lambda f: (priority.get(str(f.get("priority", "P3")), 99), str(f["id"])))
    return candidates[0] if candidates else None


def verification_commands() -> list[list[str]]:
    return [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["cargo", "check", "--all-targets"],
        ["cargo", "test", "--all-targets"],
        ["cargo", "clippy", "--all-targets", "--all-features", "--", "-D", "warnings"],
    ]


def deterministic_verify(artifact_dir: Path) -> tuple[bool, dict[str, Any]]:
    verification_dir = artifact_dir / "verification"
    verification_dir.mkdir(parents=True, exist_ok=True)
    results = []
    passed_all = True
    for index, command in enumerate(verification_commands(), start=1):
        output_file = verification_dir / f"{index:02d}-{command[0]}.txt"
        exit_code, _ = run(command, output_file=output_file)
        passed = exit_code == 0
        results.append({
            "command": command,
            "exit_code": exit_code,
            "passed": passed,
            "artifact": str(output_file.relative_to(artifact_dir)),
        })
        passed_all &= passed
    result = {
        "schema_version": 1,
        "stage": "deterministic_verification",
        "passed": passed_all,
        "results": results,
        "timestamp": utc_now(),
    }
    save_json(verification_dir / "verification-status.json", result)
    return passed_all, result


def build_agent_task(feature: dict[str, Any], repair: bool = False,
                     repair_evidence: Path | None = None) -> str:
    issue_contract = feature.get("issue_contract", {})
    acceptance_contract = feature.get("acceptance_contract", {})
    task = f"""You are working on Agent Workspace Hub (AWH).

Feature: {feature['id']}
Priority: {feature.get('priority', 'unknown')}
Description: {feature.get('description', '')}

Deterministic acceptance contract:
{yaml.safe_dump(acceptance_contract, sort_keys=False).strip()}

Issue-level requirements and verification:
{yaml.safe_dump(issue_contract, sort_keys=False).strip()}

Read before modifying code:
- docs/PROJECT_CONTEXT.md
- docs/PROJECT_ROADMAP.md
- docs/PROJECT_ROADMAP_STATUS.md
- docs/development.md
- docs/security.md
- .github/agent/system.md
- .github/agent/rules.md
- .github/agent/verification.md
- .github/agent/task-template.md
- .github/agent/completion.md
- .github/agent-engine/feature-registry.yml

Rules:
1. Implement only this feature.
2. Preserve existing AWH architecture.
3. Do not weaken or remove tests.
4. Never add mini-SWE-agent, SWE-ReX, SWE-bench, SWE-smith, or other external
   agent tooling to AWH runtime dependencies.
5. Preserve AWH security boundaries.
6. Run deterministic verification.
7. Leave the branch ready for review.
"""
    if repair:
        evidence = str(repair_evidence.relative_to(ROOT)) if repair_evidence else ".github/agent-engine/artifacts"
        task += f"""

This is the single allowed REPAIR attempt. Read the previous deterministic
verification evidence under {evidence}, reproduce the actual failure, and make
the smallest coherent correction. Do not weaken tests to obtain a passing result.
"""
    return task.strip()


def write_mini_config(path: Path, *, anthropic: dict[str, str],
                      cost_limit: str) -> None:
    """Write a v2-compatible mini-SWE-agent config for the AWH worker."""
    use_swerex = os.environ.get("AWH_USE_SWEREX", "1") == "1"
    environment: dict[str, Any] = {
        "environment_class": "swerex_docker" if use_swerex else "docker",
        "cwd": "/workspace",
        "image": "rust:latest",
        "timeout": 180,
    }
    if use_swerex:
        environment["deployment_extra_kwargs"] = {
            "docker_args": [
                "--rm",
                "--network",
                "none",
                "--cap-drop",
                "ALL",
                "--volume",
                f"{ROOT}:/workspace",
            ]
        }
    else:
        environment["run_args"] = [
            "--rm",
            "--network",
            "none",
            "--cap-drop",
            "ALL",
            "--volume",
            f"{ROOT}:/workspace",
        ]

    config = {
        "model": {
            "model_class": "litellm",
            "model_name": anthropic["model"],
            # FreeLLMAPI's router model is intentionally not in LiteLLM's
            # public price catalog, so missing price metadata must not abort
            # the executor. The hard execution bound remains step_limit.
            "cost_tracking": "ignore_errors",
            "model_kwargs": {
                "api_base": anthropic["api_base"],
            },
        },
        "agent": {
            "step_limit": 100,
            "cost_limit": float(cost_limit),
            "mode": "yolo",
            "confirm_exit": False,
        },
        "environment": environment,
    }
    path.write_text(yaml.safe_dump(config, sort_keys=False), encoding="utf-8")


def invoke_mini_swe_agent(feature: dict[str, Any], *, artifact_dir: Path,
                          anthropic: dict[str, str], cost_limit: str,
                          repair: bool = False,
                          repair_evidence: Path | None = None) -> bool:
    task = build_agent_task(feature, repair=repair, repair_evidence=repair_evidence)
    artifact_dir.mkdir(parents=True, exist_ok=True)
    (artifact_dir / "agent-task.txt").write_text(task + "\n", encoding="utf-8")
    output_file = artifact_dir / "mini-swe-agent-output.txt"
    config_file = artifact_dir / "mini-awh.yml"
    write_mini_config(config_file, anthropic=anthropic, cost_limit=cost_limit)

    agent_env = os.environ.copy()
    # FreeLLMAPI credential is supplied only through the parent process
    # environment. It is never written to task text, command arguments,
    # state, or artifacts. The container only executes shell actions.
    agent_env["ANTHROPIC_API_KEY"] = anthropic["api_key"]
    agent_env["ANTHROPIC_API_BASE"] = anthropic["api_base"]
    agent_env["MSWEA_MODEL_NAME"] = anthropic["model"]
    agent_env["MSWEA_COST_TRACKING"] = "ignore_errors"

    command = [
        "mini",
        "-c", str(config_file),
        "-t", task,
        "-y",
        "-o", str(output_file),
    ]

    print(f"Invoking mini-SWE-agent with FreeLLMAPI Anthropic: {anthropic['model']}")
    exit_code, _ = run(command, env=agent_env)

    save_json(artifact_dir / "agent-status.json", {
        "schema_version": 1,
        "stage": "repair_agent" if repair else "implementation_agent",
        "feature_id": feature["id"],
        "completed": exit_code == 0,
        "exit_code": exit_code,
        "model": anthropic["model"],
        "provider": "freellmapi-anthropic",
        "api_base": anthropic["api_base"],
        "credential": "FREELLMAPI_API_KEY",
        "credential_value_saved": False,
        "config": str(config_file.relative_to(ROOT)),
        "timestamp": utc_now(),
    })
    return exit_code == 0


def create_branch(feature_id: str) -> str:
    safe = feature_id.replace("/", "-").replace(".", "-")
    branch = f"agent/{safe}-{int(time.time())}"
    exit_code, _ = run(["git", "switch", "-c", branch])
    if exit_code != 0:
        raise RuntimeError("Failed to create isolated branch")
    return branch


def write_patch_and_status(artifact_dir: Path) -> None:
    _, patch = run(["git", "diff", "--binary"])
    (artifact_dir / "changes.patch").write_text(patch, encoding="utf-8")
    _, status = run(["git", "status", "--short"])
    (artifact_dir / "git-status.txt").write_text(status, encoding="utf-8")
    _, diff_stat = run(["git", "diff", "--stat"])
    (artifact_dir / "git-diff-stat.txt").write_text(diff_stat, encoding="utf-8")


def load_state() -> dict[str, Any]:
    path = STATE_DIR / "state.json"
    if not path.exists():
        return {
            "schema_version": 1,
            "status": "IDLE",
            "current_feature": None,
            "completed": [],
            "failed": [],
            "blocked": [],
            "created_at": utc_now(),
            "updated_at": utc_now(),
        }
    return json.loads(path.read_text(encoding="utf-8"))


def process_feature(feature: dict[str, Any], *, state: dict[str, Any],
                    anthropic: dict[str, str], cost_limit: str) -> bool:
    feature_id = feature["id"]
    artifact_dir = ARTIFACT_DIR / feature_id / str(int(time.time()))
    artifact_dir.mkdir(parents=True, exist_ok=True)

    state.update({"current_feature": feature_id, "status": "RUNNING", "updated_at": utc_now()})
    save_json(STATE_DIR / "state.json", state)

    branch = create_branch(feature_id)
    state["branch"] = branch
    save_json(STATE_DIR / "state.json", state)

    save_json(artifact_dir / "feature.json", feature)
    save_json(artifact_dir / "model-config.json", {
        "provider": "freellmapi-anthropic",
        "model": anthropic["model"],
        "api_base": anthropic["api_base"],
        "credential": "FREELLMAPI_API_KEY",
        "credential_value_saved": False,
    })

    agent_ok = invoke_mini_swe_agent(
        feature,
        artifact_dir=artifact_dir / "initial-agent",
        anthropic=anthropic,
        cost_limit=cost_limit,
    )

    initial_passed, initial_result = deterministic_verify(artifact_dir / "initial")
    save_json(artifact_dir / "initial-verification.json", initial_result)

    repair_attempted = False
    repair_ok = False
    if not agent_ok or not initial_passed:
        repair_attempted = True
        repair_ok = invoke_mini_swe_agent(
            feature,
            artifact_dir=artifact_dir / "repair-agent",
            anthropic=anthropic,
            cost_limit=cost_limit,
            repair=True,
            repair_evidence=artifact_dir / "initial",
        )

    if repair_attempted:
        final_passed, final_result = deterministic_verify(artifact_dir / "final")
        final_passed = final_passed and repair_ok
    else:
        final_passed = initial_passed and agent_ok
        final_result = initial_result

    save_json(artifact_dir / "final-verification.json", final_result)
    write_patch_and_status(artifact_dir)

    decision = "PASS" if final_passed else "FAIL"
    result = {
        "schema_version": 1,
        "feature_id": feature_id,
        "branch": branch,
        "agent_completed": agent_ok,
        "initial_verification": initial_passed,
        "repair_attempted": repair_attempted,
        "repair_completed": repair_ok,
        "final_verification": final_passed,
        "decision": decision,
        "verification_source_of_truth": "deterministic AWH verification",
        "repair_attempt_limit": MAX_REPAIR_ATTEMPTS,
        "timestamp": utc_now(),
        "artifact_directory": str(artifact_dir.relative_to(ROOT)),
    }
    save_json(artifact_dir / "feature-result.json", result)

    target = "completed" if final_passed else "failed"
    if feature_id not in state.setdefault(target, []):
        state[target].append(feature_id)
    state.update({"current_feature": None,
                  "status": "COMPLETED" if final_passed else "FAILED",
                  "updated_at": utc_now()})
    save_json(STATE_DIR / "state.json", state)
    return final_passed


def main() -> int:
    parser = argparse.ArgumentParser(description="AWH external Python agent orchestrator")
    parser.add_argument("--feature", help="Process a specific feature ID")
    parser.add_argument("--model", help="Override Anthropic-compatible model")
    parser.add_argument("--cost-limit", default=os.environ.get("AWH_AGENT_COST_LIMIT", "3"))
    parser.add_argument("--once", action="store_true", help="Run only one feature")
    args = parser.parse_args()

    anthropic = load_anthropic_configuration()
    if args.model:
        anthropic["model"] = args.model

    STATE_DIR.mkdir(parents=True, exist_ok=True)
    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)
    registry = load_registry()
    features = get_features(registry)
    state = load_state()

    print(f"AWH FreeLLMAPI Anthropic model: {anthropic['model']}")
    print(f"Features: {len(features)} | Completed: {len(state.get('completed', []))}")

    while True:
        feature = select_feature(features, args.feature, state.get("completed", []))
        if feature is None:
            state.update({"status": "COMPLETE", "current_feature": None, "updated_at": utc_now()})
            save_json(STATE_DIR / "state.json", state)
            print("No remaining features.")
            return 0

        print(f"\n=== Processing {feature['id']} ===")
        ok = process_feature(feature, state=state, anthropic=anthropic, cost_limit=args.cost_limit)
        if not ok:
            print("Feature failed deterministic verification; stopping.")
            return 1
        if args.once:
            return 0

        args.feature = None


if __name__ == "__main__":
    raise SystemExit(main())
