#!/usr/bin/env python3
"""Bounded, restoring mutation proofs for the S9 contract migrations."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import signal
from s9_native import build_native

def interrupted(signum, _frame):
    raise SystemExit(128 + signum)

signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)

root = Path(__file__).resolve().parents[2]
proof = Path(os.environ['S9_PROOF_DIR'])
mode = sys.argv[1]
if mode == 'schema':
    target = root / 'src/apps/business-os/modules/ctox/collections.schema.json'
    original = target.read_bytes()
    value = json.loads(original)
    op = next(op for op in value['migration_strategies']['workjet_computers']['1']['operations'] if op['field'] == 'device_binding_id')
    assert op['default'] == ''
    op['default'] = 'mutation-wrong-binding'
    mutated = (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode()
    command = ['node', 'src/apps/business-os/rxdb/tests/workjet-computer-migration-smoke.mjs']
    expected = b'AssertionError'
elif mode == 'reader':
    target = root / 'src/core/rxdb/tools/module_schema_artifact.mjs'
    original = target.read_bytes()
    old = b"  const document = JSON.parse(await readFile(url, 'utf8'));"
    assert original.count(old) == 1
    mutated = original.replace(old, b"  await import(new URL('schema.js', url));\n" + old)
    command = ['node', '--test', 'src/core/rxdb/tools/module_schema_artifact.test.mjs']
    expected = b'ERR_ASSERTION'
elif mode == 'inventory':
    target = root / 'src/core/business_os/shell_update.rs'
    original = target.read_bytes()
    old = b'const REQUIRED_SHELL_FILES: &[&str] = &["index.html"];'
    assert original.count(old) == 1
    mutated = original.replace(old, b'const REQUIRED_SHELL_FILES: &[&str] = &["index.html", "retired-bootstrap.mjs"];')
    command = None
    expected = b'signed_inventory_owns_the_shell_module_layout ... FAILED'
else:
    raise SystemExit('unknown mutation')
try:
    target.write_bytes(mutated)
    if mode == 'inventory':
        command = [build_native('inventory-mutant'), 'signed_inventory_owns_the_shell_module_layout', '--test-threads=2']
    result = subprocess.run(command, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=900)
    (proof / (mode + '-mutant.log')).write_bytes(result.stdout)
    assert result.returncode != 0, 'mutation survived'
    assert expected in result.stdout, 'not a functional assertion failure'
    assert b'error[E' not in result.stdout, 'compiler failure is not a mutation proof'
finally:
    target.write_bytes(original)
assert target.read_bytes() == original
if mode in ('schema', 'reader'):
    restored = subprocess.run(command, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
    (proof / (mode + '-restored.log')).write_bytes(restored.stdout)
    assert restored.returncode == 0, restored.stdout.decode(errors='replace')
(proof / (mode + '-mutation.json')).write_text(json.dumps({
    'source': str(target.relative_to(root)), 'source_sha256': hashlib.sha256(original).hexdigest(),
    'command': command, 'mutant_exit': result.returncode,
    'functional_failure': True, 'source_restored': True,
}, indent=2) + '\n')
print('S9 ' + mode + ' mutation rejected and source restored')
