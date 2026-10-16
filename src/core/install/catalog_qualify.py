#!/usr/bin/env python3
"""Real installed-profile catalog generations, latency and causal mutations."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

assert os.environ.get('BUILD_LANE_BIN')
base = (Path(os.environ['BUILD_LANE_BIN']) / '../artifacts').resolve()
out = base / ('v4-m7-fix3-' + time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()))
out.mkdir(mode=0o700)
record = dict(owner='01a087a0-169a', output=str(out), startedAt=time.time(), terminal=False, steps=[], pass_=False)
record['pass'] = False
product = Path('src/core/business_os/store_sync_turn_auth.rs')
server = Path('src/core/business_os/server.rs')
gold = None


def sha(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(chunk)
    return value.hexdigest()


def save():
    (out / 'qualification.json').write_text(json.dumps(record, indent=2))


def run(name, command, expected=0, timeout=1200):
    started = time.time()
    with (out / (name + '.log')).open('w') as log:
        child = subprocess.Popen(command, stdout=log, stderr=log, start_new_session=True)
        row = dict(name=name, pid=child.pid, pgid=child.pid, stopAfterSeconds=timeout, command=command)
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
    row.update(exit=code, seconds=time.time()-started)
    save()
    print(name, 'exit', code, flush=True)
    if code != expected:
        raise RuntimeError(name + ' unexpected exit; see ' + str(out / (name + '.log')))


try:
    os.environ.update(CARGO_PROFILE_DEV_DEBUG='0', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2', CMAKE_BUILD_PARALLEL_LEVEL='2', RUST_TEST_THREADS='2')
    previous = base / 'v4-m7-fix2-qualified-20261016T020215Z'
    no_cache = previous / 'ctox-fixed'
    assert sha(no_cache) == '735f2bf0e2e574186a1213aaf855263aa72eb567441afb929754895c6bec22ef'
    # This archived binary is the exact pre-cache production, not a mock of
    # the catalog. Its failed cold gate remains an independent earlier witness.
    for path in [product, server]:
        assert subprocess.check_output(['git','show','c593e5dc8:'+str(path)]) == subprocess.check_output(['git','show','fb2f6ac1557e:'+str(path)])
    profile = previous / 'profile'
    origin = json.loads((profile / 'installed-profile-origin.json').read_text())
    assert origin['dataPreserved'] and origin['roundtripExecuted']
    record['profileOrigin'] = origin
    build = ['cargo','build','--locked','--no-default-features','--bin','ctox','-j','2']
    if len(sys.argv) > 1:
        reused = Path(sys.argv[1]).resolve()
        proof = json.loads((reused / 'qualification.json').read_text())
        for path in [product,server]:
            assert sha(path) == proof['sourceHashes'][str(path)]
        fixed, invalid = reused / 'ctox-fixed', reused / 'ctox-invalidated-mutation'
        assert sha(fixed) == proof['fixedSha256'] and sha(invalid) == proof['invalidatedMutationSha256']
        record['reuseBuild'] = str(reused)
        record['sourceHashes'] = proof['sourceHashes']
        gold = product.read_text()
    else:
        run('prepare',['bash','-lc','ctox-prep.sh'])
        run('format',['rustfmt','--edition','2021','--config','skip_children=true',str(product),str(server)])
        gold = product.read_text()
        record['sourceHashes'] = {str(path):sha(path) for path in [product,server,Path('src/core/business_os/shell_update.rs'),Path('src/core/install/mod.rs')]}
        for path in [product,server]:
            shutil.copy2(path, out / ('source-' + path.name))
        run('fixed-build',build)
        fixed = out / 'ctox-fixed'
        shutil.copy2(Path(os.environ['CARGO_TARGET_DIR']) / 'debug/ctox', fixed)
        needle = 'if *previous == generation {'
        assert gold.count(needle) == 1
        product.write_text(gold.replace(needle,'if true {'))
        record['mutation'] = dict(file=str(product), original=needle, changed='if true {', line=gold[:gold.index(needle)].count('\n')+1)
        run('invalidation-mutation-build',build)
        invalid = out / 'ctox-invalidated-mutation'
        shutil.copy2(Path(os.environ['CARGO_TARGET_DIR']) / 'debug/ctox', invalid)
        product.write_text(gold)
    record.update(fixedSha256=sha(fixed), invalidatedMutationSha256=sha(invalid), noCacheSha256=sha(no_cache))
    save()
    for name,binary,expected in [('fixed',fixed,0),('no-invalidation',invalid,1),('no-cache',no_cache,1),('restored',fixed,0)]:
        run(name,['python3','src/core/install/static_probe.py','--mode','plain','--catalog',
                  '--profile',str(profile),'--binary',str(binary),'--sha256',sha(binary),
                  '--source',str(base/'v4-e2e-m7-fb2-native-20261015/source-fb2f6ac1557e'),
                  '--slot',str(base/'v4-m7-fix-signed-reference-20261016/0.1.46-beta.106'),
                  '--out',str(out/name)], expected=expected, timeout=180)
        value = json.loads((out/name/'result.json').read_text())
        record[name] = value
        save()
        assert 'error' not in value and value['groupAbsent'] and value['inferiorsAbsent'] and value['signaling']['groupAbsent']
        assert len(value['rows']) == 20 and all(row.get('status')==200 and 'error' not in row for row in value['rows'])
        assert value['p99Ms'] is not None
        if name == 'no-invalidation':
            assert not value['catalogChanges']['modificationFresh'], 'Mutation must specifically retain the old manifest title'
        elif name == 'no-cache':
            assert not value['latencyPass'] and value['catalogChanges']['pass'], 'Only latency, not infrastructure or freshness, must fail'
        else:
            assert value['pass']
    record['pass'] = True
except Exception as error:
    record['error'] = str(error)
finally:
    if gold is not None:
        product.write_text(gold)
        record['sourceRestored'] = product.read_text() == gold
        if "fixed" in locals() and fixed.is_file():
            # Retire the mutable target output too; future owners must not
            # accidentally launch the invalidation mutant after source restore.
            destination = Path(os.environ["CARGO_TARGET_DIR"]) / "debug/ctox"
            shutil.copy2(fixed, destination)
            record["targetBinaryRestored"] = sha(destination) == sha(fixed)
    record.update(terminal=True, finishedAt=time.time())
    record.pop('pass_',None)
    save()
    print(json.dumps({k:record[k] for k in ['output','terminal','pass']}),flush=True)
raise SystemExit(0 if record['pass'] else 1)
