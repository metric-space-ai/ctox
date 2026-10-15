#!/usr/bin/env python3
"""Bounded verified artifact stream over the two existing pinned SSH routes.

No Mac archive copy, no new host trust, no producer or receiver test process.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import os
import shlex
import signal
import subprocess
import time

OWNER = '01a087a0-169a-7e23-ba3d-71352256cbfb'
REMOTE = '''
import hashlib,json,os,pathlib,signal,sys,threading,time
c=json.loads(sys.argv[1])\ntry:os.setsid()\nexcept PermissionError:assert os.getpgrp()==os.getpid(), 'Not an owned remote group'\np=pathlib.Path(c['lifecycle']);p.parent.mkdir(parents=True,exist_ok=True)
r={'owner':c['owner'],'pid':os.getpid(),'pgid':os.getpgrp(),'purpose':c['mode'],'stop':'EOF or 25 minutes','startedAtMs':int(time.time()*1000),'terminal':False}
def save():p.write_text(json.dumps(r,indent=2))
def expire():
 r['deadlineReached']=True;save();os.killpg(os.getpgrp(),signal.SIGTERM)
timer=threading.Timer(1500,expire);timer.daemon=True;timer.start();save()
try:
 if c['mode']=='source':
  f=pathlib.Path(c['path']);assert f.is_file() and f.stat().st_size==c['bytes'];h=hashlib.sha256()
  with f.open('rb') as stream:
   while b:=stream.read(1024*1024):h.update(b)
  assert h.hexdigest()==c['sha256']
  with f.open('rb') as stream:
   while b:=stream.read(256*1024):sys.stdout.buffer.write(b)
  sys.stdout.buffer.flush()
 else:
  d=pathlib.Path(c['path']);d.mkdir(mode=0o700);h=hashlib.sha256();size=0
  with (d/'package.tar.zst').open('xb') as stream:
   while b:=sys.stdin.buffer.read(256*1024):size+=len(b);h.update(b);stream.write(b);assert size<=c['bytes']
  assert size==c['bytes'] and h.hexdigest()==c['sha256'];r['bytes']=size;r['sha256']=h.hexdigest()
 r['pass']=True
finally:
 timer.cancel();r['terminal']=True;r['finishedAtMs']=int(time.time()*1000);save()
'''


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--source', required=True)
    p.add_argument('--destination', required=True)
    p.add_argument('--sha256', required=True)
    p.add_argument('--bytes', type=int, required=True)
    p.add_argument('--receipt', required=True)
    a = p.parse_args()
    assert a.source.startswith('/mnt/nvme1/build-lane/artifacts/v4-e2e-')
    assert a.destination.startswith('/home/metricspace/build-lane/artifacts/v4-e2e-')
    assert '..' not in Path(a.source).parts and '..' not in Path(a.destination).parts
    assert len(a.sha256) == 64 and all(c in '0123456789abcdef' for c in a.sha256)
    receipt = Path(a.receipt)
    assert not receipt.exists() and a.bytes > 0
    receipt.parent.mkdir(parents=True, exist_ok=True)
    result = {'owner': OWNER, 'controllerPid': os.getpid(), 'startedAt': datetime.now(timezone.utc).isoformat(),
              'source': a.source, 'destination': a.destination, 'sha256': a.sha256,
              'bytes': a.bytes, 'stop': 'terminal or 25 minutes', 'terminal': False, 'pass': False}
    children = []
    def command(host, mode, path):
        lifecycle = (a.source + '.transfer.json') if mode == 'source' else (a.destination + '.transfer.json')
        config = {'owner': OWNER, 'mode': mode, 'path': path, 'lifecycle': lifecycle, 'sha256': a.sha256, 'bytes': a.bytes}
        return ['ssh', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes', '-o', 'ConnectTimeout=10', host,
                'python3 -c ' + shlex.quote(REMOTE) + ' ' + shlex.quote(json.dumps(config))]
    try:
        source = subprocess.Popen(command('ts-gpu3', 'source', a.source), stdout=subprocess.PIPE, start_new_session=True)
        children.append(source)
        receiver = subprocess.Popen(command('metricspace@100.120.16.93', 'receiver', a.destination), stdin=source.stdout, start_new_session=True)
        children.append(receiver)
        source.stdout.close()
        result.update(sourcePid=source.pid, sourcePgid=source.pid, receiverPid=receiver.pid, receiverPgid=receiver.pid)
        receipt.write_text(json.dumps(result, indent=2))
        deadline = time.monotonic() + 1500
        result['receiverExit'] = receiver.wait(timeout=max(1, deadline-time.monotonic()))
        result['sourceExit'] = source.wait(timeout=max(1, deadline-time.monotonic()))
        result['pass'] = result['receiverExit'] == result['sourceExit'] == 0
    except Exception as error:
        result['error'] = str(error)
    finally:
        for child in children:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
        result['localChildrenTerminal'] = all(p.poll() is not None for p in children)
        result['finishedAt'] = datetime.now(timezone.utc).isoformat()
        result['terminal'] = True
        receipt.write_text(json.dumps(result, indent=2))
    print(json.dumps(result))
    return 0 if result['pass'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
