#!/usr/bin/env python3
"""Bounded real rollback/mutation/forward rehearsal, canonical Linux lane only."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

assert sys.platform == 'linux' and os.getenv('BUILD_LANE_BIN')
source = Path.cwd()
base = (Path(os.environ['BUILD_LANE_BIN']) / '../artifacts').resolve()
out = base / ('v4-m7-fix-' + time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()))
out.mkdir(mode=0o700)
fixture = base / 'v4-m7-fix-fixture-20261016'
old = base / 'v4-e2e-old-portable-20261015'
record = {'startedAt': time.time(), 'source': str(source), 'steps': [], 'terminal': False, 'pass': False}
env = dict(os.environ, CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2', RUST_TEST_THREADS='2', CMAKE_BUILD_PARALLEL_LEVEL='2')

def save():
    (out / 'qualification.json').write_text(json.dumps(record, indent=2))

def run(name, command, expected=0, timeout=2400):
    at = time.time()
    with (out / (name + '.log')).open('w') as log:
        p = subprocess.run(command, env=env, stdout=log, stderr=log, timeout=timeout)
    record['steps'].append({'name': name, 'command': command, 'exit': p.returncode, 'seconds': time.time()-at})
    save()
    print(name, 'exit', p.returncode, flush=True)
    if expected is not None and p.returncode != expected:
        print((out / (name+'.log')).read_text()[-14000:], flush=True)
        raise RuntimeError(name + ' unexpected exit ' + str(p.returncode))
    return p.returncode

def receipt(name):
    binary = out / ('ctox-' + name)
    shutil.copy2(Path(env['CARGO_TARGET_DIR']) / 'debug/ctox', binary)
    value = {'revision': 'fb2f6ac1557e5ea7879f1cae4d899b416a296d02', 'source': str(source), 'sourceModified': True,
             'qualificationVariant': name, 'terminal': True, 'pass': True, 'binary': str(binary),
             'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
             'sourceHashes': {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [source/'src/core/install/mod.rs', source/'src/core/install/rollback_tests.rs']}}
    p = out / (name+'-build.json');p.write_text(json.dumps(value, indent=2));return str(p)

product = source / 'src/core/install/mod.rs'
gold = product.read_text()
build = ['cargo', 'build', '--locked', '--bin', 'ctox', '--no-default-features', '-j', '2']
try:
    run('fixture-custody', ['node', str(fixture/'scripts/sync-v4/e2e-import.mjs'), str(old)])
    run('runner-syntax', ['node', '--check', 'src/core/install/rollback_rehearsal.mjs'])
    run('test-format', ['rustfmt', '--check', '--edition', '2021', 'src/core/install/rollback_tests.rs'])
    run('prepare', ['bash', '-lc', 'ctox-prep.sh'])
    run('candidate-build', build, timeout=2700)
    candidate = receipt('fixed')
    # Real failed restore remains non-zero, while a valid restore retains the lease.
    run('restore-invariant', ['cargo', 'test', '--locked', '--no-default-features', '--bin', 'ctox', '-j', '2',
        'install::rollback_tests::rollback_restore_keeps_live_lease_and_restores_tenant_data', '--', '--test-threads=1'], timeout=1800)
    args = ['node', 'src/core/install/rollback_rehearsal.mjs', 'rollback', str(fixture), str(old/'build.json')]
    run('rollback-fixed', args+[candidate,str(out/'fixed')], timeout=660)
    record['fixed'] = json.loads((out/'fixed/verdict.json').read_text());save()
    # Causal product mutation: restore the old control DB and remove the sidecar exemption.
    begin = gold.index('    // The operation\'s live control store')
    end = gold.index('        path.file_name()', begin)
    mutant = gold[:begin] + '    copy_filtered(backup_root, state_root, &|path, _| {\n' + gold[end:]
    needle = '        if database == maintenance {\n            continue;\n        }\n'
    assert mutant.count(needle) == 1
    mutant = mutant.replace(needle, '')
    product.write_text(mutant)
    run('mutation-build', build, timeout=1200)
    mutated = receipt('mutated')
    run('rollback-mutated', args+[mutated,str(out/'mutated')], expected=1, timeout=660)
    failure = json.loads((out/'mutated/run/result.json').read_text())
    log = '\n'.join(p.read_text() for p in (out/'mutated/run/upgrade').glob('installer-*.log'))
    assert 'CTOX maintenance lease changed while upgrade was running' in log, 'mutation failed for an unrelated reason'
    state = json.loads((Path(failure['root'])/'runtime/update_state.json').read_text())
    active = (Path(failure['root'])/'managed/current/bin/ctox').resolve()
    old_hash = json.loads((old/'build.json').read_text())['binarySha256']
    assert state['phase']=='rolled_back' and hashlib.sha256(active.read_bytes()).hexdigest()==old_hash
    record['mutation'] = {'red': True, 'phase': state['phase'], 'activeOldSha256': old_hash,
                          'error': 'CTOX maintenance lease changed while upgrade was running'};save()
    product.write_text(gold)
    # The initial successful candidate is immutable; no need to relink the same bytes.
    run('rollback-restored', args+[candidate,str(out/'restored')], timeout=660)
    record['restored'] = json.loads((out/'restored/verdict.json').read_text());save()
    # The historical signed slot is a private fixture, never activated on a tenant.
    static = ['node', 'src/core/install/rollback_rehearsal.mjs', 'static', str(fixture),
              str(out/'restored/run/result.json'), str(out/'static'),
              str(base/'v4-m7-fix-signed-reference-20261016/0.1.46-beta.106')]
    static_exit=run('static-http', static, expected=None, timeout=190)
    record['staticExit'] = static_exit
    if (out/'static/static.json').exists():record['static']=json.loads((out/'static/static.json').read_text())
    record['pass'] = record['fixed']['pass'] and record['restored']['pass'] and record['mutation']['red']
except Exception as error:
    record['error'] = str(error)
    raise
finally:
    product.write_text(gold)
    record['sourceRestored'] = product.read_text()==gold
    record['terminal'] = True;record['finishedAt'] = time.time();save()
    print(json.dumps({'output':str(out),'pass':record['pass'],'error':record.get('error'),'staticExit':record.get('staticExit')}),flush=True)
