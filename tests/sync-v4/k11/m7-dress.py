#!/usr/bin/env python3
"""Bounded, private release/upgrade/rollback rehearsal; never a tenant deploy.

An owned synthetic fallback is explicitly separate from frozen WELSCH coverage.
Missing product connections are not_run with an owner, never fabricated passes.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import time

OWNER = '01a087a0-169a-7e23-ba3d-71352256cbfb'
HERE = Path(__file__).resolve().parent


def sha(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            result.update(chunk)
    return result.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('release', 'before', 'profile', 'output'):
        parser.add_argument('--'+flag, type=Path, required=True)
    args = parser.parse_args()
    assert os.uname().sysname == 'Linux' and os.environ.get('BUILD_LANE_BIN'), 'Canonical Linux lane required'
    allowed = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
    out = args.output.resolve()
    assert allowed in out.parents and not out.exists(), 'New private output required'
    out.mkdir(mode=0o700, parents=True)
    result = dict(schema='ctox.m7.go-nogo.v1', owner=OWNER, startedAtMs=int(time.time()*1000),
                  terminal=False, go=False, criteria=[], commands=[], processes=[])
    def save():
        (out/'m7-go-nogo.json').write_text(json.dumps(result, indent=2)+'\n')
    def criterion(name, status, owner, **details):
        result['criteria'].append(dict(id=name, status=status, owner=owner, **details))
        save()
    def run(name, command, timeout=60):
        row = dict(name=name, command=[str(v) for v in command], startedAtMs=int(time.time()*1000))
        result['commands'].append(row)
        with (out/(name+'.log')).open('w') as log:
            child = subprocess.Popen(row['command'], stdout=log, stderr=log, start_new_session=True)
            process = dict(owner=OWNER, pid=child.pid, pgid=child.pid, purpose=name, stopAfterSeconds=timeout)
            result['processes'].append(process)
            save()
            try:
                row['exit'] = child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                row['timeout'] = True
                row['exit'] = None
            finally:
                try:
                    os.killpg(child.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                try:
                    os.killpg(child.pid, 0)
                    process['groupAbsent'] = False
                except ProcessLookupError:
                    process['groupAbsent'] = True
        row['durationMs'] = int(time.time()*1000)-row['startedAtMs']
        save()
        return row['exit']
    try:
        release = json.loads(args.release.read_text())
        assert release.get('terminal') and release.get('pass'), 'Terminal canonical release receipt required'
        assert release['reference'].startswith('.github/workflows/release.yml:') and release['buildExit'] == 0
        assert sha(release['binary']) == release['binarySha256'] and sha(release['package']) == release['packageSha256']
        spec = importlib.util.spec_from_file_location('release', HERE/'m7-release.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        result['packageVerification'] = module.verify_package(release)
        bundle = Path(release['binary']).parent.parent
        for relative, digest in release['assets'].items():
            assert not Path(relative).is_absolute() and '..' not in Path(relative).parts
            assert sha(bundle/relative) == digest, 'Release asset differs: '+relative
        result['candidate'] = release['revision']
        result['release'] = {k: release[k] for k in ('revision','binary','binarySha256','package','packageSha256','sourceTree')}
        criterion('private-release-artifact', 'pass', 'DevOps', receipt=str(args.release), receiptSha256=sha(args.release),
                  limit='Private release-mode artifact; not published from main and not a full lane-suite qualification')
        profile = args.profile.resolve()
        assert allowed in profile.parents, 'Only the private lane fixture is permitted'
        marker = json.loads((profile/'.sync-v4-e2e.json').read_text())
        assert marker.get('owner') == OWNER and marker.get('synthetic') is True, 'No customer profile or protected files'
        criterion('frozen-welsch-coverage', 'not_run', 'ctox sync v4 / phase1',
                  reason='Provided profile is the owned synthetic V3 fixture, not a frozen WELSCH snapshot')
        snapshot = out/'snapshot-before'
        code = run('sqlite-api-snapshot', ['python3', HERE/'m7-backup.py', 'snapshot', '--root', profile, '--output', snapshot], 180)
        assert code == 0, 'Private snapshot failed'
        manifest = json.loads((snapshot/'snapshot.json').read_text())
        assert manifest['complete'] and manifest['databases']
        criterion('sqlite-snapshot', 'pass', 'DevOps', databases=len(manifest['databases']),
                  manifest=str(snapshot/'snapshot.json'), sha256=sha(snapshot/'snapshot.json'),
                  consistency='Per-file SQLite API snapshot of terminal owned fixture; no live customer copy')
        # Start the real foreground twice on a new SQLite-API-restored copy.
        # The public instance ID can be observed without opening encrypted data.
        # It is deliberately NOT called a public signing-key proof.
        component = out/'foreground-profile'
        component.mkdir(mode=0o700)
        (component/'.sync-v4-e2e.json').write_text(json.dumps(marker))
        # Fresh empty state is only a startup control. SQLite-only clones do
        # not include the product's sealed-key/file custody and must never be
        # presented as a valid frozen-profile restoration.
        (component/'runtime').mkdir(mode=0o700)
        for name in ('src','contracts','Cargo.toml'):
            (component/name).symlink_to(bundle/name)
        home = component/'home'
        home.mkdir(mode=0o700)
        env = dict(PATH=os.environ['PATH'], HOME=str(home), TMPDIR=os.environ['TMPDIR'], LANG='C.UTF-8',
                   XDG_CONFIG_HOME=str(home/'.config'), XDG_CACHE_HOME=str(home/'.cache'),
                   CTOX_ROOT=str(component), CTOX_STATE_ROOT=str(component/'runtime'))
        starts = []
        for attempt in range(2):
            log_path = out/('foreground-'+str(attempt)+'.log')
            with log_path.open('w') as log:
                at = time.monotonic()
                child = subprocess.Popen([release['binary'],'service','--foreground','--root',str(component)],
                                         cwd=component, env=env, stdout=log, stderr=log, start_new_session=True)
                process = dict(owner=OWNER,pid=child.pid,pgid=child.pid,purpose='foreground restart component',stopAfterSeconds=25)
                result['processes'].append(process)
                start = dict(ready=False, readyMs=None, instanceId=None, log=str(log_path))
                starts.append(start)
                save()
                try:
                    ready = False
                    deadline = time.monotonic()+20
                    while child.poll() is None and time.monotonic()<deadline:
                        if 'ctox v4 store daemon ready:' in log_path.read_text():
                            ready = True
                            break
                        time.sleep(.05)
                    identity = component/'runtime/business-os-instance-id'
                    start.update(ready=ready,readyMs=(time.monotonic()-at)*1000,
                                 instanceId=identity.read_text().strip() if identity.exists() else None)
                finally:
                    try:
                        os.killpg(child.pid,signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(child.pid,signal.SIGKILL)
                        child.wait()
                    start['exit'] = child.returncode
                    try:
                        os.killpg(child.pid,0)
                        process['groupAbsent'] = False
                    except ProcessLookupError:
                        process['groupAbsent'] = True
        stable = all(x['ready'] and x['exit']==0 for x in starts) and bool(starts[0]['instanceId']) and starts[0]['instanceId']==starts[1]['instanceId']
        criterion('foreground-restart-component','pass' if stable else 'fail','DevOps',
                  measured=starts, limit='Candidate image on fresh empty state, not frozen-profile coverage, installed upgrade or signing-key witness')
        # This only probes admission capability. A failed sudo never launches
        # unshare or an installer, and no host user/system bus is contacted.
        capable = run('namespace-admission', ['sudo','-n','true'], 10) == 0
        if not capable:
            criterion('supported-update-rollback', 'not_run', 'gpu3 lane owner',
                      reason='Private systemd namespace requires approved setup; sudo -n admission failed. No host service fallback.')
        else:
            private = out/'upgrade-profile'
            private.mkdir(mode=0o700)
            (private/'.sync-v4-e2e.json').write_text(json.dumps(marker))
            code = run('sqlite-api-restore', ['python3',HERE/'m7-backup.py','restore','--output',snapshot,'--destination',private/'runtime'],180)
            assert code == 0, 'Snapshot restore failed'
            transition = out/'transition'
            code = run('supported-update-rollback', ['node',HERE/'upgrade-controller.mjs',args.before,args.release,private,transition,'yes'],660)
            proof = transition/'installer.json'
            details = json.loads(proof.read_text()) if proof.exists() else {}
            success = code == 0 and details.get('roundtripExecuted') and details.get('canonicalReleasePackage')
            criterion('supported-update-rollback', 'pass' if success else 'fail', 'DevOps',
                      evidence=str(proof), exit=code, dataReadback='not_run: separate frozen coverage required')
        for name, owner, reason in [
            ('base-import-daemon-attachment','phase1 / Architecture','Library-only E21 witness is not a productive import attached to this installed daemon.'),
            ('cutover-coverage','phase1','No cutover-coverage.json custody supplied for frozen WELSCH data.'),
            ('instance-key-restart','Architecture CUTOVER-4','Requires installed foreground public-identity witness; unit/library evidence is not relabeled.'),
            ('installed-get-root-p99','DevOps','Requires the completed installed foreground and its actual signed slot.'),
            ('app-hot-plug','Harness #868','Requires installed V4 runtime and real browser binding; fake core is not acceptance.'),
            ('workjet-project-list','Shell v4 / Workjet','not_run(shell-slot): no signed V4 artifact from main supplied.'),
            ('rights-authority','Architecture CUTOVER-5','Final productive rights/fence attachment not qualified in this candidate.'),
            ('signed-v4-slot','Shell v4','No signed V4 slot artifact from main supplied; beta.106 is only a V3 mechanics reference.'),
            ('main-release-delivery','ctox sync v4','Candidate is an isolated integration rehearsal; main milestone/full suite and publication are not executed.')]:
            criterion(name, 'not_run', owner, reason=reason)
    except Exception as error:
        result['error'] = str(error)
        criterion('rehearsal-execution','fail','DevOps',reason=str(error))
    finally:
        result['terminal'] = True
        result['finishedAtMs'] = int(time.time()*1000)
        result['go'] = bool(result['criteria']) and all(c['status']=='pass' for c in result['criteria'])
        save()
    print(json.dumps({k:v for k,v in result.items() if k not in ('commands','processes')}))
    return 0 if result['go'] else 1 if result.get('error') or any(c['status']=='fail' for c in result['criteria']) else 2


if __name__ == '__main__':
    raise SystemExit(main())
