#!/usr/bin/env node
'use strict';

/*
 * Validate long-running CTOX Sync Engine 9.5 operational evidence and build the
 * final artifact for gates that cannot be proven by a short local smoke:
 * 72h canary, native restore drill and 30-day pilots.
 */

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const selfTest = process.argv.includes('--self-test');
const gate = flagValue('--gate');
const evidenceLogPath = path.resolve(flagValue('--evidence-log') || defaultEvidenceLogPath(gate || ''));
const measurementsPath = path.resolve(flagValue('--measurements') || defaultMeasurementsPath(gate || ''));
const outputPath = path.resolve(flagValue('--output') || defaultOutputPath(gate || ''));
const browserBundlePath = flagValue('--browser-bundle');
const smokeBinaryPath = flagValue('--smoke-binary');

const gateDefinitions = {
  canary_72h: {
    inputSchema: 'ctox.sync.production_readiness_95.canary_observation.v1',
    outputKind: 'canary_72h',
    marker: 'ctox_sync_production_readiness_95_canary_72h=1',
    build(log, now) {
      const validation = validateBaseLog(log, this.inputSchema, ['started_at', 'ended_at', 'operator', 'evidence_uri_or_hash']);
      const durationHours = durationHoursBetween(log.started_at, log.ended_at);
      const retryCount = Number(log.retry_count);
      const p0p1 = Number(log.p0_p1_incidents);
      const faults = Array.isArray(log.injected_faults) ? log.injected_faults : [];
      if (!Number.isFinite(durationHours)) validation.push('duration_hours');
      if (retryCount !== 0) validation.push('retry_count_zero');
      if (p0p1 !== 0) validation.push('p0_p1_incidents_zero');
      if (!faults.includes('network_partition') || !faults.includes('daemon_restart')) {
        validation.push('required_faults');
      }
      return {
        ok: validation.length === 0 && durationHours >= 72,
        duration_hours: durationHours,
        retry_count: Number.isFinite(retryCount) ? retryCount : null,
        p0_p1_incidents: Number.isFinite(p0p1) ? p0p1 : null,
        injected_faults: faults,
        observation: observationEnvelope(log, validation),
      };
    },
  },
  native_restore_drill: {
    inputSchema: 'ctox.sync.production_readiness_95.native_restore_drill_log.v1',
    outputKind: 'native_restore_drill',
    marker: 'ctox_sync_production_readiness_95_native_restore_drill=1',
    build(log, now) {
      const validation = validateBaseLog(log, this.inputSchema, ['drilled_at', 'operator', 'evidence_uri_or_hash']);
      const ageDays = ageDaysFrom(log.drilled_at, now);
      if (log.off_host_encrypted_snapshot !== true) validation.push('off_host_encrypted_snapshot_true');
      if (!isSha256Like(log.snapshot_manifest_hash)) validation.push('snapshot_manifest_hash');
      if (!isSha256Like(log.restored_database_hash)) validation.push('restored_database_hash');
      if (!Number.isFinite(Number(log.rpo_minutes))) validation.push('rpo_minutes');
      if (!Number.isFinite(Number(log.rto_minutes))) validation.push('rto_minutes');
      return {
        ok: validation.length === 0 && ageDays !== null && ageDays <= 7,
        age_days: ageDays,
        off_host_encrypted_snapshot: log.off_host_encrypted_snapshot === true,
        rpo_minutes: Number.isFinite(Number(log.rpo_minutes)) ? Number(log.rpo_minutes) : null,
        rto_minutes: Number.isFinite(Number(log.rto_minutes)) ? Number(log.rto_minutes) : null,
        restored_database_hash: log.restored_database_hash || null,
        snapshot_manifest_hash: log.snapshot_manifest_hash || null,
        drill: observationEnvelope(log, validation),
      };
    },
  },
  record_workbench_30_day_pilot: pilotDefinition('record_workbench_30_day_pilot', 'record_workbench'),
  workflow_30_day_pilot: pilotDefinition('workflow_30_day_pilot', 'workflow'),
};

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

if (!gateDefinitions[gate]) {
  fail(`missing or unsupported --gate; expected ${Object.keys(gateDefinitions).join(', ')}`);
}

const definition = gateDefinitions[gate];
const measurements = definition.build(readEvidenceLog(evidenceLogPath), new Date());
fs.mkdirSync(path.dirname(measurementsPath), { recursive: true });
fs.writeFileSync(measurementsPath, `${JSON.stringify(measurements, null, 2)}\n`);
if (measurements.ok !== true) {
  console.error(`ctox_sync_production_readiness_95_operational_gate=0 output=${path.relative(root, measurementsPath)} reason=missing_or_invalid_operational_evidence`);
  process.exit(1);
}
if (!smokeBinaryPath || !fs.existsSync(path.resolve(smokeBinaryPath))) {
  console.error(`ctox_sync_production_readiness_95_operational_gate=0 output=${path.relative(root, measurementsPath)} reason=missing_smoke_binary`);
  process.exit(1);
}
execFileSync(process.execPath, [
  path.join(root, 'src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js'),
  '--kind',
  definition.outputKind,
  '--input',
  measurementsPath,
  '--output',
  outputPath,
  ...(browserBundlePath ? ['--browser-bundle', browserBundlePath] : []),
  ...(smokeBinaryPath ? ['--smoke-binary', smokeBinaryPath] : []),
], { cwd: root, stdio: 'inherit' });
console.log(`${definition.marker} output=${path.relative(root, outputPath)}`);

function pilotDefinition(outputKind, pilotName) {
  return {
    inputSchema: 'ctox.sync.production_readiness_95.pilot_observation.v1',
    outputKind,
    marker: `ctox_sync_production_readiness_95_${outputKind}=1`,
    build(log, now) {
      const validation = validateBaseLog(log, this.inputSchema, ['started_at', 'ended_at', 'operator', 'evidence_uri_or_hash']);
      const durationDays = durationDaysBetween(log.started_at, log.ended_at);
      if (log.pilot !== pilotName) validation.push('pilot_name');
      for (const field of ['p0_p1_incidents', 'data_loss_incidents', 'duplicate_effects', 'unauthorized_read_write']) {
        if (Number(log[field]) !== 0) validation.push(`${field}_zero`);
      }
      if (Number(log.slo_convergence_percent) < 99.9) validation.push('slo_convergence_percent');
      if (!Array.isArray(log.cohorts) || log.cohorts.length === 0) validation.push('cohorts');
      return {
        ok: validation.length === 0 && durationDays >= 30,
        pilot: pilotName,
        duration_days: durationDays,
        p0_p1_incidents: numericOrNull(log.p0_p1_incidents),
        data_loss_incidents: numericOrNull(log.data_loss_incidents),
        duplicate_effects: numericOrNull(log.duplicate_effects),
        unauthorized_read_write: numericOrNull(log.unauthorized_read_write),
        slo_convergence_percent: numericOrNull(log.slo_convergence_percent),
        cohorts: Array.isArray(log.cohorts) ? log.cohorts : [],
        observation: observationEnvelope(log, validation),
      };
    },
  };
}

function readEvidenceLog(filePath) {
  if (!fs.existsSync(filePath)) return { missing: true };
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function validateBaseLog(log, schema, requiredFields) {
  const errors = [];
  if (log.missing) errors.push('evidence_log_missing');
  if (log.schema !== schema) errors.push('schema');
  for (const field of requiredFields) {
    if (field === 'evidence_uri_or_hash') {
      if (!validEvidenceReference(log.evidence_uri, log.evidence_hash)) errors.push(field);
    } else if (!log[field]) {
      errors.push(field);
    }
  }
  for (const field of ['started_at', 'ended_at', 'drilled_at']) {
    if (log[field] && Number.isNaN(Date.parse(log[field]))) errors.push(`${field}_date`);
  }
  if (log.operator && typeof log.operator !== 'string') errors.push('operator_string');
  return errors;
}

function observationEnvelope(log, validationErrors) {
  return {
    path: path.relative(root, evidenceLogPath),
    schema: log.schema || null,
    operator: log.operator || null,
    evidence_uri: log.evidence_uri || null,
    evidence_hash: log.evidence_hash || null,
    validation_errors: validationErrors,
  };
}

function durationHoursBetween(start, end) {
  const value = durationMsBetween(start, end);
  return value === null ? null : Math.floor(value / (60 * 60 * 1000));
}

function durationDaysBetween(start, end) {
  const value = durationMsBetween(start, end);
  return value === null ? null : Math.floor(value / (24 * 60 * 60 * 1000));
}

function durationMsBetween(start, end) {
  const started = Date.parse(start || '');
  const ended = Date.parse(end || '');
  if (Number.isNaN(started) || Number.isNaN(ended) || ended < started) return null;
  return ended - started;
}

function ageDaysFrom(value, now) {
  const timestamp = Date.parse(value || '');
  if (Number.isNaN(timestamp) || timestamp > now.getTime()) return null;
  return Math.ceil((now.getTime() - timestamp) / (24 * 60 * 60 * 1000));
}

function isSha256Like(value) {
  return typeof value === 'string' && /^(sha256:)?[0-9a-f]{64}$/i.test(value);
}

function validEvidenceReference(evidenceUri, evidenceHash) {
  return isEvidenceUri(evidenceUri) || isSha256Like(evidenceHash);
}

function isEvidenceUri(value) {
  return typeof value === 'string' && /^(artifact|file|https):\/\/.+/i.test(value);
}

function numericOrNull(value) {
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function defaultEvidenceLogPath(gateName) {
  return path.join(root, 'runtime/build', `ctox-sync-production-readiness-95-${gateName || 'operational'}-evidence-log.json`);
}

function defaultMeasurementsPath(gateName) {
  return path.join(root, 'runtime/build', `ctox-sync-production-readiness-95-${gateName || 'operational'}-measurements.json`);
}

function defaultOutputPath(gateName) {
  const outputNames = {
    canary_72h: 'ctox-sync-production-readiness-95-canary.json',
    native_restore_drill: 'ctox-sync-production-readiness-95-restore-drill.json',
    record_workbench_30_day_pilot: 'ctox-sync-production-readiness-95-record-workbench-pilot.json',
    workflow_30_day_pilot: 'ctox-sync-production-readiness-95-workflow-pilot.json',
  };
  return path.join(root, 'runtime/build', outputNames[gateName] || 'ctox-sync-production-readiness-95-operational-gate.json');
}

function runSelfTest() {
  const now = new Date('2026-06-18T00:00:00.000Z');
  const canary = gateDefinitions.canary_72h.build({
    schema: 'ctox.sync.production_readiness_95.canary_observation.v1',
    started_at: '2026-06-10T00:00:00.000Z',
    ended_at: '2026-06-13T00:00:00.000Z',
    operator: 'release-operator',
    evidence_hash: `sha256:${'a'.repeat(64)}`,
    retry_count: 0,
    p0_p1_incidents: 0,
    injected_faults: ['network_partition', 'daemon_restart'],
  }, now);
  if (!canary.ok || canary.duration_hours !== 72) throw new Error('canary self-test');
  const restore = gateDefinitions.native_restore_drill.build({
    schema: 'ctox.sync.production_readiness_95.native_restore_drill_log.v1',
    drilled_at: '2026-06-17T00:00:00.000Z',
    operator: 'release-operator',
    evidence_uri: 'artifact://restore',
    off_host_encrypted_snapshot: true,
    snapshot_manifest_hash: 'a'.repeat(64),
    restored_database_hash: 'b'.repeat(64),
    rpo_minutes: 10,
    rto_minutes: 45,
  }, now);
  if (!restore.ok || restore.age_days !== 1) throw new Error('restore self-test');
  const pilot = gateDefinitions.record_workbench_30_day_pilot.build({
    schema: 'ctox.sync.production_readiness_95.pilot_observation.v1',
    pilot: 'record_workbench',
    started_at: '2026-05-01T00:00:00.000Z',
    ended_at: '2026-05-31T00:00:00.000Z',
    operator: 'release-operator',
    evidence_hash: `sha256:${'b'.repeat(64)}`,
    p0_p1_incidents: 0,
    data_loss_incidents: 0,
    duplicate_effects: 0,
    unauthorized_read_write: 0,
    slo_convergence_percent: 99.95,
    cohorts: ['founder-dogfood'],
  }, now);
  if (!pilot.ok || pilot.duration_days !== 30) throw new Error('pilot self-test');
  const failedPilot = gateDefinitions.workflow_30_day_pilot.build({ ...pilot, pilot: 'record_workbench' }, now);
  if (failedPilot.ok || !failedPilot.observation.validation_errors.includes('schema')) {
    throw new Error('invalid pilot accepted');
  }
  const source = fs.readFileSync(__filename, 'utf8');
  for (const token of [
    'ctox_sync_production_readiness_95_operational_gate_self_test=1',
    'ctox.sync.production_readiness_95.canary_observation.v1',
    'ctox.sync.production_readiness_95.native_restore_drill_log.v1',
    'ctox.sync.production_readiness_95.pilot_observation.v1',
    'canary_72h',
    'native_restore_drill',
    'record_workbench_30_day_pilot',
    'workflow_30_day_pilot',
    '--evidence-log',
    'missing_or_invalid_operational_evidence',
  ]) {
    if (!source.includes(token)) throw new Error(`missing token ${token}`);
  }
  console.log(`ctox_sync_production_readiness_95_operational_gate_self_test=1 gates=${Object.keys(gateDefinitions).length}`);
}

function fail(message) {
  console.error(`ctox_sync_production_readiness_95_operational_gate_error=${JSON.stringify(message)}`);
  process.exit(1);
}

function flagValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return '';
  return process.argv[index + 1] || '';
}
