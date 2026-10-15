#!/usr/bin/env python3
"""Compile once, then run named native contracts; missing selections are errors."""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
PROOF = Path(os.environ['S9_PROOF_DIR'])
FILTERS = (
    'shell_update::tests::',
    'customer_module_static_assets_fail_closed_without_valid_binding',
    'shell_generation_guard_rejects_old_assets_without_mixing_releases',
    'shell_generation_assets_are_cached_without_blocking_revalidation',
    'crew_controls_enforce_roles_even_with_explicit_grants_and_replay_receipts',
    'cockpit_collections_are_founder_readable_but_user_denied',
)


def build_native(label):
    result = subprocess.run(['cargo', 'test', '--locked', '--bin', 'ctox', '--no-run', '--message-format=json'],
                            cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=1800)
    (PROOF / (label + '-build.jsonl')).write_bytes(result.stdout)
    (PROOF / (label + '-build.stderr')).write_bytes(result.stderr)
    artifacts = []
    for line in result.stdout.splitlines():
        try:
            item = json.loads(line)
        except ValueError:
            continue
        if result.returncode and item.get('reason') == 'compiler-message':
            print(item.get('message', {}).get('rendered', ''), file=sys.stderr)
        if item.get('reason') == 'compiler-artifact' and item.get('target', {}).get('name') == 'ctox' and item.get('profile', {}).get('test') and item.get('executable'):
            artifacts.append(item['executable'])
    if result.returncode:
        print(result.stderr.decode(errors='replace')[-12000:], file=sys.stderr)
        raise RuntimeError('native contract compilation failed')
    if len(set(artifacts)) != 1:
        raise RuntimeError('expected the actual ctox native test executable')
    return artifacts[0]


def run_selected(binary, filters):
    listing = subprocess.run([binary, '--list'], cwd=ROOT, capture_output=True, text=True, check=True, timeout=120).stdout
    (PROOF / 'native-contracts.list').write_text(listing)
    names = [line.removesuffix(': test') for line in listing.splitlines() if line.endswith(': test')]
    for name in filters:
        if not any(name in candidate for candidate in names):
            raise RuntimeError('selected contract does not exist: ' + name)
        result = subprocess.run([binary, name, '--test-threads=2'], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=300)
        (PROOF / (name + '.log')).write_bytes(result.stdout)
        if result.returncode:
            print(result.stdout.decode(errors='replace'), file=sys.stderr)
            raise RuntimeError('native contract failed: ' + name)


if __name__ == '__main__':
    label = sys.argv[1] if len(sys.argv) > 1 else 'native'
    run_selected(build_native(label), FILTERS)
    print('S9 native contract selections passed: ' + label)
