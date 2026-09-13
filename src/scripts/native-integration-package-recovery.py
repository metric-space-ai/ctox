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
import zipfile

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(os.environ['RUNNER_TEMP']) / 'native-integration'
EVIDENCE = OUT / 'evidence'
EVIDENCE.mkdir(parents=True, exist_ok=True)
DEADLINE = time.monotonic() + 3600
TARGET = 'x86_64-unknown-linux-gnu'
FILTERS = ['coding_agents::pi_sidecar::', 'reply_capture::tests',
           'business_chat', 'repair_queue_projections',
           'business_os::mcp_channel::app_authority::tests::',
           'business_os::mcp_channel::gateway_lifecycle_tests::',
           'business_os::mcp_channel::tests::gateway_',
           'business_os::mcp_channel::tests::person_research',
           'business_os::mcp_channel::tests::mcp_ignores_spoofed_context_role_without_gateway_trust',
           'install::tests::state_backup', 'install::tests::restore_state_backup',
           'install::tests::update_backup_retention', 'install::tests::aborted_updates']
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


VERIFIED_REVISION = '28136534d0c39e2f7b4c5853225b4777d20786c7'
PRODUCT = 'd99d146a82f561e36dc0445f38c07c7d73b17c17'
PRIOR_RUN = '34762882617'
EVIDENCE_DIGEST = '31c6c1a2d9c4828acbb69d30a402f747df9b28a28df26363be5aea77151a67e0'


def validate_receipt(prior, logs):
    if (prior['revision'] != VERIFIED_REVISION
            or prior['reviewed_source_revision'] != PRODUCT
            or prior['workflow_run'] != PRIOR_RUN):
        raise RuntimeError('Prior evidence source/run mismatch')
    expected_stages = ['test-compile', 'test-list', 'native-tests', 'cargo-check',
                       'rxdb-native-tests', 'browser-dependencies', 'browser-runtime',
                       'rxdb-wire-fixture', 'rxdb-js-tests']
    if ([stage['name'] for stage in prior['stages']] != expected_stages
            or any(stage['exit'] != 0 for stage in prior['stages'])):
        raise RuntimeError('Required prior verification stages did not pass')
    names = set(prior['discovered_tests'])
    passed = set(re.findall(r'^test (\S+) \.\.\. ok$', logs['native-tests.log'], re.M))
    if len(names) != 109 or names != passed or not all(prior['group_counts'].values()):
        raise RuntimeError('Prior native discovery/execution mismatch')
    if not re.search(r'test result: ok\. 109 passed; 0 failed; 0 ignored;', logs['native-tests.log']):
        raise RuntimeError('Prior native tests not all successful')
    summaries = re.findall(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;', logs['rxdb-native-tests.log'])
    if sum(map(int, summaries)) != 460:
        raise RuntimeError('Prior RxDB native evidence incomplete')
    js = logs['rxdb-js-tests.log']
    if ('129 passed, 0 failed, 0 skipped' not in js
            or not re.search(r'^PASS +cross-process-wire-smoke', js, re.M)
            or not re.search(r'^PASS +cross-process-file-fetch-smoke', js, re.M)):
        raise RuntimeError('Prior real wire/file-fetch JS evidence incomplete')


def main():
    revision = capture(['git', 'rev-parse', 'HEAD'])
    capture(['git', 'merge-base', '--is-ancestor', VERIFIED_REVISION, revision])
    changed = set(capture(['git', 'diff', '--no-ext-diff', '--name-only', VERIFIED_REVISION, revision]).splitlines())
    allowed = {'.github/workflows/native-integration-package-recovery.yml',
               'src/scripts/native-integration-package-recovery.py'}
    if changed - allowed or capture(['git', 'status', '--porcelain', '--untracked-files=no']):
        raise RuntimeError('Recovery changes verified product/build inputs')
    archive_path = Path(os.environ['RUNNER_TEMP']) / 'prior-native-evidence.zip'
    if digest(archive_path) != EVIDENCE_DIGEST:
        raise RuntimeError('Prior evidence archive digest mismatch')
    prior_dir = EVIDENCE / 'prior-verification'
    prior_dir.mkdir()
    logs = {}
    with zipfile.ZipFile(archive_path) as archive:
        if sum(entry.file_size for entry in archive.infolist()) > 20 * 1024 * 1024:
            raise RuntimeError('Prior evidence unexpectedly large')
        for entry in archive.infolist():
            if entry.is_dir() or Path(entry.filename).name != entry.filename:
                raise RuntimeError('Prior evidence must contain flat regular files')
            data = archive.read(entry)
            (prior_dir / entry.filename).write_bytes(data)
            logs[entry.filename] = data.decode('utf-8')
    prior = json.loads(logs['result.json'])
    validate_receipt(prior, logs)
    if (capture(['rustc', '-Vv']) != prior['rust']
            or capture(['node', '--version']) != prior['node']):
        raise RuntimeError('Recovery compiler or Node differs from verified build inputs')
    sidecar = ROOT / 'src/core/coding_agents/pi-sidecar/dist/ctox-pi-sidecar.mjs'
    lock = ROOT / 'src/core/coding_agents/pi-sidecar/package-lock.json'
    if digest(sidecar) != prior['sidecar_sha256'] or digest(lock) != prior['sidecar_lock_sha256']:
        raise RuntimeError('Recovery sidecar build inputs differ from verified source')
    RECORD.update(prior)
    RECORD.pop('error', None)
    RECORD.update(complete=False, revision=revision, workflow_run=os.environ['GITHUB_RUN_ID'],
                  verification_revision=VERIFIED_REVISION, reused_verification_run=PRIOR_RUN,
                  reused_evidence_sha256=EVIDENCE_DIGEST,
                  recovery_budget_seconds=3600)
    metadata = json.loads(capture(['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1']))
    target_dir = Path(metadata['target_directory'])
    RECORD['cargo_target_directory'] = str(target_dir)
    save()
    run('native-build', ['cargo', 'build', '--locked', '--release', '--bin', 'ctox',
                         '--target', TARGET, '--jobs', '2'])
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
