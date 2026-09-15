# Recovery guard

The autonomous planner must never reset or overwrite an existing `BLOCKED` operation.

If `.openhands/state.json` is `BLOCKED`, the planner must fail closed and leave the checkpoint untouched. The recovery workflow is responsible for claiming the stale operation with CAS (`BLOCKED -> RECOVERING`) and deciding the safe continuation. For a stale build operation, recovery may finalize `RECOVERING -> BUILDING` only after re-validating the operation/feature identity and GitHub PR/branch reality.

This guard exists to prevent a new planner run from erasing an interrupted build and starting a duplicate operation.
