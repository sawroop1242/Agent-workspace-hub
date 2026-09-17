#!/usr/bin/env python3
"""Dependency-aware feature selection in .github/agent-engine/orchestrator.py."""

from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
ORCHESTRATOR = ROOT / ".github" / "agent-engine" / "orchestrator.py"


def load_module():
    spec = importlib.util.spec_from_file_location("agent_engine_orchestrator", ORCHESTRATOR)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture()
def orchestrator():
    return load_module()


@pytest.fixture()
def features():
    return [
        {"id": "base.capability", "priority": "P0"},
        {"id": "mid.feature", "priority": "P0", "depends_on": ["base.capability"]},
        {"id": "external.deps", "priority": "P0", "depends_on": ["workspace.filesystem"]},
    ]


def test_feature_skipped_while_registered_dependency_incomplete(orchestrator, features):
    selected = orchestrator.select_feature(features, None, [])
    assert selected["id"] == "base.capability"


def test_feature_selected_once_dependency_completed(orchestrator, features):
    selected = orchestrator.select_feature(features, None, ["base.capability"])
    assert selected["id"] in ("mid.feature", "external.deps")
    unmet = orchestrator.unmet_registered_dependencies(selected, features, ["base.capability"])
    assert unmet == []


def test_external_unregistered_dependency_never_blocks(orchestrator, features):
    selected = orchestrator.select_feature(
        [f for f in features if f["id"] == "external.deps"], None, [])
    assert selected["id"] == "external.deps"


def test_explicit_feature_blocked_by_unmet_registered_dependency(orchestrator, features, capsys):
    with pytest.raises(SystemExit) as excinfo:
        orchestrator.select_feature(features, "mid.feature", [])
    assert excinfo.value.code != 0
    assert "BLOCKED: mid.feature depends on unresolved feature base.capability" in capsys.readouterr().out


def test_explicit_feature_selected_when_dependency_completed(orchestrator, features):
    selected = orchestrator.select_feature(features, "mid.feature", ["base.capability"])
    assert selected["id"] == "mid.feature"
