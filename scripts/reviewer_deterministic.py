#!/usr/bin/env python3
"""Deterministic, non-executing PR checks for Agent 3.

This deliberately analyzes GitHub metadata and the textual diff only. It never
checks out or executes untrusted PR code. LLM review consumes this evidence and
must not override confirmed deterministic findings.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / ".openhands" / "deterministic-review.json"


def run(*args: str) -> str:
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT)


def finding(fid: str, severity: str, category: str, title: str, evidence: str) -> dict:
    return {
        "id": fid,
        "severity": severity,
        "confidence": 1.0,
        "category": category,
        "title": title,
        "evidence": evidence[:2000],
        "verification": {"status": "CONFIRMED", "method": "deterministic-diff-rule"},
    }


def main() -> int:
    pr = os.environ.get("AWH_PR", "").strip()
    expected_sha = os.environ.get("AWH_REVIEW_SHA", "").strip()
    if not pr or not expected_sha:
        raise SystemExit("AWH_PR and AWH_REVIEW_SHA are required")

    metadata = json.loads(run("gh", "pr", "view", pr, "--repo", os.environ["GITHUB_REPOSITORY"], "--json", "headRefOid,baseRefName,state,isCrossRepository,headRefName"))
    current_sha = metadata["headRefOid"]
    if current_sha != expected_sha:
        raise SystemExit(f"REVIEW_INVALIDATED: expected {expected_sha}, current {current_sha}")
    if metadata["baseRefName"] != "rust" or metadata["state"] != "OPEN" or metadata["isCrossRepository"]:
        raise SystemExit("PR is not eligible for deterministic review")

    diff = run("gh", "pr", "diff", str(pr), "--repo", os.environ["GITHUB_REPOSITORY"], "--patch")
    files_text = run("gh", "pr", "view", str(pr), "--repo", os.environ["GITHUB_REPOSITORY"], "--json", "files")
    files = json.loads(files_text).get("files", [])
    paths = [x.get("path", "") for x in files]
    findings: list[dict] = []

    if re.search(r"(?im)^\+.*permissions:\s*$", diff) and re.search(r"(?im)^\+\s+contents:\s+write", diff):
        findings.append(finding("RULE-WF-001", "HIGH", "workflow-security", "PR grants workflow contents write permission", "Added workflow permissions include contents: write."))
    if re.search(r"(?im)^\+.*pull_request_target", diff) and re.search(r"(?im)^\+.*(checkout|run:)", diff):
        findings.append(finding("RULE-WF-002", "HIGH", "workflow-security", "Privileged pull_request_target workflow executes or checks out code", "Diff changes a pull_request_target workflow and also changes checkout/run behavior; inspect for untrusted-code execution."))
    if re.search(r"(?im)^\+.*(force-push|--force(?:-with-lease)?|git push --force)", diff):
        findings.append(finding("RULE-GIT-001", "HIGH", "git-safety", "PR introduces force-push behavior", "Added diff text contains a force-push operation."))
    if re.search(r"(?im)^\+.*(NVIDIA_API_KEY|OPENAI_API_KEY|GH_TOKEN|GITHUB_TOKEN|password|secret)\s*[:=]\s*['\"]?[A-Za-z0-9_\-]{12,}", diff):
        findings.append(finding("RULE-SEC-001", "CRITICAL", "secrets", "PR appears to add a credential-like literal", "Added lines contain a credential-shaped assignment. Verify immediately."))
    if re.search(r"(?im)^\+.*\bunsafe\b", diff):
        findings.append(finding("RULE-RUST-001", "HIGH", "rust-safety", "PR adds unsafe Rust", "Added lines contain the unsafe keyword."))
    if re.search(r"(?im)^\+.*\b(?:unwrap|expect)\s*\(", diff):
        findings.append(finding("RULE-RUST-002", "MEDIUM", "error-handling", "PR adds unwrap/expect", "Added lines contain unwrap/expect; verify panic behavior is justified."))
    if re.search(r"(?im)^\+.*(?:Command::new|std::process::Command|tokio::process::Command)", diff):
        findings.append(finding("RULE-EXEC-001", "HIGH", "command-execution", "PR adds process execution", "Added lines invoke a process command API; review command construction and injection boundaries."))
    if re.search(r"(?im)^\+.*(?:TcpListener|0\.0\.0\.0|SocketAddr.*0\.0\.0\.0)", diff):
        findings.append(finding("RULE-NET-001", "HIGH", "network-security", "PR exposes a non-loopback listener", "Added lines indicate binding to 0.0.0.0 or an equivalent non-loopback listener."))
    if re.search(r"(?im)^\+.*(?:\.openhands/state\.json|checkpoint_state|active_pr_sha)", diff):
        findings.append(finding("RULE-STATE-001", "HIGH", "checkpoint-state", "PR changes checkpoint state handling", "Added lines touch checkpoint/state persistence; verify CAS, operation identity and stale-event protection."))
    if any(p.startswith(".github/workflows/") for p in paths) and not any(p.startswith("tests/") for p in paths):
        findings.append(finding("RULE-WF-003", "MEDIUM", "ci", "Workflow changes have no accompanying test-file change", "Workflow files changed but no tests/ path changed; verify workflow contract coverage."))
    if any(p.startswith("tests/") for p in paths) and any("test" in p.lower() for p in paths):
        test_note = "Test files are changed; deterministic engine does not execute them because PR code is untrusted in pull_request_target."
    else:
        test_note = "No test-file change detected."

    result = {
        "schema_version": 1,
        "reviewer_version": "agent3-v2",
        "pr": int(pr),
        "reviewed_sha": expected_sha,
        "mode": os.environ.get("AWH_REVIEW_MODE", "pipeline"),
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "scope": {"base": metadata["baseRefName"], "head": metadata["headRefName"], "changed_files": paths},
        "deterministic": {
            "diff_available": True,
            "executed_pr_code": False,
            "test_execution": "NOT_RUN_UNTRUSTED_PR_CODE",
            "test_note": test_note,
        },
        "findings": findings,
        "summary": {"finding_count": len(findings), "critical_or_high": sum(x["severity"] in {"CRITICAL", "HIGH"} for x in findings)},
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
