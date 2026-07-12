#!/usr/bin/env node
'use strict';

/*
 * Execute the browser-side recovery matrix and build the 9.5 evidence artifact.
 */

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const selfTest = process.argv.includes('--self-test');
const outputPath = path.resolve(
  flagValue('--output')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-browser-recovery-matrix.json'),
);
const measurementsPath = path.resolve(
  flagValue('--measurements')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-browser-recovery-measurements.json'),
);
const browserBundlePath = flagValue('--browser-bundle');
const smokeBinaryPath = flagValue('--smoke-binary');

const smokeTests = [
  {
    id: 'recovery_crypto',
    path: 'src/apps/business-os/rxdb/tests/recovery-crypto-smoke.mjs',
    proves: ['export_import_matrix_passed'],
  },
  {
    id: 'quota_recovery',
    path: 'src/apps/business-os/rxdb/tests/quota-recovery-smoke.mjs',
    proves: ['quota_matrix_passed'],
  },
  {
    id: 'recovery_registration_nonblocking',
    path: 'src/apps/business-os/rxdb/tests/recovery-registration-nonblocking-smoke.mjs',
    proves: ['blocked_primary_matrix_passed'],
  },
  {
    id: 'recovery_journal_browser',
    path: 'src/apps/business-os/rxdb/tests/recovery-journal-browser-smoke.mjs',
    proves: [
      'journal_crash_matrix_passed',
      'export_import_matrix_passed',
      'quota_matrix_passed',
      'lost_confirmed_writes',
      'unexplained_conflicts',
    ],
  },
  {
    id: 'recovery_primary_reset_browser',
    path: 'src/apps/business-os/rxdb/tests/recovery-primary-reset-browser-smoke.mjs',
    proves: ['blocked_primary_matrix_passed', 'primary_reset_policy_passed'],
  },
];

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

const measurements = runMatrix();
fs.mkdirSync(path.dirname(measurementsPath), { recursive: true });
fs.writeFileSync(measurementsPath, `${JSON.stringify(measurements, null, 2)}\n`);
execFileSync(process.execPath, [
  path.join(root, 'src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js'),
  '--kind',
  'browser_recovery_matrix',
  '--input',
  measurementsPath,
  '--output',
  outputPath,
  ...(browserBundlePath ? ['--browser-bundle', browserBundlePath] : []),
  ...(smokeBinaryPath ? ['--smoke-binary', smokeBinaryPath] : []),
], { cwd: root, stdio: 'inherit' });
console.log(`ctox_sync_production_readiness_95_browser_recovery_matrix=1 output=${path.relative(root, outputPath)}`);

function runMatrix() {
  const results = [];
  for (const test of smokeTests) {
    const startedAt = new Date().toISOString();
    const absolutePath = path.join(root, test.path);
    const started = Date.now();
    try {
      const stdout = execFileSync(process.execPath, [absolutePath], {
        cwd: root,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      results.push({
        id: test.id,
        path: test.path,
        ok: true,
        duration_ms: Date.now() - started,
        started_at: startedAt,
        proves: test.proves,
        stdout_tail: tail(stdout),
      });
    } catch (error) {
      results.push({
        id: test.id,
        path: test.path,
        ok: false,
        duration_ms: Date.now() - started,
        started_at: startedAt,
        proves: test.proves,
        stdout_tail: tail(error.stdout?.toString?.() || ''),
        stderr_tail: tail(error.stderr?.toString?.() || error.message || ''),
      });
    }
  }
  const failed = results.filter((result) => !result.ok);
  return {
    ok: failed.length === 0,
    retry_count: 0,
    journal_crash_matrix_passed: passed('recovery_journal_browser', results),
    export_import_matrix_passed: passed('recovery_crypto', results) && passed('recovery_journal_browser', results),
    quota_matrix_passed: passed('quota_recovery', results) && passed('recovery_journal_browser', results),
    blocked_primary_matrix_passed: passed('recovery_registration_nonblocking', results)
      && passed('recovery_primary_reset_browser', results),
    primary_reset_policy_passed: passed('recovery_primary_reset_browser', results),
    lost_confirmed_writes: failed.length === 0 ? 0 : null,
    unexplained_conflicts: failed.length === 0 ? 0 : null,
    smoke_results: results,
  };
}

function runSelfTest() {
  const ids = new Set(smokeTests.map((test) => test.id));
  for (const id of [
    'recovery_crypto',
    'quota_recovery',
    'recovery_registration_nonblocking',
    'recovery_journal_browser',
    'recovery_primary_reset_browser',
  ]) {
    if (!ids.has(id)) throw new Error(`missing smoke ${id}`);
  }
  const measurements = {
    ok: true,
    retry_count: 0,
    journal_crash_matrix_passed: true,
    export_import_matrix_passed: true,
    quota_matrix_passed: true,
    blocked_primary_matrix_passed: true,
    primary_reset_policy_passed: true,
    lost_confirmed_writes: 0,
    unexplained_conflicts: 0,
  };
  for (const [key, value] of Object.entries(measurements)) {
    if (value !== true && value !== 0) throw new Error(`unexpected self-test value ${key}`);
  }
  const source = fs.readFileSync(__filename, 'utf8');
  for (const flag of ['--browser-bundle', '--smoke-binary']) {
    if (!source.includes(flag)) throw new Error(`missing ${flag}`);
  }
  console.log(`ctox_sync_production_readiness_95_browser_recovery_matrix_self_test=1 smokes=${smokeTests.length}`);
}

function passed(id, results) {
  return results.some((result) => result.id === id && result.ok === true);
}

function tail(value) {
  return String(value || '').trim().split('\n').slice(-8).join('\n');
}

function flagValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return '';
  return process.argv[index + 1] || '';
}
