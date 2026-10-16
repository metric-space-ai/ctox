#!/usr/bin/env python3
"""One bounded P6 start: compose pushed E36/E38 heads, then use the canonical lane."""
import datetime,json,os,shlex,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
EXPECTED=Path.home()/'.local/state/ctox-sync-v4/clones/p6'
assert ROOT==EXPECTED.resolve(),'Only the assigned P6 clone is authorized'
GIT=['git','-c','url.https://github.com/.insteadOf=git@github.com:']
def git(*args):return subprocess.check_output(GIT+list(args),cwd=ROOT,text=True).strip()
assert not git('status','--porcelain'),'Commit/preserve the P6 runner before composition'
review=git('rev-parse','HEAD');branch=git('branch','--show-current')
assert branch=='v4/p6-invariants','Start from the assigned PR branch'
git('fetch','origin')
refs=git('ls-remote','origin','refs/heads/v4/cutover-7','refs/heads/v4/boot-v4')
heads={line.split()[1]:line.split()[0] for line in refs.splitlines()}
if not all('refs/heads/'+b in heads for b in ['v4/cutover-7','v4/boot-v4']):
 print(json.dumps({'status':'not_run(branches-pending)','refs':heads}));raise SystemExit(3)
scratch='v4/p6-k-prepared-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
git('checkout','-b',scratch,'origin/v4/integration')
try:
 for b in ['v4/cutover-7','v4/boot-v4']:git('merge','--no-edit',heads['refs/heads/'+b])
 paths=['tests/sync-v4/k13','tests/sync-v4/k21/scenario.mjs','tests/sync-v4/k22/scenario.mjs']
 git('restore','--source='+review,'--',*paths)
 git('add','--',*paths)
 if git('diff','--cached','--name-only'):git('commit','-m','test(sync): compose P6 runner with pushed E36/E38 heads')
 revision=git('rev-parse','HEAD')
 print(json.dumps({'scratch':scratch,'revision':revision,'review':review,'heads':heads}),flush=True)
 temporary=Path('/Volumes/tmp/dev-artifacts/ctox/p6-invariants/tmp')
 assert Path('/Volumes/tmp').is_mount(),'Disposable volume must be mounted'
 temporary.mkdir(parents=True,exist_ok=True)
 env=dict(os.environ,TMPDIR=str(temporary))
 # 90 minutes overall; a caller must stop a still-queued lane at 40 minutes.
 deadline=int(time.time()+90*60)
 command='ctox-prep.sh && '+shlex.join(['python3','tests/sync-v4/k13/rerun.py','--revision',revision,'--deadline',str(deadline)])
 subprocess.run([str(Path.home()/'.codex/bin/gpu-build-run.sh'),'--status','--host','all'],env=env,check=True)
 result=subprocess.run(['greppy','bash-smart','-e','P6_K_|receipt|waiting|skip|error','--',str(Path.home()/'.codex/bin/gpu-build-run.sh'),
  '--owner','01a14204-d613','--task','v4-p6-invariants','--target-key','v4-v4-p6-invariants','--host','auto','--jobs','2','--src',str(ROOT),'--','bash','-lc',command],cwd=ROOT,env=env)
 raise SystemExit(result.returncode)
finally:
 # Never hide unexpected product/test mutations by changing/resetting them.
 if not git('status','--porcelain'):git('checkout',branch)
