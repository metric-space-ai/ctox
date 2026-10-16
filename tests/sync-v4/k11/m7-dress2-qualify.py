#!/usr/bin/env python3
"""M7 follow-up: one candidate release, real recovery, guarded criteria rerun.

Absolute deadline bounds build plus all affected steps. No polling/admission
chain, no tenant mutation, no success inferred from a missing prerequisite.
"""
import argparse
from datetime import datetime
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def main(a):
    assert os.environ.get('BUILD_LANE_BIN') and os.uname().sysname == 'Linux'
    base = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
    out = a.output.resolve()
    assert base in out.parents and not out.exists()
    out.mkdir(mode=0o700)
    here = Path(__file__).resolve().parent
    deadline = datetime.fromisoformat(a.deadline.replace('Z','+00:00')).timestamp()
    result = dict(owner='01a087a0-169a-7e23-ba3d-71352256cbfb',startedAtMs=int(time.time()*1000),
                  terminal=False,steps=[],fullM7Accepted=False)
    def save():
        (out/'qualification.json').write_text(json.dumps(result,indent=2)+'\n')
    def interrupted(signum,_):
        raise RuntimeError('Owned M7 follow-up interrupted: '+str(signum))
    signal.signal(signal.SIGTERM,interrupted)
    signal.signal(signal.SIGINT,interrupted)
    def run(name, command, cap, expected=(0,)):
        remaining = deadline-time.time()-20
        if remaining<5: raise TimeoutError('Absolute M7 deadline before '+name)
        row = dict(name=name,command=[str(x) for x in command],startedAtMs=int(time.time()*1000))
        result['steps'].append(row)
        with (out/(name+'.log')).open('w') as log:
            child = subprocess.Popen(row['command'],stdout=log,stderr=log,start_new_session=True)
            row.update(pid=child.pid,pgid=child.pid,stopAfterSeconds=min(cap,remaining))
            save()
            try: row['exit']=child.wait(timeout=row['stopAfterSeconds'])
            finally:
                try: os.killpg(child.pid,signal.SIGTERM)
                except ProcessLookupError: pass
                try: child.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid,signal.SIGKILL);child.wait()
                try: os.killpg(child.pid,signal.SIGKILL)
                except ProcessLookupError: pass
                try: os.killpg(child.pid,0);row['groupAbsent']=False
                except ProcessLookupError: row['groupAbsent']=True
                row['durationMs']=int(time.time()*1000)-row['startedAtMs']
                save()
        print(name,'exit',row['exit'],'output',out,flush=True)
        if row['exit'] not in expected or not row['groupAbsent']:
            raise RuntimeError(name+' failed; '+str(out/(name+'.log')))
        return row
    try:
        result['head']=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
        result['sourceTree']=subprocess.check_output(['git','rev-parse','HEAD:src'],text=True).strip()
        subprocess.run(['git','diff','--exit-code','--','src'],check=True,capture_output=True)
        for file in list(here.glob('m7-*.py'))+list((here.parent/'k24').glob('*.py')):
            compile(file.read_bytes(),str(file),'exec')
        for file in ('upgrade.mjs','upgrade-controller.mjs','sandbox.mjs','installer-smoke.mjs'):
            run('syntax-'+file,['node','--check',here/file],15)
        run('syntax-k24-remote',['node','--check',here.parent/'k24/remote-adapter.mjs'],15)
        run('release',['python3',here/'m7-release.py','--output',out/'release'],4500)
        release = out/'release/release.json'
        result['release']=json.loads(release.read_text())
        fixture = base/'v4-m7-fix2-qualified-20261016T020215Z/profile'
        before = base/'v4-e2e-old-portable-20261015/build.json'
        run('dress',['python3',here/'m7-dress.py','--release',release,'--before',before,
                     '--profile',fixture,'--output',out/'dress'],1100,(0,1,2))
        result['matrix']=json.loads((out/'dress/m7-go-nogo.json').read_text())
        r=result['release']
        run('static',['python3','src/core/install/static_probe.py','--mode','plain','--profile',fixture,
            '--binary',r['binary'],'--sha256',r['binarySha256'],'--source',Path(r['binary']).parent.parent,
            '--slot',base/'v4-m7-fix-signed-reference-20261016/0.1.46-beta.106','--out',out/'static'],180,(0,1))
        result['static']=json.loads((out/'static/result.json').read_text())
        installed=out/'dress/install'
        proof=installed/'upgrade/installer.json'
        if proof.exists() and json.loads(proof.read_text()).get('roundtripExecuted'):
            root=next(installed.glob('tenant-*'))
            run('k24-target',['python3',here.parent/'k24/native-target-probe.py','--root',root,
                             '--release',release,'--output',out/'k24-target'],90,(0,1,2))
            result['k24Target']=json.loads((out/'k24-target/target.json').read_text())
        else:
            result['k24Target']=dict(status='not_run',owner='DevOps installer',reason='No completed real installed recovery profile to probe')
    except Exception as error:
        result['error']=str(error)
    finally:
        result.update(terminal=True,finishedAtMs=int(time.time()*1000))
        save()
    print(json.dumps(dict(output=str(out),terminal=True,error=result.get('error'),fullM7Accepted=False)))
    return 0 if not result.get('error') else 1


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',required=True,type=Path)
    p.add_argument('--deadline',required=True)
    raise SystemExit(main(p.parse_args()))
