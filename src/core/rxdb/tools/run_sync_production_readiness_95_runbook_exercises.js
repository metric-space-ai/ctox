#!/usr/bin/env node
'use strict';

/*
 * Validate CTOX Sync Engine 9.5 runbook exercise evidence and build the final
 * runbook_exercises artifact. This runner consumes real operator exercise logs;
 * it does not mark runbooks exercised without dated evidence.
 */

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const selfTest = process.argv.includes('--self-test');
const outputPath = path.resolve(
  flagValue('--output')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-runbook-exercises.json'),
);
const measurementsPath = path.resolve(
  flagValue('--measurements')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-runbook-exercises-measurements.json'),
);
const exerciseLogPath = path.resolve(
  flagValue('--exercise-log')
    || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-runbook-exercise-log.json'),
);
const browserBundlePath = flagValue('--browser-bundle');
const smokeBinaryPath = flagValue('--smoke-binary');

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

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

const measurements = buildMeasurements(readExerciseLog(exerciseLogPath));
fs.mkdirSync(path.dirname(measurementsPath), { recursive: true });
fs.writeFileSync(measurementsPath, `${JSON.stringify(measurements, null, 2)}\n`);
if (measurements.ok !== true) {
  console.error(`ctox_sync_production_readiness_95_runbook_exercises=0 output=${path.relative(root, measurementsPath)} reason=missing_or_invalid_exercise_evidence`);
  process.exit(1);
}
if (!smokeBinaryPath || !fs.existsSync(path.resolve(smokeBinaryPath))) {
  console.error(`ctox_sync_production_readiness_95_runbook_exercises=0 output=${path.relative(root, measurementsPath)} reason=missing_smoke_binary`);
  process.exit(1);
}
execFileSync(process.execPath, [
  path.join(root, 'src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js'),
  '--kind',
  'runbook_exercises',
  '--input',
  measurementsPath,
  '--output',
  outputPath,
  ...(browserBundlePath ? ['--browser-bundle', browserBundlePath] : []),
  ...(smokeBinaryPath ? ['--smoke-binary', smokeBinaryPath] : []),
], { cwd: root, stdio: 'inherit' });
console.log(`ctox_sync_production_readiness_95_runbook_exercises=1 output=${path.relative(root, outputPath)}`);

function readExerciseLog(filePath) {
  if (!fs.existsSync(filePath)) {
    return { missing: true, exercises: [] };
  }
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function buildMeasurements(log, now = new Date()) {
  const validationErrors = validateExerciseLog(log);
  const exercises = Array.isArray(log.exercises) ? log.exercises : [];
  const exercisedRunbooks = [...new Set(exercises
    .filter((exercise) => exercise?.outcome === 'passed')
    .map((exercise) => exercise.runbook)
    .filter(Boolean))].sort();
  const openFollowups = exercises.reduce((count, exercise) => count + (Array.isArray(exercise?.followups)
    ? exercise.followups.filter((followup) => followup?.status !== 'closed').length
    : 0), 0);
  const ageDays = maxAgeDays(exercises, now);
  const missingRunbooks = requiredRunbooks.filter((runbook) => !exercisedRunbooks.includes(runbook));
  const ok = validationErrors.length === 0
    && missingRunbooks.length === 0
    && openFollowups === 0
    && ageDays !== null
    && ageDays <= 90;
  return {
    ok,
    age_days: ageDays,
    open_followups: openFollowups,
    exercised_runbooks: exercisedRunbooks,
    required_runbooks: requiredRunbooks,
    missing_runbooks: missingRunbooks,
    exercise_log: {
      path: path.relative(root, exerciseLogPath),
      schema: log.schema || null,
      coordinator: log.coordinator || null,
      validation_errors: validationErrors,
    },
  };
}

function validateExerciseLog(log) {
  const errors = [];
  if (log.missing) errors.push('exercise_log_missing');
  if (log.schema !== 'ctox.sync.production_readiness_95.runbook_exercise_log.v1') errors.push('schema');
  if (!log.coordinator || typeof log.coordinator !== 'string') errors.push('coordinator');
  if (!Array.isArray(log.exercises)) {
    errors.push('exercises_array');
    return errors;
  }
  for (const [index, exercise] of log.exercises.entries()) {
    const prefix = `exercise_${index}`;
    if (!requiredRunbooks.includes(exercise?.runbook)) errors.push(`${prefix}_runbook`);
    if (exercise?.outcome !== 'passed') errors.push(`${prefix}_outcome_passed`);
    if (!exercise?.exercised_at || Number.isNaN(Date.parse(exercise.exercised_at))) {
      errors.push(`${prefix}_exercised_at_date`);
    }
    if (!validEvidenceReference(exercise?.evidence_uri, exercise?.evidence_hash)) errors.push(`${prefix}_evidence`);
    if (!Array.isArray(exercise?.followups)) errors.push(`${prefix}_followups_array`);
  }
  return errors;
}

function validEvidenceReference(evidenceUri, evidenceHash) {
  return isEvidenceUri(evidenceUri) || isSha256Like(evidenceHash);
}

function isEvidenceUri(value) {
  return typeof value === 'string' && /^(artifact|file|https):\/\/.+/i.test(value);
}

function isSha256Like(value) {
  return typeof value === 'string' && /^(sha256:)?[0-9a-f]{64}$/i.test(value);
}

function maxAgeDays(exercises, now) {
  const ages = exercises
    .map((exercise) => Date.parse(exercise?.exercised_at))
    .filter((timestamp) => !Number.isNaN(timestamp))
    .map((timestamp) => Math.ceil((now.getTime() - timestamp) / (24 * 60 * 60 * 1000)));
  if (!ages.length) return null;
  return Math.max(...ages);
}

function runSelfTest() {
  const now = new Date('2026-06-18T00:00:00.000Z');
  const validLog = {
    schema: 'ctox.sync.production_readiness_95.runbook_exercise_log.v1',
    coordinator: 'release-operator',
    exercises: requiredRunbooks.map((runbook) => ({
      runbook,
      outcome: 'passed',
      exercised_at: '2026-06-01T00:00:00.000Z',
      evidence_hash: `sha256:${'a'.repeat(64)}`,
      followups: [],
    })),
  };
  const measurements = buildMeasurements(validLog, now);
  if (!measurements.ok) throw new Error(`valid runbook log rejected: ${measurements.exercise_log.validation_errors.join(',')}`);
  if (measurements.age_days !== 17) throw new Error(`unexpected age ${measurements.age_days}`);
  const missing = buildMeasurements({
    ...validLog,
    exercises: validLog.exercises.slice(1),
  }, now);
  if (!missing.missing_runbooks.includes(requiredRunbooks[0])) throw new Error('missing runbook not detected');
  const open = buildMeasurements({
    ...validLog,
    exercises: [{ ...validLog.exercises[0], followups: [{ id: 'f1', status: 'open' }] }, ...validLog.exercises.slice(1)],
  }, now);
  if (open.open_followups !== 1 || open.ok) throw new Error('open followup accepted');
  const source = fs.readFileSync(__filename, 'utf8');
  for (const token of [
    'ctox_sync_production_readiness_95_runbook_exercises=1',
    'ctox_sync_production_readiness_95_runbook_exercises_self_test=1',
    'ctox.sync.production_readiness_95.runbook_exercise_log.v1',
    'signaling_turn_outage',
    'mcp_access_incident',
    '--exercise-log',
    'missing_or_invalid_exercise_evidence',
  ]) {
    if (!source.includes(token)) throw new Error(`missing token ${token}`);
  }
  console.log(`ctox_sync_production_readiness_95_runbook_exercises_self_test=1 runbooks=${requiredRunbooks.length}`);
}

function flagValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return '';
  return process.argv[index + 1] || '';
}
