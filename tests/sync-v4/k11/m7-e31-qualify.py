#!/usr/bin/env python3
"""Bounded real HTTP E31 contract; mutation restores source even on failure."""
import argparse
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--deadline',required=True)
a=p.parse_args()
assert os.environ.get('BUILD_LANE_BIN') and os.uname().sysname=='Linux'
base=(Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
out=a.output.resolve()
assert base in out.parents and not out.exists()
out.mkdir(mode=0o700)
deadline=datetime.fromisoformat(a.deadline.replace('Z','+00:00')).timestamp()
source=Path('src/core/business_os/server.rs')
original=source.read_bytes()
result=dict(owner='01a087a0-169a-7e23-ba3d-71352256cbfb',steps=[],terminal=False,
            sourceSha256=hashlib.sha256(original).hexdigest(),pass_=False)
def save(): (out/'e31.json').write_text(json.dumps(result,indent=2)+'\n')
def interrupt(sig,frame): raise RuntimeError('owned E31 interrupted '+str(sig))
signal.signal(signal.SIGTERM,interrupt)
signal.signal(signal.SIGINT,interrupt)
def run(name,expected):
    remain=deadline-time.time()-15
    if remain<5: raise TimeoutError('E31 deadline before '+name)
    command=['cargo','test','--locked','-j','2','--bin','ctox',
             'business_os::server::tests::v4_bootstrap_entry_never_falls_back_to_v2',
             '--','--exact','--nocapture','--test-threads=2']
    row=dict(name=name,command=command,startedAtMs=int(time.time()*1000))
    result['steps'].append(row)
    with (out/(name+'.log')).open('w') as log:
        child=subprocess.Popen(command,stdout=log,stderr=log,start_new_session=True)
        row.update(pid=child.pid,pgid=child.pid,stopAt=a.deadline);save()
        try: row['exit']=child.wait(timeout=remain)
        finally:
            try: os.killpg(child.pid,signal.SIGTERM)
            except ProcessLookupError: pass
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired: os.killpg(child.pid,signal.SIGKILL);child.wait()
            row['durationMs']=int(time.time()*1000)-row['startedAtMs'];save()
    text=(out/(name+'.log')).read_text()
    row['executed']='running 1 test' in text
    row['receipt']=[line for line in text.splitlines() if 'E31_HTTP_RECEIPT' in line]
    save()
    assert row['executed'], 'No real E31 test ran: '+name
    assert row['exit']==expected, str(row)
try:
    run('control',0)
    # Force the legacy entry even for an installed V4 tree: same actual HTTP path.
    text=original.decode()
    old='''    if version.map_or_else(
        || app_root.join("v4").is_dir(),
        |version| version.starts_with("0.2."),
    )'''
    assert text.count(old)==1
    source.write_text(text.replace(old,'    if false'))
    run('mutant-legacy-entry',101)
    source.write_bytes(original)
    run('restored-control',0)
    result['pass_']=True
except Exception as error: result['error']=str(error)
finally:
    source.write_bytes(original)
    result.update(terminal=True,restored=source.read_bytes()==original,finishedAtMs=int(time.time()*1000));save()
print(json.dumps(result),flush=True)
raise SystemExit(0 if result['pass_'] else 1)
