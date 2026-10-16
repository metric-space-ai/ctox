#!/usr/bin/env python3
"""One bounded Linux qualification consuming the one immutable release build."""
import json
import importlib.util
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from datetime import datetime

assert os.environ.get('BUILD_LANE_BIN') and os.uname().sysname == 'Linux'
base = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
out = base/('m7dq-'+time.strftime('%Y%m%dT%H%M%SZ',time.gmtime()))
out.mkdir(mode=0o700)
here = Path('tests/sync-v4/k11')
profile = base/'v4-m7-fix2-qualified-20261016T020215Z/profile'
producer = Path(sys.argv[1]).resolve()
deadline = datetime.fromisoformat(sys.argv[2].replace('Z','+00:00')).timestamp()
report = dict(output=str(out),startedAtMs=int(time.time()*1000),terminal=False,
              deadlineUtc=sys.argv[2],pass_=False,fullM7Accepted=False,steps=[])
def save():
    (out/'qualification.json').write_text(json.dumps(report,indent=2)+'\n')
def interrupted(signum,_frame):
    raise RuntimeError('Owned qualification interrupted: '+str(signum))
signal.signal(signal.SIGTERM,interrupted)
signal.signal(signal.SIGINT,interrupted)
def run(name,command,expected=(0,),timeout=180,env=None):
    remaining = deadline-time.time()
    if remaining < 10:
        raise TimeoutError('Absolute qualification deadline reached before '+name)
    timeout = min(timeout, remaining-5)
    row=dict(name=name,command=[str(x) for x in command],startedAtMs=int(time.time()*1000))
    report['steps'].append(row)
    with (out/(name+'.log')).open('w') as log:
        child=subprocess.Popen(row['command'],stdout=log,stderr=log,start_new_session=True,env=env)
        row.update(pid=child.pid,pgid=child.pid,stopAfterSeconds=timeout)
        save()
        try:
            row['exit']=child.wait(timeout=timeout)
        finally:
            try: os.killpg(child.pid,signal.SIGTERM)
            except ProcessLookupError: pass
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid,signal.SIGKILL)
                child.wait()
            try: os.killpg(child.pid,signal.SIGKILL)
            except ProcessLookupError: pass
            try:
                os.killpg(child.pid,0)
                row['groupAbsent']=False
            except ProcessLookupError: row['groupAbsent']=True
    row['durationMs']=int(time.time()*1000)-row['startedAtMs']
    save()
    print(name,'exit',row['exit'],flush=True)
    if row['exit'] not in expected or not row['groupAbsent']:
        raise RuntimeError(name+' failed; '+str(out/(name+'.log')))
try:
    for name in ('m7-release.py','m7-dress.py','m7-artifact-qualify.py','m7-data-readback.py','m7-manager-probe.py'):
        compile((here/name).read_bytes(),str(here/name),'exec')
    for name in ('upgrade.mjs','upgrade-controller.mjs','installer-smoke.mjs','installer-outcome.mjs','sandbox.mjs'):
        run('syntax-'+name,['node','--check',here/name],timeout=15)
    run('package',['python3',here/'m7-release.py','--reuse-build',producer,'--output',out/'release'],timeout=600)
    receipt=out/'release/release.json'
    release=json.loads(receipt.read_text())
    report['release']={k:release[k] for k in ('revision','runnerRevision','binary','binarySha256','package','packageSha256','sourceTree')}
    live=out/'liveness-root'
    (live/'home').mkdir(parents=True,mode=0o700)
    live_env=dict(PATH=os.environ['PATH'],HOME=str(live/'home'),TMPDIR=os.environ['TMPDIR'],
                  CTOX_ROOT=str(live),CTOX_STATE_ROOT=str(live/'runtime'),LANG='C.UTF-8',
                  XDG_CONFIG_HOME=str(live/'home/.config'),XDG_CACHE_HOME=str(live/'home/.cache'))
    run('release-spawn-liveness',[release['binary'],'process-mining','spawn-liveness','--root',live],timeout=180,env=live_env)
    run('custody-mutations',['python3',here/'m7-artifact-qualify.py',receipt,out/'custody',profile],timeout=600)
    run('dress',['python3',here/'m7-dress.py','--release',receipt,'--before',
        base/'v4-e2e-old-portable-20261015/build.json','--profile',profile,'--output',out/'dress'],expected=(0,1,2),timeout=1000)
    report['dress']=json.loads((out/'dress/m7-go-nogo.json').read_text())
    # Reference-slot static component only. A V3 reference slot must not make
    # installed-get-root-p99 or signed-v4-slot pass in the M7 decision matrix.
    run('reference-static',['python3','src/core/install/static_probe.py','--mode','plain',
        '--profile',profile,'--binary',release['binary'],'--sha256',release['binarySha256'],
        '--source',Path(release['binary']).parent.parent,
        '--slot',base/'v4-m7-fix-signed-reference-20261016/0.1.46-beta.106',
        '--out',out/'static'],expected=(0,1),timeout=180)
    static=json.loads((out/'static/result.json').read_text())
    report['referenceStatic']=dict(p99Ms=static.get('p99Ms'),rows=len(static.get('rows',[])),
        pass_=static.get('pass'),limit='Release image + V3 reference slot; not installed V4 shell/browser acceptance')
    report['pass_']=not any(c['status']=='fail' for c in report['dress']['criteria']) and static.get('pass') is True
except Exception as error:
    report['error']=str(error)
finally:
    report['terminal']=True
    report['finishedAtMs']=int(time.time()*1000)
    decision=out/'dress/m7-go-nogo.json'
    if decision.exists():
        matrix=json.loads(decision.read_text())
    else:
        spec=importlib.util.spec_from_file_location('dress',here/'m7-dress.py')
        module=importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        matrix=dict(schema='ctox.m7.go-nogo.v1',owner=module.OWNER,terminal=True,go=False,
                    candidate=report.get('release',{}).get('revision'),
                    criteria=[dict(id=name,status='not_run',owner=owner,
                        reason='Qualification stopped before the rehearsal: '+report.get('error','unknown failure'))
                        for name,owner in module.CRITERIA.items()])
    live=next((s for s in report['steps'] if s['name']=='release-spawn-liveness'),None)
    matrix['criteria'].append(dict(id='release-spawn-liveness',
        status=('pass' if live.get('exit')==0 and live.get('groupAbsent') else 'fail') if live else 'not_run',
        owner='Harness',evidence=str(out/'release-spawn-liveness.log'),
        reason='Actual canonical release guard exit and process cleanup; not the full lane suite'))
    if report.get('referenceStatic'):
        matrix['criteria'].append(dict(id='reference-static-component',
            status='pass' if report['referenceStatic'].get('pass_') is True else 'fail',owner='DevOps',
            measured=report['referenceStatic'],evidence=str(out/'static/result.json')))
    if report.get('error'):
        matrix['criteria'].append(dict(id='qualification-execution',status='fail',owner='DevOps',
            reason=report['error'],evidence=str(out/'qualification.json')))
    matrix['qualificationFinishedAtMs']=report['finishedAtMs']
    matrix['go']=bool(matrix['criteria']) and all(c['status']=='pass' for c in matrix['criteria'])
    matrix['remainingBlockers']=[{k:c.get(k) for k in ('id','status','owner','reason')}
                                for c in matrix['criteria'] if c['status']!='pass']
    (out/'m7-go-nogo.json').write_text(json.dumps(matrix,indent=2)+'\n')
    save()
print(json.dumps({k:v for k,v in report.items() if k not in ('dress','steps')}))
raise SystemExit(0 if report['pass_'] else 1)
