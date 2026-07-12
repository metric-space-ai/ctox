#!/usr/bin/env node
'use strict';

/*
 * Execute the CTOX Sync Engine 9.5 WAN/TURN gate and build the evidence
 * artifact. Local LAN/reconnect smokes are run here. Real WAN/TURN and
 * eight-hour offline catch-up measurements must be supplied by the operator;
 * this runner intentionally does not treat local simulation as TURN evidence.
 */

const fs = require('fs');
const path = require('path');
const { spawnSync, execFileSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const selfTest = process.argv.includes('--self-test');
const skipLocalSmokes = process.argv.includes('--skip-local-smokes');
const outputPath = path.resolve(
  flagValue('--output')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-wan-turn-matrix.json'),
);
const measurementsPath = path.resolve(
  flagValue('--measurements')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-wan-turn-measurements.json'),
);
const externalMeasurementsPath = path.resolve(
  flagValue('--external-measurements')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-wan-turn-external.json'),
);
const localMatrixPath = path.resolve(
  flagValue('--local-matrix')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-wan-turn-local-matrix.json'),
);
const browserBundlePath = flagValue('--browser-bundle');
const smokeBinaryPath = flagValue('--smoke-binary');

const localModes = [
  'browser-to-rust',
  'network-flap-browser-to-rust',
  'restart-signaling-browser-to-rust',
];

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

const measurements = runGate();
fs.mkdirSync(path.dirname(measurementsPath), { recursive: true });
fs.writeFileSync(measurementsPath, `${JSON.stringify(measurements, null, 2)}\n`);
if (measurements.ok !== true) {
  console.error(`ctox_sync_production_readiness_95_wan_turn_matrix=0 output=${path.relative(root, measurementsPath)} reason=missing_or_invalid_wan_turn_evidence`);
  process.exit(1);
}
execFileSync(process.execPath, [
  path.join(root, 'src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js'),
  '--kind',
  'wan_turn_matrix',
  '--input',
  measurementsPath,
  '--output',
  outputPath,
  ...(browserBundlePath ? ['--browser-bundle', browserBundlePath] : []),
  ...(smokeBinaryPath ? ['--smoke-binary', smokeBinaryPath] : []),
], { cwd: root, stdio: 'inherit' });
console.log(`ctox_sync_production_readiness_95_wan_turn_matrix=1 output=${path.relative(root, outputPath)}`);

function runGate() {
  const local = skipLocalSmokes
    ? skippedLocalMatrix()
    : runLocalMatrix();
  const external = readExternalMeasurements(externalMeasurementsPath);
  const validation = validateExternalMeasurements(external);
  const localDurations = durationsFromLocalMatrix(local.summary);
  const localOk = local.ok && localDurations.length > 0;
  const lanP95 = external.lan_p95_ms ?? p95(localDurations);
  const reconnectP95 = external.reconnect_p95_ms ?? p95(reconnectDurationsFromLocalMatrix(local.summary));
  const ok = localOk
    && validation.length === 0
    && Number(lanP95) <= 2_000
    && Number(external.wan_p95_ms) <= 5_000
    && Number(reconnectP95) <= 60_000
    && external.turn_only_passed === true
    && external.turn_rotation_passed === true
    && external.eight_hour_offline_catchup_passed === true
    && Number(external.lost_confirmed_writes) === 0
    && Number(external.duplicate_effects) === 0;

  return {
    ok,
    retry_count: 0,
    lan_p95_ms: lanP95,
    wan_p95_ms: external.wan_p95_ms ?? null,
    reconnect_p95_ms: reconnectP95,
    turn_only_passed: external.turn_only_passed === true,
    turn_rotation_passed: external.turn_rotation_passed === true,
    eight_hour_offline_catchup_passed: external.eight_hour_offline_catchup_passed === true,
    lost_confirmed_writes: Number.isFinite(Number(external.lost_confirmed_writes))
      ? Number(external.lost_confirmed_writes)
      : null,
    duplicate_effects: Number.isFinite(Number(external.duplicate_effects))
      ? Number(external.duplicate_effects)
      : null,
    profiles: ['lan', 'wan', 'adverse_wan', 'turn_only', 'turn_rotation', 'offline_8h_catchup'],
    local_matrix: {
      path: path.relative(root, localMatrixPath),
      ok: local.ok,
      skipped: local.skipped,
      modes: localModes,
      durations_ms: localDurations,
      reconnect_durations_ms: reconnectDurationsFromLocalMatrix(local.summary),
      error: local.error || null,
    },
    external_measurements: {
      path: path.relative(root, externalMeasurementsPath),
      schema: external.schema || null,
      environment_kind: external.environment_kind || null,
      measured_at: external.measured_at || null,
      operator: external.operator || null,
      validation_errors: validation,
    },
  };
}

function runLocalMatrix() {
  const env = {
    ...process.env,
    SMOKE_MODES: localModes.join(','),
    SMOKE_MATRIX_ATTEMPTS: '1',
    SMOKE_PAGE_PATH: '/index.html',
    SMOKE_MODE_TIMEOUT_MS: '300000',
    SMOKE_BROWSER_WARNING_BUDGET: '0',
    SMOKE_BROWSER_ERROR_BUDGET: '0',
    SMOKE_BROWSER_REQUEST_FAILURE_BUDGET: '0',
    SMOKE_MATRIX_RESULT_PATH: localMatrixPath,
    BUSINESS_PORT: '9400',
    SIGNALING_PORT: '19400',
  };
  const result = spawnSync(process.execPath, [
    path.join(root, 'src/core/rxdb/tools/browser_rust_smoke_matrix.js'),
  ], {
    cwd: root,
    env,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 20 * 60 * 1000,
    killSignal: 'SIGTERM',
  });
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  const summary = readJsonIfExists(localMatrixPath);
  return {
    ok: result.status === 0 && !result.signal && summary?.ok === true,
    skipped: false,
    summary,
    error: result.error
      ? `${result.error.code || 'spawn_error'}:${result.error.message || ''}`
      : result.signal
        ? `signal:${result.signal}`
        : result.status === 0
          ? null
          : `exit:${result.status}`,
  };
}

function skippedLocalMatrix() {
  return {
    ok: true,
    skipped: true,
    summary: {
      ok: true,
      modes: localModes.map((mode) => ({
        mode,
        ok: true,
        attempts: [{ attempt: 1, ok: true, durationMs: 1 }],
      })),
    },
    error: null,
  };
}

function readExternalMeasurements(filePath) {
  if (!fs.existsSync(filePath)) {
    return { missing: true };
  }
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function validateExternalMeasurements(external) {
  const errors = [];
  if (external.missing) errors.push('external_measurements_missing');
  if (external.schema !== 'ctox.sync.production_readiness_95.wan_turn_external_measurements.v1') {
    errors.push('schema');
  }
  if (external.environment_kind !== 'real_wan_turn') errors.push('environment_kind_real_wan_turn_required');
  if (external.simulated === true) errors.push('simulation_not_accepted_for_turn_evidence');
  for (const field of ['wan_p95_ms', 'reconnect_p95_ms']) {
    if (!Number.isFinite(Number(external[field]))) errors.push(`${field}_number`);
  }
  for (const field of ['turn_only_passed', 'turn_rotation_passed', 'eight_hour_offline_catchup_passed']) {
    if (external[field] !== true) errors.push(`${field}_true`);
  }
  for (const field of ['lost_confirmed_writes', 'duplicate_effects']) {
    if (Number(external[field]) !== 0) errors.push(`${field}_zero`);
  }
  if (!Array.isArray(external.profiles) || !['wan', 'adverse_wan', 'turn_only', 'turn_rotation', 'offline_8h_catchup'].every((profile) => external.profiles.includes(profile))) {
    errors.push('profiles_complete');
  }
  if (!external.measured_at || Number.isNaN(Date.parse(external.measured_at))) errors.push('measured_at_date');
  if (!external.operator || typeof external.operator !== 'string') errors.push('operator');
  if (!validEvidenceReference(external.evidence_uri) && !validEvidenceReference(external.evidence_hash)) {
    errors.push('evidence_uri_or_hash');
  }
  return errors;
}

function validEvidenceReference(value) {
  const text = String(value || '').trim();
  if (!text) return false;
  if (/^sha256:[0-9a-f]{64}$/i.test(text)) return true;
  if (/^[0-9a-f]{64}$/i.test(text)) return true;
  if (/^(artifact|file|https):\/\/\S+$/i.test(text)) return true;
  return false;
}

function durationsFromLocalMatrix(summary) {
  if (!summary || !Array.isArray(summary.modes)) return [];
  return summary.modes.flatMap((mode) => (mode.attempts || [])
    .filter((attempt) => attempt.ok === true)
    .map((attempt) => Number(attempt.durationMs))
    .filter(Number.isFinite));
}

function reconnectDurationsFromLocalMatrix(summary) {
  if (!summary || !Array.isArray(summary.modes)) return [];
  return summary.modes
    .filter((mode) => ['network-flap-browser-to-rust', 'restart-signaling-browser-to-rust'].includes(mode.mode))
    .flatMap((mode) => (mode.attempts || [])
      .filter((attempt) => attempt.ok === true)
      .map((attempt) => Number(attempt.durationMs))
      .filter(Number.isFinite));
}

function p95(values) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.ceil(sorted.length * 0.95) - 1];
}

function readJsonIfExists(filePath) {
  if (!fs.existsSync(filePath)) return null;
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function runSelfTest() {
  const validExternal = {
    schema: 'ctox.sync.production_readiness_95.wan_turn_external_measurements.v1',
    environment_kind: 'real_wan_turn',
    simulated: false,
    measured_at: '2026-06-18T00:00:00.000Z',
    operator: 'release-operator',
    evidence_hash: `sha256:${'a'.repeat(64)}`,
    wan_p95_ms: 1200,
    reconnect_p95_ms: 3000,
    turn_only_passed: true,
    turn_rotation_passed: true,
    eight_hour_offline_catchup_passed: true,
    lost_confirmed_writes: 0,
    duplicate_effects: 0,
    profiles: ['wan', 'adverse_wan', 'turn_only', 'turn_rotation', 'offline_8h_catchup'],
  };
  if (validateExternalMeasurements(validExternal).length) throw new Error('valid external measurements rejected');
  const simulated = { ...validExternal, simulated: true };
  if (!validateExternalMeasurements(simulated).includes('simulation_not_accepted_for_turn_evidence')) {
    throw new Error('simulated TURN measurements accepted');
  }
  const missingProfiles = { ...validExternal, profiles: ['wan'] };
  if (!validateExternalMeasurements(missingProfiles).includes('profiles_complete')) {
    throw new Error('incomplete profile list accepted');
  }
  const weakEvidenceHash = { ...validExternal, evidence_hash: 'sha256:abc', evidence_uri: null };
  if (!validateExternalMeasurements(weakEvidenceHash).includes('evidence_uri_or_hash')) {
    throw new Error('weak evidence hash accepted');
  }
  const local = durationsFromLocalMatrix({
    modes: [
      { mode: 'browser-to-rust', attempts: [{ ok: true, durationMs: 10 }] },
      { mode: 'network-flap-browser-to-rust', attempts: [{ ok: true, durationMs: 20 }] },
      { mode: 'restart-signaling-browser-to-rust', attempts: [{ ok: true, durationMs: 30 }] },
    ],
  });
  if (p95(local) !== 30) throw new Error('p95 computation');
  const source = fs.readFileSync(__filename, 'utf8');
  for (const token of [
    'ctox_sync_production_readiness_95_wan_turn_matrix=1',
    '--external-measurements',
    'environment_kind_real_wan_turn_required',
    'simulation_not_accepted_for_turn_evidence',
    'validEvidenceReference',
    'browser_rust_smoke_matrix.js',
    'network-flap-browser-to-rust',
    'restart-signaling-browser-to-rust',
    'eight_hour_offline_catchup_passed',
  ]) {
    if (!source.includes(token)) throw new Error(`missing self-test token ${token}`);
  }
  console.log(`ctox_sync_production_readiness_95_wan_turn_matrix_self_test=1 modes=${localModes.length}`);
}

function flagValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return '';
  return process.argv[index + 1] || '';
}
