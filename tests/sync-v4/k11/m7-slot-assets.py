#!/usr/bin/env python3
"""Verify the actual public archive against its published checksum asset."""
import argparse,json,hashlib,urllib.request,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--tag',required=True);p.add_argument('--sha256',required=True);p.add_argument('--output',required=True);a=p.parse_args()
version=a.tag.removeprefix('business-os-shell-v');assert a.tag=='business-os-shell-v'+version and version.startswith('0.2.0-beta.')
out=Path(a.output);assert '/build-lane/artifacts/' in str(out.resolve());out.mkdir(mode=0o700)
base='https://github.com/metric-space-ai/ctox/releases/download/'+a.tag+'/';name='ctox-business-os-shell-'+version+'.tar.gz';r={'tag':a.tag,'startedAtMs':int(time.time()*1000),'terminal':False,'pass':False}
try:
 with urllib.request.urlopen(base+name+'.sha256',timeout=30) as response:checksum=response.read(4096).decode()
 expected=checksum.split()[0];assert expected==a.sha256,'Published checksum differs from authorized pinned archive'
 h=hashlib.sha256();size=0
 with urllib.request.urlopen(base+name,timeout=60) as response,(out/name).open('wb') as target:
  while block:=response.read(1024*1024):h.update(block);size+=len(block);target.write(block)
 r.update(expectedSha256=expected,actualSha256=h.hexdigest(),bytes=size,checksumAsset=checksum.strip(),archive=str(out/name));assert h.hexdigest()==expected,'Actual archive checksum differs'
 r['pass']=True
finally:r.update(terminal=True,finishedAtMs=int(time.time()*1000));(out/'receipt.json').write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps(r))
