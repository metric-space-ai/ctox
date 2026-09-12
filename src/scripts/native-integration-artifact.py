#!/usr/bin/env python3
"""Bounded Linux integration proof and installer-compatible candidate bundle."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tarfile
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(os.environ['RUNNER_TEMP']) / 'native-integration'
EVIDENCE = OUT / 'evidence'
EVIDENCE.mkdir(parents=True, exist_ok=True)
DEADLINE = time.monotonic() + 4200
TARGET = 'x86_64-unknown-linux-gnu'
FILTERS = ['coding_agents::pi_sidecar::', 'reply_capture::tests',
           'business_chat', 'repair_queue_projections']
RECORD = {'stages': [], 'complete': False}


def save():
    (EVIDENCE / 'result.json').write_text(json.dumps(RECORD, indent=2) + '\n')


def capture(args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def run(name, args):
    log = EVIDENCE / (name + '.log')
    remaining = int(DEADLINE - time.monotonic())
    if remaining <= 0:
        raise TimeoutError('Shared validation deadline exceeded')
    with log.open('w') as output:
        process = subprocess.Popen(args, cwd=ROOT, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            process.wait(timeout=remaining)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            raise
    RECORD['stages'].append({'name': name, 'command': args, 'exit': process.returncode})
    save()
    if process.returncode:
        raise RuntimeError(f'{name} failed with exit {process.returncode}; see {log.name}')
    return log.read_text()


def digest(path):
    result = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            result.update(block)
    return result.hexdigest()


def main():
    revision = capture(['git', 'rev-parse', 'HEAD'])
    reviewed = os.environ['REVIEWED_SOURCE_REVISION']
    ancestors = {'reviewed_pi': reviewed,
                 'pr113': 'fe33119aa0f960a4501bc68c22cafee2e1799c51',
                 'pr114': '1715d255ccb2023f39fe75768175712bd1178288',
                 'pr121': 'debf7a22c98d79201d285e3db9aacb441a16a5a2',
                 'pr124': 'c0dc5df35c162bb850eab91a03cc04ab038df0e3'}
    for ancestor in ancestors.values():
        capture(['git', 'merge-base', '--is-ancestor', ancestor, revision])
    RECORD['verified_ancestors'] = ancestors
    changed = capture(['git', 'diff', '--name-only', reviewed, revision]).splitlines()
    allowed = {'.github/workflows/native-integration-artifact.yml',
               'src/scripts/native-integration-artifact.py'}
    if set(changed) - allowed:
        raise RuntimeError('Product source differs from the reviewed revision')
    if capture(['git', 'status', '--porcelain', '--untracked-files=no']):
        raise RuntimeError('Tracked source changed during prerequisite setup')
    sidecar = ROOT / 'src/core/coding_agents/pi-sidecar/dist/ctox-pi-sidecar.mjs'
    RECORD.update(revision=revision, reviewed_source_revision=reviewed,
                  target=TARGET, profile='release (repository defaults)',
                  rust=capture(['rustc', '-Vv']), node=capture(['node', '--version']),
                  sidecar_sha256=digest(sidecar),
                  sidecar_lock_sha256=digest(sidecar.parent.parent / 'package-lock.json'),
                  workflow_run=os.environ.get('GITHUB_RUN_ID'))
    save()
    command = ['cargo', 'test', '--locked', '--release', '--bin', 'ctox',
               '--target', TARGET, '--jobs', '2', '--', *FILTERS]
    listing = run('test-list', command + ['--list'])
    names = re.findall(r'^(.+): test$', listing, re.MULTILINE)
    counts = {selector: sum(selector in name for name in names) for selector in FILTERS}
    if not all(counts.values()):
        raise RuntimeError(f'A required test group is absent: {counts}')
    required = 'inherited_minimax_route_drives_real_pi_tools_through_native_bridge'
    if not any(required in name for name in names):
        raise RuntimeError('Required real-Pi/mock-provider regression is absent')
    RECORD.update(discovered_tests=names, group_counts=counts)
    save()
    output = run('native-tests', command + ['--test-threads=2'])
    summaries = re.findall(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;', output)
    if len(summaries) != 1 or int(summaries[0]) != len(names):
        raise RuntimeError('Test execution does not prove every discovered case passed')
    run('native-build', ['cargo', 'build', '--locked', '--release', '--bin', 'ctox',
                         '--target', TARGET, '--jobs', '2'])
    target_dir = Path(os.environ.get('CARGO_TARGET_DIR', 'target'))
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    binary = target_dir / TARGET / 'release/ctox'
    RECORD['binary_sha256'] = digest(binary)
    run('spawn-liveness', [str(binary), 'process-mining', 'spawn-liveness'])
    bundle = OUT / 'bundle'
    bundle.mkdir()
    # Export only tracked runtime source; never copy checkout dependencies or state.
    paths = ['install.sh', 'install.ps1', 'LICENSE', 'Cargo.toml', 'docs/legal/NOTICE',
             'contracts/source_origins_manifest.json', 'contracts/binary_bundle_manifest.txt',
             'src/apps/business-os', 'src/core/harness/Cargo.toml',
             'src/core/rxdb/Cargo.toml', 'src/core/rxdb/tools/local_signaling_server.js',
             'src/skills']
    archive = OUT / 'tracked-runtime.tar'
    subprocess.run(['git', 'archive', '--format=tar', '-o', str(archive), revision,
                    '--', *paths], cwd=ROOT, check=True)
    subprocess.run(['tar', '-xf', str(archive), '-C', str(bundle)], check=True)
    (bundle / 'bin').mkdir()
    shutil.copy2(binary, bundle / 'bin/ctox')
    shutil.copytree(bundle / 'src/skills', bundle / 'skills')
    shutil.copy2(bundle / 'docs/legal/NOTICE', bundle / 'NOTICE')
    shutil.copytree(bundle / 'src/apps/business-os/rxdb', bundle / 'rxdb-js/app-local',
                    ignore=shutil.ignore_patterns('node_modules'))
    for line in (ROOT / 'contracts/binary_bundle_manifest.txt').read_text().splitlines():
        path = line.strip()
        if path and not path.startswith('#') and not (bundle / path).exists():
            raise RuntimeError(f'Installer bundle manifest path missing: {path}')
    RECORD['complete'] = True
    save()
    (bundle / 'build-provenance.json').write_text(json.dumps(RECORD, indent=2) + '\n')
    artifacts = OUT / 'artifacts'
    artifacts.mkdir()
    packaged = artifacts / 'ctox-linux-x64.tar.gz'
    with tarfile.open(packaged, 'w:gz') as output:
        output.add(bundle, arcname='.')
    (artifacts / 'ctox-linux-x64.tar.gz.sha256').write_text(digest(packaged) + '  ctox-linux-x64.tar.gz\n')
    shutil.copy2(bundle / 'build-provenance.json', artifacts / 'build-provenance.json')
    RECORD['artifact_sha256'] = digest(packaged)
    save()


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        RECORD.update(complete=False, error=str(error))
        save()
        raise
