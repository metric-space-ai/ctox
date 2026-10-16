#!/usr/bin/env python3
"""One bounded Linux qualification consuming the one immutable release build."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

assert os.environ.get('BUILD_LANE_BIN') and os.uname().sysname == 'Linux'
base = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
out = base/('m7dq-'+time.strftime('%Y%m%dT%H%M%SZ',time.gmtime()))
out.mkdir(mode=0o700)
here = Path('tests/sync-v4/k11')
profile = base/'v4-m7-fix2-qualified-20261016T020215Z/profile'
producer = Path(sys.argv[1]).resolve()
report = dict(output=str(out),startedAtMs=int(time.time()*1000),terminal=False,
              pass_=False,fullM7Accepted=False,steps=[])
def save():
    (out/'qualification.json').write_text(json.dumps(report,indent=2)+'\n')
def interrupted(signum,_frame):
    raise RuntimeError('Owned qualification interrupted: '+str(signum))
signal.signal(signal.SIGTERM,interrupted)
signal.signal(signal.SIGINT,interrupted)
def run(name,command,expected=(0,),timeout=180):
    row=dict(name=name,command=[str(x) for x in command],startedAtMs=int(time.time()*1000))
    report['steps'].append(row)
    with (out/(name+'.log')).open('w') as log:
        child=subprocess.Popen(row['command'],stdout=log,stderr=log,start_new_session=True)
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
    run('custody-mutations',['python3',here/'m7-artifact-qualify.py',receipt,out/'custody',profile],timeout=600)
    run('dress',['python3',here/'m7-dress.py','--release',receipt,'--before',
        base/'v4-e2e-old-portable-20261015/build.json','--profile',profile,'--output',out/'dress'],expected=(0,2),timeout=1000)
    report['dress']=json.loads((out/'dress/m7-go-nogo.json').read_text())
    # Reference-slot static component only. A V3 reference slot must not make
    # installed-get-root-p99 or signed-v4-slot pass in the M7 decision matrix.
    run('reference-static',['python3','src/core/install/static_probe.py','--mode','plain',
        '--profile',profile,'--binary',release['binary'],'--sha256',release['binarySha256'],
        '--source',Path(release['binary']).parent.parent,
        '--slot',base/'v4-m7-fix-signed-reference-20261016/0.1.46-beta.106',
        '--out',out/'static'],timeout=180)
    static=json.loads((out/'static/result.json').read_text())
    report['referenceStatic']=dict(p99Ms=static.get('p99Ms'),rows=len(static.get('rows',[])),
        pass_=static.get('pass'),limit='Release image + V3 reference slot; not installed V4 shell/browser acceptance')
    report['pass_']=True
except Exception as error:
    report['error']=str(error)
finally:
    report['terminal']=True
    report['finishedAtMs']=int(time.time()*1000)
    save()
print(json.dumps({k:v for k,v in report.items() if k not in ('dress','steps')}))
raise SystemExit(0 if report['pass_'] else 1)
