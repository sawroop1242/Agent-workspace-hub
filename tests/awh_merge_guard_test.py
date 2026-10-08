#!/usr/bin/env python3
"""Regression tests for the immutable Agent 3 review SHA merge guard."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "awh_merge_guard.py"


def load_module(tmp_path: Path):
    spec = importlib.util.spec_from_file_location("awh_merge_guard", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.STATE_PATH = tmp_path / "state.json"
    return module


def write_state(path: Path, **overrides):
    state = {
        "version": 4,
        "status": "MERGING",
        "operation_id": "F:MERGE",
        "recovery_attempt": 0,
        "recovery_claim": None,
        "active_feature": "F",
        "active_pr": 51,
        "active_branch": "feature/F",
        "active_pr_sha": "a" * 40,
        "review_round": 1,
        "builder_attempt": 1,
        "last_error": None,
        "last_review": "VERDICT: APPROVE",
        "updated_at": "2026-09-15T00:00:00Z",
    }
    state.update(overrides)
    path.write_text(json.dumps(state) + "\n", encoding="utf-8")
    return state


def gh_stub(monkeypatch, module, data):
    class Result:
        returncode = 0
        stdout = json.dumps(data)
        stderr = ""

    monkeypatch.setenv("GITHUB_REPOSITORY", "sawroop1242/Agent-workspace-hub")
    monkeypatch.setattr(module.subprocess, "run", lambda *args, **kwargs: Result())


def pr_data(sha: str):
    return {
        "state": "OPEN",
        "mergedAt": None,
        "headRefOid": sha,
        "baseRefName": "rust",
        "isCrossRepository": False,
        "headRefName": "feature/F",
    }


def test_validate_accepts_exact_reviewed_head(tmp_path, monkeypatch):
    module = load_module(tmp_path)
    sha = "a" * 40
    write_state(tmp_path / "state.json", active_pr_sha=sha)
    gh_stub(monkeypatch, module, pr_data(sha))

    assert module.validate(51, sha, "F") == sha


def test_validate_rejects_changed_pr_head(tmp_path, monkeypatch):
    module = load_module(tmp_path)
    reviewed = "a" * 40
    current = "b" * 40
    write_state(tmp_path / "state.json", active_pr_sha=reviewed)
    gh_stub(monkeypatch, module, pr_data(current))

    with pytest.raises(module.MergeGuardError, match="PR head changed after review"):
        module.validate(51, reviewed, "F")


def test_validate_rejects_event_sha_not_matching_checkpoint(tmp_path, monkeypatch):
    module = load_module(tmp_path)
    checkpoint = "a" * 40
    event = "b" * 40
    write_state(tmp_path / "state.json", active_pr_sha=checkpoint)
    gh_stub(monkeypatch, module, pr_data(checkpoint))

    with pytest.raises(module.MergeGuardError, match="does not match checkpoint"):
        module.validate(51, event, "F")


def test_validate_rejects_wrong_pr_or_base(tmp_path, monkeypatch):
    module = load_module(tmp_path)
    sha = "a" * 40
    write_state(tmp_path / "state.json", active_pr=52, active_pr_sha=sha)
    gh_stub(monkeypatch, module, pr_data(sha))
    with pytest.raises(module.MergeGuardError, match="active_pr"):
        module.validate(51, sha, "F")

    write_state(tmp_path / "state.json", active_pr=51, active_pr_sha=sha)
    gh_stub(monkeypatch, module, {**pr_data(sha), "baseRefName": "main"})
    with pytest.raises(module.MergeGuardError, match="PR base"):
        module.validate(51, sha, "F")


def test_merge_uses_expected_head_sha(monkeypatch, tmp_path):
    module = load_module(tmp_path)
    sha = "a" * 40
    write_state(tmp_path / "state.json", active_pr_sha=sha)
    calls = []

    def fake_run(args, **kwargs):
        calls.append(args)

        class Result:
            returncode = 0
            stdout = json.dumps(pr_data(sha)) if args[0] == "gh" and args[1:3] == ["pr", "view"] else "merged"
            stderr = ""

        return Result()

    monkeypatch.setenv("GITHUB_REPOSITORY", "sawroop1242/Agent-workspace-hub")
    monkeypatch.setattr(module.subprocess, "run", fake_run)
    module.merge(51, sha, "F")

    merge_calls = [call for call in calls if call[0:3] == ["gh", "pr", "merge"]]
    assert len(merge_calls) == 1
    assert "--match-head-commit" in merge_calls[0]
    assert merge_calls[0][merge_calls[0].index("--match-head-commit") + 1] == sha
