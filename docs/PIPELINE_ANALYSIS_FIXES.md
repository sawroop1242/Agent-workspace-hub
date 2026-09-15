# Agent Workspace Hub - Three-Agent Autonomous Pipeline Analysis

## System Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                    AUTONOMOUS DEVELOPMENT LOOP                       │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  AGENT 1 (PLANNER)         AGENT 2 (BUILDER)      AGENT 3 (REVIEWER)│
│  ─────────────────         ────────────────       ──────────────────│
│  • Feature Generation      • Implementation       • Code Review      │
│  • Task Decomposition      • Testing             • Quality Assurance│
│  • Spec Creation          • Verification         • Verdict Logic    │
│                                                                       │
│              ↓                    ↓                      ↓            │
│   AWH-AUTONOMOUS-LOOP → AWH-BUILDER → AWH-REVIEWER → MERGE           │
│                                                                       │
│   ┌───────────────────────────────────────────────────────────────┐  │
│   │         CHECKPOINT STATE MACHINE (CAS-Protected)             │  │
│   │                                                               │  │
│   │  IDLE → PLANNING → BUILDING → PR_OPEN → REVIEWING          │  │
│   │           ↓          ↓          ↓          ↓                │  │
│   │         BLOCKED ← BLOCKED ← BLOCKED ← BLOCKED              │  │
│   │           ↑          ↑          ↑          ↑                │  │
│   │           └─────────────────────────────────┘ (recovery)   │  │
│   │                                                               │  │
│   │  REVIEWING → FIXING → REVIEWING (loop up to 3x)             │  │
│   │  REVIEWING → MERGING → COMPLETED → IDLE (success)           │  │
│   └───────────────────────────────────────────────────────────────┘  │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

---

## CRITICAL ISSUES FOUND

### ❌ ISSUE 1: Checkpoint Validation Early Termination
**Location:** `awh-autonomous-loop.yml` lines 58-68
**Severity:** CRITICAL - Blocks all recovery attempts

The "Refuse new planning" step uses hard exit instead of recovery logic:
- Returns exit 1 when checkpoint is BLOCKED
- BLOCKED is a recoverable state but treated as fatal
- Prevents automatic recovery workflows from running

**Root Cause:** The loop workflow acts as a gatekeeper but doesn't implement recovery-aware logic.

---

### ❌ ISSUE 2: Dispatch Without Pre-Validation
**Location:** `awh-autonomous-loop.yml` lines 162-175
**Severity:** CRITICAL - Dispatch succeeds even if checkpoint is invalid

The dispatch step validates operation_id but NOT checkpoint status:
- Doesn't verify checkpoint is in BUILDING state before dispatch
- Doesn't retry on transient network failures
- No backoff between dispatch attempts
- Silently fails if GH API is unavailable

**Root Cause:** Missing defensive validation before critical state machine transitions.

---

### ❌ ISSUE 3: LLM Key Rotation Inconsistency
**Location:** `awh-builder.yml:141-164`, `awh-reviewer.yml:127-146`
**Severity:** HIGH - Duplicated error handling logic

Both builder and reviewer implement their own key rotation:
- No backoff between retries (rapid API hammering)
- Different error message formatting
- No unified timeout handling
- Inconsistent rate-limit detection regex

**Root Cause:** Utility function not extracted; code duplication leads to inconsistency.

---

### ❌ ISSUE 4: Reviewer Verdict Parsing Fragility
**Location:** `awh-reviewer.yml` lines 186-187
**Severity:** HIGH - Pipeline fails on malformed review output

The verdict parsing is extremely brittle:
```python
verdicts=[x.split(':',1)[1].strip() for x in review.splitlines() if x.startswith('VERDICT:')]
if len(verdicts)!=1 or verdicts[0] not in {'APPROVE','CHANGES_REQUIRED','BLOCKED'}:
    raise SystemExit('invalid reviewer verdict')
```

Problems:
- No error context (what verdict was found?)
- No debug output if parsing fails
- Crashes entire workflow on single malformed line
- No fallback to safe default (e.g., BLOCKED)

**Root Cause:** Production code should validate and log, not crash silently.

---

### ❌ ISSUE 5: Missing Automatic Recovery Loop
**Location:** No `awh-recovery.yml` or scheduled recovery mechanism
**Severity:** CRITICAL - Stalled checkpoints require manual intervention

Currently:
- BLOCKED checkpoints can only be recovered by human action
- No scheduled recovery monitor
- No automatic state inspection and repair
- No exponential backoff for stale operations

**Root Cause:** Recovery is implemented but never triggered automatically.

---

## ISSUE 6: Network Retry Logic Missing
**Location:** All dispatch calls (`gh api`)
**Severity:** MEDIUM - Transient failures are unhandled

Example failure points:
- GitHub API rate limiting (without backoff)
- Network timeout on `gh api` call (immediate fail)
- Temporary DNS/connection issues (no retry)

---

## Detailed Fix Recommendations

### FIX 1: Add Recovery-Aware Checkpoint Guard

Replace lines 58-68 in `awh-autonomous-loop.yml`:

```yaml
- name: Guard against active planning with recovery awareness
  run: |
    STATUS=$(jq -r '.status' .openhands/state.json)
    
    case "$STATUS" in
      IDLE|COMPLETED)
        echo "✓ Checkpoint permits new planning: $STATUS"
        exit 0
        ;;
      BLOCKED)
        echo "⚠ Checkpoint is BLOCKED. Checking if recovery is possible..."
        
        # Attempt automatic recovery for BLOCKED checkpoints
        RESULT=$(python scripts/awh_pipeline.py claim-recovery \
          --feature "$(jq -r '.active_feature // empty' .openhands/state.json)" \
          --operation-id "$(jq -r '.operation_id // empty' .openhands/state.json)" \
          --claim-owner "awh-autonomous-loop-recovery" 2>&1 || echo "RECOVERY_FAILED")
        
        if echo "$RESULT" | grep -q "CLAIMED"; then
          echo "✓ Successfully claimed recovery"
          exit 0
        elif echo "$RESULT" | grep -q "NO_OP"; then
          echo "ℹ Checkpoint not stale enough for recovery (age < 90m)"
          exit 1
        else
          echo "✗ Recovery failed or lost claim"
          echo "$RESULT"
          exit 1
        fi
        ;;
      *)
        echo "✗ Checkpoint is in non-idle, non-recoverable state: $STATUS"
        echo "  Active feature: $(jq -r '.active_feature // "none"' .openhands/state.json)"
        echo "  Active PR: $(jq -r '.active_pr // "none"' .openhands/state.json)"
        echo "  Manual intervention required"
        exit 1
        ;;
    esac
```

---

### FIX 2: Add Strict Dispatch Validation with Retries

Replace lines 162-175 in `awh-autonomous-loop.yml`:

```yaml
- name: Dispatch Agent 2 with validation and retry logic
  env:
    FEATURE_ID: ${{ steps.plan.outputs.feature_id }}
  run: |
    # Pre-dispatch validation
    STATUS=$(jq -r '.status' .openhands/state.json)
    if [ "$STATUS" != "BUILDING" ]; then
      echo "✗ FATAL: Checkpoint status is $STATUS (expected BUILDING)"
      echo "  Cannot dispatch Agent 2"
      exit 1
    fi
    
    # Verify operation identity
    CURRENT_OP=$(jq -r '.operation_id // empty' .openhands/state.json)
    EXPECTED_OP="${FEATURE_ID}:BUILD:1"
    if [ -z "$CURRENT_OP" ] || [ "$CURRENT_OP" != "$EXPECTED_OP" ]; then
      echo "✗ FATAL: Operation mismatch"
      echo "  Expected: $EXPECTED_OP"
      echo "  Current:  $CURRENT_OP"
      exit 1
    fi
    
    # Verify task exists
    test -s .openhands/generated-task.md || {
      echo "✗ FATAL: Task file not found"
      exit 1
    }
    
    # Prepare dispatch payload
    jq -n --arg feature "$FEATURE_ID" --arg op "$CURRENT_OP" \
      '{event_type:"awh.build",client_payload:{feature_id:$feature,operation_id:$op}}' \
      > /tmp/payload.json
    
    # Dispatch with exponential backoff retry
    for attempt in 1 2 3 4; do
      echo "Dispatch attempt $attempt/4..."
      
      if gh api "repos/$GITHUB_REPOSITORY/dispatches" \
         --method POST --input /tmp/payload.json 2>/tmp/dispatch.err; then
        echo "✓ Dispatch succeeded on attempt $attempt"
        exit 0
      fi
      
      # Check if error is retryable
      if grep -Eiq 'timeout|connection|temporary|unavailable|rate' /tmp/dispatch.err; then
        if [ $attempt -lt 4 ]; then
          BACKOFF=$((2 ** (attempt - 1)))
          echo "⚠ Retryable error ($(cat /tmp/dispatch.err | head -c 100))"
          echo "  Backing off ${BACKOFF}s before retry..."
          sleep $BACKOFF
          continue
        fi
      fi
      
      # Non-retryable or final attempt failed
      if [ $attempt -eq 4 ]; then
        echo "✗ All dispatch attempts failed:"
        cat /tmp/dispatch.err
        exit 1
      fi
    done
```

---

### FIX 3: Extract Shared LLM Key Rotation Logic

Create `scripts/llm_agent_runner.sh`:

```bash
#!/bin/bash
# Unified LLM agent runner with key rotation and intelligent retry

set -euo pipefail

AGENT_NAME="${1:?Agent name required (e.g., planner, builder, reviewer)}"
SCRIPT_PATH="${2:?Script path required}"
OUTPUT_FILE="${3:-/tmp/${AGENT_NAME}-output.txt}"

# Expected keys: K1, K2, K3, KF (fallback)
K1="${K1:-}"
K2="${K2:-}"
K3="${K3:-}"
KF="${KF:-}"

declare -a KEYS=("K1" "K2" "K3" "KF")
MAX_ATTEMPTS=4

run_agent() {
  local key_name="$1"
  local key_value="$2"
  local attempt="$3"
  
  echo "[${AGENT_NAME}] Attempt $attempt: Using key $key_name"
  
  export AWH_LLM_API_KEY="$key_value"
  python "$SCRIPT_PATH" > "$OUTPUT_FILE" 2>&1
  local rc=$?
  
  echo "[${AGENT_NAME}] Exit code: $rc"
  cat "$OUTPUT_FILE"
  
  return $rc
}

check_rate_limit() {
  grep -Eiq '(^|[^0-9])429([^0-9]|$)|rate.?limit|too many requests' "$OUTPUT_FILE"
}

check_retriable() {
  grep -Eiq 'timeout|connection|temporary|unavailable' "$OUTPUT_FILE"
}

# Try each key with exponential backoff
for i in "${!KEYS[@]}"; do
  key_var="${KEYS[$i]}"
  key_val="${!key_var:-}"
  
  if [ -z "$key_val" ]; then
    echo "[${AGENT_NAME}] Skipping empty key: $key_var"
    continue
  fi
  
  attempt=$((i + 1))
  
  if run_agent "$key_var" "$key_val" "$attempt"; then
    echo "[${AGENT_NAME}] ✓ Success with $key_var"
    exit 0
  fi
  
  # Check failure mode
  if check_rate_limit; then
    echo "[${AGENT_NAME}] ⚠ Rate limit detected, rotating to next key"
    if [ $attempt -lt ${#KEYS[@]} ]; then
      backoff=$((2 ** (attempt - 1)))
      echo "[${AGENT_NAME}] Backing off ${backoff}s before next key"
      sleep "$backoff"
    fi
    continue
  fi
  
  if check_retriable; then
    echo "[${AGENT_NAME}] ⚠ Retriable error detected"
    if [ $attempt -lt ${#KEYS[@]} ]; then
      backoff=$((3 ** (attempt - 1)))
      echo "[${AGENT_NAME}] Backing off ${backoff}s before retry"
      sleep "$backoff"
      continue
    fi
  fi
  
  # Non-retriable error
  echo "[${AGENT_NAME}] ✗ Non-retriable error (exit code: $?)"
  exit 1
done

echo "[${AGENT_NAME}] ✗ All keys exhausted"
exit 1
```

Then update both workflows to use this script:

```yaml
# In awh-builder.yml
- name: Run Agent 2 with unified key rotation
  env:
    K1: ${{ secrets.AWH_AGENT2_NVIDIA_KEY_1 }}
    K2: ${{ secrets.AWH_AGENT2_NVIDIA_KEY_2 }}
    K3: ${{ secrets.AWH_AGENT2_NVIDIA_KEY_3 }}
    KF: ${{ secrets.AWH_ROUTINE_NVIDIA_KEY }}
    AWH_AGENT_ROLE: builder
    AWH_TASK: ${{ env.AWH_TASK }}
  run: bash scripts/llm_agent_runner.sh builder scripts/openhands_agent.py
```

---

### FIX 4: Robust Verdict Parsing with Debug Output

Create `scripts/reviewer_verdict_parser.py`:

```python
#!/usr/bin/env python3
"""Robust reviewer verdict parser with comprehensive error handling."""

import sys
import json
from pathlib import Path

def parse_verdict(review_file: str) -> str:
    """Extract and validate reviewer verdict."""
    
    try:
        content = Path(review_file).read_text(encoding='utf-8')
    except Exception as e:
        print(f"✗ Failed to read review file: {e}", file=sys.stderr)
        sys.exit(1)
    
    verdicts = []
    lines_checked = 0
    
    for line_no, line in enumerate(content.splitlines(), 1):
        lines_checked += 1
        
        if line.strip().startswith('VERDICT:'):
            # Extract verdict value
            parts = line.split(':', 1)
            if len(parts) == 2:
                verdict = parts[1].strip().upper()
                verdicts.append((line_no, verdict))
    
    # Validation
    if not verdicts:
        print(f"✗ ERROR: No VERDICT found in review", file=sys.stderr)
        print(f"  Scanned {lines_checked} lines", file=sys.stderr)
        print(f"\n--- First 30 lines of review ---", file=sys.stderr)
        for i, line in enumerate(content.splitlines()[:30], 1):
            print(f"{i:3d}: {line}", file=sys.stderr)
        sys.exit(1)
    
    if len(verdicts) > 1:
        print(f"✗ ERROR: Multiple VERDICTs found:", file=sys.stderr)
        for line_no, verdict in verdicts:
            print(f"  Line {line_no}: {verdict}", file=sys.stderr)
        sys.exit(1)
    
    line_no, verdict = verdicts[0]
    valid = {'APPROVE', 'CHANGES_REQUIRED', 'BLOCKED'}
    
    if verdict not in valid:
        print(f"✗ ERROR: Invalid verdict '{verdict}' at line {line_no}", file=sys.stderr)
        print(f"  Must be one of: {', '.join(sorted(valid))}", file=sys.stderr)
        sys.exit(1)
    
    print(f"✓ Parsed verdict: {verdict} (line {line_no})", file=sys.stderr)
    print(verdict)
    return verdict

if __name__ == '__main__':
    review_file = '.openhands/review.md'
    verdict = parse_verdict(review_file)
    sys.exit(0)
```

Update reviewer workflow:

```yaml
- name: Validate and extract verdict
  run: |
    VERDICT=$(python scripts/reviewer_verdict_parser.py) || exit 1
    echo "verdict=$VERDICT" >> "$GITHUB_ENV"
    echo "✓ Extracted verdict: $VERDICT"
```

---

### FIX 5: Enable Automatic Recovery Scheduling

Create `.github/workflows/awh-recovery-schedule.yml`:

```yaml
name: AWH Automatic Recovery Monitor
on:
  schedule:
    - cron: '*/30 * * * *'  # Run every 30 minutes
  workflow_dispatch:
    inputs:
      force_recovery:
        description: Force recovery attempt regardless of staleness
        type: boolean
        default: false

permissions:
  contents: write
  
jobs:
  recovery:
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
        with:
          ref: rust
          fetch-depth: 0
          token: ${{ secrets.AWH_AUTOMATION_TOKEN }}
      
      - name: Sync latest checkpoint
        run: python scripts/safe_git.py sync --branch rust
      
      - name: Check checkpoint status
        id: status
        run: |
          STATUS=$(jq -r '.status' .openhands/state.json)
          AGE=$(python scripts/checkpoint_state.py age-minutes)
          echo "status=$STATUS" >> "$GITHUB_OUTPUT"
          echo "age=$AGE" >> "$GITHUB_OUTPUT"
          echo "Checkpoint: status=$STATUS, age=${AGE}min"
      
      - name: Attempt recovery if stalled
        if: |
          steps.status.outputs.status == 'BLOCKED' || 
          steps.status.outputs.status == 'RECOVERING' ||
          (steps.status.outputs.age > 90 && steps.status.outputs.status != 'IDLE' && steps.status.outputs.status != 'COMPLETED')
        run: |
          python scripts/awh_pipeline.py claim-recovery \
            --feature "$(jq -r '.active_feature // empty' .openhands/state.json)" \
            --operation-id "$(jq -r '.operation_id // empty' .openhands/state.json)" \
            --claim-owner "awh-recovery-monitor" \
            --stale-minutes 30 \
            --lease-minutes 15 \
          || echo "Recovery claim unsuccessful (may be normal)"
```

---

## Summary Table

| Issue | Severity | Affected Workflow | Fix Status |
|-------|----------|-------------------|-----------|
| Early termination on BLOCKED | 🔴 CRITICAL | `awh-autonomous-loop.yml` | ✅ FIX 1 |
| Missing dispatch validation | 🔴 CRITICAL | `awh-autonomous-loop.yml` | ✅ FIX 2 |
| Key rotation duplication | 🟠 HIGH | `awh-builder.yml`, `awh-reviewer.yml` | ✅ FIX 3 |
| Fragile verdict parsing | 🟠 HIGH | `awh-reviewer.yml` | ✅ FIX 4 |
| No automatic recovery | 🔴 CRITICAL | All workflows | ✅ FIX 5 |
| Missing network retries | 🟡 MEDIUM | All dispatch calls | ✅ FIX 2 |

---

## Implementation Priority

1. **IMMEDIATE** (blocking production use):
   - FIX 1: Recovery-aware checkpoint guard
   - FIX 2: Dispatch validation + retries
   - FIX 4: Robust verdict parsing

2. **SHORT TERM** (within sprint):
   - FIX 3: Extract key rotation utility
   - FIX 5: Enable recovery scheduling

3. **ONGOING** (continuous improvement):
   - Add comprehensive integration tests
   - Implement metrics/observability
   - Document manual recovery procedures

