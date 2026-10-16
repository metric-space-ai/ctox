#!/usr/bin/env python3
"""Try only the supported signed slot stage/activate path in an owned profile.

The unsigned local candidate is inspected, never copied into the slot store or
signed with a test key. Stage failure is evidence, not permission for a bypass.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tarfile
import time

OWNER = '01a087a0-169a-7e23-ba3d-71352256cbfb'
CANDIDATE_SHA = '91de54d168a4cce3f2be74df2726d4c20ccf71089ae1c57f5d088421b78113c7'


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def main(a):
    assert os.uname().sysname == 'Linux' and os.environ.get('BUILD_LANE_BIN')
    base = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
    root, out = a.root.resolve(), a.output.resolve()
    assert base in root.parents and base in out.parents and not out.exists()
    marker = json.loads((root/'.sync-v4-e2e.json').read_text())
    assert marker.get('owner') == OWNER and marker.get('synthetic') is True
    release = json.loads(a.release.read_text())
    assert release['terminal'] and release['pass']
    current = (root/'managed/current').resolve(strict=True)
    binary = current/'bin/ctox'
    assert root in current.parents and sha(binary) == release['binarySha256']
    out.mkdir(mode=0o700)
    result = dict(owner=OWNER, synthetic=True, startedAtMs=int(time.time()*1000),
                  terminal=False, commands=[], activated=False, status='not_run',
                  release=release['revision'], binarySha256=release['binarySha256'])
    def save():
        (out/'slot.json').write_text(json.dumps(result, indent=2)+'\n')
    def interrupt(sig, _):
        raise RuntimeError('Owned slot probe interrupted '+str(sig))
    signal.signal(signal.SIGTERM, interrupt)
    signal.signal(signal.SIGINT, interrupt)
    env = {k: os.environ[k] for k in ('PATH','TMPDIR','BUILD_LANE_BIN')}
    home = root/'installer-home'
    env.update(HOME=str(home), LANG='C.UTF-8', CTOX_ROOT=str(current),
               CTOX_STATE_ROOT=str(root/'runtime'), CTOX_INSTALL_ROOT=str(root/'managed'),
               XDG_CONFIG_HOME=str(home/'.config'), XDG_CACHE_HOME=str(home/'.cache'))
    def run(name, args):
        command = [str(binary), 'business-os', 'shell-update', *args]
        row = dict(name=name, command=command, startedAtMs=int(time.time()*1000))
        result['commands'].append(row)
        with (out/(name+'.log')).open('w') as log:
            child = subprocess.Popen(command, cwd=current, env=env, stdout=log,
                                     stderr=log, start_new_session=True)
            row.update(pid=child.pid, pgid=child.pid, stopAfterSeconds=50)
            save()
            try:
                row['exit'] = child.wait(timeout=50)
            finally:
                try: os.killpg(child.pid, signal.SIGTERM)
                except ProcessLookupError: pass
                try: child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL); child.wait()
                try: os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError: pass
                try: os.killpg(child.pid, 0); row['groupAbsent'] = False
                except ProcessLookupError: row['groupAbsent'] = True
                row['durationMs'] = int(time.time()*1000)-row['startedAtMs']
                save()
        assert row['groupAbsent'], 'Slot CLI group remains live'
        return row['exit']
    try:
        assert sha(a.candidate) == CANDIDATE_SHA, 'Candidate bytes changed; request a new pin'
        with tarfile.open(a.candidate, 'r:gz') as archive:
            names = sorted(m.name.removeprefix('./') for m in archive if m.isfile())
        result['candidate'] = dict(path=str(a.candidate), sha256=CANDIDATE_SHA,
            entryV4='v4/index.html' in names, bootV4='v4/boot.js' in names,
            swV4='v4/sw.js' in names, legacyBoot='v4/boot/index.html' in names,
            kernelModules=sum(n.startswith('kernel/') and n.endswith('.mjs') for n in names),
            signedEnvelope=any(n.endswith('.release.v2.json') for n in names), files=len(names),
            entrySamples=names[:8], bootstrapSuffixMatches=[n for n in names if
                n.endswith(('/v4/index.html','/v4/boot.js','/v4/sw.js','/kernel/boot.mjs'))])
        run('before', ['status'])
        stage = run('stage', ['stage', '--version', '0.2.0-beta.1'])
        result['stageExit'] = stage
        if stage:
            result.update(status='fail', ownerNeeded='Shell v4 / controller',
                reason='Supported signed release stage failed; see stage.log. Unsigned local bytes were not substituted.')
        else:
            # The official verifier, not this probe, decides signature/inventory.
            # Bind its immutable package to the controller's exact candidate;
            # a same-version release with other bytes is not this rehearsal.
            staged = (current/'runtime/business-os-shell/slots/0.2.0-beta.1/.ctox-shell-release.v2.json').resolve(strict=True)
            assert root in staged.parents, 'Staged manifest outside private profile'
            envelope = json.loads(staged.read_text())
            result['stagedArtifactSha256'] = envelope['payload']['artifact']['sha256']
            assert result['stagedArtifactSha256'] == CANDIDATE_SHA, 'Signed package differs from supplied candidate'
            active = run('activate', ['activate'])
            result['activated'] = active == 0
            result['status'] = 'pass' if active == 0 else 'fail'
        run('after', ['status'])
    except Exception as error:
        result.update(status='fail', error=str(error))
    finally:
        result.update(terminal=True, finishedAtMs=int(time.time()*1000)); save()
    print(json.dumps({k:v for k,v in result.items() if k!='commands'}))
    return 0 if result['status']=='pass' else 2


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for flag in ('root','release','candidate','output'):
        p.add_argument('--'+flag, type=Path, required=True)
    raise SystemExit(main(p.parse_args()))
