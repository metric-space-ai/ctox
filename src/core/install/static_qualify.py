#!/usr/bin/env python3
"""Qualify the static handler on a SQLite snapshot of the real installed fixture."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import time

assert os.environ.get('BUILD_LANE_BIN')
base = (Path(os.environ['BUILD_LANE_BIN']) / '../artifacts').resolve()
out = base / ('v4-m7-fix2-qualified-' + time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()))
out.mkdir(mode=0o700)
record = dict(startedAt=time.time(), terminal=False, pass_=False, output=str(out), steps=[])
record['pass'] = False


def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(chunk)
    return value.hexdigest()


def save():
    (out / 'qualification.json').write_text(json.dumps(record, indent=2))


def run(name, command, timeout=1200, expected=0):
    at = time.time()
    with (out / (name + '.log')).open('w') as log:
        result = subprocess.run(command, stdout=log, stderr=log, timeout=timeout)
    row = dict(name=name, exit=result.returncode, seconds=time.time()-at)
    record['steps'].append(row)
    save()
    print(json.dumps(row), flush=True)
    if result.returncode != expected:
        raise RuntimeError(name + ' failed; see ' + str(out / (name + '.log')))


try:
    # No credentials or customer data: this is the previously completed own
    # synthetic installation. Snapshot via SQLite, never a raw running DB copy.
    snapshot = r'''
import os,json,pathlib,sqlite3,tempfile,tarfile,sys,shutil,hashlib
root=pathlib.Path('/home/metricspace/build-lane/artifacts/v4-e2e-m7-fix-consume-20261016T011443Z/restored/run/tenant-4794af03db8c')
marker=json.loads((root/'.sync-v4-e2e.json').read_text())
assert marker['owner']=='01a087a0-169a-7e23-ba3d-71352256cbfb' and marker['synthetic'] is True
receipt=json.loads((root.parent/'result.json').read_text())
assert receipt['dataPreserved'] and receipt['installer']['roundtripExecuted'] and receipt['controller']['groupAbsent']
for proc in pathlib.Path('/proc').iterdir():
 if proc.name.isdigit() and int(proc.name)!=os.getpid():
  try: assert str(root).encode() not in (proc/'cmdline').read_bytes(), 'Fixture still in use: '+proc.name
  except (FileNotFoundError,PermissionError,ProcessLookupError): pass
with tempfile.TemporaryDirectory(prefix='m7-fix2-snapshot-',dir='/dev/shm') as scratch:
 dest=pathlib.Path(scratch)/'profile';dest.mkdir();(dest/'.sync-v4-e2e.json').write_text(json.dumps(marker))
 dbs=[]
 for p in sorted((root/'runtime').rglob('*')):
  rel=p.relative_to(root)
  if 'home' in rel.parts or 'business-os-shell' in rel.parts or p.is_symlink():continue
  if p.is_file() and p.name.endswith(('.sqlite3','.sqlite','.db')):
   target=dest/rel;target.parent.mkdir(parents=True,exist_ok=True)
   with sqlite3.connect('file:'+str(p)+'?mode=ro',uri=True) as src,sqlite3.connect(target) as db:
    src.backup(db);assert db.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
   dbs.append(str(rel))
  elif p.is_file() and 'local-modules' in rel.parts:
   target=dest/rel;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,target)
 (dest/'installed-profile-origin.json').write_text(json.dumps({'root':str(root),'dataPreserved':True,'roundtripExecuted':True,'databases':dbs,'mechanism':'SQLite backup API, source opened mode=ro; own terminal installed fixture'}))
 with tarfile.open(fileobj=sys.stdout.buffer,mode='w|gz') as archive:archive.add(dest,arcname='profile')
'''
    archive = out / 'installed-profile.tar.gz'
    with archive.open('wb') as target:
        result = subprocess.run(['ssh', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes',
                                 'metricspace@100.120.16.93', 'python3', '-'], input=snapshot.encode(), stdout=target,
                                stderr=subprocess.PIPE, timeout=90)
    if result.returncode:
        raise RuntimeError('Own installed snapshot failed: ' + result.stderr.decode()[-1500:])
    with tarfile.open(archive) as files:
        for member in files.getmembers():
            assert not member.issym() and not member.islnk() and not Path(member.name).is_absolute() and '..' not in Path(member.name).parts
        files.extractall(out)
    record['profileOrigin'] = json.loads((out / 'profile/installed-profile-origin.json').read_text())
    record['profileArchiveSha256'] = digest(archive)
    old = base / 'v4-e2e-m7-fix-build-20261016T004312Z'
    original = subprocess.check_output(['git', 'show', '66a3c6d3ab34d1170132a0b6307bc4b27e1a07d4:src/core/business_os/shell_update.rs'])
    current = Path('src/core/business_os/shell_update.rs').read_bytes()
    needle = b'    verify_slot(&path, Some(&slot))?;'
    replacement = (b'    // Admit once before the HTTP listener starts. The index handler reuses this\n'
                   b'    // verified inventory; every immutable asset read still checks its own bytes.\n'
                   b'    // Re-verifying the entire slot on the first GET / blocks before headers.\n'
                   b'    verified_release(root, slot)?.context("selected shell release is missing")?;')
    assert original.count(needle) == 1 and original.replace(needle, replacement) == current
    assert digest('src/core/install/mod.rs') == digest(old / 'source-mod.rs')
    assert subprocess.check_output(['git', 'show', 'fb2f6ac1557e:src/core/business_os/shell_update.rs']) == original
    record['mutation'] = dict(file='src/core/business_os/shell_update.rs', line=1052,
                              fixed=replacement.decode(), removedAdmission=needle.decode(),
                              baselineBinary=str(old / 'ctox-fixed'), baselineSha256=digest(old / 'ctox-fixed'),
                              sourceEquivalence='baseline shell_update bytes equal pinned 66a3c6d3a; installer bytes match immutable compiled capsule')
    os.environ.update(CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0',
                      CARGO_BUILD_JOBS='2', RUST_TEST_THREADS='2', CMAKE_BUILD_PARALLEL_LEVEL='2')
    run('prepare', ['bash', '-lc', 'ctox-prep.sh'])
    run('format', ['rustfmt', '--check', '--edition', '2021', '--config', 'skip_children=true', 'src/core/business_os/shell_update.rs'])
    run('build', ['cargo', 'build', '--locked', '--no-default-features', '--bin', 'ctox', '-j', '2'])
    candidate = out / 'ctox-fixed'
    shutil.copy2(Path(os.environ['CARGO_TARGET_DIR']) / 'debug/ctox', candidate)
    record['binarySha256'] = digest(candidate)
    record['sourceHashes'] = {p: digest(p) for p in ['src/core/business_os/shell_update.rs', 'src/core/install/mod.rs',
                                                    'src/core/install/static_probe.py', 'src/core/install/static_qualify.py']}
    save()
    # Existing signature/inventory/per-read corruption guards, not mock timing.
    run('slot-contracts', ['cargo', 'test', '--locked', '--no-default-features', '--bin', 'ctox', '-j', '2', 'business_os::shell_', '--', '--test-threads=2'])
    for name, binary in [('fixed', candidate), ('removed-admission', old / 'ctox-fixed'), ('restored', candidate)]:
        run(name, ['python3', 'src/core/install/static_probe.py', '--mode', 'plain', '--profile', str(out / 'profile'),
                   '--binary', str(binary), '--sha256', digest(binary),
                   '--source', str(base / 'v4-e2e-m7-fb2-native-20261015/source-fb2f6ac1557e'),
                   '--slot', str(base / 'v4-m7-fix-signed-reference-20261016/0.1.46-beta.106'),
                   '--out', str(out / name)], timeout=180, expected=1 if name == 'removed-admission' else 0)
        value = json.loads((out / name / 'result.json').read_text())
        record[name] = value
        save()
        assert value['pass'] is (name != 'removed-admission'), (name, value)
    record['pass'] = True
except Exception as error:
    record['error'] = str(error)
finally:
    record.update(terminal=True, finishedAt=time.time())
    record.pop('pass_', None)
    save()
    print(json.dumps({k:record[k] for k in ['output','terminal','pass']}, default=str), flush=True)
raise SystemExit(0 if record['pass'] else 1)
