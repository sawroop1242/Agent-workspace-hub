#!/usr/bin/env python3
"""AWH pipeline safety layer: CAS stage entry, recovery claims, event validation, and failure recording."""
from __future__ import annotations
import argparse, enum, importlib.util, json, os, re, subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
ROOT=Path(__file__).resolve().parents[1]; SCRIPTS=ROOT/'scripts'; REPO_ROOT=Path(os.environ.get('AWH_REPO_ROOT',ROOT)); SHARED_BRANCH='rust'; STATE_FILE='.openhands/state.json'; STALE_EXIT=3; GIT_ENV={**os.environ,'GIT_TERMINAL_PROMPT':'0'}
SECRET_PATTERNS=[(re.compile(p),r) for p,r in [(r'ghp_[A-Za-z0-9]{16,}','ghp_[REDACTED]'),(r'gho_[A-Za-z0-9]{16,}','gho_[REDACTED]'),(r'ghu_[A-Za-z0-9]{16,}','ghu_[REDACTED]'),(r'ghs_[A-Za-z0-9]{16,}','ghs_[REDACTED]'),(r'ghr_[A-Za-z0-9]{16,}','ghr_[REDACTED]'),(r'github_pat_[A-Za-z0-9_]{16,}','github_pat_[REDACTED]'),(r'nvapi-[A-Za-z0-9_-]{16,}','nvapi-[REDACTED]'),(r'sk-[A-Za-z0-9]{16,}','sk-[REDACTED]'),(r'(?i)bearer\s+[A-Za-z0-9._~+/-]{8,}','Bearer [REDACTED]')]]
MAX_DETAIL_CHARS=2000
STAGES={'builder':{'agent':'builder','target':'BUILDING','counter':'builder_attempt','op':'BUILD'},'fixer':{'agent':'fixer','target':'FIXING','counter':'review_round','op':'FIX'},'reviewer':{'agent':'reviewer','target':'REVIEWING','counter':'review_round','op':'REVIEW'},'merger':{'agent':'merger','target':'MERGING','counter':None,'op':'MERGE'}}
AGENT_ADMITTED_STATUSES={'builder':{'BUILDING','FIXING'},'fixer':{'FIXING'},'reviewer':{'PR_OPEN','REVIEWING'},'merger':{'PR_OPEN','MERGING'},'starter':{'IDLE','COMPLETED','PLANNING'}}
def load_script_module(name):
 s=importlib.util.spec_from_file_location(name,SCRIPTS/f'{name}.py'); assert s and s.loader; m=importlib.util.module_from_spec(s); s.loader.exec_module(m); return m
def sanitize_detail(text):
 c=text
 for p,r in SECRET_PATTERNS:c=p.sub(r,c)
 c=re.sub(r'[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]','',c)
 return (c[:MAX_DETAIL_CHARS]+'...[truncated]') if len(c)>MAX_DETAIL_CHARS else c.strip()
def utc_now(): return datetime.now(timezone.utc).isoformat().replace('+00:00','Z')
def minutes_since(v):
 if not v:return None
 try:return (datetime.now(timezone.utc)-datetime.fromisoformat(v.replace('Z','+00:00'))).total_seconds()/60
 except ValueError:return None
def split_operation_id(v):
 if not v:return None
 p,_,c=v.rpartition(':')
 return (p,int(c)) if c.isdigit() else None
def run_checkpoint(argv):
 m=load_script_module('checkpoint_state'); m.main(argv); return m.load_state()
def load_state():return load_script_module('checkpoint_state').load_state()
def stage_and_push(_,message):
 r=subprocess.run(['git','add',STATE_FILE],capture_output=True,text=True,env=GIT_ENV)
 if r.returncode:raise SystemExit(f'awh_pipeline: git add failed: {sanitize_detail(r.stderr)}')
 load_script_module('safe_git').push(SHARED_BRANCH,message)
def stage_and_commit(message):
 subprocess.run(['git','-C',str(REPO_ROOT),'add',STATE_FILE],check=True,env=GIT_ENV)
 r=subprocess.run(['git','-C',str(REPO_ROOT),'commit','-m',message],capture_output=True,text=True,env=GIT_ENV)
 if r.returncode:raise SystemExit(f'awh_pipeline: claim commit failed: {sanitize_detail((r.stderr or r.stdout).strip())}')
def validate_event(agent,state,feature,pr):
 status=state['status']; af=state['active_feature']; ap=state['active_pr']; admitted=AGENT_ADMITTED_STATUSES.get(agent)
 if admitted is None:return False,f'unknown agent {agent!r}'
 if status not in admitted:return False,f'status {status!r} does not admit a {agent} run (expected {" or ".join(sorted(admitted))})'
 if agent in {'builder','fixer'}:
  if not feature:return False,'event carries no feature_id'
  if af is not None and feature!=af:return False,f'event feature {feature!r} does not match active_feature {af!r}'
 elif agent in {'reviewer','merger'}:
  if pr is None:return False,'event carries no pr_number'
  if ap is not None and pr!=ap:return False,f'event pr {pr} does not match active_pr {ap}'
  if af is not None and feature is not None and feature!=af:return False,f'event feature {feature!r} does not match active_feature {af!r}'
 elif status=='PLANNING' and af is not None and feature and feature!=af:return False,f'resume requested {feature!r} but active_feature is {af!r}'
 return True,'Event validated: event agrees with checkpoint'
def begin_stage(stage,feature,extra,message,expected_operation_id=None):
 spec=STAGES[stage]; state=load_state(); ok,reason=validate_event(spec['agent'],state,feature,extra.get('active_pr'))
 if not ok:print(f'STALE_EVENT: {reason}; doing nothing.');raise SystemExit(STALE_EXIT)
 if expected_operation_id is not None:
  if not expected_operation_id.strip():print('STALE_EVENT: event operation_id is empty; doing nothing.');raise SystemExit(STALE_EXIT)
  if state['operation_id']!=expected_operation_id:print(f'STALE_EVENT: event operation_id {expected_operation_id!r} does not match checkpoint operation_id {state["operation_id"]!r}; doing nothing.');raise SystemExit(STALE_EXIT)
 assignments=dict(extra); assignments['active_feature']=feature; assignments.setdefault('last_error',None); cf=spec['counter']; prefix=f'{feature}:{spec["op"]}'; checkpoint_operation_id=state['operation_id']
 if cf is None:op=f'{prefix}'
 else:
  n=int(state[cf]); parsed=split_operation_id(state['operation_id'])
  if expected_operation_id is not None:
   if not parsed or parsed[0]!=prefix or parsed[1]!=n:print('STALE_EVENT: checkpoint operation_id is inconsistent with the current stage counter; doing nothing.');raise SystemExit(STALE_EXIT)
   op=expected_operation_id; print(f'idempotent retry for operation {op}')
  elif parsed and parsed[0]==prefix and parsed[1]==n:op=state['operation_id']; print(f'idempotent retry for operation {op}')
  elif parsed is None or parsed[0]!=prefix:
   n+=1;op=f'{prefix}:{n}';assignments[cf]=n;print(f'new operation {op} (attempt {n})')
  else:raise SystemExit(STALE_EXIT)
 if expected_operation_id is not None and op!=expected_operation_id:print('STALE_EVENT: computed operation_id does not match event operation_id; doing nothing.');raise SystemExit(STALE_EXIT)
 assignments['operation_id']=op; args=['transition']
 for st in sorted(AGENT_ADMITTED_STATUSES[spec['agent']]):args+=['--expect-status',st]
 args+=['--to',spec['target']]
 # CAS against the checkpoint operation being consumed. For a new operation,
 # op is the value being written, so expecting op here would always fail.
 if checkpoint_operation_id:args+=['--expect-operation-id',checkpoint_operation_id]
 for k,v in assignments.items():args+=['--set',f'{k}={json.dumps(v)}']
 run_checkpoint(args);stage_and_push(None,message);print(f'operation_id={op}');return {'operation_id':op,'status':spec['target']}
class ClaimResult(enum.Enum):CLAIMED='CLAIMED';LOST_CLAIM='LOST_CLAIM';NO_OP='NO_OP'
def sync_to_remote():load_script_module('safe_git').sync(SHARED_BRANCH)
def claim_recovery(*,feature_id,expected_operation_id,claim_owner,stale_minutes=90,lease_minutes=120):
 if not claim_owner.strip():raise SystemExit('claim owner required')
 cp=load_script_module('checkpoint_state');sync_to_remote();state=read_remote_state()
 if state is None:raise SystemExit('cannot read remote checkpoint')
 status=state['status']
 if status not in set(cp.STALEABLE_STATUSES):print(f'No recoverable checkpoint: status={status}');return ClaimResult.NO_OP
 claim=state.get('recovery_claim')
 if status=='RECOVERING':return _handle_existing_claim(state,claim,claim_owner,feature_id,expected_operation_id,lease_minutes)
 age=minutes_since(state.get('updated_at'))
 if age is None or age<stale_minutes:print('Checkpoint is not safely stale; refusing recovery.');return ClaimResult.NO_OP
 if state['active_feature']!=feature_id or state['operation_id']!=expected_operation_id:return ClaimResult.LOST_CLAIM
 return _publish_claim(state,feature_id=feature_id,claim_owner=claim_owner,expect_status=status,expect_operation_id=state['operation_id'],stale_status=status)
def _handle_existing_claim(state,claim,owner,feature,expected,lease):
 if claim and claim.get('owner')==owner:
  if claim.get('feature_id')!=feature or claim.get('operation_id')!=expected:return ClaimResult.LOST_CLAIM
  return ClaimResult.CLAIMED
 if claim:
  age=minutes_since(claim.get('claimed_at'))
  if age is None or age<lease:return ClaimResult.LOST_CLAIM
  if claim.get('operation_id')!=expected:return ClaimResult.LOST_CLAIM
 return _publish_claim(state,feature_id=feature,claim_owner=owner,expect_status='RECOVERING',expect_operation_id=state['operation_id'],stale_status=(claim or {}).get('stale_status') or state['status'])
def _publish_claim(state,*,feature_id,claim_owner,expect_status,expect_operation_id,stale_status):
 attempt=int(state['recovery_attempt'])+1; original=state['operation_id']; claim={'operation_id':original,'feature_id':state['active_feature'],'attempt':attempt,'claimed_at':utc_now(),'owner':claim_owner,'stale_status':stale_status}; rop=f'{state["active_feature"] or "PIPELINE"}:RECOVERY:{attempt}'
 args=['transition','--expect-status',expect_status,'--to','RECOVERING','--set',f'operation_id={json.dumps(rop)}','--set',f'recovery_attempt={attempt}','--set',f'recovery_claim={json.dumps(claim)}','--set','last_error=null']
 if expect_operation_id:args+=['--expect-operation-id',expect_operation_id]
 try:run_checkpoint(args)
 except SystemExit:return ClaimResult.LOST_CLAIM
 stage_and_commit(f'chore(automation): claim recovery {state["active_feature"]}')
 try:load_script_module('safe_git').push_claim(repo=REPO_ROOT,remote='origin',branch=SHARED_BRANCH)
 except load_script_module('safe_git').LostClaimError:return ClaimResult.LOST_CLAIM
 print(f'awh_pipeline: recovery claim acquired (attempt {attempt}, owner {claim_owner}).');return ClaimResult.CLAIMED
def read_remote_state():
 r=subprocess.run(['git','show',f'origin/{SHARED_BRANCH}:{STATE_FILE}'],capture_output=True,text=True,env=GIT_ENV)
 try:return json.loads(r.stdout) if r.returncode==0 else None
 except json.JSONDecodeError:return None
def remote_branch_exists(branch):return subprocess.run(['git','ls-remote','--exit-code','--heads','origin',branch],capture_output=True,env=GIT_ENV).returncode==0
def inspect_github(pr,state):
 po=pm=False
 if pr:
  try:d=gh_json('pr','view',str(pr),'--json','state,isCrossRepository,baseRefName');po=d.get('state')=='OPEN' and not d.get('isCrossRepository',False) and d.get('baseRefName')==SHARED_BRANCH;pm=d.get('state')=='MERGED'
  except SystemExit as e:
   if 'not found' not in str(e).lower():raise
 b=state.get('active_branch') or (f'feature/{state["active_feature"]}' if state.get('active_feature') else None);return po,pm,remote_branch_exists(b) if b else False
def gh_json(*a):
 r=subprocess.run(['gh',*a],capture_output=True,text=True)
 if r.returncode:raise SystemExit(sanitize_detail(r.stderr))
 return json.loads(r.stdout) if r.stdout.strip() else None
def decide_recovery(state,pr_open,pr_merged,branch_exists):
 c=state.get('recovery_claim') or {};s=c.get('stale_status') or state['status'];f=state['active_feature'];p=state['active_pr']
 if s=='PLANNING':return {'target':'PLANNING','dispatch':{'event':'awh.start','feature_id':f},'reason':'planning never completed; resume feature selection'}
 if s=='BUILDING':
  if pr_merged:return {'target':'COMPLETED','dispatch':None,'reason':'PR was merged while the builder checkpoint stalled'}
  if pr_open:return {'target':'PR_OPEN','dispatch':{'event':'awh.review','pr_number':p,'feature_id':f},'reason':'builder produced a PR but crashed before recording it; resume at review'}
  return {'target':'BUILDING','dispatch':{'event':'awh.build','feature_id':f},'reason':'no feature branch or PR exists; resume the build'}
 if s in {'PR_OPEN','REVIEWING'}:
  if pr_merged:return {'target':'COMPLETED','dispatch':None,'reason':'PR was merged while the review checkpoint stalled'}
  if pr_open:return {'target':'REVIEWING','dispatch':{'event':'awh.review','pr_number':p,'feature_id':f},'reason':'PR is open; (re)run the review'}
  return {'target':'BLOCKED','dispatch':None,'reason':f'checkpoint references PR {p} which no longer exists'}
 if s=='FIXING':
  if pr_merged:return {'target':'COMPLETED','dispatch':None,'reason':'PR was merged while the fix checkpoint stalled'}
  if pr_open:return {'target':'FIXING','dispatch':{'event':'awh.fix','feature_id':f,'review':state.get('last_review')},'reason':'PR is open; resume the recorded fix'}
  return {'target':'BLOCKED','dispatch':None,'reason':f'checkpoint references PR {p} which no longer exists'}
 if s=='MERGING':
  if pr_merged:return {'target':'COMPLETED','dispatch':{'event':'awh.start'},'reason':'PR already merged; record completion and continue the loop'}
  if pr_open:return {'target':'MERGING','dispatch':{'event':'awh.review-complete','pr_number':p,'feature_id':f,'review_state':'APPROVE'},'reason':'approved PR never merged; retry the merge'}
  return {'target':'BLOCKED','dispatch':None,'reason':f'checkpoint references PR {p} which no longer exists'}
 return {'target':'BLOCKED','dispatch':None,'reason':f'no safe continuation from {s!r}'}
def finalize_recovery(owner,target,assignments,message):
 state=load_state();claim=state.get('recovery_claim') or {}
 if state['status']!='RECOVERING' or claim.get('owner') not in (None,owner):raise SystemExit(STALE_EXIT)
 if target not in {'BLOCKED','BUILDING','PLANNING','PR_OPEN','REVIEWING','FIXING','MERGING','COMPLETED'}:raise SystemExit(f'invalid recovery target {target}')
 payload=dict(assignments);payload['recovery_claim']=None;payload.setdefault('last_error',None)
 if target!='BLOCKED' and claim.get('operation_id'):payload['operation_id']=claim['operation_id']
 args=['transition','--expect-status','RECOVERING','--to',target,'--expect-operation-id',state['operation_id']]
 for k,v in payload.items():args+=['--set',f'{k}={json.dumps(v)}']
 run_checkpoint(args);stage_and_push(None,message);return load_state()
def record_failure(agent,stage,detail,feature,pr,branch,commit,expected_operation_id=None):
 cp=load_script_module('checkpoint_state');s=cp.load_state()
 if expected_operation_id is not None:
  if not expected_operation_id.strip():print('STALE_EVENT: failure report has no operation_id; doing nothing.');return False
  if s['operation_id']!=expected_operation_id:print(f'STALE_EVENT: failure report operation_id {expected_operation_id!r} does not match checkpoint operation_id {s["operation_id"]!r}; doing nothing.');return False
 d=json.dumps({'agent':agent,'stage':stage,'operation_id':s['operation_id'],'feature_id':feature or s['active_feature'],'pr':pr or s['active_pr'],'branch':branch or s['active_branch'],'commit':commit,'attempt':s['builder_attempt'],'review_round':s['review_round'],'status_at_failure':s['status'],'detail':sanitize_detail(detail)},sort_keys=True);print(d)
 if s['status'] not in cp.STALEABLE_STATUSES:return False
 args=['transition','--expect-status',s['status'],'--to','BLOCKED','--set',f'last_error={json.dumps(d)}']
 if s['operation_id']:args+=['--expect-operation-id',s['operation_id']]
 run_checkpoint(args);stage_and_push(cp,f'chore(automation): record {agent} {stage} failure');return True
def _parse_assignments(pairs):
 out={}
 for x in pairs:k,_,v=x.partition('=');out[k]=json.loads(v) if _ else v
 return out
def main():
 p=argparse.ArgumentParser();sp=p.add_subparsers(dest='command',required=True)
 q=sp.add_parser('validate-event');q.add_argument('--agent',required=True);q.add_argument('--feature');q.add_argument('--pr',type=int)
 q=sp.add_parser('begin-stage');q.add_argument('--stage',choices=STAGES);q.add_argument('--feature',required=True);q.add_argument('--pr',type=int);q.add_argument('--operation-id');q.add_argument('--message',required=True);q.add_argument('--set',action='append',default=[])
 q=sp.add_parser('claim-recovery');q.add_argument('--feature',required=True);q.add_argument('--operation-id',required=True);q.add_argument('--claim-owner',required=True);q.add_argument('--stale-minutes',type=int,default=90);q.add_argument('--lease-minutes',type=int,default=120)
 sp.add_parser('decide-recovery');q=sp.add_parser('finalize-recovery');q.add_argument('--owner',required=True);q.add_argument('--to',required=True);q.add_argument('--message',required=True);q.add_argument('--set',action='append',default=[])
 q=sp.add_parser('record-failure');q.add_argument('--agent',required=True);q.add_argument('--stage',required=True);q.add_argument('--detail',required=True);q.add_argument('--feature');q.add_argument('--pr',type=int);q.add_argument('--branch');q.add_argument('--commit');q.add_argument('--operation-id');a=p.parse_args()
 if a.command=='validate-event':ok,r=validate_event(a.agent,load_state(),a.feature,a.pr);print(r);return 0 if ok else STALE_EXIT
 if a.command=='begin-stage':
  extra=_parse_assignments(a.set)
  if a.pr is not None:extra['active_pr']=a.pr
  begin_stage(a.stage,a.feature,extra,a.message,a.operation_id);return 0
 if a.command=='claim-recovery':print(claim_recovery(feature_id=a.feature,expected_operation_id=a.operation_id,claim_owner=a.claim_owner,stale_minutes=a.stale_minutes,lease_minutes=a.lease_minutes).value);return 0
 if a.command=='decide-recovery':s=load_state();po,pm,be=inspect_github(s.get('active_pr'),s);d=decide_recovery(s,po,pm,be);print(f"TARGET={d['target']}");print(f"REASON={d['reason']}");x=d.get('dispatch');print(f"DISPATCH_EVENT={x.get('event','') if x else ''}");print(f"DISPATCH_PAYLOAD={json.dumps({k:v for k,v in (x or {}).items() if k!='event'})}");return 0
 if a.command=='finalize-recovery':finalize_recovery(a.owner,a.to,_parse_assignments(a.set),a.message);return 0
 if a.command=='record-failure':return 0 if record_failure(a.agent,a.stage,a.detail,a.feature,a.pr,a.branch,a.commit,a.operation_id) else 1
 return 1
if __name__=='__main__':raise SystemExit(main())