#!/usr/bin/env python3
"""Actual release-archive omission, rejected by the receiver, then control."""
import importlib.util
import json
import os
from pathlib import Path
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
(out/'mutation.json').write_text(json.dumps(dict(pass_=True, baseline=baseline,
    mutation=dict(archive=str(mutation), sha256=mutant['packageSha256'], removed=missing,
                  expected='red', reason=rejection), restored=restored,
    scope='Archive custody contract only; no installed/browser/M7 acceptance'), indent=2)+'\n')
print(json.dumps(dict(archiveContract='green', mutation='red: missing real app.js', control='green')))
