#!/usr/bin/env python3
"""Actual release-archive omission, rejected by the receiver, then control."""
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tarfile

assert os.environ.get('BUILD_LANE_BIN') and os.uname().sysname == 'Linux'
spec = importlib.util.spec_from_file_location('release', Path(__file__).with_name('m7-release.py'))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
source = Path(sys.argv[1]).resolve()
out = Path(sys.argv[2]).resolve()
allowed = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
assert allowed in out.parents and not out.exists()
out.mkdir(mode=0o700)
report = json.loads(source.read_text())
baseline = release.verify_package(report)
mutation = out/'missing-app.tar.gz'
missing = 'src/apps/business-os/app.js'
removed = False
with tarfile.open(report['package'], 'r:gz') as src, tarfile.open(mutation, 'w:gz') as dst:
    for member in src:
        if Path(member.name).as_posix() == missing:
            removed = True
            continue
        dst.addfile(member, src.extractfile(member) if member.isfile() else None)
assert removed, 'Mutation must remove an actual required asset'
mutant = {**report, 'package': str(mutation), 'packageSha256': release.digest(mutation)}
try:
    release.verify_package(mutant)
except ValueError as error:
    assert str(error) == 'Release archive asset inventory/content differs', str(error)
    rejection = str(error)
else:
    raise AssertionError('Missing actual app.js was accepted')
restored = release.verify_package(report)
proof = dict(pass_=True, baseline=baseline,
    mutation=dict(archive=str(mutation), sha256=mutant['packageSha256'], removed=missing,
                  expected='red', reason=rejection), restored=restored,
    scope='Archive/readback custody contracts only; no installed/browser/M7 acceptance')
if len(sys.argv)>3:
    profile = Path(sys.argv[3]).resolve()
    assert allowed in profile.parents
    mark = json.loads((profile/'.sync-v4-e2e.json').read_text())
    assert mark.get('synthetic') is True and mark.get('owner') == report['owner']
    copy = out/'readback-profile'
    (copy/'runtime').mkdir(parents=True,mode=0o700)
    (copy/'.sync-v4-e2e.json').write_text(json.dumps(mark))
    original = profile/'runtime/business-os-rxdb.sqlite3'
    destination = copy/'runtime/business-os-rxdb.sqlite3'
    def restore_database():
        with sqlite3.connect(original.as_uri()+'?mode=ro',uri=True) as src, sqlite3.connect(destination) as dst:
            src.backup(dst)
    restore_database()
    helper = Path(__file__).with_name('m7-data-readback.py')
    def check(name,expected):
        command=['python3',str(helper),'--root',str(copy),'--output',str(out/(name+'.json'))]
        if name!='before': command.extend(['--expect',str(out/'before.json')])
        result=subprocess.run(command,capture_output=True,text=True,timeout=30)
        (out/(name+'.log')).write_text(result.stdout+result.stderr)
        if expected==0: assert result.returncode==0,result.stderr
        else: assert result.returncode!=0 and 'Restored app row/schema fingerprint differs' in result.stderr,result.stderr
        return dict(exit=result.returncode,log=str(out/(name+'.log')))
    before=check('before',0)
    with sqlite3.connect(destination) as db:
        removed_id=db.execute('SELECT id FROM ctox_business_os__e2e_history__v0 WHERE deleted=1 ORDER BY id LIMIT 1').fetchone()[0]
        db.execute('DELETE FROM ctox_business_os__e2e_history__v0 WHERE id=?',(removed_id,))
    negative=check('missing-tombstone',1)
    restore_database()
    positive=check('restored',0)
    proof['readback']=dict(before=before,mutation=negative,restored=positive,removedSyntheticTombstone=removed_id,
                           beforeFingerprint=str(out/'before.json'),restoredFingerprint=str(out/'restored.json'))
(out/'mutation.json').write_text(json.dumps(proof, indent=2)+'\n')
print(json.dumps(dict(archiveContract='green', mutation='red: missing real app.js', control='green')))
