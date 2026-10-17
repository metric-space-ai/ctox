#!/usr/bin/env python3
"""Verify published main artifact custody for the private installer; never build/sign."""
import argparse,hashlib,json,os,tarfile,time
from pathlib import Path

def digest(path):
    h=hashlib.sha256()
    with Path(path).open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024),b''):h.update(chunk)
    return h.hexdigest()

def main(a):
    assert os.uname().sysname=='Linux' and os.environ.get('BUILD_LANE_BIN')
    allowed=(Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve();out=a.output.resolve()
    assert allowed in out.parents and not out.exists()
    assert len(a.main_sha)==40 and all(c in '0123456789abcdef' for c in a.main_sha)
    assert digest(a.archive)==a.artifact_sha256,'Published archive checksum differs'
    out.mkdir(parents=True,mode=0o700);bundle=out/'bundle';bundle.mkdir(mode=0o700)
    assets={};size=0
    with tarfile.open(a.archive,'r:gz') as archive:
        for entry in archive:
            path=Path(entry.name)
            assert not path.is_absolute() and '..' not in path.parts,'Unsafe archive member'
            if entry.isdir():continue
            assert entry.isfile(),'Canonical release contains link/special member'
            name=path.as_posix();assert name not in assets,'Duplicate release member'
            size+=entry.size;assert size<=8*1024**3 and len(assets)<100000,'Release extraction budget exceeded'
            dest=bundle/path;dest.parent.mkdir(parents=True,exist_ok=True)
            with archive.extractfile(entry) as source,dest.open('xb') as target:
                for chunk in iter(lambda:source.read(1024*1024),b''):target.write(chunk)
            dest.chmod(entry.mode & 0o755);assets[name]=digest(dest)
    binary=bundle/'bin/ctox';assert assets.get('bin/ctox')==a.binary_sha256,'Published binary checksum differs'
    r={'schema':'ctox.m7.release.v1','revision':a.main_sha,'source':str(bundle),'binary':str(binary),'binarySha256':a.binary_sha256,'package':str(a.archive.resolve()),'packageSha256':a.artifact_sha256,'assets':assets,'reference':'.github/workflows/release.yml:published-main-artifact-custody','terminal':True,'pass':True,'buildExecutedHere':False,'scope':'operator-provided published main archive and binary checksums; no build/signature claim','verifiedAtMs':int(time.time()*1000)}
    (out/'release.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps({'receipt':str(out/'release.json'),'binarySha256':a.binary_sha256,'archiveSha256':a.artifact_sha256,'buildExecutedHere':False}))

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for key in ['archive','output']:p.add_argument('--'+key,type=Path,required=True)
    for key in ['artifact-sha256','binary-sha256','main-sha']:p.add_argument('--'+key,required=True)
    main(p.parse_args())
