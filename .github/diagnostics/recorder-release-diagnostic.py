#!/usr/bin/env python3
"""One no-build recorder diagnosis; never release acceptance or deployment."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tarfile
import time
import zipfile

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(os.environ['RUNNER_TEMP']) / 'recorder-release-diagnostic'
OUT.mkdir(parents=True, exist_ok=True)
ZIP_SHA = '153e75da321e2ad018fba344bb38afc580ec1a610da4b84ce34b1be0aea4926f'
TAR_SHA = '1c2b4adff565fbfd84bfeaa68c76a34f3e0e782033b8934ff866ba2fc9ecc964'
BIN_SHA = 'e59a95337af6e5290f0564bc729099e2c3b1f6b4b3e0c7c6ca57f0482531f571'

def run(*args, **kwargs):
    return subprocess.check_output(args, text=True, timeout=60, **kwargs).strip()

def sha(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()

def save(name, value):
    (OUT / name).write_text(json.dumps(value, indent=2) + '\n')

def main():
    assert os.environ.get('GITHUB_EVENT_NAME') == 'workflow_dispatch'
    assert os.environ.get('GITHUB_REPOSITORY') == 'metric-space-ai/ctox'
    assert shutil.disk_usage(OUT).free >= 8 * 1024**3, 'less than8GiB free'
    assert os.getloadavg()[0] <= os.cpu_count(), 'runner overloaded'
    perf = Path(os.environ['DIAGNOSTIC_PERF']).resolve(strict=True)
    chrome = Path('/usr/bin/google-chrome').resolve(strict=True)
    assert shutil.which('node') and shutil.which('gh'), 'missing node/gh'
    run('node', '-e', "require('./src/apps/business-os/node_modules/playwright')", cwd=ROOT)
    save('preflight.json', {
        'scope': 'different-release-build-profile-diagnosis-only',
        'release_acceptance': False, 'attempt_limit': 1,
        'source_sha': run('git', 'rev-parse', 'HEAD', cwd=ROOT),
        'built_revision': '8f369b2ef665e2f3f85fa5953f7af5206800abcf',
        'perf_path': str(perf), 'perf_sha256': sha(perf),
        'perf_version': run(str(perf), '--version'),
        'perf_package': run('dpkg-query', '-S', str(perf)),
        'packages': run('dpkg-query', '-W', '-f=${Package} ${Version}\n', 'linux-tools*'),
        'kernel': run('uname', '-a'), 'chrome': run(str(chrome), '--version'),
        'node': run('node', '--version'), 'free_bytes': shutil.disk_usage(OUT).free,
        'load': os.getloadavg(), 'cpu_count': os.cpu_count(),
    })
    archive = OUT / 'candidate.zip'
    with archive.open('wb') as target:
        subprocess.run(['gh', 'api', 'repos/metric-space-ai/ctox/actions/artifacts/10620151316/zip'],
                       stdout=target, check=True, timeout=120)
    assert sha(archive) == ZIP_SHA, 'artifact ZIP identity mismatch'
    package = OUT / 'ctox-linux-x64.tar.gz'
    with zipfile.ZipFile(archive) as z:
        matches = [n for n in z.namelist() if n.endswith('ctox-linux-x64.tar.gz')]
        assert len(matches) == 1, 'ambiguous package'
        with z.open(matches[0]) as source, package.open('wb') as dest:
            shutil.copyfileobj(source, dest)
    assert sha(package) == TAR_SHA, 'package identity mismatch'
    binary = OUT / 'ctox'
    with tarfile.open(package) as t:
        member = t.getmember('./bin/ctox')
        assert member.isfile(), 'binary must be regular file'
        with t.extractfile(member) as source, binary.open('wb') as dest:
            shutil.copyfileobj(source, dest)
    assert sha(binary) == BIN_SHA, 'binary identity mismatch'
    binary.chmod(0o700)
    patch = ROOT / '.github/diagnostics/recorder-start-diagnostics.patch'
    assert sha(patch) == 'e262f95c1d26d683005debe84769c3e4c44d29970a4e80d8ee3bd19b7a7aad17'
    run('git', 'apply', '--check', str(patch), cwd=ROOT)
    run('git', 'apply', str(patch), cwd=ROOT)
    scripts = ['.github/diagnostics/recorder-release-diagnostic.py',
               'src/core/rxdb/tools/browser_rust_smoke.js',
               'src/core/rxdb/tools/native_symbol_profile.js',
               'src/core/rxdb/tools/native_cpu_profile.js',
               'src/apps/business-os/package-lock.json']
    save('identities.json', {'binary_sha256': sha(binary), 'zip_sha256': ZIP_SHA,
                           'package_sha256': TAR_SHA,
                           'scripts': {p: sha(ROOT / p) for p in scripts}})
    env = dict(os.environ, CTOX_BIN=str(binary),
               SMOKE_MODE='business-os-threads-rightclick-ui', SMOKE_PAGE_PATH='/index.html',
               BUSINESS_PORT='8882', SIGNALING_PORT='18881',
               SMOKE_PROCESS_LIFECYCLE_PATH=str(OUT / 'context-processes.json'),
               PLAYWRIGHT_CHROMIUM_EXECUTABLE=str(chrome),
               CARGO_BUILD_JOBS='2', RUST_TEST_THREADS='2', RAYON_NUM_THREADS='2')
    # Token is needed only for immutable artifact download, never by the fixture.
    env.pop('GH_TOKEN', None)
    started = time.time()
    with (OUT / 'fixture.log').open('wb') as log:
        child = subprocess.Popen(['node', 'src/core/rxdb/tools/browser_rust_smoke.js',
                                  '--native-symbol-perf=' + str(perf)],
                                 cwd=ROOT, env=env, stdout=log, stderr=log, start_new_session=True)
        save('attempt.json', {'pid': child.pid, 'pgid': child.pid, 'owner': os.environ['GITHUB_RUN_ID'],
                              'started_at': started, 'attempt': 1, 'stop_after_seconds': 480})
        try:
            status = child.wait(timeout=480)
        finally:
            # Stop only the fixture's own process group, including stranded children.
            try:
                os.killpg(child.pid, signal.SIGINT)
                time.sleep(2)
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait(timeout=20)
    profiles = sorted(OUT.glob('context-processes.native-symbols-*.json'))
    assert len(profiles) == 1, 'exactly one native recorder profile required'
    profile = json.loads(profiles[0].read_text())
    inventories = sorted(OUT.glob('context-processes.native-cpu-*.jsonl'))
    assert len(inventories) == 1, 'missing bounded thread inventories'
    samples = [json.loads(line) for line in inventories[0].read_text().splitlines()]
    assert isinstance(profile.get('startedAtMs'), (int, float)), 'missing recorder start timestamp'
    assert isinstance(profile.get('recordStoppedAtMs'), (int, float)), 'missing recorder stop timestamp'
    before = [s for s in samples if s.get('kind') == 'sample' and s['atMs'] <= profile['startedAtMs']]
    after = [s for s in samples if s.get('kind') == 'sample' and s['atMs'] >= profile['recordStoppedAtMs']]
    save('before-after-threads.json', {'before': before[-1:] or None, 'after': after[:1] or None})
    save('result.json', {'fixture_exit': status, 'elapsed_seconds': time.time() - started,
                        'profile': profile, 'release_acceptance': False,
                        'different_build_profile': True, 'attempts': 1})
    assert status == 0, f'fixture failed{status}'
    assert profile.get('available') is True and profile.get('reason') == 'sampled', 'recording unavailable'
    assert before and after, 'missing before/after thread observation'

if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        save('failure.json', {'type': type(error).__name__, 'message': str(error), 'release_acceptance': False})
        raise
