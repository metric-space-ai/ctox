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
source=Path('src/core/service/store_http.rs')
original=source.read_bytes()
adapter=Path('tests/sync-v4/k24/core-adapter.py')
adapter_original=adapter.read_bytes()
result=dict(owner='01a087a0-169a-7e23-ba3d-71352256cbfb',steps=[],terminal=False,
            sourceSha256=hashlib.sha256(original).hexdigest(),mutationFile=str(source),
            invariantSha256=hashlib.sha256(Path('src/core/business_os/server.rs').read_bytes()).hexdigest(),pass_=False)
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
    if expected==101:
        assert 'GET / served V2 or legacy launch context' in text, 'Mutation failed for another cause'
def observer_proof():
    if deadline-time.time()<50: raise TimeoutError('No bounded observer window remains')
    rows=[]
    result['configurationObserver']=dict(pass_=False, steps=rows,
        sourceSha256=hashlib.sha256(adapter_original).hexdigest(),
        limit='SQLite observer invariant; not installed browser/K24 acceptance')
    def check(name, expected):
        command=['python3','tests/sync-v4/k24/config-fingerprint-contract.py']
        with (out/(name+'.log')).open('w') as log:
            child=subprocess.Popen(command,stdout=log,stderr=log)
            row=dict(name=name,command=command,pid=child.pid)
            rows.append(row);save()
            try: row['exit']=child.wait(timeout=15)
            finally:
                if child.poll() is None:child.kill();child.wait()
            save()
        assert row['exit']==expected, str(row)
        if expected==1:
            assert 'An actual configuration value change was invisible' in (out/(name+'.log')).read_text(), 'Observer mutation failed for another cause'
    try:
        check('configuration-control',0)
        text=adapter_original.decode()
        lines=text.splitlines(keepends=True)
        matches=[i for i,line in enumerate(lines) if line.startswith('                cells = [dict(blobSha256=')]
        assert len(matches)==1
        lines[matches[0]]='                cells = []\n'
        adapter.write_text(''.join(lines))
        check('configuration-mutant-omits-values',1)
        adapter.write_bytes(adapter_original)
        check('configuration-restored-control',0)
        result['configurationObserver']['pass_']=True
    finally:
        adapter.write_bytes(adapter_original);save()

try:
    observer_proof()
    run('control',0)
    # Mutate the ACTUAL E36 foreground handler after its positive control.
    # Never mutate an unused legacy helper and call that a proof.
    text=original.decode()
    prefix, marker, rest=text.partition('fn answer(')
    body, boundary, suffix=rest.partition('\npub(crate) fn serve(')
    assert marker and boundary, 'E36 answer function boundary changed'
    old='"v4/boot/index.html"'
    assert body.count(old)==1, 'Pin the current E31 entry expression before mutation'
    source.write_text(prefix+marker+body.replace(old,'"index.html"')+boundary+suffix)
    run('mutant-legacy-entry',101)
    source.write_bytes(original)
    run('restored-control',0)
    result['pass_']=True
except Exception as error: result['error']=str(error)
finally:
    source.write_bytes(original)
    adapter.write_bytes(adapter_original)
    result.update(terminal=True,restored=source.read_bytes()==original and adapter.read_bytes()==adapter_original,finishedAtMs=int(time.time()*1000));save()
print(json.dumps(result),flush=True)
raise SystemExit(0 if result['pass_'] else 1)
