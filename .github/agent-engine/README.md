# AWH External Agent Engine

This directory defines the external autonomous engineering system used to complete AWH from the project roadmap.

It is **not part of the AWH runtime**. mini-SWE-agent, SWE-ReX, SWE-bench, and SWE-smith are CI/evaluation infrastructure only. They must never be added to `Cargo.toml`, `Cargo.lock`, `src/`, or the runtime image.

## Roles

| Component | Role |
|---|---|
| Feature Registry | Machine-readable roadmap/backlog and dependency order |
| Planner | Converts an incomplete feature into a bounded implementation task |
| mini-SWE-agent | Implements or repairs one task |
| SWE-ReX | Optional isolated execution backend for agent commands |
| SWE-smith | Generates AWH-specific repair/evaluation tasks externally |
| SWE-bench | Evaluates software-engineering agent capability; does not define AWH requirements |
| Deterministic CI | Final source-of-truth verification |

The upstream SWE project currently recommends mini-SWE-agent as the default agent; SWE-ReX provides sandboxed shell execution, while SWE-smith generates repository-specific SWE tasks. citeturn0search3turn0search1turn0search5

## Operating model

```text
Roadmap
  -> Feature Registry
    -> Planner
      -> isolated task
        -> mini-SWE-agent
          -> SWE-ReX sandbox when enabled
            -> implementation/repair
              -> deterministic AWH verification
                -> evaluation evidence
                  -> PR proposal
```

## Hard boundaries

1. Never push implementation changes directly to `rust` or `main`.
2. Never allow external agent tooling into AWH runtime dependencies.
3. Never treat an LLM response as verification.
4. Never weaken tests or grading criteria to obtain a pass.
5. Never expose GitHub/model secrets to generated prompts or artifacts.
6. Never mark a feature complete until its deterministic verification passes.
7. Human/repository review remains the merge authority.

## Task lifecycle

```text
PLANNED
  -> READY
  -> RUNNING
  -> VERIFYING
  -> REPAIRING (when verification fails)
  -> EVALUATING
  -> PR_READY
  -> VERIFIED
```

A task is complete only when the repository's verification contract is satisfied. See `.github/agent/verification.md`.

## Feature order

The initial registry follows the current AWH roadmap priorities: agent-grade editing, first-class worktrees, unified capability enforcement, unified provenance, TUI runtime integration, multi-agent collaboration, Control API hardening, remote AWH, ecosystem integrations, and advanced infrastructure.

## Workflow

`27-agent-engine.yml` is the external orchestration entry point. It can run in planning, implementation, repair, evaluation-preflight, and task-generation-preflight modes. Implementation runs produce evidence/patch artifacts rather than pushing to protected branches.
