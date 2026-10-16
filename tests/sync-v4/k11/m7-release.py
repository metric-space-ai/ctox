#!/usr/bin/env python3
"""Private M7 artifact using the canonical CLI release build/package contract.

This does not publish a GitHub release or certify the full release gate.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tarfile
import time


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def verify_package(report):
    """Verify actual compressed bytes, not just the unpacked producer tree."""
    if digest(Path(report['package'])) != report['packageSha256']:
        raise ValueError('Release package SHA256 differs')
    actual = {}
    with tarfile.open(report['package'], 'r:gz') as archive:
        for member in archive:
            path = Path(member.name)
            if path.is_absolute() or '..' in path.parts:
                raise ValueError('Release archive escapes its root')
            if member.isdir():
                continue
            if not member.isfile():
                raise ValueError('Release archive contains a link or special file')
            key = path.as_posix()
            if key in actual:
                raise ValueError('Duplicate release archive entry')
            value = hashlib.sha256()
            stream = archive.extractfile(member)
            for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                value.update(chunk)
            actual[key] = value.hexdigest()
    if actual != report['assets']:
        raise ValueError('Release archive asset inventory/content differs')
    if actual.get('bin/ctox') != report['binarySha256']:
        raise ValueError('Release archive binary differs from compiled image')
    return dict(verified=True, files=len(actual), packageSha256=report['packageSha256'])


def main():
    def interrupted(signum, _frame):
        raise RuntimeError('Owned release producer interrupted: '+str(signum))
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--reuse-build', type=Path)
    args = parser.parse_args()
    if os.uname().sysname != 'Linux' or not os.environ.get('BUILD_LANE_BIN'):
        raise SystemExit('Canonical Linux lane required')
    allowed = Path(os.environ['BUILD_LANE_BIN']).resolve().parent / 'artifacts'
    out = args.output.resolve()
    if allowed not in out.parents or out.exists():
        raise SystemExit('New private lane artifact directory required')
    out.mkdir(mode=0o700, parents=True)
    root = Path.cwd()
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    report = dict(schema='ctox.m7.release.v1', owner='01a087a0-169a-7e23-ba3d-71352256cbfb',
                  revision=revision, source=str(root), startedAtMs=int(time.time()*1000),
                  reference='.github/workflows/release.yml:693-751',
                  sourceTree=subprocess.check_output(['git','rev-parse','HEAD:src'], text=True).strip(),
                  productionPublished=False, fullReleaseGate=False, terminal=False, **{'pass': False})
    receipt = out / 'release.json'
    def save():
        receipt.write_text(json.dumps(report, indent=2)+'\n')
    save()
    try:
        env = {**os.environ, 'CARGO_BUILD_JOBS': '2', 'CMAKE_BUILD_PARALLEL_LEVEL': '2',
               'RUST_TEST_THREADS': '2', 'CTOX_BUILD_VERSION': 'native-m7-'+revision[:12]}
        command = ['cargo', 'build', '--locked', '--release', '--target', 'x86_64-unknown-linux-gnu', '-j', '2']
        report['command'] = command
        save()
        if args.reuse_build:
            prior = json.loads(args.reuse_build.read_text())
            if not prior.get('terminal') or not (prior.get('pass') or prior.get('pass_')) or prior.get('command') != command:
                raise ValueError('Terminal canonical release build required')
            src_tree = subprocess.check_output(['git','rev-parse',prior['revision']+':src'], text=True).strip()
            if src_tree != report['sourceTree'] or prior.get('buildExit') != 0:
                raise ValueError('Reused release product source differs')
            binary = Path(prior['binary'])
            if digest(binary) != prior['binarySha256']:
                raise ValueError('Reused release binary checksum differs')
            report['buildReuse'] = dict(receipt=str(args.reuse_build), sha256=digest(args.reuse_build), revision=prior['revision'])
            report['runnerRevision'] = revision
            report['revision'] = prior['revision']
            report['buildExit'] = 0
        else:
            with (out / 'build.log').open('w') as log:
                child = subprocess.Popen(command, env=env, stdout=log, stderr=log, start_new_session=True)
                report['process'] = dict(pid=child.pid, pgid=child.pid, stopAfterSeconds=4500)
                save()
                try:
                    result = child.wait(timeout=4500)
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid, signal.SIGTERM)
                        try:
                            child.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            os.killpg(child.pid, signal.SIGKILL)
                            child.wait()
                    try:
                        os.killpg(child.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    report['process']['terminal'] = True
            report['buildExit'] = result
            if result:
                raise RuntimeError('Canonical release build failed; see build.log')
            target = Path(env.get('CARGO_TARGET_DIR', 'target'))
            binary = target / 'x86_64-unknown-linux-gnu/release/ctox'
        bundle = out / 'bundle'
        bundle.mkdir()
        def copy(source, dest):
            dest.parent.mkdir(parents=True, exist_ok=True)
            if source.is_dir():
                shutil.copytree(source, dest, ignore=shutil.ignore_patterns('node_modules', '.git'))
            else:
                shutil.copy2(source, dest)
        copy(binary, bundle / 'bin/ctox')
        copy(root/'src/apps/business-os', bundle/'src/apps/business-os')
        for line in (root/'contracts/binary_bundle_manifest.txt').read_text().splitlines():
            line = line.strip()
            if not line or line.startswith('#'):
                continue
            path = Path(line)
            if path.is_absolute() or '..' in path.parts:
                raise ValueError('Unsafe bundle manifest path')
            if not (bundle/path).exists():
                copy(root/path, bundle/path)
        for source, destination in [('src/skills','skills'), ('install.sh','install.sh'),
                ('install.ps1','install.ps1'), ('LICENSE','LICENSE'), ('docs/legal/NOTICE','NOTICE'),
                ('src/core/transfers/aria2-rust/LICENSE','licenses/aria2-rust/LICENSE'),
                ('src/core/transfers/aria2-GPL-2.0.txt','licenses/aria2-rust/COPYING'),
                ('src/core/transfers/aria2-rust/PROVENANCE.json','licenses/aria2-rust/PROVENANCE.json'),
                ('src/core/transfers/vendor/smb/LICENSE.md','licenses/smb/LICENSE.md'),
                ('src/core/transfers/vendor/SMB-UPSTREAM.md','licenses/smb/UPSTREAM.md')]:
            if not (bundle/destination).exists():
                copy(root/source, bundle/destination)
        rxdb = root/'src/apps/business-os/rxdb'
        for relative in ('manifest.json','README.md','dist/ctox-rxdb-js.mjs'):
            copy(rxdb/relative, bundle/'rxdb-js/app-local'/relative)
        for directory in ('src','tests'):
            for path in (rxdb/directory).glob('*.mjs'):
                copy(path, bundle/'rxdb-js/app-local'/directory/path.name)
        report['binarySha256'] = digest(bundle/'bin/ctox')
        report['binary'] = str(bundle/'bin/ctox')
        report['assets'] = {p.relative_to(bundle).as_posix(): digest(p) for p in bundle.rglob('*') if p.is_file()}
        package = out/'ctox-linux-x64.tar.gz'
        with tarfile.open(package, 'w:gz') as archive:
            archive.add(bundle, arcname='.')
        report['package'] = str(package)
        report['packageSha256'] = digest(package)
        report['packageVerification'] = verify_package(report)
        (out/'ctox-linux-x64.tar.gz.sha256').write_text(report['packageSha256']+'  ctox-linux-x64.tar.gz\n')
        report['pass'] = True
    except Exception as error:
        report['error'] = str(error)
    finally:
        report['terminal'] = True
        report['finishedAtMs'] = int(time.time()*1000)
        save()
    print(json.dumps({key: value for key, value in report.items() if key != 'assets'}))
    return 0 if report['pass'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
