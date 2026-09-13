#!/usr/bin/env python3
"""Commit staged changes and push them to a shared branch, never with force."""

from __future__ import annotations

import argparse
import os
import subprocess

MAX_PUSH_ATTEMPTS = 3

# git rejects a plain push either because the remote moved ahead ("fetch
# first") or because histories diverged ("non-fast-forward"); both are
# recoverable with fetch + rebase. Any other rejection is a real failure.
NON_FAST_FORWARD_MARKERS = ("non-fast-forward", "fetch first")


def fail(message: str) -> None:
    raise SystemExit(f"safe_git: {message}")


def git(*arguments: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    """Run git non-interactively; never prompt, and capture output for logs."""
    result = subprocess.run(
        ["git", *arguments],
        capture_output=True,
        text=True,
        env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
    )
    if check and result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        fail(f"'git {' '.join(arguments)}' failed (exit {result.returncode}):\n{detail}")
    return result


def fetch(branch: str) -> None:
    git("fetch", "origin", branch)
    print(f"safe_git: fetched origin/{branch}.")


def require_head_on_branch(branch: str) -> None:
    result = git("symbolic-ref", "--quiet", "HEAD", check=False)
    head = result.stdout.strip()
    if result.returncode != 0 or head != f"refs/heads/{branch}":
        fail(f"HEAD is not on refs/heads/{branch} (got {head or 'detached HEAD'}); refusing to push.")


def commit_staged(message: str) -> None:
    staged = [name for name in git("diff", "--cached", "--name-only").stdout.splitlines() if name.strip()]
    if not staged:
        fail("nothing is staged to commit; refusing to push an empty change. Stage files before calling safe_git.")
    result = git("commit", "-m", message, check=False)
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        if "unable to auto-detect" in detail or "Please tell me who you are" in detail:
            fail(
                "git identity is not configured for this clone; run "
                "'git config user.name <name>' and 'git config user.email <email>' first."
            )
        fail(f"'git commit -m' failed (exit {result.returncode}):\n{detail}")
    print(f"safe_git: committed {len(staged)} staged file(s): {', '.join(staged)}")


def is_non_fast_forward(result: subprocess.CompletedProcess[str]) -> bool:
    combined = (result.stderr + result.stdout).lower()
    return any(marker in combined for marker in NON_FAST_FORWARD_MARKERS)


def rebase_onto(branch: str) -> None:
    result = git("rebase", f"origin/{branch}", check=False)
    if result.returncode == 0:
        print(f"safe_git: rebased onto origin/{branch}.")
        return
    abort = git("rebase", "--abort", check=False)
    suffix = "" if abort.returncode == 0 else f" (git rebase --abort itself failed with exit {abort.returncode})"
    fail(
        f"rebase onto origin/{branch} hit a conflict; the rebase was aborted{suffix} and nothing was pushed. "
        "Resolve the conflict manually - safe_git never force-pushes and never auto-resolves."
    )


def push(branch: str, message: str) -> None:
    fetch(branch)
    require_head_on_branch(branch)
    commit_staged(message)
    for attempt in range(1, MAX_PUSH_ATTEMPTS + 1):
        result = git("push", "origin", branch, check=False)
        if result.returncode == 0:
            print(f"safe_git: pushed to origin/{branch} on attempt {attempt}.")
            return
        if not is_non_fast_forward(result):
            detail = (result.stderr or result.stdout).strip()
            fail(f"push to origin/{branch} failed (exit {result.returncode}):\n{detail}")
        if attempt == MAX_PUSH_ATTEMPTS:
            fail(
                f"push to origin/{branch} was rejected as non-fast-forward on all {MAX_PUSH_ATTEMPTS} attempts; "
                "giving up rather than retrying forever."
            )
        print(f"safe_git: push attempt {attempt} rejected (non-fast-forward); fetching and rebasing.")
        fetch(branch)
        rebase_onto(branch)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("push",))
    parser.add_argument("--branch", required=True, metavar="BRANCH")
    parser.add_argument("--message", required=True, metavar="TEXT")
    args = parser.parse_args(argv)

    if not args.message.strip():
        fail("--message must not be empty")
    if args.action == "push":
        push(args.branch, args.message)


if __name__ == "__main__":
    main()
