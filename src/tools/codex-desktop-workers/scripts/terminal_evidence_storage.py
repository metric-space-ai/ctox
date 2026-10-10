#!/usr/bin/env python3
"""Verified remote raw evidence; local evidence summaries stay within 20 MB."""
import csv
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import shlex
import stat
import struct
import subprocess
import uuid

ROOT = Path.home() / '.codex/task-evidence/terminal-pr-learning'
HOST = 'ts-gpu3'
DEST = '/mnt/sata8t/cache/task-evidence/terminal-pr-learning'
MANIFEST = ROOT / 'MOVED-TO-GPU3-20261010.tsv'
OUTPUTS = ROOT / 'GPU3-RAW-OUTPUTS.tsv'
LIMIT = 20_000_000
SSH = ['ssh', '-C', '-o', 'IPQoS=none', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=15', '-o', 'StrictHostKeyChecking=yes', HOST]


def relative(path):
    try:
        rel = Path(path).absolute().relative_to(ROOT)
    except ValueError:
        return None
    if '..' in rel.parts or any(c in str(rel) for c in '\t\r\n\0'):
        raise ValueError('Unsafe evidence path')
    return str(rel)


def entries():
    result = {}
    for manifest in (MANIFEST, OUTPUTS):
        if manifest.is_file():
            text = subprocess.check_output(['greppy', 'rg', '--no-heading', '--no-line-number', '^', str(manifest)], text=True)
            for row in csv.DictReader(io.StringIO(text), delimiter='\t'):
                result[row['file']] = row
    return result


def locator(path):
    rel = relative(path)
    return entries().get(rel) if rel is not None else None


def read_text(path):
    """Return verified content; never create a local raw cache."""
    path = Path(path)
    if path.exists():
        return subprocess.check_output(['greppy', 'rg', '--no-heading', '--no-line-number', '^', str(path)], text=True)
    row = locator(path)
    if row is None:
        raise FileNotFoundError(str(path))
    target = DEST + '/' + row['file']
    if row['target_path'] != target:
        raise ValueError('Unexpected remote evidence locator')
    code = 'import sys;from pathlib import Path;p=Path('+repr(target)+');\nwith p.open("rb") as f:\n for chunk in iter(lambda:f.read(1048576),b""):sys.stdout.buffer.write(chunk)'
    raw = subprocess.check_output(SSH + ['python3 -c ' + shlex.quote(code)], timeout=300)
    if len(raw) != int(row['bytes']) or hashlib.sha256(raw).hexdigest() != row['sha256']:
        raise ValueError('Remote evidence checksum mismatch: ' + row['file'])
    return raw.decode('utf-8')


_REMOTE_WRITE = r'''
import fcntl,hashlib,json,os,stat,struct,sys,uuid
from pathlib import Path
q=json.loads(sys.argv[1]);root=Path(q['root']);rel=Path(q['file'])
if rel.is_absolute() or '..' in rel.parts:raise ValueError('unsafe remote path')
p=root/rel;p.parent.mkdir(parents=True,exist_ok=True)
stage=p.with_name(p.name+'.'+uuid.uuid4().hex+'.writing')
try:
 h=hashlib.sha256();size=0
 def exact(n):
  out=bytearray()
  while len(out)<n:
   c=sys.stdin.buffer.read(n-len(out))
   if not c:raise EOFError('incomplete raw evidence stream')
   out.extend(c)
  return bytes(out)
 with stage.open('xb') as f:
  while True:
   n=struct.unpack('>I',exact(4))[0]
   if n==0:break
   if n>1048576:raise ValueError('oversized raw evidence frame')
   c=exact(n);f.write(c);h.update(c);size+=len(c)
  footer=json.loads(sys.stdin.buffer.readline())
  if footer!={'bytes':size,'sha256':h.hexdigest()}:raise ValueError('raw evidence stream checksum mismatch')
  f.flush();os.fsync(f.fileno())

 stage.chmod(0o600);new=h.hexdigest();previous=None
 # Serialize comparison and publication across writers, not only the local index.
 with p.with_name(p.name+'.write.lock').open('a') as lease:
  fcntl.flock(lease,fcntl.LOCK_EX)
  if p.exists():
   if not stat.S_ISREG(p.lstat().st_mode):raise ValueError('destination is not a regular file')
   old=hashlib.sha256()
   with p.open('rb') as f:
    for c in iter(lambda:f.read(1048576),b''):old.update(c)
   oldsha=old.hexdigest()
   if oldsha!=new:
    if oldsha!=q.get('expected'):raise ValueError('untracked or changed destination; refusing overwrite')
    backup=root/'.versions'/rel/(oldsha+'.raw');backup.parent.mkdir(parents=True,exist_ok=True)
    if not backup.exists():os.link(p,backup)
    preserved=hashlib.sha256()
    with backup.open('rb') as f:
     for c in iter(lambda:f.read(1048576),b''):preserved.update(c)
    if preserved.hexdigest()!=oldsha:raise ValueError('previous remote version backup mismatch')
    previous={'target_path':str(backup),'sha256':oldsha,'bytes':backup.stat().st_size}
  elif q.get('expected'):raise ValueError('recorded remote destination missing; refusing replacement')
  os.replace(stage,p)
  dfd=os.open(str(p.parent),os.O_RDONLY)
  try:os.fsync(dfd)
  finally:os.close(dfd)
 json.dump({'file':q['file'],'bytes':size,'sha256':new,'target_path':str(p),'previous':previous},sys.stdout)
finally:
 if stage.exists():stage.unlink()
'''


def record_output(row):
    ROOT.mkdir(parents=True, exist_ok=True)
    lock = ROOT / '.gpu3-raw-output-index.lock'
    with lock.open('a') as lease:
        fcntl.flock(lease, fcntl.LOCK_EX)
        header = not OUTPUTS.exists()
        with OUTPUTS.open('a', newline='') as f:
            writer = csv.writer(f, delimiter='\t', lineterminator='\n')
            if header:
                writer.writerow(['file', 'bytes', 'sha256', 'target_path'])
            writer.writerow([row['file'],row['bytes'],row['sha256'],row['target_path']])
            f.flush();os.fsync(f.fileno())
        if row.get('previous'):
            history = ROOT / 'GPU3-RAW-VERSIONS.jsonl'
            with history.open('a') as f:
                f.write(json.dumps(dict(file=row['file'], **row['previous']))+'\n')
                f.flush();os.fsync(f.fileno())


def _process(rel, expected):
    args = json.dumps(dict(root=DEST, file=rel, expected=expected))
    return subprocess.Popen(SSH + ['python3 -c '+shlex.quote(_REMOTE_WRITE)+' '+shlex.quote(args)],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def _send(stream, data):
    view = memoryview(data)
    for offset in range(0, len(view), 1024**2):
        part = view[offset:offset+1024**2]
        stream.stdin.write(struct.pack('>I', len(part)))
        stream.stdin.write(part)


def save_json(path, value, *, force_remote=False):
    """Stream raw/large JSON to gpu3; preserve verified previous remote versions."""
    path = Path(path)
    rel = relative(path)
    if rel is None:
        raise ValueError('Remote storage is restricted to terminal-pr-learning')
    old = locator(path)
    if force_remote and path.exists():
        raise ValueError('Existing local raw evidence must be verified/offloaded before replacement: '+str(path))
    stream = _process(rel, old['sha256'] if old else None) if old or force_remote else None
    pending = bytearray()
    h = hashlib.sha256()
    size = 0
    try:
        chunks = iter(json.JSONEncoder(ensure_ascii=False, indent=2).iterencode(value))
        for text in chunks:
            chunk = text.encode('utf-8')
            h.update(chunk);size += len(chunk)
            if stream is None and len(pending) + len(chunk) <= LIMIT:
                pending.extend(chunk)
                continue
            if stream is None:
                if path.exists():
                    raise ValueError('Large existing local evidence must be verified/offloaded before replacement: '+str(path))
                stream = _process(rel, None)
                _send(stream,pending);pending.clear()
            _send(stream,chunk)
        h.update(b'\n');size+=1
        if stream is None and len(pending)+1>LIMIT:
            if path.exists():raise ValueError('Large existing local evidence must be offloaded first')
            stream=_process(rel,None);_send(stream,pending);pending.clear()
        if stream is None:
            pending.extend(b'\n');path.parent.mkdir(parents=True,exist_ok=True)
            stage=path.with_name(path.name+'.'+uuid.uuid4().hex+'.writing')
            try:
                with stage.open('xb') as f:
                    f.write(pending);f.flush();os.fsync(f.fileno())
                stage.chmod(0o600);os.replace(stage,path)
            finally:
                if stage.exists():stage.unlink()
            return dict(local=True,bytes=size,sha256=h.hexdigest(),path=str(path))
        _send(stream,b'\n')
        stream.stdin.write(struct.pack('>I',0))
        stream.stdin.write((json.dumps(dict(bytes=size,sha256=h.hexdigest()))+'\n').encode())
        stream.stdin.close();stream.stdin=None
        out,err=stream.communicate(timeout=300)
        if stream.returncode:
            raise RuntimeError('gpu3 raw write failed: '+err.decode('utf-8',errors='replace')[-2000:])
        row=json.loads(out)
        if row['bytes']!=size or row['sha256']!=h.hexdigest() or row['target_path']!=DEST+'/'+rel:
            raise ValueError('gpu3 write receipt does not match generated bytes')
        record_output(row)
        return dict(local=False,host=HOST,**row)
    except BaseException:
        if stream is not None and stream.poll() is None:
            if stream.stdin is not None:
                stream.stdin.close();stream.stdin=None
            try:stream.wait(timeout=10)
            except subprocess.TimeoutExpired:stream.terminate();stream.wait(timeout=10)
        raise
