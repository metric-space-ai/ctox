#!/usr/bin/env python3
"""Qualify the real E21 import library on the preserved, synthetic M7 profile.

Inputs are immutable source capsules with file custody. This is deliberately
not a claim that the daemon exposes a productive import command.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tarfile
import time

assert os.environ.get('BUILD_LANE_BIN'), 'Canonical Linux lane required'
base = (Path(os.environ['BUILD_LANE_BIN']) / '../artifacts').resolve()
capsule = Path(sys.argv[1]).resolve()
assert capsule.is_relative_to(base)
out = base / ('v4-m7-e21-import-' + time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()))
out.mkdir(mode=0o700)
record = dict(owner='01a087a0-169a', output=str(out), startedAt=time.time(),
              terminal=False, pass_=False, steps=[], daemonImportConnected=False,
              signedAppsPublished=False)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save():
    (out / 'qualification.json').write_text(json.dumps(record, indent=2))


def run(name, args, timeout=600):
    start = time.time()
    with (out / (name + '.log')).open('w') as log:
        child = subprocess.Popen(args, stdout=log, stderr=log, start_new_session=True)
        row = dict(name=name, command=args, pid=child.pid, pgid=child.pid,
                   stopAfterSeconds=timeout)
        record['steps'].append(row)
        save()
        try:
            code = child.wait(timeout=timeout)
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
    row.update(exit=code, seconds=time.time()-start)
    save()
    if code:
        raise RuntimeError(name + ' failed; see ' + str(out / (name + '.log')))


def extract(name, destination, custody):
    archive = capsule / (name + '.tar')
    assert sha(archive) == custody['sha256']
    expected = {row['path']: row['sha256'] for row in custody['files']}
    with tarfile.open(archive) as source:
        for entry in source:
            path = destination / entry.name
            assert not entry.name.startswith('/') and '..' not in Path(entry.name).parts
            assert entry.isfile() or entry.isdir(), 'No capsule links or special files'
            if entry.isfile():
                data = source.extractfile(entry).read()
                assert hashlib.sha256(data).hexdigest() == expected.pop(entry.name)
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
    assert not expected


try:
    os.environ.update(CARGO_PROFILE_DEV_DEBUG='0', CARGO_INCREMENTAL='0',
                      CARGO_BUILD_JOBS='2', RUST_TEST_THREADS='2')
    custody = json.loads((capsule / 'custody.json').read_text())
    record['custody'] = custody
    source, shell = out / 'source', out / 'shell-source'
    extract('store', source, custody['store'])
    extract('shell', shell, custody['shell'])
    old_inputs = out / 'previous-inputs'
    # This is our terminal synthetic fixture, not a tenant. Its source remains
    # immutable; SCP transfers only manifests/schemas and the owned probe.
    run('previous-input-custody', ['scp', '-q', '-r', '-oBatchMode=yes',
        '-oStrictHostKeyChecking=yes',
        'metricspace@100.120.16.93:/home/metricspace/build-lane/artifacts/v4-e2e-m7-fb2-base-input-20261015',
        str(old_inputs)], timeout=120)
    inputs = out / 'inputs'
    shutil.copytree(old_inputs, inputs)
    for name in ['manifests', 'schemas']:
        target = inputs / 'v4' / name
        probe = target / ('e2e-probe.manifest.json' if name == 'manifests' else 'e2e-probe.collections.schema.json')
        probe_bytes = probe.read_bytes()
        shutil.rmtree(target)
        shutil.copytree(shell / 'src/apps/business-os/v4' / name, target)
        probe.write_bytes(probe_bytes)
    shutil.copy2(shell / 'src/apps/business-os/v4/tools/manifest/out/ownership.json',
                 inputs / 'v4/tools/manifest/out/ownership.json')
    libraries = [json.loads(p.read_text()) for p in (inputs / 'v4/manifests').glob('lib-*.manifest.json')]
    assert len(libraries) == 2 and all(p['kind'] == 'lib' for p in libraries)
    record['libraries'] = [p['app_id'] for p in libraries]
    record['inputHashes'] = {str(p.relative_to(inputs)): sha(p)
                             for p in sorted(inputs.rglob("*")) if p.is_file()}
    probe = capsule / 'm7-import.rs'
    assert sha(probe) == custody['probe']['sha256']
    crate = source / 'src/core/store'
    shutil.copy2(probe, crate / 'm7-import.rs')
    # Only add an executable harness to the copied, otherwise untouched crate.
    with (crate / 'Cargo.toml').open('a') as f:
        f.write('\n[[bin]]\nname="m7-import-profile"\npath="m7-import.rs"\n')
    run('build', ['cargo', 'build', '--manifest-path', str(crate / 'Cargo.toml'),
                  '--bin', 'm7-import-profile', '-j', '2'])
    binary = out / 'm7-import-profile'
    shutil.copy2(Path(os.environ['CARGO_TARGET_DIR']) / 'debug/m7-import-profile', binary)
    record['binarySha256'] = sha(binary)
    profile = base / 'v4-m7-fix2-qualified-20261016T020215Z/profile'
    origin = json.loads((profile / 'installed-profile-origin.json').read_text())
    assert origin['dataPreserved'] and origin['roundtripExecuted']
    record['profileOrigin'] = origin
    run('import', [str(binary), str(profile / 'runtime'), str(inputs),
                   str(out / 'import.sqlite3'), str(out / 'import.json')], timeout=180)
    result = json.loads((out / 'import.json').read_text())
    assert result['pass'] and result['source_writes'] == 0
    assert not any(name.startswith("lib:") for name in result["activated_schema_apps"])
    first = next(row for row in result["legacy"] if not row["replay"])
    assert {row["collection"]: row["exact_documents_compared"]
            for row in first["collections"]} == {"e2e_leads": 40, "e2e_history": 100}
    assert sum(row["payload_free_tombstones"] for row in first["collections"]) == 80
    assert all(row['head_before'] == row['head_after'] for row in result['legacy'] if row['replay'])
    record.update(result=result, pass_=True)
except Exception as error:
    record['error'] = str(error)
finally:
    record.update(terminal=True, finishedAt=time.time())
    record['pass'] = record.pop('pass_')
    save()
    print(json.dumps({k: record[k] for k in ['output', 'terminal', 'pass']}))
raise SystemExit(0 if record['pass'] else 1)
