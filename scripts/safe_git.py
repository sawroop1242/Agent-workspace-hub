#!/usr/bin/env python3
"""Commit staged changes and push them to a shared branch, never with force.

Also provides `sync`: fetch the shared branch and fast-forward the local
branch onto it, refusing to proceed when the histories have diverged."""

from __future__ import annotations

import argparse
import os
import subprocess
from pathlib import Path

MAX_PUSH_ATTEMPTS = 3

# git rejects a plain push either because the remote moved ahead ("fetch
# first") or because histories diverged ("non-fast-forward"); both are
# recoverable with fetch + rebase. Any other rejection is a real failure.
NON_FAST_FORWARD_MARKERS = ("non-fast-forward", "fetch first")


class GitError(RuntimeError):
    """A git operation failed in a way the caller cannot safely resolve."""


class LostClaimError(RuntimeError):
    """The remote branch changed before a recovery claim could be published."""


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


def sync(branch: str) -> None:
    """Fetch the shared branch and fast-forward the local branch to it.

    Never rebases, never resets, never discards local history: if the local
    branch has diverged from the remote, the caller must resolve it through
    the normal push path (fetch -> rebase -> retry). A clean sync leaves the
    clone on exactly origin/branch with no local commits, which is the state
    every AWH workflow needs before reading the authoritative checkpoint.
    """
    fetch(branch)
    require_head_on_branch(branch)
    result = git("merge", "--ff-only", f"origin/{branch}", check=False)
    if result.returncode != 0:
        behind = git("rev-list", "--count", f"{branch}..origin/{branch}", check=False).stdout.strip()
        ahead = git("rev-list", "--count", f"origin/{branch}..{branch}", check=False).stdout.strip()
        fail(
            f"local branch {branch} has diverged from origin/{branch} "
            f"(ahead {ahead}, behind {behind}); sync is fast-forward only "
            "and never rewrites local or remote history. Resolve the divergence explicitly."
        )
    print(f"safe_git: synced {branch} to origin/{branch} (fast-forward only).")


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


def run_git(
    repo: Path,
    arguments: list[str],
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    """Run git in an explicit repository, non-interactively, output captured."""
    result = subprocess.run(
        ["git", "-C", str(repo), *arguments],
        capture_output=True,
        text=True,
        env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
    )
    if check and result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        raise GitError(f"'git {' '.join(arguments)}' in {repo} failed (exit {result.returncode}):\n{detail}")
    return result


def push_claim(repo: Path, remote: str, branch: str) -> None:
    """Publish a recovery claim.

    IMPORTANT:
    - Never fetch/rebase/retry after a rejected push.
    - A non-fast-forward means another writer changed the checkpoint.
    - The caller must treat this as a lost claim.

    Unlike safe_push, this path never reconciles with the remote: the claim
    was computed against the exact state that was observed, so any remote
    movement invalidates it rather than something to build on top of.
    """
    head = run_git(repo, ["symbolic-ref", "--quiet", "HEAD"], check=False)
    if head.returncode != 0 or head.stdout.strip() != f"refs/heads/{branch}":
        raise GitError(
            f"HEAD is not on refs/heads/{branch} (got {head.stdout.strip() or 'detached HEAD'}); refusing to push the claim."
        )

    result = run_git(repo, ["push", remote, f"HEAD:{branch}"], check=False)

    if result.returncode == 0:
        return

    output = f"{result.stdout}\n{result.stderr}"

    if is_non_fast_forward(result):
        raise LostClaimError("recovery claim lost: remote branch advanced")

    raise GitError(f"recovery claim push failed: {output}")


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
    parser.add_argument("action", choices=("push", "sync"))
    parser.add_argument("--branch", required=True, metavar="BRANCH")
    parser.add_argument("--message", metavar="TEXT", help="commit message for a push")
    args = parser.parse_args(argv)

    if args.action == "push":
        if not args.message or not args.message.strip():
            fail("--message must not be empty")
        push(args.branch, args.message)
    elif args.action == "sync":
        if args.message:
            fail("--message is only valid for a push, not a sync")
        sync(args.branch)


if __name__ == "__main__":
    main()
