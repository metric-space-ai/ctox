#!/usr/bin/env node
'use strict';

/*
 * Build one CTOX Sync Engine 9.5/10 production-readiness evidence artifact.
 *
 * This helper does not manufacture green evidence. It wraps operator/CI-provided
 * gate measurements with the required source envelope and validates the fields
 * the release audit will later enforce.
 */

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');
const os = require('os');

const root = path.resolve(__dirname, '../../../..');
const selfTest = process.argv.includes('--self-test');
const kind = flagValue('--kind');
const inputPath = flagValue('--input');
const outputPath = flagValue('--output');
const allowFailed = process.argv.includes('--allow-failed');
const browserBundlePath = path.resolve(flagValue('--browser-bundle') || path.join(root, 'src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs'));
const smokeBinaryPath = path.resolve(flagValue('--smoke-binary') || path.join(root, 'runtime/build/core-rxdb-integration-target/debug/ctox'));

const requiredRunbooks = [
  'signaling_turn_outage',
  'webrtc_backpressure_stall',
  'journal_growth_replay',
  'quota_exhaustion',
  'blocked_indexeddb_primary',
  'browser_origin_loss',
  'native_sqlite_restore',
  'schema_migration_block',
  'saga_compensation_failure',
  'conflict_flood',
  'app_package_revocation',
  'key_revocation',
  'mcp_access_incident',
];

const artifactDefinitions = {
  canary_72h: {
    schema: 'ctox.sync.production_readiness_95.canary.v1',
    defaultOutput: 'runtime/build/ctox-sync-production-readiness-95-canary.json',
    checks: [
      ['ok', (value) => value === true],
      ['duration_hours', (value) => Number(value) >= 72],
      ['retry_count', (value) => Number(value) === 0],
      ['p0_p1_incidents', (value) => Number(value) === 0],
    ],
  },
  native_restore_drill: {
    schema: 'ctox.sync.production_readiness_95.restore_drill.v1',
    defaultOutput: 'runtime/build/ctox-sync-production-readiness-95-restore-drill.json',
    checks: [
      ['ok', (value) => value === true],
      ['age_days', (value) => Number(value) <= 7],
      ['off_host_encrypted_snapshot', (value) => value === true],
    ],
  },
  wan_turn_matrix: {
    schema: 'ctox.sync.production_readiness_95.wan_turn_matrix.v1',
    defaultOutput: 'runtime/build/ctox-sync-production-readiness-95-wan-turn-matrix.json',
    checks: [
      ['ok', (value) => value === true],
      ['retry_count', (value) => Number(value) === 0],
      ['lan_p95_ms', (value) => Number(value) <= 2_000],
      ['wan_p95_ms', (value) => Number(value) <= 5_000],
      ['reconnect_p95_ms', (value) => Number(value) <= 60_000],
      ['turn_only_passed', (value) => value === true],
      ['turn_rotation_passed', (value) => value === true],
      ['eight_hour_offline_catchup_passed', (value) => value === true],
      ['lost_confirmed_writes', (value) => Number(value) === 0],
      ['duplicate_effects', (value) => Number(value) === 0],
    ],
  },
  browser_recovery_matrix: {
    schema: 'ctox.sync.production_readiness_95.browser_recovery_matrix.v1',
    defaultOutput: 'runtime/build/ctox-sync-production-readiness-95-browser-recovery-matrix.json',
    checks: [
      ['ok', (value) => value === true],
      ['retry_count', (value) => Number(value) === 0],
      ['journal_crash_matrix_passed', (value) => value === true],
      ['export_import_matrix_passed', (value) => value === true],
      ['quota_matrix_passed', (value) => value === true],
      ['blocked_primary_matrix_passed', (value) => value === true],
      ['primary_reset_policy_passed', (value) => value === true],
      ['lost_confirmed_writes', (value) => Number(value) === 0],
      ['unexplained_conflicts', (value) => Number(value) === 0],
    ],
  },
  app_runtime_package_gate: {
    schema: 'ctox.sync.production_readiness_95.app_runtime_package_gate.v1',
    defaultOutput: 'runtime/build/ctox-sync-production-readiness-95-app-runtime-package-gate.json',
    checks: [
      ['ok', (value) => value === true],
      ['retry_count', (value) => Number(value) === 0],
      ['signed_packages_enforced', (value) => value === true],
      ['revocation_enforced', (value) => value === true],
      ['declarative_migrations_enforced', (value) => value === true],
      ['no_backend_recompile_for_new_schema', (value) => value === true],
      ['no_manual_daemon_restart_for_activation', (value) => value === true],
      ['definition_snapshot_passed', (value) => value === true],
      ['runtime_hash_reconcile_passed', (value) => value === true],
    ],
  },
  record_workbench_30_day_pilot: {
    schema: 'ctox.sync.production_readiness_95.pilot.v1',
    defaultOutput: 'runtime/build/ctox-sync-production-readiness-95-record-workbench-pilot.json',
    checks: pilotChecks(),
  },
  workflow_30_day_pilot: {
    schema: 'ctox.sync.production_readiness_95.pilot.v1',
    defaultOutput: 'runtime/build/ctox-sync-production-readiness-95-workflow-pilot.json',
    checks: pilotChecks(),
  },
  runbook_exercises: {
    schema: 'ctox.sync.production_readiness_95.runbook_exercises.v1',
    defaultOutput: 'runtime/build/ctox-sync-production-readiness-95-runbook-exercises.json',
    checks: [
      ['ok', (value) => value === true],
      ['age_days', (value) => Number(value) <= 90],
      ['open_followups', (value) => Number(value || 0) === 0],
      ['exercised_runbooks', (value) => Array.isArray(value) && requiredRunbooks.every((id) => value.includes(id))],
    ],
  },
};

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

if (!artifactDefinitions[kind]) {
  fail(`unknown --kind ${kind || '<missing>'}; expected one of ${Object.keys(artifactDefinitions).join(', ')}`);
}
if (!inputPath) fail('missing --input <json>');

const input = readJson(path.resolve(inputPath));
const artifact = buildArtifact(kind, input, {
  allowFailed,
  browserBundlePath,
  smokeBinaryPath,
});
const destination = path.resolve(outputPath || artifactDefinitions[kind].defaultOutput);
fs.mkdirSync(path.dirname(destination), { recursive: true });
fs.writeFileSync(destination, `${JSON.stringify(artifact, null, 2)}\n`);
console.log(`ctox_sync_production_readiness_95_artifact_built kind=${kind} output=${path.relative(root, destination)} ok=${artifact.ok ? 1 : 0} dirty=${artifact.source.dirty ? 1 : 0}`);

function buildArtifact(artifactKind, input, options) {
  const definition = artifactDefinitions[artifactKind];
  if (!input || typeof input !== 'object' || Array.isArray(input)) {
    fail('input must be a JSON object');
  }
  if (input.template === true) fail('input must not be a template artifact');
  if (input.source || input.attempts || input.schema) {
    fail('input must contain measured gate fields only; source, attempts and schema are generated by this tool');
  }
  validateHashInput('browser bundle', options.browserBundlePath);
  validateHashInput('smoke binary', options.smokeBinaryPath);
  if (options.validateSmokeBinaryRevision !== false) {
    validateSmokeBinaryRevision(options.smokeBinaryPath);
  }
  const artifact = {
    schema: definition.schema,
    template: false,
    generated_at: new Date().toISOString(),
    source: {
      commit: readCurrentGitCommit(),
      dirty: gitDirty(),
      artifactHashes: {
        browserBundleSha256: sha256File(options.browserBundlePath),
        smokeBinarySha256: sha256File(options.smokeBinaryPath),
      },
    },
    attempts: {
      requested: Number(flagValue('--requested-attempts') || 1),
      accepted: Number(flagValue('--accepted-attempts') || 1),
      retries: Number(flagValue('--retries') || 0),
    },
    ...input,
  };
  const blockers = validateArtifact(artifactKind, artifact);
  if (blockers.length && !(options.allowFailed && blockers.every((blocker) => blocker === 'field:ok'))) {
    fail(`artifact validation failed: ${blockers.join(',')}`);
  }
  return artifact;
}

function validateHashInput(label, filePath) {
  if (!filePath || typeof filePath !== 'string') {
    fail(`missing ${label} path`);
  }
  if (!fs.existsSync(filePath)) {
    fail(`missing ${label} at ${filePath}`);
  }
  const stat = fs.statSync(filePath);
  if (!stat.isFile()) {
    fail(`${label} is not a file at ${filePath}`);
  }
}

function validateSmokeBinaryRevision(binaryPath) {
  let output = '';
  try {
    output = execFileSync(binaryPath, ['version'], {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      timeout: 20_000,
    });
  } catch (error) {
    fail(`smoke binary revision check failed for ${binaryPath}: ${error.message}`);
  }
  const binaryCommit = parseCtoxVersionCommit(output);
  const currentCommit = readCurrentGitCommit();
  if (!binaryCommit) {
    fail(`smoke binary revision check failed for ${binaryPath}: version output does not contain git commit`);
  }
  if (!currentCommit.startsWith(binaryCommit) && !binaryCommit.startsWith(currentCommit)) {
    fail(`smoke binary commit ${binaryCommit} does not match current commit ${currentCommit}`);
  }
}

function parseCtoxVersionCommit(output) {
  let version = String(output || '');
  try {
    const parsed = JSON.parse(version);
    version = String(parsed.version || version);
  } catch (_) {
    // Plain-text version output is also accepted.
  }
  const match = version.match(/(?:^|[-+])g([0-9a-f]{7,40})(?:[-+]|$)/i);
  return match ? match[1].toLowerCase() : '';
}

function validateArtifact(artifactKind, artifact) {
  const definition = artifactDefinitions[artifactKind];
  const blockers = [];
  if (artifact.schema !== definition.schema) blockers.push('schema');
  if (artifact.template === true) blockers.push('template_artifact');
  if (artifact.attempts.requested !== 1) blockers.push('requested_attempts_not_one');
  if (artifact.attempts.accepted !== 1) blockers.push('accepted_attempts_not_one');
  if (artifact.attempts.retries !== 0) blockers.push('attempt_retries_nonzero');
  for (const [field, predicate] of definition.checks) {
    if (!predicate(artifact[field])) blockers.push(`field:${field}`);
  }
  return blockers;
}

function pilotChecks() {
  return [
    ['ok', (value) => value === true],
    ['duration_days', (value) => Number(value) >= 30],
    ['p0_p1_incidents', (value) => Number(value) === 0],
    ['data_loss_incidents', (value) => Number(value) === 0],
    ['duplicate_effects', (value) => Number(value) === 0],
    ['unauthorized_read_write', (value) => Number(value) === 0],
    ['slo_convergence_percent', (value) => Number(value) >= 99.9],
  ];
}

function runSelfTest() {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-readiness-artifact-'));
  const bundle = path.join(tmp, 'bundle.mjs');
  const binary = path.join(tmp, 'ctox');
  fs.writeFileSync(bundle, 'bundle');
  fs.writeFileSync(binary, 'binary');
  const canary = buildArtifact('canary_72h', {
    ok: true,
    duration_hours: 72,
    retry_count: 0,
    p0_p1_incidents: 0,
  }, {
    allowFailed: false,
    browserBundlePath: bundle,
    smokeBinaryPath: binary,
    validateSmokeBinaryRevision: false,
  });
  if (canary.schema !== 'ctox.sync.production_readiness_95.canary.v1') throw new Error('canary schema');
  if (canary.template !== false) throw new Error('template marker');
  if (canary.source.artifactHashes.browserBundleSha256 !== sha256File(bundle)) throw new Error('bundle hash');
  assertFails('template input', () => buildArtifact('canary_72h', { template: true }, {
    allowFailed: false,
    browserBundlePath: bundle,
    smokeBinaryPath: binary,
    validateSmokeBinaryRevision: false,
  }), 'template artifact');
  assertFails('failed field', () => buildArtifact('wan_turn_matrix', { ok: true }, {
    allowFailed: false,
    browserBundlePath: bundle,
    smokeBinaryPath: binary,
    validateSmokeBinaryRevision: false,
  }), 'artifact validation failed');
  assertFails('missing binary', () => buildArtifact('canary_72h', {
    ok: true,
    duration_hours: 72,
    retry_count: 0,
    p0_p1_incidents: 0,
  }, {
    allowFailed: false,
    browserBundlePath: bundle,
    smokeBinaryPath: path.join(tmp, 'missing-ctox'),
    validateSmokeBinaryRevision: false,
  }), 'missing smoke binary');
  if (parseCtoxVersionCommit('{"version":"v0.3.31-409-g8cc9b0e6b-dirty"}') !== '8cc9b0e6b') {
    throw new Error('JSON version parser');
  }
  if (parseCtoxVersionCommit('ctox v0.3.31-409-g8cc9b0e6b') !== '8cc9b0e6b') {
    throw new Error('text version parser');
  }
  console.log('ctox_sync_production_readiness_95_artifact_builder_self_test=1');
}

function assertFails(label, fn, expected) {
  const originalExit = process.exitCode;
  try {
    fn();
  } catch (error) {
    process.exitCode = originalExit;
    if (!String(error.message).includes(expected)) {
      throw new Error(`${label}: expected ${expected}; got ${error.message}`);
    }
    return;
  }
  throw new Error(`${label}: expected failure`);
}

function fail(message) {
  throw new Error(message);
}

function readJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    fail(`failed to read JSON ${filePath}: ${error.message}`);
  }
}

function readCurrentGitCommit() {
  return execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
}

function gitDirty() {
  return execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim().length > 0;
}

function sha256File(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function flagValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return '';
  return process.argv[index + 1] || '';
}
