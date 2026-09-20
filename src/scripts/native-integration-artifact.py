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
DEADLINE = time.monotonic() + 7200
TARGET = 'x86_64-unknown-linux-gnu'
FILTERS = ['coding_agents::pi_sidecar::', 'reply_capture::tests',
           'business_chat', 'repair_queue_projections',
           'mcp_app_authority', 'app_source_', 'gateway_managed_',
           'replicated_queue_command_persists_and_revalidates_native_authorization',
           'capability_epoch_revokes_tokens_after_role_or_grant_change',
           'person_research_binding_runtime_leads_preserve_identity_and_scope',
           'person_research_record_binding_accepts_nested_lead_data',
           'authenticated_automation_', 'outbound_runtime_library_',
           'outbound_scrape_test_',
           'research_control_revalidation_uses_native_actor_and_original_permission',
           'web_stack_auth_owner_resolution_',
           'web_stack_auth_assist_reuses_active_task_across_request_ids',
           'web_stack_generated_research_control_task_revalidates_persisted_native_owner',
           'outbound_custom_research_adapter_queues_universal_scraping_generation']
REQUIRED_RUNTIME_TESTS = {
    'authenticated_automation_stdin_is_bounded_and_command_specific',
    'authenticated_automation_ipc_preserves_source_and_auth_gate',
    'authenticated_automation_ipc_does_not_bypass_command_session_validation',
    'authenticated_automation_ipc_rejects_missing_oversize_and_misrouted_source',
    'authenticated_automation_cli_reader_forwards_source_to_daemon_socket',
    'authenticated_automation_public_dispatcher_forwards_source_to_daemon_socket',
    'outbound_runtime_library_preserves_activated_revision_over_bundle',
    'outbound_runtime_library_invalid_materialization_does_not_import_bundle',
    'outbound_runtime_library_novel_first_use_generates_then_executes_registered_script',
    'outbound_scrape_test_missing_identity_has_no_scrape_or_repair_effects',
    'outbound_scrape_test_uses_explicit_identity_instead_of_adapter_metadata',
    'outbound_scrape_test_contract_rejects_invalid_budget_and_drops_claimed_authority',
    'research_control_revalidation_uses_native_actor_and_original_permission',
    'web_stack_auth_owner_resolution_prefers_verified_task_over_flag_and_env',
    'web_stack_auth_owner_resolution_uses_command_authorization_chat_and_session',
    'web_stack_auth_assist_reuses_active_task_across_request_ids',
    'web_stack_generated_research_control_task_revalidates_persisted_native_owner',
}
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
    print(f'[stage] {name} started', flush=True)
    with log.open('w') as output:
        process = subprocess.Popen(args, cwd=ROOT, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            while True:
                try:
                    process.wait(timeout=min(30, max(1, DEADLINE - time.monotonic())))
                    break
                except subprocess.TimeoutExpired:
                    if time.monotonic() >= DEADLINE:
                        raise
                    stat = log.stat()
                    print(json.dumps({'stage': name, 'pid': process.pid,
                                      'log_bytes': stat.st_size,
                                      'log_idle_seconds': int(time.time() - stat.st_mtime)}),
                          flush=True)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            raise
    print(f'[stage] {name} completed exit={process.returncode}', flush=True)
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
    run('customer-identity', ['node',
        'src/apps/business-os/rxdb/tests/customer-identifier-inventory-smoke.mjs'])
    run('content-guard', ['node', 'src/apps/business-os/scripts/audit-business-os-content.mjs'])
    sync_tests = [
        'sync-native-read.test.mjs',
        'sync-collection-registry.test.mjs',
        'sync-contract.test.mjs',
        'sync-desktop-icon-replication.test.mjs',
        'sync-room-circuit.test.mjs',
    ]
    sync_output = run('shell-native-read-regressions', [
        'node', '--test', '--test-concurrency=1',
        *['src/apps/business-os/shared/' + name for name in sync_tests],
    ])
    for metric, expected in [('tests', 31), ('pass', 31), ('fail', 0), ('skipped', 0)]:
        if re.findall(r'^# ' + metric + r' (\d+)$', sync_output, re.MULTILINE) != [str(expected)]:
            raise RuntimeError(f'Unexpected shell native-read regression {metric} count')
    RECORD['shell_native_read_tests'] = 31
    save()
    compiled = run('test-compile', ['cargo', 'test', '--locked', '--release',
                   '--bin', 'ctox', '--target', TARGET, '--jobs', '2',
                   '--no-run', '--message-format=json'])
    executables = set()
    for line in compiled.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (event.get('reason') == 'compiler-artifact'
                and event.get('target', {}).get('name') == 'ctox'
                and event.get('profile', {}).get('test') is True
                and event.get('executable')):
            executables.add(event['executable'])
    if len(executables) != 1:
        raise RuntimeError('Expected exactly one compiled ctox test executable')
    executable = executables.pop()
    RECORD['test_executable'] = executable
    RECORD['test_executable_sha256'] = digest(Path(executable))
    command = [executable, *FILTERS]
    listing = run('test-list', command + ['--list'])
    names = re.findall(r'^(.+): test$', listing, re.MULTILINE)
    counts = {selector: sum(selector in name for name in names) for selector in FILTERS}
    if not all(counts.values()):
        raise RuntimeError(f'A required test group is absent: {counts}')
    if counts['mcp_app_authority'] < 14:
        raise RuntimeError(f"Missing managed-authority regressions: {counts}")
    required = {
        'coding_agents::pi_sidecar::tests::inherited_minimax_route_drives_real_pi_tools_through_native_bridge',
        'coding_agents::pi_sidecar::tests::responses_edit_owner_applies_only_complete_source_and_session',
        'coding_agents::pi_sidecar::tests::incomplete_failure_detail_only_preserves_allowlisted_enum_and_counts',
        'coding_agents::pi_sidecar::tests::module_source_is_complete_beyond_collection_and_module_page_limits',
        'coding_agents::pi_sidecar::tests::module_source_preserves_native_and_rxdb_version_precedence',
        'coding_agents::pi_sidecar::tests::module_source_rejects_invalid_or_ambiguous_snapshots_without_content',
        'coding_agents::pi_sidecar::tests::missing_module_source_stops_before_model_or_sidecar_and_session_write',
    }
    missing = required - set(names)
    if missing:
        raise RuntimeError(f'Required native Pi regressions are absent: {sorted(missing)}')
    runtime_counts = {test: sum(name.rsplit('::', 1)[-1] == test for name in names)
                      for test in REQUIRED_RUNTIME_TESTS}
    if any(count != 1 for count in runtime_counts.values()):
        raise RuntimeError(f'Required runtime regressions absent or ambiguous: {runtime_counts}')
    RECORD.update(discovered_tests=names, group_counts=counts,
                  required_runtime_tests=runtime_counts)
    save()
    output = run('native-tests', command + ['--test-threads=2'])
    summaries = re.findall(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;', output)
    if len(summaries) != 1 or int(summaries[0]) != len(names):
        raise RuntimeError('Test execution does not prove every discovered case passed')
    metadata = json.loads(capture(['cargo', 'metadata', '--locked', '--no-deps',
                                   '--format-version', '1']))
    target_dir = Path(metadata['target_directory'])
    RECORD['cargo_target_directory'] = str(target_dir)
    # Business OS directory requirements, on the same reviewed product source.
    run('cargo-check', ['cargo', 'check', '--locked', '--jobs', '2'])
    run('rxdb-native-tests', ['cargo', 'test', '--locked', '--manifest-path',
                             'src/core/rxdb/Cargo.toml', '--jobs', '2',
                             '--', '--test-threads=2'])
    run('browser-dependencies', ['npm', '--prefix', 'src/apps/business-os', 'ci'])
    run('browser-runtime', ['npm', '--prefix', 'src/apps/business-os', 'exec',
                            'playwright', 'install', '--with-deps', 'chromium'])
    run('shell-contract', ['node', 'src/apps/business-os/scripts/assert-shell-v2-contract.mjs'])
    startup = run('shell-startup-cache', ['node', '--test',
                  'src/apps/business-os/scripts/test-shell-window-cache-startup.mjs'])
    if not re.search(r'^# tests 2$', startup, re.MULTILINE) or not re.search(
            r'^# pass 2$', startup, re.MULTILINE):
        raise RuntimeError('Shell startup cache regressions did not both pass')
    geometry_dir = EVIDENCE / 'shell-geometry'
    run('shell-geometry', ['node', 'src/apps/business-os/scripts/shell-v2-geometry-lab.mjs',
                          '--apps', 'mail', '--widths', '1180,720',
                          '--output-dir', str(geometry_dir)])
    geometry = json.loads((geometry_dir / 'report.json').read_text())
    if len(geometry) != 2 or {(item['app'], item['width']) for item in geometry} != {
            ('mail', 1180), ('mail', 720)} or not all(item['ok'] for item in geometry):
        raise RuntimeError('Required Mail shell geometry cases did not all pass')
    RECORD['shell_geometry'] = geometry
    save()
    run('rxdb-wire-fixture', ['cargo', 'build', '--locked', '--manifest-path',
                             'src/core/rxdb/Cargo.toml', '--example',
                             'v15_wire_daemon', '--target-dir', str(target_dir),
                             '--jobs', '2'])
    os.environ['COMMAND_PLANE_BASELINE_PATH'] = str(EVIDENCE / 'command-plane.json')
    run('rxdb-js-tests', ['node', 'src/apps/business-os/rxdb/tests/run-all.mjs',
                         '--require-wire-daemon'])
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
