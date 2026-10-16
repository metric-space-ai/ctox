#!/usr/bin/env python3
"""One bounded release/dress/official-slot/E38 qualification, no admission loop."""
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
    deadline = datetime.fromisoformat(a.deadline.replace('Z','+00:00')).timestamp()
    here = Path(__file__).resolve().parent
    result = dict(owner='01a087a0-169a-7e23-ba3d-71352256cbfb',terminal=False,
                  startedAtMs=int(time.time()*1000),steps=[],go=False)
    def save(): (out/'qualification.json').write_text(json.dumps(result,indent=2)+'\n')
    def interrupt(sig,_): raise RuntimeError('Owned dress4 interrupted '+str(sig))
    signal.signal(signal.SIGTERM,interrupt); signal.signal(signal.SIGINT,interrupt)
    def run(name,args,cap,expected=(0,)):
        remaining=deadline-time.time()-20
        if remaining<5: raise TimeoutError('Absolute deadline before '+name)
        row=dict(name=name,command=[str(x) for x in args],startedAtMs=int(time.time()*1000))
        result['steps'].append(row)
        with (out/(name+'.log')).open('w') as log:
            child=subprocess.Popen(row['command'],stdout=log,stderr=log,start_new_session=True)
            row.update(pid=child.pid,pgid=child.pid,stopAfterSeconds=min(cap,remaining));save()
            try: row['exit']=child.wait(timeout=row['stopAfterSeconds'])
            finally:
                try: os.killpg(child.pid,signal.SIGTERM)
                except ProcessLookupError: pass
                try: child.wait(timeout=8)
                except subprocess.TimeoutExpired: os.killpg(child.pid,signal.SIGKILL);child.wait()
                try: os.killpg(child.pid,signal.SIGKILL)
                except ProcessLookupError: pass
                try: os.killpg(child.pid,0);row['groupAbsent']=False
                except ProcessLookupError: row['groupAbsent']=True
                row['durationMs']=int(time.time()*1000)-row['startedAtMs'];save()
        print(name,'exit',row.get('exit'),'evidence',out,flush=True)
        assert row.get('exit') in expected and row['groupAbsent'], name+' failed'
    try:
        result['head']=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
        subprocess.run(['git','diff','--exit-code','HEAD','--','src','Cargo.toml','Cargo.lock','.cargo'],check=True)
        for path in list(here.glob('m7-*.py'))+list((here.parent/'k24').glob('*.py')):
            compile(path.read_bytes(),str(path),'exec')
        result['syntax']='pass';save()
        # A fixture HTTP contract is a component witness, never installed-slot
        # acceptance. Keep its mutation deadline separate from the turn bound.
        contract_deadline = min(deadline-2700, time.time()+1800)
        if contract_deadline-time.time()>60:
            run('e38-contract',['python3',here/'m7-e31-qualify.py','--output',out/'e38',
                '--deadline',datetime.fromtimestamp(contract_deadline).astimezone().isoformat()],
                contract_deadline-time.time()+10,(0,1))
            result['e38Contract']=json.loads((out/'e38/e31.json').read_text());save()
        if a.release:
            release=a.release.resolve()
            prior=json.loads(release.read_text())
            assert prior['sourceTree']==subprocess.check_output(['git','rev-parse','HEAD:src'],text=True).strip()
            assert prior['terminal'] and prior['pass']
            result['sameProductReceiptReused']=str(release)
        else:
            run('release',['python3',here/'m7-release.py','--output',out/'release'],4500)
            release=out/'release/release.json'
        r=json.loads(release.read_text())
        result['release']={k:r[k] for k in ('revision','sourceTree','binary','binarySha256','package','packageSha256')};save()
        run('dress',['python3',here/'m7-dress.py','--release',release,
            '--before',base/'v4-e2e-old-portable-20261015/build.json',
            '--profile',base/'v4-m7-fix2-qualified-20261016T020215Z/profile',
            '--output',out/'dress'],1100,(0,1,2))
        result['matrix']=json.loads((out/'dress/m7-go-nogo.json').read_text());save()
        root=out/'dress/install/tenant-m7'
        proof=out/'dress/install/upgrade/installer.json'
        if not proof.exists() or not json.loads(proof.read_text()).get('roundtripExecuted'):
            raise RuntimeError('No completed supported installed roundtrip; no substitute profile')
        run('slot',['python3',here/'m7-slot-probe.py','--root',root,'--release',release,
            '--candidate',a.candidate,'--output',out/'slot'],180,(0,2))
        result['slot']=json.loads((out/'slot/slot.json').read_text());save()
        run('target',['python3',here.parent/'k24/native-target-probe.py','--root',root,
            '--release',release,'--output',out/'target'],150,(0,2))
        result['target']=json.loads((out/'target/target.json').read_text());save()
    except Exception as error: result['error']=str(error)
    finally:
        result.update(terminal=True,finishedAtMs=int(time.time()*1000));save()
    print(json.dumps(dict(terminal=True,go=False,error=result.get('error'),output=str(out))))
    return 1 if result.get('error') else 0


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for flag in ('output','candidate'):p.add_argument('--'+flag,type=Path,required=True)
    p.add_argument('--deadline',required=True)
    p.add_argument('--release',type=Path)
    raise SystemExit(main(p.parse_args()))
