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
DEADLINE = time.monotonic() + 4800
TARGET = 'x86_64-unknown-linux-gnu'
FILTERS = ['coding_agents::pi_sidecar::', 'reply_capture::tests',
           'business_chat', 'repair_queue_projections',
           'business_os::guest_runtime::', 'business_os::guest_commands::tests',
           'business_os::command_plane::guest_command_tests',
           'mission::review::tests',
           'blocked_review_cannot_close_through_pass_or_no_send',
           'incomplete_plan_can_be_reviewed_without_fabricating_completed_steps',
           'durable_execution_progress_reserves_ten_percent_for_native_review',
           'service_owned_turn_cannot_persist_success_without_a_completed_plan',
           'current_path_new_revision_preserves_validated_bytes',
           'completed_empty_query', 'invalid_empty_query_receipts',
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
    allowed = {'.github/workflows/native-existing-verification.yml',
               'src/scripts/native-existing-verification.py'}
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
    expected_counts = {
        'current_path_new_revision_preserves_validated_bytes': 2,
        'service_owned_turn_cannot_persist_success_without_a_completed_plan': 1,
    }
    if any(counts[key] != count for key, count in expected_counts.items()):
        raise RuntimeError(f'Required exact test counts differ: {counts}')
    empty_cases = {
        'completed_empty_query_persists_current_receipt_without_fabricated_or_old_records',
        'invalid_empty_query_receipts_persist_failure_and_never_become_success',
        'outbound_completed_empty_query_preserves_extraction_test_failure',
        'outbound_completed_empty_query_rejects_missing_mismatched_and_tampered_evidence',
    }
    if not empty_cases.issubset({name.rsplit('::', 1)[-1] for name in names}):
        raise RuntimeError('Required completed-empty receipt cases are absent')
    guest_cases = {
        'opened_char_device_rdev_must_match_sysfs_dev',
        'udev_named_symlink_resolves_to_vport_identity',
        'udev_named_symlink_rejects_invalid_target',
        'started_frame_wait_on_nonblocking_pair_is_cancellable',
        'truncated_request_on_nonblocking_pair_hits_deadline_without_effect',
        'blocked_peer_write_on_nonblocking_pair_hits_deadline',
        'input_error_after_dispatch_is_unknown_and_cannot_be_reused_or_replayed',
        'pre_dispatch_invalid_input_is_failed_without_effect',
    }
    if not guest_cases.issubset({name.rsplit('::', 1)[-1] for name in names}):
        raise RuntimeError('Required guest channel cases are absent')
    if not all(counts.values()):
        raise RuntimeError(f'A required test group is absent: {counts}')
    required = {
        'business_os::mcp_channel::tests::person_research_binding_accepts_runtime_lead_name_fields',
        'business_os::mcp_channel::tests::person_research_binding_runtime_leads_preserve_identity_and_scope',
        'business_os::mcp_channel::app_authority::tests::mcp_app_authority_local_source_compare_and_save_preserves_target_and_conflicts',
        'business_os::mcp_channel::gateway_lifecycle_tests::healthy_rotations_never_sleep_or_accumulate_failure_backoff',
        'business_os::mcp_channel::gateway_lifecycle_tests::healthy_rotation_resets_saturated_failure_backoff',
        'business_os::mcp_channel::gateway_lifecycle_tests::stream_ends_and_errors_share_bounded_failure_backoff',
        'business_os::mcp_channel::gateway_lifecycle_tests::reconnect_disabled_returns_each_outcome_without_retry_or_sleep',
        'business_os::mcp_channel::gateway_lifecycle_tests::cancellation_drops_active_connection_after_rotation',
        'business_os::mcp_channel::gateway_lifecycle_tests::cancellation_drops_failure_backoff_without_another_attempt',
        'install::tests::state_backup_restores_encrypted_credentials_without_original_store',
        'install::tests::state_backup_rejects_master_key_symlink',
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
    RECORD.update(discovered_tests=names, group_counts=counts)
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
    run('rxdb-wire-fixture', ['cargo', 'build', '--locked', '--manifest-path',
                             'src/core/rxdb/Cargo.toml', '--example',
                             'v15_wire_daemon', '--target-dir', str(target_dir),
                             '--jobs', '2'])
    os.environ['COMMAND_PLANE_BASELINE_PATH'] = str(EVIDENCE / 'command-plane.json')
    run('rxdb-js-tests', ['node', 'src/apps/business-os/rxdb/tests/run-all.mjs',
                         '--require-wire-daemon'])
    RECORD['complete'] = True
    RECORD['scope'] = 'Native verification only; no release build or runtime artifact'
    save()


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        RECORD.update(complete=False, error=str(error))
        save()
        raise
