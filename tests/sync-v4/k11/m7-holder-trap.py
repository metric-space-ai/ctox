#!/usr/bin/env python3
"""Observe only our private namespace/profile, including deleted DB handles."""
import argparse, json, os, subprocess, time
from pathlib import Path

def capture(root, output, label):
    root=Path(root).resolve(); out=Path(output)
    assert json.loads((root/'.sync-v4-e2e.json').read_text())['synthetic'] is True
    assert '/artifacts/' in str(root) and os.getenv('BUILD_LANE_BIN')
    result={'atMs':int(time.time()*1000),'label':label,'root':str(root),'pidNamespace':os.readlink('/proc/self/ns/pid'),'processes':[],'holders':[]}
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit(): continue
        try:
            stat=(proc/'stat').read_text(); tail=stat[stat.rfind(')')+2:].split()
            cmd=(proc/'cmdline').read_bytes().replace(b'\0',b' ').decode(errors='replace')
            row={'pid':int(proc.name),'ppid':int(tail[1]),'startTicks':tail[19],'command':cmd,'fds':[]}
            for fd in (proc/'fd').iterdir():
                try:
                    target=os.readlink(fd)
                    if str(root) in target and any(x in target for x in ('.sqlite3','.lock','.owner','/store/')):
                        row['fds'].append({'fd':fd.name,'path':target,'fdinfo':(proc/'fdinfo'/fd.name).read_text()})
                except (OSError,PermissionError): pass
            if row['fds']: result['holders'].append(row)
            if row['fds'] or any(x in cmd for x in ('ctox','systemd','dbus','upgrade','m7-')): result['processes'].append(row)
        except (OSError,ValueError): pass
    result['locks']=Path('/proc/locks').read_text()
    paths=[str(root/'runtime/ctox.sqlite3')+suffix for suffix in ('','-wal','-shm')]
    commands=[('lsof',['lsof','-nP','--',*paths]),('fuser',['fuser','-v',*paths]),('units',['systemctl','--user','show','ctox.service','ctox-watchdog.service','ctox-watchdog.timer','--property=Id,ActiveState,SubState,MainPID,ExecMainStartTimestamp,ExecMainExitTimestamp,InactiveEnterTimestamp,ControlGroup'])]
    for name,command in (commands if label=='rollback-exit-1' else []):
        try:
            p=subprocess.run(command,capture_output=True,text=True,timeout=5)
            result[name]={'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
        except (OSError,subprocess.TimeoutExpired) as e: result[name]={'error':str(e)}
    out.write_text(json.dumps(result,indent=2)+'\n')
    return result
if __name__=='__main__':
    p=argparse.ArgumentParser(); p.add_argument('--root',required=True);p.add_argument('--output',required=True);p.add_argument('--label',required=True);p.add_argument('--observe-pid',type=int)
    a=p.parse_args()
    if a.observe_pid:
        # Fixed sample schedule inside one measured rollback, never an admission loop.
        start=time.monotonic()
        for second in (0,1,5,9):
            time.sleep(max(0,start+second-time.monotonic()))
            if not Path('/proc/'+str(a.observe_pid)).exists(): break
            capture(a.root,a.output+'.'+str(second)+'.json',a.label+'+'+str(second)+'s')
    else: capture(a.root,a.output,a.label)
