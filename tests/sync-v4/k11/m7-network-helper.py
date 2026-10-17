#!/usr/bin/env python3
"""Root-only setup of slirp in our descendant namespace, never host networking."""
import json,os,signal,select,subprocess,sys,time
from pathlib import Path
config=Path(sys.argv[1]).resolve(); wrapper=int(sys.argv[2]); tools=Path(sys.argv[3]).resolve()
c=json.loads(config.read_text()); out=Path(c['output']); request=json.loads((out/'network-request.json').read_text())
assert os.getuid()==0 and c['owner']=='01a087a0-169a' and '/build-lane/artifacts/' in str(out)
assert request['net']!=c['host']['net']
# Only a descendant of the exact namespace launcher can receive this link.
parents={}
for p in Path('/proc').iterdir():
 if p.name.isdigit():
  try: parents[int(p.name)]=int((p/'stat').read_text().rsplit(')',1)[1].split()[1])
  except (OSError,ValueError):pass
matches=[]
for pid in parents:
 q=pid; seen=set()
 while q in parents and q not in seen and q!=wrapper:seen.add(q);q=parents[q]
 if q!=wrapper:continue
 try:
  if os.readlink(f'/proc/{pid}/ns/net')==request['net']:matches.append(pid)
 except OSError:pass
assert matches,'No owned descendant namespace matches the readiness request'
target=min(matches); binary=tools/'usr/bin/slirp4netns'; assert binary.is_file()
read,write=os.pipe(); child=None
r={'owner':c['owner'],'namespacePid':target,'namespace':request['net'],'hostNetworkUnchanged':True,'terminal':False}
def save():(out/'network-helper.json').write_text(json.dumps(r,indent=2))
def stop(*_):raise InterruptedError('Owned network helper stopped')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
try:
 env=dict(os.environ,LD_LIBRARY_PATH=str(tools/'usr/lib/x86_64-linux-gnu'))
 with (out/'slirp-native.log').open('w') as log:
  child=subprocess.Popen([str(binary),'--configure','--disable-host-loopback','--ready-fd='+str(write),str(target),'tap0'],stdout=log,stderr=log,env=env,pass_fds=(write,))
 os.close(write);r['slirpPid']=child.pid;save()
 assert select.select([read],[],[],15)[0] and os.read(read,1),'Private slirp readiness timeout'
 r['ready']=True;save();os.write(1,b'1')
 child.wait(timeout=580)
except InterruptedError:pass
finally:
 os.close(read)
 if child and child.poll() is None:
  child.terminate()
  try:child.wait(timeout=5)
  except subprocess.TimeoutExpired:child.kill();child.wait()
 r.update(terminal=True,childTerminal=child is None or child.poll() is not None,exit=None if child is None else child.returncode);save()
