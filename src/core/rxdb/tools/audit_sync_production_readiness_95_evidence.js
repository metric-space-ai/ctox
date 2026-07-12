#!/usr/bin/env node
'use strict';

/*
 * Audit evidence for the CTOX Sync Engine 9.5/10 production-readiness gates.
 *
 * Default mode is diagnostic: missing long-running artifacts are recorded as
 * blockers in the JSON report, but the process exits 0. Use --require-complete
 * in a final promotion gate to make every missing/incomplete item fatal.
 */

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const requireComplete = process.argv.includes('--require-complete');
const selfTest = process.argv.includes('--self-test');
const outputPath = process.env.CTOX_SYNC_READINESS_95_AUDIT_PATH
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-evidence-audit.json');
const releaseSoakPath = process.env.CTOX_SYNC_READINESS_95_RELEASE_SOAK
  || path.join(root, 'rxdb-soak-summary.json');
const nightlySoakPath = process.env.CTOX_SYNC_READINESS_95_NIGHTLY_SOAK
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-nightly-soak.json');
const defaultMatrixPath = process.env.CTOX_SYNC_READINESS_95_DEFAULT_MATRIX
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-default-matrix.json');
const businessMatrixPath = process.env.CTOX_SYNC_READINESS_95_BUSINESS_MATRIX
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-business-os-matrix.json');
const canaryPath = process.env.CTOX_SYNC_READINESS_95_CANARY
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-canary.json');
const restoreDrillPath = process.env.CTOX_SYNC_READINESS_95_RESTORE_DRILL
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-restore-drill.json');
const recordPilotPath = process.env.CTOX_SYNC_READINESS_95_RECORD_PILOT
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-record-workbench-pilot.json');
const workflowPilotPath = process.env.CTOX_SYNC_READINESS_95_WORKFLOW_PILOT
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-workflow-pilot.json');
const runbookExercisesPath = process.env.CTOX_SYNC_READINESS_95_RUNBOOK_EXERCISES
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-runbook-exercises.json');
const wanTurnMatrixPath = process.env.CTOX_SYNC_READINESS_95_WAN_TURN_MATRIX
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-wan-turn-matrix.json');
const browserRecoveryMatrixPath = process.env.CTOX_SYNC_READINESS_95_BROWSER_RECOVERY_MATRIX
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-browser-recovery-matrix.json');
const appRuntimePackageGatePath = process.env.CTOX_SYNC_READINESS_95_APP_RUNTIME_PACKAGE_GATE
  || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-app-runtime-package-gate.json');
const securitySignoffPath = path.join(root, 'docs/business-os-security-privacy-signoff.json');

const {
  businessOsProductionSmokeModes,
} = require('./business_os_production_smoke_registry');

const expectedReleaseModes = extractStringArrayFromSource(
  path.join(root, 'src/core/rxdb/tools/browser_rust_soak.js'),
  'defaultSoakModes',
);
const expectedDefaultMatrixModes = extractStringArrayFromSource(
  path.join(root, 'src/core/rxdb/tools/browser_rust_smoke_matrix.js'),
  'defaultModes',
);
const fullMatrixMinimumModes = 40;
const candidateCommit = readCurrentGitCommit();
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
const requiredSecurityControls = [
  'dynamic_app_runtime_boundary',
  'source_visibility',
  'data_review_locked_state',
  'mcp_agent_scope',
  'audit_support_redaction',
  'external_effect_boundary',
  'release_artifact_integrity',
  'sync_recovery_crypto_boundary',
  'webrtc_peer_identity_transport',
  'saga_idempotency_compensation',
  'production_evidence_runbook_integrity',
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

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

const report = buildAuditReport();
fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
console.log(`ctox_sync_production_readiness_95_evidence_ok=${report.ok ? 1 : 0} blockers=${report.blockers.length} output=${path.relative(root, outputPath)}`);

if (requireComplete && report.blockers.length) {
  process.exit(1);
}

function buildAuditReport() {
  const gates = [
    auditSoakGate({
      id: 'release_soak_3x33_no_retry',
      path: releaseSoakPath,
      minCycles: 3,
      expectedModes: expectedReleaseModes,
    }),
    auditSoakGate({
      id: 'nightly_soak_9x33_no_retry',
      path: nightlySoakPath,
      minCycles: 9,
      expectedModes: expectedReleaseModes,
    }),
    auditFullMatrixGate(),
    auditSimpleJsonGate({
      id: 'canary_72h',
      path: canaryPath,
      schema: 'ctox.sync.production_readiness_95.canary.v1',
      checks: [
        ['ok', (value) => value === true],
        ['duration_hours', (value) => Number(value) >= 72],
        ['retry_count', (value) => Number(value) === 0],
        ['p0_p1_incidents', (value) => Number(value) === 0],
      ],
    }),
    auditSimpleJsonGate({
      id: 'native_restore_drill',
      path: restoreDrillPath,
      schema: 'ctox.sync.production_readiness_95.restore_drill.v1',
      checks: [
        ['ok', (value) => value === true],
        ['age_days', (value) => Number(value) <= 7],
        ['off_host_encrypted_snapshot', (value) => value === true],
      ],
    }),
    auditWanTurnMatrix(),
    auditBrowserRecoveryMatrix(),
    auditAppRuntimePackageGate(),
    auditSecuritySignoff(),
    auditPilotGate({
      id: 'record_workbench_30_day_pilot',
      path: recordPilotPath,
    }),
    auditPilotGate({
      id: 'workflow_30_day_pilot',
      path: workflowPilotPath,
    }),
    auditRunbookExercises(),
  ];
  const blockers = gates.flatMap((gate) => gate.blockers.map((blocker) => `${gate.id}:${blocker}`));
  return {
    schema: 'ctox.sync.production_readiness_95.evidence_audit.v1',
    ok: blockers.length === 0,
    require_complete: requireComplete,
    generated_at: new Date().toISOString(),
    candidate_commit: candidateCommit || null,
    gates,
    blockers,
  };
}

function auditSoakGate({ id, path: artifactPath, minCycles, expectedModes }) {
  const artifact = readJson(artifactPath);
  const blockers = [];
  if (!artifact) {
    return gate(id, artifactPath, false, ['missing_artifact']);
  }
  blockers.push(...auditTemplateMarker(artifact));
  const actualModes = splitModes(artifact.modes || '');
  const missingModes = expectedModes.filter((mode) => !actualModes.includes(mode));
  blockers.push(...auditSourceEvidenceFields(artifact.source));
  if (artifact.ok !== true) blockers.push('ok_false');
  if (Number(artifact.cycles || 0) < minCycles) blockers.push(`cycles_below_${minCycles}`);
  if (Number(artifact.retryCount || 0) !== 0) blockers.push('retry_count_nonzero');
  if (missingModes.length) blockers.push(`missing_modes:${missingModes.join('|')}`);
  return gate(id, artifactPath, blockers.length === 0, blockers, {
    cycles: artifact.cycles ?? null,
    retryCount: artifact.retryCount ?? null,
    modeCount: actualModes.length,
    commit: artifact.source?.commit || null,
  });
}

function auditFullMatrixGate() {
  const defaultMatrix = readJson(defaultMatrixPath);
  const businessMatrix = readJson(businessMatrixPath);
  const blockers = [];
  const defaultModes = defaultMatrix?.requestedModes || [];
  const productionModes = businessMatrix?.requestedModes || [];
  const uniqueModes = new Set([...defaultModes, ...productionModes]);
  if (!defaultMatrix) blockers.push('missing_default_matrix');
  if (!businessMatrix) blockers.push('missing_business_matrix');
  if (defaultMatrix) blockers.push(...prefixBlockers('default_matrix', auditTemplateMarker(defaultMatrix)));
  if (businessMatrix) blockers.push(...prefixBlockers('business_matrix', auditTemplateMarker(businessMatrix)));
  if (defaultMatrix) blockers.push(...auditGitRevision('default_matrix', defaultMatrix.gitRevision));
  if (businessMatrix) blockers.push(...auditGitRevision('business_matrix', businessMatrix.gitRevision));
  if (defaultMatrix) blockers.push(...prefixBlockers('default_matrix', auditSourceEvidenceFields(defaultMatrix.source)));
  if (businessMatrix) blockers.push(...prefixBlockers('business_matrix', auditSourceEvidenceFields(businessMatrix.source)));
  if (defaultMatrix && defaultMatrix.ok !== true) blockers.push('default_matrix_ok_false');
  if (businessMatrix && businessMatrix.ok !== true) blockers.push('business_matrix_ok_false');
  if (defaultMatrix && defaultMatrix.configuration?.attempts !== 1) blockers.push('default_matrix_attempts_not_one');
  if (businessMatrix && businessMatrix.configuration?.attempts !== 1) blockers.push('business_matrix_attempts_not_one');
  const missingDefaultModes = expectedDefaultMatrixModes.filter((mode) => !defaultModes.includes(mode));
  if (missingDefaultModes.length) blockers.push(`missing_default_modes:${missingDefaultModes.join('|')}`);
  const missingProductionModes = businessOsProductionSmokeModes.filter((mode) => !productionModes.includes(mode));
  if (missingProductionModes.length) blockers.push(`missing_production_modes:${missingProductionModes.join('|')}`);
  if (uniqueModes.size < fullMatrixMinimumModes) blockers.push(`full_matrix_modes_below_${fullMatrixMinimumModes}`);
  return gate('full_matrix_min_40_no_retry', [defaultMatrixPath, businessMatrixPath], blockers.length === 0, blockers, {
    defaultModeCount: defaultModes.length,
    productionModeCount: productionModes.length,
    uniqueModeCount: uniqueModes.size,
  });
}

function auditSecuritySignoff() {
  const signoff = readJson(securitySignoffPath);
  if (!signoff) return gate('security_privacy_signoff', securitySignoffPath, false, ['missing_artifact']);
  const blockers = auditSecuritySignoffArtifact(signoff);
  return gate('security_privacy_signoff', securitySignoffPath, blockers.length === 0, blockers, {
    status: signoff.status || null,
    evidence_revision: signoff.evidence_revision || null,
  });
}

function auditSecuritySignoffArtifact(signoff) {
  const blockers = auditTemplateMarker(signoff);
  if (signoff.schema !== 'ctox.business_os.security_privacy_signoff.v1') blockers.push('schema');
  if (signoff.status !== 'signed-off') blockers.push(`status_${signoff.status || 'missing'}`);
  blockers.push(...auditAncestorGitRevision('security_signoff_evidence_revision', signoff.evidence_revision));
  if (signoff.reviewer === 'TBD' || typeof signoff.reviewer !== 'string' || signoff.reviewer.length === 0) {
    blockers.push('reviewer_not_set');
  }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(String(signoff.reviewed_at || ''))) {
    blockers.push('reviewed_at_not_date');
  }
  if (!signoff.controls || typeof signoff.controls !== 'object' || Array.isArray(signoff.controls)) {
    blockers.push('controls_missing');
  } else {
    for (const control of requiredSecurityControls) {
      const entry = signoff.controls[control];
      if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
        blockers.push(`control_missing:${control}`);
        continue;
      }
      if (entry.status !== 'signed-off') blockers.push(`control_not_signed:${control}`);
      if (!Array.isArray(entry.evidence) || entry.evidence.length === 0) {
        blockers.push(`control_missing_evidence:${control}`);
      }
    }
  }
  if (!signoff.source_hashes || typeof signoff.source_hashes !== 'object' || Array.isArray(signoff.source_hashes)) {
    blockers.push('source_hashes_missing');
  } else {
    for (const requiredPath of requiredSecuritySourceHashes) {
      if (!Object.hasOwn(signoff.source_hashes, requiredPath)) {
        blockers.push(`source_hash_required_missing:${requiredPath}`);
      }
    }
    for (const [relativePath, expectedHash] of Object.entries(signoff.source_hashes)) {
      if (!isSha256(expectedHash)) {
        blockers.push(`source_hash_invalid:${relativePath}`);
        continue;
      }
      const absolutePath = path.join(root, relativePath);
      if (!fs.existsSync(absolutePath)) {
        blockers.push(`source_hash_missing_file:${relativePath}`);
        continue;
      }
      if (sha256File(absolutePath) !== expectedHash.toLowerCase()) {
        blockers.push(`source_hash_mismatch:${relativePath}`);
      }
    }
  }
  return blockers;
}

function auditWanTurnMatrix() {
  return auditSimpleJsonGate({
    id: 'wan_turn_matrix',
    path: wanTurnMatrixPath,
    schema: 'ctox.sync.production_readiness_95.wan_turn_matrix.v1',
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
  });
}

function auditBrowserRecoveryMatrix() {
  return auditSimpleJsonGate({
    id: 'browser_recovery_matrix',
    path: browserRecoveryMatrixPath,
    schema: 'ctox.sync.production_readiness_95.browser_recovery_matrix.v1',
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
  });
}

function auditAppRuntimePackageGate() {
  return auditSimpleJsonGate({
    id: 'app_runtime_package_gate',
    path: appRuntimePackageGatePath,
    schema: 'ctox.sync.production_readiness_95.app_runtime_package_gate.v1',
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
  });
}

function auditRunbookExercises() {
  const artifact = readJson(runbookExercisesPath);
  if (!artifact) {
    return gate('runbook_exercises', runbookExercisesPath, false, ['missing_artifact'], {
      requiredRunbookCount: requiredRunbooks.length,
    });
  }
  const blockers = [...auditTemplateMarker(artifact), ...auditRunbookExerciseArtifact(artifact)];
  const exercised = Array.isArray(artifact.exercised_runbooks) ? artifact.exercised_runbooks : [];
  return gate('runbook_exercises', runbookExercisesPath, blockers.length === 0, blockers, {
    exercisedRunbookCount: exercised.length,
    requiredRunbookCount: requiredRunbooks.length,
  });
}

function auditPilotGate({ id, path: artifactPath }) {
  return auditSimpleJsonGate({
    id,
    path: artifactPath,
    schema: 'ctox.sync.production_readiness_95.pilot.v1',
    checks: [
      ['ok', (value) => value === true],
      ['duration_days', (value) => Number(value) >= 30],
      ['p0_p1_incidents', (value) => Number(value) === 0],
      ['data_loss_incidents', (value) => Number(value) === 0],
      ['duplicate_effects', (value) => Number(value) === 0],
      ['unauthorized_read_write', (value) => Number(value) === 0],
      ['slo_convergence_percent', (value) => Number(value) >= 99.9],
    ],
  });
}

function auditSimpleJsonGate({ id, path: artifactPath, schema, checks }) {
  const artifact = readJson(artifactPath);
  const blockers = [];
  if (!artifact) {
    return gate(id, artifactPath, false, ['missing_artifact']);
  }
  blockers.push(...auditTemplateMarker(artifact));
  if (artifact.schema !== schema) blockers.push('schema');
  blockers.push(...auditReleaseEvidenceFields(artifact));
  for (const [field, predicate] of checks) {
    const value = artifact[field];
    if (!predicate(value)) blockers.push(`field:${field}`);
  }
  return gate(id, artifactPath, blockers.length === 0, blockers);
}

function auditReleaseEvidenceFields(artifact) {
  const blockers = auditSourceEvidenceFields(artifact.source);
  blockers.push(...auditAttemptEvidenceFields(artifact.attempts || {}));
  return blockers;
}

function runSelfTest() {
  const validSource = {
    commit: candidateCommit,
    dirty: false,
    artifactHashes: {
      browserBundleSha256: 'a'.repeat(64),
      smokeBinarySha256: 'b'.repeat(64),
    },
  };
  const validAttempts = { requested: 1, accepted: 1, retries: 0 };
  assertNoBlockers('valid release evidence', auditReleaseEvidenceFields({
    source: validSource,
    attempts: validAttempts,
  }));
  assertHasBlocker('dirty source', auditReleaseEvidenceFields({
    source: { ...validSource, dirty: true },
    attempts: validAttempts,
  }), 'dirty_source');
  assertHasBlocker('wrong commit', auditReleaseEvidenceFields({
    source: { ...validSource, commit: '0'.repeat(40) },
    attempts: validAttempts,
  }), 'release_commit_mismatch');
  assertHasBlocker('missing bundle hash', auditReleaseEvidenceFields({
    source: {
      ...validSource,
      artifactHashes: { ...validSource.artifactHashes, browserBundleSha256: null },
    },
    attempts: validAttempts,
  }), 'missing_browser_bundle_hash');
  assertHasBlocker('retry attempt', auditReleaseEvidenceFields({
    source: validSource,
    attempts: { requested: 1, accepted: 1, retries: 1 },
  }), 'attempt_retries_nonzero');
  assertHasBlocker('template marker', auditTemplateMarker({ template: true }), 'template_artifact');
  assertHasBlocker('missing runbooks', auditRunbookExerciseArtifact({
    schema: 'ctox.sync.production_readiness_95.runbook_exercises.v1',
    ok: true,
    age_days: 1,
    exercised_runbooks: requiredRunbooks.slice(0, 1),
    open_followups: 0,
  }), 'missing_runbooks:');
  assertHasBlocker('security signoff stale commit', auditSecuritySignoffArtifact({
    schema: 'ctox.business_os.security_privacy_signoff.v1',
    status: 'signed-off',
    reviewer: 'Reviewer',
    reviewed_at: '2026-07-11',
    evidence_revision: '0'.repeat(40),
    controls: {
      release_artifact_integrity: { status: 'signed-off' },
    },
    source_hashes: {},
  }), 'security_signoff_evidence_revision_mismatch');
  assertHasBlocker('security signoff source hash mismatch', auditSecuritySignoffArtifact({
    schema: 'ctox.business_os.security_privacy_signoff.v1',
    status: 'signed-off',
    reviewer: 'Reviewer',
    reviewed_at: '2026-07-11',
    evidence_revision: candidateCommit,
    controls: {
      release_artifact_integrity: { status: 'signed-off' },
    },
    source_hashes: {
      'src/core/rxdb/tools/audit_sync_production_readiness_95_evidence.js': '0'.repeat(64),
    },
  }), 'source_hash_mismatch:');
  assertHasBlocker('security signoff missing required control', auditSecuritySignoffArtifact({
    schema: 'ctox.business_os.security_privacy_signoff.v1',
    status: 'signed-off',
    reviewer: 'Reviewer',
    reviewed_at: '2026-07-11',
    evidence_revision: candidateCommit,
    controls: {},
    source_hashes: {},
  }), 'control_missing:dynamic_app_runtime_boundary');
  assertNoBlockers('valid security signoff', auditSecuritySignoffArtifact({
    schema: 'ctox.business_os.security_privacy_signoff.v1',
    status: 'signed-off',
    reviewer: 'Reviewer',
    reviewed_at: '2026-07-11',
    evidence_revision: candidateCommit,
    controls: Object.fromEntries(requiredSecurityControls.map((control) => [
      control,
      { status: 'signed-off', evidence: [`evidence:${control}`] },
    ])),
    source_hashes: Object.fromEntries(requiredSecuritySourceHashes.map((relativePath) => [
      relativePath,
      sha256File(path.join(root, relativePath)),
    ])),
  }));
  assertHasBlocker('security signoff missing required source hash', auditSecuritySignoffArtifact({
    schema: 'ctox.business_os.security_privacy_signoff.v1',
    status: 'signed-off',
    reviewer: 'Reviewer',
    reviewed_at: '2026-07-11',
    evidence_revision: candidateCommit,
    controls: Object.fromEntries(requiredSecurityControls.map((control) => [
      control,
      { status: 'signed-off', evidence: [`evidence:${control}`] },
    ])),
    source_hashes: {},
  }), 'source_hash_required_missing:.github/workflows/release.yml');
  console.log(`ctox_sync_production_readiness_95_evidence_self_test=1 candidate=${candidateCommit.slice(0, 12)}`);
}

function auditRunbookExerciseArtifact(artifact) {
  const blockers = [];
  const exercised = Array.isArray(artifact.exercised_runbooks) ? artifact.exercised_runbooks : [];
  const missing = requiredRunbooks.filter((id) => !exercised.includes(id));
  if (artifact.schema !== 'ctox.sync.production_readiness_95.runbook_exercises.v1') blockers.push('schema');
  if (artifact.ok !== true) blockers.push('ok_false');
  if (Number(artifact.age_days) > 90) blockers.push('age_days_above_90');
  if (Number(artifact.open_followups || 0) !== 0) blockers.push('open_followups_nonzero');
  if (missing.length) blockers.push(`missing_runbooks:${missing.join('|')}`);
  return blockers;
}

function assertNoBlockers(label, blockers) {
  if (blockers.length) {
    throw new Error(`${label} unexpectedly failed: ${blockers.join(',')}`);
  }
}

function assertHasBlocker(label, blockers, expectedPrefix) {
  if (!blockers.some((blocker) => blocker === expectedPrefix || blocker.startsWith(expectedPrefix))) {
    throw new Error(`${label} did not produce ${expectedPrefix}; got ${blockers.join(',')}`);
  }
}

function auditSourceEvidenceFields(source) {
  const blockers = [];
  if (!source || typeof source !== 'object' || Array.isArray(source)) {
    return ['missing_source'];
  }
  if (typeof source.commit !== 'string' || !/^[0-9a-f]{7,40}$/i.test(source.commit)) {
    blockers.push('missing_release_commit');
  } else {
    blockers.push(...auditGitRevision('release_commit', source.commit));
  }
  if (source.dirty !== false) {
    blockers.push('dirty_source');
  }
  if (!source.artifactHashes || typeof source.artifactHashes !== 'object') {
    blockers.push('missing_artifact_hashes');
    return blockers;
  }
  if (!isSha256(source.artifactHashes.browserBundleSha256)) {
    blockers.push('missing_browser_bundle_hash');
  }
  if (!isSha256(source.artifactHashes.smokeBinarySha256)) {
    blockers.push('missing_smoke_binary_hash');
  }
  return blockers;
}

function auditAttemptEvidenceFields(attempts) {
  if (!attempts || typeof attempts !== 'object' || Array.isArray(attempts)) {
    return ['missing_attempt_evidence'];
  }
  const blockers = [];
  if (Number(attempts.requested) !== 1) blockers.push('requested_attempts_not_one');
  if (Number(attempts.accepted) !== 1) blockers.push('accepted_attempts_not_one');
  if (Number(attempts.retries || 0) !== 0) blockers.push('attempt_retries_nonzero');
  return blockers;
}

function prefixBlockers(prefix, blockers) {
  return blockers.map((blocker) => `${prefix}_${blocker}`);
}

function auditTemplateMarker(artifact) {
  return artifact?.template === true ? ['template_artifact'] : [];
}

function auditGitRevision(label, revision) {
  if (typeof revision !== 'string' || !/^[0-9a-f]{7,40}$/i.test(revision)) {
    return [`${label}_missing_or_invalid`];
  }
  if (!candidateCommit) {
    return ['candidate_commit_unavailable'];
  }
  if (revision !== candidateCommit && !candidateCommit.startsWith(revision)) {
    return [`${label}_mismatch`];
  }
  return [];
}

function auditAncestorGitRevision(label, revision) {
  if (typeof revision !== 'string' || !/^[0-9a-f]{7,40}$/i.test(revision)) {
    return [`${label}_missing_or_invalid`];
  }
  if (!candidateCommit) {
    return ['candidate_commit_unavailable'];
  }
  if (revision === candidateCommit || candidateCommit.startsWith(revision)) {
    return [];
  }
  if (isGitAncestor(revision, candidateCommit)) {
    return [];
  }
  return [`${label}_mismatch`];
}

function isGitAncestor(ancestor, descendant) {
  try {
    execFileSync('git', ['merge-base', '--is-ancestor', ancestor, descendant], {
      cwd: root,
      stdio: 'ignore',
    });
    return true;
  } catch {
    return false;
  }
}

function readCurrentGitCommit() {
  try {
    return execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
}

function isSha256(value) {
  return typeof value === 'string' && /^[0-9a-f]{64}$/i.test(value);
}

function sha256File(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function gate(id, artifactPath, ok, blockers, details = {}) {
  return {
    id,
    ok,
    artifact: Array.isArray(artifactPath)
      ? artifactPath.map((entry) => path.relative(root, entry))
      : path.relative(root, artifactPath),
    blockers,
    details,
  };
}

function readJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch {
    return null;
  }
}

function splitModes(value) {
  return String(value || '')
    .split(',')
    .map((mode) => mode.trim())
    .filter(Boolean);
}

function extractStringArrayFromSource(filePath, constName) {
  const source = fs.readFileSync(filePath, 'utf8');
  const body = extractConstArrayBody(source, constName);
  if (body == null) throw new Error(`${constName} array not found in ${path.relative(root, filePath)}`);
  return [...body.matchAll(/'([^']+)'/g)].map((entry) => entry[1]);
}

function extractConstArrayBody(source, constName) {
  if (!/^[A-Za-z_$][\w$]*$/.test(String(constName || ''))) return null;
  const declaration = `const ${constName}`;
  const start = String(source || '').indexOf(declaration);
  if (start === -1) return null;
  const equals = source.indexOf('=', start + declaration.length);
  if (equals === -1) return null;
  const open = source.indexOf('[', equals + 1);
  if (open === -1) return null;
  let depth = 0;
  let quote = '';
  for (let index = open; index < source.length; index += 1) {
    const char = source[index];
    if (quote) {
      if (char === '\\') {
        index += 1;
      } else if (char === quote) {
        quote = '';
      }
      continue;
    }
    if (char === '"' || char === "'" || char === '`') {
      quote = char;
      continue;
    }
    if (char === '[') depth += 1;
    if (char === ']') {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, index);
    }
  }
  return null;
}
