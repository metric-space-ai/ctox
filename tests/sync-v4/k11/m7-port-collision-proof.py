#!/usr/bin/env python3
"""Same-image fixture proof: default shared port fails bind, private port works.

Only an owned offline synthetic profile; trace our own child, never attach.
The one public runtime key is restored even on interruption. No tenant access.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import time


def main(a):
    base=(Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
    root,out=a.root.resolve(),a.output.resolve()
    assert base in root.parents and base in out.parents and not out.exists()
    here=Path(__file__).resolve().parent
    spec=importlib.util.spec_from_file_location('private_fixture',here/'m7-bootstrap-fixture.py')
    fixture=importlib.util.module_from_spec(spec);spec.loader.exec_module(fixture)
    r=json.loads(a.release.read_text());current=(root/'managed/current').resolve(strict=True)
    binary=current/'bin/ctox';assert root in binary.parents
    assert hashlib.sha256(binary.read_bytes()).hexdigest()==r['binarySha256']
    out.mkdir(mode=0o700)
    result=dict(owner='01a087a0-169a-7e23-ba3d-71352256cbfb',terminal=False,pass_=False,
        source=r['revision'],binarySha256=r['binarySha256'],steps=[])
    def save():(out/'port-ab.json').write_text(json.dumps(result,indent=2)+'\n')
    def stop(sig,_):raise RuntimeError('Owned port proof interrupted '+str(sig))
    signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
    def run(name,cmd,env=None):
        row=dict(name=name,command=[str(x) for x in cmd]);result['steps'].append(row)
        with (out/(name+'.log')).open('w') as log:
            child=subprocess.Popen(row['command'],cwd=current,env=env,stdout=log,stderr=log,start_new_session=True)
            row.update(pid=child.pid,pgid=child.pid,stopAfterSeconds=150);save()
            try:row['exit']=child.wait(timeout=150)
            finally:
                try:os.killpg(child.pid,signal.SIGTERM)
                except ProcessLookupError:pass
                try:child.wait(timeout=8)
                except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
                try:os.killpg(child.pid,signal.SIGKILL)
                except ProcessLookupError:pass
                try:os.killpg(child.pid,0);row['groupAbsent']=False
                except ProcessLookupError:row['groupAbsent']=True
                save()
        assert row['groupAbsent'];return row['exit']
    configured=False
    try:
        result['fixture']=fixture.configure(root);configured=True
        result['sharedListenerBefore']=subprocess.check_output(['ss','-ltn','sport = :8765'],text=True)
        assert 'LISTEN' in result['sharedListenerBefore'],'No shared-port collision to reproduce'
        database=root/'runtime/ctox-runtime.sqlite3';key='CTOX_BUSINESS_OS_WEB_ADDR'
        with sqlite3.connect(database) as db:db.execute('DELETE FROM runtime_env_kv WHERE env_key=?',(key,))
        home=root/'installer-home'
        env={k:os.environ[k] for k in ('PATH','TMPDIR','BUILD_LANE_BIN')}
        env.update(HOME=str(home),LANG='C.UTF-8',CTOX_ROOT=str(current),CTOX_STATE_ROOT=str(root/'runtime'),
            CTOX_INSTALL_ROOT=str(root/'managed'),XDG_CONFIG_HOME=str(home/'.config'),XDG_CACHE_HOME=str(home/'.cache'))
        code=run('default-port-mutant',['/usr/bin/strace','-f','-tt','-e','trace=bind','-o',out/'bind.trace',binary,'service','--foreground'],env)
        trace=(out/'bind.trace').read_text()
        result['bindFailure']=[line for line in trace.splitlines() if 'htons(8765)' in line and 'EADDRINUSE' in line]
        assert code==1 and result['bindFailure'],'Failure was not the expected port bind collision'
        result['fixtureRestored']=fixture.configure(root)
        code=run('private-port-control',['python3',here.parent/'k24/native-target-probe.py','--root',root,
            '--release',a.release,'--output',out/'control'])
        assert code in (0,2)
        control=json.loads((out/'control/target.json').read_text());result['control']=control
        result['sharedListenerAfter']=subprocess.check_output(['ss','-ltn','sport = :8765'],text=True)
        assert control['nativeReady'] and control['httpRoot']['status']=='pass'
        assert 'LISTEN' in result['sharedListenerAfter']
        result['pass_']=True
    except Exception as error:result['error']=str(error)
    finally:
        if configured:
            try:result['configurationRestored']=fixture.configure(root)['value']=='127.0.0.1:0'
            except Exception as error:result['restoreError']=str(error);result['pass_']=False
        result.update(terminal=True,finishedAtMs=int(time.time()*1000));save()
    print(json.dumps(dict(terminal=True,pass_=result['pass_'],error=result.get('error'),output=str(out))))
    return 0 if result['pass_'] else 1


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for flag in ('root','release','output'):p.add_argument('--'+flag,type=Path,required=True)
    raise SystemExit(main(p.parse_args()))
