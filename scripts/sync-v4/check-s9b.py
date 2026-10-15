#!/usr/bin/env python3
"""Bounded S9b source-authority proof; invoked only in the official lane."""
import ast
import hashlib
import json
import os
from pathlib import Path
import subprocess
import signal
from s9_native import build_native, run_selected

def interrupted(signum, _frame):
    raise SystemExit(128 + signum)

signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)

root = Path(__file__).resolve().parents[2]
proof = Path(os.environ['S9_PROOF_DIR'])
proof.mkdir(parents=True, exist_ok=True)
deletion_script = Path(os.environ['CARGO_TARGET_DIR']) / 's9b-acceptance-input/apply-s9.sh'
assert hashlib.sha256(deletion_script.read_bytes()).hexdigest() == 'f01e78e6d1f342cccdac80decc16291bc63352ba35decd39f98a7120650be2c8'
deletion = subprocess.run(['bash', str(deletion_script)], cwd=root, capture_output=True, timeout=180)
(proof / 's9-dry-run.log').write_bytes(deletion.stdout + deletion.stderr)
(proof / 's9-dry-run.json').write_text(json.dumps({'script_sha256': hashlib.sha256(deletion_script.read_bytes()).hexdigest(), 'args': [], 'exit': deletion.returncode, 'pass': deletion.returncode == 0}, indent=2) + '\n')
print('S9b: format, inventory and desktop backend contracts', flush=True)
files = [
    'src/core/business_os/store.rs',
    'src/core/business_os/store_sync_turn_auth.rs',
    'src/core/business_os/server.rs',
    'src/core/business_os/module_manifest_loader.rs',
    'src/core/business_os/module_lifecycle.rs',
]
subprocess.run(['rustfmt', '--edition', '2021', '--config', 'skip_children=true', *files], check=True)
(proof / 'formatted.patch').write_bytes(subprocess.check_output(['git', 'diff', '--', *files]))
for script in ('src/scripts/native-integration-artifact.py', 'scripts/sync-v4/check-s9b.py'):
    ast.parse((root / script).read_text())
subprocess.run(['node', '--check', 'scripts/sync-v4/e2e-run.mjs'], check=True)
for label, args in (
    ('desktop-backend', ['node', '--test', '--test-concurrency=2', 'src/apps/business-os-desktop/test/url-safety.test.cjs', 'src/apps/business-os-desktop/test/session-view.test.cjs', 'src/apps/business-os-desktop/test/managed-launch-refresh.test.cjs']),
    ('retained-sbom', ['npm', '--prefix', 'src/apps/business-os', 'run', 'test:shell-sbom']),
    ('critical-plan', ['python3', 'scripts/sync-v4/gate-critical.py', '--plan']),
    ('v4-command-selection', ['python3', 'scripts/sync-v4/gate-critical.py', '--scope', 'critical', '--only-file', 'src/apps/business-os/kernel/tests/command-state.test.mjs', '--evidence', str(proof / 'v4-command')]),
):
    result = subprocess.run(args, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (proof / (label + '.log')).write_bytes(result.stdout)
    if result.returncode:
        print(result.stdout.decode(errors='replace')[-16000:])
        raise SystemExit(result.returncode)
print('S9b: native source check', flush=True)
check = subprocess.run(['cargo', 'check', '--locked', '--bin', 'ctox'], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(proof / 'root-check.log').write_bytes(check.stdout)
if check.returncode:
    print(check.stdout.decode(errors='replace')[-16000:])
    raise SystemExit(check.returncode)
filters = [
    'packaged_and_runtime_apps_do_not_mint_native_authority',
    'customer_module_static_assets_fail_closed_without_valid_binding',
    'bootstrap_module_loader_uses_system_installed_and_local_sources_only',
    'app_store_install_uninstall_allows_explicit_module_grants',
    'module_update_legacy_manifest_uses_explicit_separate_module_roots',
    'source_load_requires_source_view_permission',
    'workspace_permission_grant_allows_user_upsert_without_admin_role',
    'app_shared_read_grant_requires_review_and_preserves_write_boundary',
    'business_os_policy_default_role_matrix_covers_major_permissions',
    'shell_update::tests::',
]
print('S9b: native catalog, permissions and signed inventory', flush=True)
binary = build_native('s9b-native')
run_selected(binary, filters)
# A functional mutation must fail the real catalog loader's authority assertion.
target = root / 'src/core/business_os/module_manifest_loader.rs'
original = target.read_bytes()
old = b'            manifest.core = false;'
assert original.count(old) == 1
try:
    target.write_bytes(original.replace(old, b'            manifest.core = true;'))
    mutant = build_native('s9b-authority-mutant')
    result = subprocess.run([mutant, filters[0], '--test-threads=2'], capture_output=True)
    output = result.stdout + result.stderr
    (proof / 'authority-mutant.log').write_bytes(output)
    assert result.returncode != 0 and b'app metadata must never mint native/platform authority' in output
finally:
    target.write_bytes(original)
(proof / 'authority-mutation.json').write_text(json.dumps({
    'source': str(target.relative_to(root)), 'source_sha256': hashlib.sha256(original).hexdigest(),
    'mutant_exit': result.returncode, 'functional_failure': True, 'source_restored': target.read_bytes() == original,
}, indent=2) + '\n')
run_selected(build_native('s9b-restored'), filters)
print('S9b native authority mutation red; restored catalog/policy contracts green')
for label, args in (
    ('native-rxdb', ['cargo', 'test', '--locked', '--manifest-path', 'src/core/rxdb/Cargo.toml', '--', '--test-threads=2']),
    ('wire-build', ['cargo', 'build', '--locked', '--manifest-path', 'src/core/rxdb/Cargo.toml', '--example', 'v15_wire_daemon']),
):
    result = subprocess.run(args, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (proof / (label + '.log')).write_bytes(result.stdout)
    if result.returncode:
        print(result.stdout.decode(errors='replace')[-16000:])
        raise SystemExit(result.returncode)
link = root / 'runtime/build/cargo-target'
link.parent.mkdir(parents=True, exist_ok=True)
if not link.exists():
    link.symlink_to(Path(os.environ['CARGO_TARGET_DIR']))
result = subprocess.run(['node', 'src/apps/business-os/rxdb/tests/run-all.mjs', '--require-wire-daemon'],
                        cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(proof / 'rxdb-js.log').write_bytes(result.stdout)
if result.returncode:
    print(result.stdout.decode(errors='replace')[-16000:])
raise SystemExit(result.returncode or deletion.returncode)
