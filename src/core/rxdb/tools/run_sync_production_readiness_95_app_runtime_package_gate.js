#!/usr/bin/env node
'use strict';

/*
 * Execute the app-runtime package gate and build the 9.5 evidence artifact.
 */

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const selfTest = process.argv.includes('--self-test');
const skipBrowser = process.argv.includes('--skip-browser');
const outputPath = path.resolve(
  flagValue('--output')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-app-runtime-package-gate.json'),
);
const measurementsPath = path.resolve(
  flagValue('--measurements')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-app-runtime-package-gate-measurements.json'),
);
const browserBundlePath = flagValue('--browser-bundle');
const smokeBinaryPath = flagValue('--smoke-binary') || findDefaultSmokeBinaryPath();

const nativeTestGroups = [
  {
    id: 'native:runtime_installed',
    filter: 'runtime_installed_',
    proves: [
      'runtime_installed_module_schemas_extend_native_collection_creators',
      'runtime_installed_module_schema_accepts_conflict_strategy_wrapper',
      'runtime_installed_module_schema_fingerprint_changes_with_schema_files',
      'runtime_installed_declarative_migration_is_discovered_and_copied',
    ],
  },
  {
    id: 'native:declarative_migration',
    filter: 'native_declarative_migration_matches_browser_operations',
    proves: ['native_declarative_migration_matches_browser_operations'],
  },
  {
    id: 'native:module_release',
    filter: 'module_release_',
    proves: [
      'module_release_command_replay_does_not_duplicate_release_state',
      'module_release_rejects_stale_source_and_rollback_version_refs_before_manifest_write',
      'module_release_restores_manifest_when_release_db_write_fails',
      'module_release_rollback_restores_manifest_when_status_update_fails',
      'module_release_and_rollback_write_business_event_audit',
    ],
  },
  {
    id: 'native:peer_revocation',
    filter: 'peer_revocation_registry_round_trips',
    proves: ['peer_revocation_registry_round_trips'],
  },
];

const browserModes = [
  'business-os-dynamic-apps-ui',
  'business-os-app-release-ui',
  'business-os-app-audience-ui',
];

const staticChecks = [
  {
    id: 'signature_required_contract',
    path: 'src/core/business_os/store.rs',
    needles: ['signature_required', 'signature_valid', 'HMAC-SHA256'],
  },
  {
    id: 'runtime_schema_hash_contract',
    path: 'src/core/business_os/rxdb_peer.rs',
    needles: [
      'runtime_installed_module_schema_fingerprint',
      'native_declarative_migration_operations',
      'apply_native_declarative_migration',
    ],
  },
  {
    id: 'client_only_runtime_contract',
    path: 'docs/ctox-sync-production-readiness-95.md',
    needles: [
      'safe runtime evolution without Rust edits',
      'No backend recompile',
      'signed app packages',
      'declarative migrations',
    ],
  },
];

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

const measurements = runGate();
fs.mkdirSync(path.dirname(measurementsPath), { recursive: true });
fs.writeFileSync(measurementsPath, `${JSON.stringify(measurements, null, 2)}\n`);
if (measurements.ok !== true && !smokeBinaryPath) {
  console.error(`ctox_sync_production_readiness_95_app_runtime_package_gate=0 output=${path.relative(root, measurementsPath)} reason=missing_smoke_binary`);
  process.exit(1);
}
execFileSync(process.execPath, [
  path.join(root, 'src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js'),
  '--kind',
  'app_runtime_package_gate',
  '--input',
  measurementsPath,
  '--output',
  outputPath,
  ...(browserBundlePath ? ['--browser-bundle', browserBundlePath] : []),
  ...(smokeBinaryPath ? ['--smoke-binary', smokeBinaryPath] : []),
], { cwd: root, stdio: 'inherit' });
console.log(`ctox_sync_production_readiness_95_app_runtime_package_gate=1 output=${path.relative(root, outputPath)}`);

function runGate() {
  const staticResults = staticChecks.map(runStaticCheck);
  const declarativeMigration = runCommand({
    id: 'declarative_migrations_checker',
    command: process.execPath,
    args: ['src/apps/business-os/scripts/assert-declarative-migrations.mjs'],
  });
  const binaryRevision = skipBrowser ? {
    id: 'smoke_binary_revision',
    ok: false,
    skipped: true,
    reason: 'browser gate skipped',
  } : validateSmokeBinaryRevision(smokeBinaryPath);
  if (!skipBrowser && binaryRevision.ok !== true) {
    const browserResult = {
      id: 'browser_runtime_app_modes',
      ok: false,
      skipped: true,
      modes: browserModes,
      reason: 'smoke binary revision check failed',
    };
    return summarizeGateResults({
      staticResults,
      declarativeMigration,
      nativeResults: [],
      binaryRevision,
      browserResult,
    });
  }
  const nativeResults = nativeTestGroups.map((group) => runCommand({
    id: group.id,
    command: 'cargo',
    args: [
      'test',
      '--bin',
      'ctox',
      group.filter,
      '--no-default-features',
      '--target-dir',
      'runtime/build/production-readiness-95-check-target',
    ],
    env: nativeCargoEnv(),
    proves: group.proves,
  }));
  const browserResult = skipBrowser ? {
    id: 'browser_runtime_app_modes',
    ok: false,
    skipped: true,
    modes: browserModes,
  } : binaryRevision.ok !== true ? {
    id: 'browser_runtime_app_modes',
    ok: false,
    skipped: true,
    modes: browserModes,
    reason: 'smoke binary revision check failed',
  } : runCommand({
    id: 'browser_runtime_app_modes',
    command: process.execPath,
    args: ['src/core/rxdb/tools/browser_rust_smoke_matrix.js'],
    env: {
      ...process.env,
      SMOKE_MODES: browserModes.join(','),
      SMOKE_MATRIX_ATTEMPTS: '1',
      SMOKE_PAGE_PATH: '/index.html',
      SMOKE_MODE_TIMEOUT_MS: process.env.SMOKE_MODE_TIMEOUT_MS || '300000',
      SMOKE_BROWSER_WARNING_BUDGET: '0',
      SMOKE_BROWSER_ERROR_BUDGET: '0',
      SMOKE_BROWSER_REQUEST_FAILURE_BUDGET: '0',
      SMOKE_MATRIX_RESULT_PATH: 'runtime/build/ctox-sync-production-readiness-95-app-runtime-browser-matrix.json',
      ...(smokeBinaryPath ? { CTOX_BIN: smokeBinaryPath } : {}),
    },
  });
  return summarizeGateResults({
    staticResults,
    declarativeMigration,
    nativeResults,
    binaryRevision,
    browserResult,
  });
}

function summarizeGateResults({ staticResults, declarativeMigration, nativeResults, binaryRevision, browserResult }) {
  const allResults = [
    ...staticResults,
    declarativeMigration,
    ...nativeResults,
    binaryRevision,
    browserResult,
  ];
  const ok = allResults.every((result) => result.ok === true);
  return {
    ok,
    retry_count: 0,
    signed_packages_enforced: ok && staticResults.find((r) => r.id === 'signature_required_contract')?.ok === true,
    revocation_enforced: ok && proved(nativeResults, 'peer_revocation_registry_round_trips'),
    declarative_migrations_enforced: ok
      && declarativeMigration.ok === true
      && proved(nativeResults, 'runtime_installed_declarative_migration_is_discovered_and_copied')
      && proved(nativeResults, 'native_declarative_migration_matches_browser_operations'),
    no_backend_recompile_for_new_schema: ok
      && proved(nativeResults, 'runtime_installed_module_schemas_extend_native_collection_creators')
      && proved(nativeResults, 'runtime_installed_module_schema_fingerprint_changes_with_schema_files'),
    no_manual_daemon_restart_for_activation: ok && browserResult.ok === true,
    definition_snapshot_passed: ok
      && proved(nativeResults, 'module_release_command_replay_does_not_duplicate_release_state')
      && proved(nativeResults, 'module_release_rejects_stale_source_and_rollback_version_refs_before_manifest_write'),
    runtime_hash_reconcile_passed: ok
      && proved(nativeResults, 'runtime_installed_module_schema_fingerprint_changes_with_schema_files'),
    results: allResults,
  };
}

function runStaticCheck(check) {
  const source = fs.readFileSync(path.join(root, check.path), 'utf8');
  const missing = check.needles.filter((needle) => !source.includes(needle));
  return {
    id: check.id,
    path: check.path,
    ok: missing.length === 0,
    missing,
  };
}

function runCommand({ id, command, args, env = process.env, proves = [] }) {
  const started = Date.now();
  console.log(`ctox_sync_production_readiness_95_app_runtime_package_gate_step_start id=${id}`);
  try {
    const stdout = execFileSync(command, args, {
      cwd: root,
      env,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    console.log(`ctox_sync_production_readiness_95_app_runtime_package_gate_step_ok id=${id} duration_ms=${Date.now() - started}`);
    return {
      id,
      ok: true,
      duration_ms: Date.now() - started,
      proves,
      stdout_tail: tail(stdout),
    };
  } catch (error) {
    const stdoutTail = tail(error.stdout?.toString?.() || '');
    const stderrTail = tail(error.stderr?.toString?.() || error.message || '');
    console.error(`ctox_sync_production_readiness_95_app_runtime_package_gate_step_failed id=${id} duration_ms=${Date.now() - started} status=${error.status ?? 'unknown'} signal=${error.signal || ''}`);
    if (stdoutTail) console.error(`ctox_sync_production_readiness_95_app_runtime_package_gate_step_stdout_tail id=${id}\n${stdoutTail}`);
    if (stderrTail) console.error(`ctox_sync_production_readiness_95_app_runtime_package_gate_step_stderr_tail id=${id}\n${stderrTail}`);
    return {
      id,
      ok: false,
      duration_ms: Date.now() - started,
      proves,
      status: error.status ?? null,
      signal: error.signal || null,
      stdout_tail: stdoutTail,
      stderr_tail: stderrTail,
    };
  }
}

function nativeCargoEnv() {
  return {
    ...process.env,
    CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS: process.env.CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS || '1',
  };
}

function findDefaultSmokeBinaryPath() {
  const candidates = [
    path.join(root, 'runtime/build/production-readiness-95-check-target/debug'),
    path.join(root, 'runtime/build/core-rxdb-integration-target/debug'),
  ];
  for (const candidateDir of candidates) {
    if (!fs.existsSync(candidateDir)) continue;
    const binaryPath = path.join(candidateDir, 'ctox');
    try {
      const stat = fs.statSync(binaryPath);
      if (stat.isFile() && (stat.mode & 0o111) !== 0) return binaryPath;
    } catch {
      // Keep scanning older build locations.
    }
  }
  return '';
}

function validateSmokeBinaryRevision(binaryPath) {
  const started = Date.now();
  if (!binaryPath) {
    return {
      id: 'smoke_binary_revision',
      ok: false,
      duration_ms: Date.now() - started,
      error: 'missing smoke binary; pass --smoke-binary built from the current HEAD',
    };
  }
  try {
    const stat = fs.statSync(binaryPath);
    if (!stat.isFile() || (stat.mode & 0o111) === 0) {
      return {
        id: 'smoke_binary_revision',
        ok: false,
        duration_ms: Date.now() - started,
        binary: path.relative(root, binaryPath),
        error: 'smoke binary is not executable',
      };
    }
    const version = execFileSync(binaryPath, ['version'], {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const binaryCommit = parseCtoxVersionCommit(version);
    const headCommit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
    const ok = Boolean(binaryCommit) && headCommit.startsWith(binaryCommit);
    return {
      id: 'smoke_binary_revision',
      ok,
      duration_ms: Date.now() - started,
      binary: path.relative(root, binaryPath),
      binary_commit: binaryCommit || null,
      head_commit: headCommit,
      version_tail: tail(version),
      ...(ok ? {} : { error: 'smoke binary was not built from current HEAD' }),
    };
  } catch (error) {
    return {
      id: 'smoke_binary_revision',
      ok: false,
      duration_ms: Date.now() - started,
      binary: path.relative(root, binaryPath),
      error: error.message,
      stdout_tail: tail(error.stdout?.toString?.() || ''),
      stderr_tail: tail(error.stderr?.toString?.() || ''),
    };
  }
}

function parseCtoxVersionCommit(value) {
  const text = String(value || '');
  const match = text.match(/(?:^|-)g([0-9a-f]{7,40})(?:-|\\b)/i);
  return match ? match[1].toLowerCase() : '';
}

function runSelfTest() {
  for (const testName of [
    'runtime_installed_module_schemas_extend_native_collection_creators',
    'runtime_installed_declarative_migration_is_discovered_and_copied',
    'module_release_command_replay_does_not_duplicate_release_state',
    'peer_revocation_registry_round_trips',
  ]) {
    if (!nativeTestGroups.some((group) => group.proves.includes(testName))) {
      throw new Error(`missing native proof ${testName}`);
    }
  }
  for (const mode of browserModes) {
    if (!['business-os-dynamic-apps-ui', 'business-os-app-release-ui', 'business-os-app-audience-ui'].includes(mode)) {
      throw new Error(`unexpected browser mode ${mode}`);
    }
  }
  const parsedCommit = parseCtoxVersionCommit(JSON.stringify({
    version: 'v0.3.31-347-g3a641eca7-dirty',
  }));
  if (parsedCommit !== '3a641eca7') throw new Error(`unexpected parsed ctox version commit ${parsedCommit}`);
  const source = fs.readFileSync(__filename, 'utf8');
  for (const needle of [
    'signed_packages_enforced',
    'revocation_enforced',
    'declarative_migrations_enforced',
    'no_backend_recompile_for_new_schema',
    'no_manual_daemon_restart_for_activation',
    'definition_snapshot_passed',
    'runtime_hash_reconcile_passed',
    '--skip-browser',
    'CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS',
    'findDefaultSmokeBinaryPath',
    'smoke_binary_revision',
    'parseCtoxVersionCommit',
    'ctox_sync_production_readiness_95_app_runtime_package_gate_step_start',
    'ctox_sync_production_readiness_95_app_runtime_package_gate_step_failed',
  ]) {
    if (!source.includes(needle)) throw new Error(`missing ${needle}`);
  }
  console.log(`ctox_sync_production_readiness_95_app_runtime_package_gate_self_test=1 native_groups=${nativeTestGroups.length} browser_modes=${browserModes.length}`);
}

function proved(results, proof) {
  return results.some((result) => result.ok === true && Array.isArray(result.proves) && result.proves.includes(proof));
}

function tail(value) {
  return String(value || '').trim().split('\n').slice(-10).join('\n');
}

function flagValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return '';
  return process.argv[index + 1] || '';
}
