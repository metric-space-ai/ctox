#!/usr/bin/env python3
"""Measure unprivileged isolation and real-manager admission, separately.

The namespace control must pass before a service manager is attempted. Removing
--net must be rejected before any mount, network or daemon operation.
"""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def main(output):
    assert os.environ.get('BUILD_LANE_BIN') and os.uname().sysname == 'Linux'
    allowed = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
    assert allowed in output.parents and not output.exists()
    output.mkdir(mode=0o700)
    here = Path(__file__).resolve().parent
    host = {kind: os.readlink('/proc/self/ns/'+kind) for kind in ('user','net','mnt','pid','cgroup')}
    result = dict(owner='01a087a0-169a-7e23-ba3d-71352256cbfb', startedAtMs=int(time.time()*1000),
                  host=host, steps=[], terminal=False, namespaceVerified=False, managerVerified=False)
    def run(name, args):
        row = dict(name=name, args=[str(a) for a in args])
        result['steps'].append(row)
        with (output/(name+'.log')).open('w') as log:
            p = subprocess.Popen(row['args'], stdout=log, stderr=log, start_new_session=True)
            row.update(pid=p.pid,pgid=p.pid,stopAfterSeconds=40)
            try:
                row['exit'] = p.wait(timeout=40)
            finally:
                try: os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError: pass
                p.wait()
                try:
                    os.killpg(p.pid, 0)
                    row['groupAbsent'] = False
                except ProcessLookupError:
                    row['groupAbsent'] = True
        return row
    guard = ('import json,os; from pathlib import Path; '
             'c=json.loads(Path(__import__("sys").argv[1]).read_text()); '
             'n={k:os.readlink("/proc/self/ns/"+k) for k in c}; '
             'assert all(n[k]!=c[k] for k in c), "Private namespaces required before setup"; '
             'assert os.getuid()==0 and os.getpid()==1; '
             'print(json.dumps(dict(namespaceVerified=True,namespaces=n,uidMap=Path("/proc/self/uid_map").read_text())))')
    hostfile = output/'host.json'
    hostfile.write_text(json.dumps({k:v for k,v in host.items() if k!='cgroup'}))
    prefix = ['unshare','--user','--map-root-user','--net','--mount','--pid','--fork','--mount-proc']
    try:
        positive = run('namespace-control', prefix+['python3','-c',guard,hostfile])
        mutation = run('namespace-mutation', [v for v in prefix if v!='--net']+['python3','-c',guard,hostfile])
        result['namespaceVerified'] = positive['exit']==0 and positive['groupAbsent']
        result['mutationVerified'] = mutation['exit']!=0 and mutation['groupAbsent'] and 'Private namespaces required before setup' in (output/'namespace-mutation.log').read_text()
        if result['namespaceVerified']:
            root = output/'manager-profile'
            root.mkdir(mode=0o700)
            config = dict(owner=result['owner'], root=str(root), output=str(output), uid=os.getuid(),gid=os.getgid(),
                          tmpdir=os.environ['TMPDIR'], path=os.environ['PATH'],laneBin=os.environ['BUILD_LANE_BIN'],
                          mappedUser=True, node=subprocess.check_output(['which','node'],text=True).strip(),host=host)
            file = output/'sandbox.json'
            file.write_text(json.dumps(config))
            manager = run('real-manager', prefix+[config['node'],here/'sandbox.mjs',file])
            result['managerVerified'] = manager['exit']==0 and manager['groupAbsent']
            if not result['managerVerified']:
                result['managerLimitation'] = 'See real-manager.log: no fake manager or host bus fallback; privileged fallback, if needed, is gpu4-only.'
    finally:
        result['terminal'] = True
        result['finishedAtMs'] = int(time.time()*1000)
        (output/'namespace.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result))
    return 0 if result['namespaceVerified'] and result['mutationVerified'] else 1


if __name__ == '__main__':
    raise SystemExit(main(Path(sys.argv[1]).resolve()))
