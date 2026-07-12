#!/usr/bin/env node
'use strict';

/*
 * Print operator templates for CTOX Sync Engine 9.5/10 evidence artifacts.
 *
 * These templates are intentionally marked with `template: true` and `ok: false`.
 * The production-readiness audit rejects template artifacts explicitly.
 */

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const root = path.resolve(__dirname, '../../../..');
const selfTest = process.argv.includes('--self-test');
const outputDir = flagValue('--output-dir');

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
const requiredSecuritySourceHashes = [
  '.github/workflows/release.yml',
  '.github/workflows/rxdb-production-readiness.yml',
  'docs/ctox-sync-production-readiness-95.md',
  'docs/ctox-sync-production-readiness-runbooks.md',
  'src/apps/business-os/scripts/assert-security-privacy-signoff.mjs',
  'src/core/rxdb/tools/assert_sync_production_readiness_95.js',
  'src/core/rxdb/tools/audit_sync_production_readiness_95_evidence.js',
  'src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js',
  'src/core/rxdb/tools/browser_rust_smoke_matrix.js',
  'src/core/rxdb/tools/business_os_production_smoke_registry.js',
  'src/core/rxdb/tools/print_sync_production_readiness_95_report.js',
  'src/core/rxdb/tools/print_sync_production_readiness_95_templates.js',
  'src/core/rxdb/tools/run_sync_production_readiness_95_app_runtime_package_gate.js',
  'src/core/rxdb/tools/run_sync_production_readiness_95_browser_recovery_matrix.js',
  'src/core/rxdb/tools/run_sync_production_readiness_95_full_matrix.js',
  'src/core/rxdb/tools/run_sync_production_readiness_95_operational_gate.js',
  'src/core/rxdb/tools/run_sync_production_readiness_95_runbook_exercises.js',
  'src/core/rxdb/tools/run_sync_production_readiness_95_wan_turn_matrix.js',
];

const templates = [
  template('canary_72h', 'ctox.sync.production_readiness_95.canary.v1', 'ctox-sync-production-readiness-95-canary.json', {
    duration_hours: 0,
    retry_count: 0,
    p0_p1_incidents: 0,
    injected_faults: [
      'network_partition',
      'leader_handover',
      'quota_pressure',
      'daemon_restart',
      'checkpoint_fault',
      'conflict_fault',
      'command_saga_fault',
    ],
    observation_schema: 'ctox.sync.production_readiness_95.canary_observation.v1',
    observation_required: true,
  }),
  template('native_restore_drill', 'ctox.sync.production_readiness_95.restore_drill.v1', 'ctox-sync-production-readiness-95-restore-drill.json', {
    age_days: null,
    off_host_encrypted_snapshot: false,
    rpo_minutes: null,
    rto_minutes: null,
    restored_database_hash: '<sha256>',
    snapshot_manifest_hash: '<sha256>',
    drill_log_schema: 'ctox.sync.production_readiness_95.native_restore_drill_log.v1',
    drill_log_required: true,
  }),
  template('wan_turn_matrix', 'ctox.sync.production_readiness_95.wan_turn_matrix.v1', 'ctox-sync-production-readiness-95-wan-turn-matrix.json', {
    retry_count: 0,
    lan_p95_ms: null,
    wan_p95_ms: null,
    reconnect_p95_ms: null,
    turn_only_passed: false,
    turn_rotation_passed: false,
    eight_hour_offline_catchup_passed: false,
    lost_confirmed_writes: null,
    duplicate_effects: null,
    profiles: ['lan', 'wan', 'adverse_wan', 'turn_only'],
    external_measurements_schema: 'ctox.sync.production_readiness_95.wan_turn_external_measurements.v1',
    external_measurements_required: true,
  }),
  template('browser_recovery_matrix', 'ctox.sync.production_readiness_95.browser_recovery_matrix.v1', 'ctox-sync-production-readiness-95-browser-recovery-matrix.json', {
    retry_count: 0,
    journal_crash_matrix_passed: false,
    export_import_matrix_passed: false,
    quota_matrix_passed: false,
    blocked_primary_matrix_passed: false,
    primary_reset_policy_passed: false,
    lost_confirmed_writes: null,
    unexplained_conflicts: null,
  }),
  template('app_runtime_package_gate', 'ctox.sync.production_readiness_95.app_runtime_package_gate.v1', 'ctox-sync-production-readiness-95-app-runtime-package-gate.json', {
    retry_count: 0,
    signed_packages_enforced: false,
    revocation_enforced: false,
    declarative_migrations_enforced: false,
    no_backend_recompile_for_new_schema: false,
    no_manual_daemon_restart_for_activation: false,
    definition_snapshot_passed: false,
    runtime_hash_reconcile_passed: false,
  }),
  template('record_workbench_30_day_pilot', 'ctox.sync.production_readiness_95.pilot.v1', 'ctox-sync-production-readiness-95-record-workbench-pilot.json', {
    pilot: 'record_workbench',
    duration_days: 0,
    p0_p1_incidents: 0,
    data_loss_incidents: 0,
    duplicate_effects: 0,
    unauthorized_read_write: 0,
    slo_convergence_percent: null,
    pilot_observation_schema: 'ctox.sync.production_readiness_95.pilot_observation.v1',
    pilot_observation_required: true,
  }),
  template('workflow_30_day_pilot', 'ctox.sync.production_readiness_95.pilot.v1', 'ctox-sync-production-readiness-95-workflow-pilot.json', {
    pilot: 'multi_collection_workflow',
    duration_days: 0,
    p0_p1_incidents: 0,
    data_loss_incidents: 0,
    duplicate_effects: 0,
    unauthorized_read_write: 0,
    slo_convergence_percent: null,
    pilot_observation_schema: 'ctox.sync.production_readiness_95.pilot_observation.v1',
    pilot_observation_required: true,
  }),
  template('runbook_exercises', 'ctox.sync.production_readiness_95.runbook_exercises.v1', 'ctox-sync-production-readiness-95-runbook-exercises.json', {
    age_days: null,
    exercised_runbooks: requiredRunbooks,
    open_followups: null,
    exercise_log_schema: 'ctox.sync.production_readiness_95.runbook_exercise_log.v1',
    exercise_log_required: true,
  }),
];

const catalog = {
  schema: 'ctox.sync.production_readiness_95.evidence_templates.v1',
  generated_at: new Date().toISOString(),
  output_dir: outputDir ? path.relative(root, path.resolve(outputDir)) : null,
  note: 'Templates are rejected by the 9.5 audit until template=false, ok=true and real release evidence is filled in.',
  security_source_hash_paths: requiredSecuritySourceHashes,
  security_source_hashes: Object.fromEntries(requiredSecuritySourceHashes.map((relativePath) => [
    relativePath,
    sha256File(repoPath(relativePath)),
  ])),
  templates,
};

if (selfTest) {
  runSelfTest(catalog);
  process.exit(0);
}

if (outputDir) {
  writeTemplates(outputDir, templates);
}

process.stdout.write(`${JSON.stringify(catalog, null, 2)}\n`);

function template(id, schema, fileName, fields) {
  return {
    id,
    target_path: `runtime/build/${fileName}`,
    template_path: `runtime/build/ctox-sync-production-readiness-95-templates/${fileName}`,
    artifact: {
      schema,
      template: true,
      ok: false,
      generated_at: '<ISO-8601 timestamp>',
      source: {
        commit: '<current release commit>',
        dirty: false,
        artifactHashes: {
          browserBundleSha256: '<sha256>',
          smokeBinarySha256: '<sha256>',
        },
      },
      attempts: {
        requested: 1,
        accepted: 1,
        retries: 0,
      },
      ...fields,
    },
  };
}

function writeTemplates(dir, entries) {
  const absoluteDir = operatorOutputDir(dir);
  fs.mkdirSync(absoluteDir, { recursive: true });
  for (const entry of entries) {
    fs.writeFileSync(
      childPath(absoluteDir, path.basename(entry.template_path)),
      `${JSON.stringify(entry.artifact, null, 2)}\n`,
    );
  }
}

function runSelfTest(candidate) {
  const ids = new Set(candidate.templates.map((entry) => entry.id));
  const requiredIds = [
    'canary_72h',
    'native_restore_drill',
    'wan_turn_matrix',
    'browser_recovery_matrix',
    'app_runtime_package_gate',
    'record_workbench_30_day_pilot',
    'workflow_30_day_pilot',
    'runbook_exercises',
  ];
  for (const id of requiredIds) {
    if (!ids.has(id)) throw new Error(`missing template id ${id}`);
  }
  for (const entry of candidate.templates) {
    if (!entry.target_path.startsWith('runtime/build/')) throw new Error(`${entry.id}: target_path`);
    if (entry.artifact.template !== true) throw new Error(`${entry.id}: template marker`);
    if (entry.artifact.ok !== false) throw new Error(`${entry.id}: ok must be false`);
    if (!entry.artifact.source?.artifactHashes?.browserBundleSha256) throw new Error(`${entry.id}: browser hash placeholder`);
    if (!entry.artifact.source?.artifactHashes?.smokeBinarySha256) throw new Error(`${entry.id}: binary hash placeholder`);
    if (entry.artifact.attempts?.requested !== 1 || entry.artifact.attempts?.accepted !== 1) {
      throw new Error(`${entry.id}: attempts`);
    }
  }
  for (const relativePath of requiredSecuritySourceHashes) {
    const actual = candidate.security_source_hashes?.[relativePath];
    if (!/^[0-9a-f]{64}$/.test(String(actual || ''))) {
      throw new Error(`security source hash missing ${relativePath}`);
    }
  }
  const runbookTemplate = candidate.templates.find((entry) => entry.id === 'runbook_exercises');
  for (const runbook of requiredRunbooks) {
    if (!runbookTemplate.artifact.exercised_runbooks.includes(runbook)) {
      throw new Error(`runbook template missing ${runbook}`);
    }
  }
  console.log(`ctox_sync_production_readiness_95_templates_self_test=1 templates=${candidate.templates.length}`);
}

function sha256File(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function repoPath(relativePath) {
  const value = String(relativePath || '');
  if (!value || path.isAbsolute(value)) {
    throw new Error(`unsafe relative path: ${JSON.stringify(relativePath)}`);
  }
  const normalized = path.normalize(value);
  if (normalized === '..' || normalized.startsWith(`..${path.sep}`)) {
    throw new Error(`path escapes repository root: ${JSON.stringify(relativePath)}`);
  }
  return `${root}${path.sep}${normalized}`;
}

function childPath(dir, entryName) {
  const name = String(entryName || '');
  if (!name || name === '.' || name === '..' || name.includes('/') || name.includes('\\')) {
    throw new Error(`unsafe file name: ${JSON.stringify(entryName)}`);
  }
  return `${dir}${path.sep}${name}`;
}

function operatorOutputDir(dir) {
  const value = String(dir || '');
  if (!value || value.includes('\0')) {
    throw new Error(`unsafe output dir: ${JSON.stringify(dir)}`);
  }
  const normalized = path.normalize(value);
  if (path.isAbsolute(normalized)) {
    return normalized;
  }
  if (normalized === '..' || normalized.startsWith(`..${path.sep}`)) {
    throw new Error(`output dir escapes working directory: ${JSON.stringify(dir)}`);
  }
  return `${process.cwd()}${path.sep}${normalized}`;
}

function flagValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return '';
  return process.argv[index + 1] || '';
}
