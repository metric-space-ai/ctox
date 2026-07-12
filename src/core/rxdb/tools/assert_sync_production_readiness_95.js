#!/usr/bin/env node
'use strict';

/*
 * Guard the CTOX Sync Engine 9.5/10 production-readiness contract.
 *
 * This does not prove the long-running release/canary gates have passed. It
 * prevents the tracked readiness contract, workflow wiring and signoff blockers
 * from silently drifting while those gates are being implemented.
 */

const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '../../../..');
const readinessPath = path.join(root, 'docs/ctox-sync-production-readiness-95.md');
const runbooksPath = path.join(root, 'docs/ctox-sync-production-readiness-runbooks.md');
const hardeningPlanPath = path.join(root, 'docs/ctox-sync-command-bus-hardening-plan.md');
const gitignorePath = path.join(root, '.gitignore');
const ciWorkflowPath = path.join(root, '.github/workflows/ci.yml');
const releaseWorkflowPath = path.join(root, '.github/workflows/release.yml');
const soakWorkflowPath = path.join(root, '.github/workflows/rxdb-soak.yml');
const readinessWorkflowPath = path.join(root, '.github/workflows/rxdb-production-readiness.yml');
const soakGuardPath = path.join(root, 'src/core/rxdb/tools/assert_rxdb_soak_workflow.js');
const securitySignoffPath = path.join(root, 'docs/business-os-security-privacy-signoff.json');
const productionSignoffPath = path.join(root, 'docs/business-os-production-release-signoff.md');
const validationPath = path.join(root, 'runtime/build/ctox-sync-production-readiness-95-validation.json');
const smokeMatrixPath = path.join(root, 'src/core/rxdb/tools/browser_rust_smoke_matrix.js');
const productionSmokeRegistryPath = path.join(root, 'src/core/rxdb/tools/business_os_production_smoke_registry.js');
const businessOsCliPath = path.join(root, 'src/core/service/business_os.rs');
const evidenceAuditPath = path.join(root, 'src/core/rxdb/tools/audit_sync_production_readiness_95_evidence.js');
const evidenceArtifactBuilderPath = path.join(root, 'src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js');
const evidenceTemplatesPath = path.join(root, 'src/core/rxdb/tools/print_sync_production_readiness_95_templates.js');
const evidenceReportPath = path.join(root, 'src/core/rxdb/tools/print_sync_production_readiness_95_report.js');
const appRuntimePackageRunnerPath = path.join(root, 'src/core/rxdb/tools/run_sync_production_readiness_95_app_runtime_package_gate.js');
const browserRecoveryRunnerPath = path.join(root, 'src/core/rxdb/tools/run_sync_production_readiness_95_browser_recovery_matrix.js');
const fullMatrixRunnerPath = path.join(root, 'src/core/rxdb/tools/run_sync_production_readiness_95_full_matrix.js');
const wanTurnRunnerPath = path.join(root, 'src/core/rxdb/tools/run_sync_production_readiness_95_wan_turn_matrix.js');
const runbookExercisesRunnerPath = path.join(root, 'src/core/rxdb/tools/run_sync_production_readiness_95_runbook_exercises.js');
const operationalGateRunnerPath = path.join(root, 'src/core/rxdb/tools/run_sync_production_readiness_95_operational_gate.js');

const problems = [];

const readiness = readRequired(readinessPath);
const runbooks = readRequired(runbooksPath);
const hardeningPlan = readRequired(hardeningPlanPath);
const gitignore = readRequired(gitignorePath);
const ciWorkflow = readRequired(ciWorkflowPath);
const releaseWorkflow = readRequired(releaseWorkflowPath);
const soakWorkflow = readRequired(soakWorkflowPath);
const readinessWorkflow = readRequired(readinessWorkflowPath);
const soakGuard = readRequired(soakGuardPath);
const productionSignoff = readRequired(productionSignoffPath);
const smokeMatrix = readRequired(smokeMatrixPath);
const businessOsCli = readRequired(businessOsCliPath);
const evidenceAudit = readRequired(evidenceAuditPath);
const evidenceArtifactBuilder = readRequired(evidenceArtifactBuilderPath);
const evidenceTemplates = readRequired(evidenceTemplatesPath);
const evidenceReport = readRequired(evidenceReportPath);
const appRuntimePackageRunner = readRequired(appRuntimePackageRunnerPath);
const browserRecoveryRunner = readRequired(browserRecoveryRunnerPath);
const fullMatrixRunner = readRequired(fullMatrixRunnerPath);
const wanTurnRunner = readRequired(wanTurnRunnerPath);
const runbookExercisesRunner = readRequired(runbookExercisesRunnerPath);
const operationalGateRunner = readRequired(operationalGateRunnerPath);
const productionSmokeRegistry = require(productionSmokeRegistryPath);
const securitySignoff = readJsonRequired(securitySignoffPath);
const smokeMatrixModes = extractSmokeMatrixDefaultModes(smokeMatrix);
const productionModes = productionSmokeRegistry.businessOsProductionSmokeModes || [];
const uniqueFullMatrixModeCount = new Set([...smokeMatrixModes, ...productionModes]).size;
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

requireIncludes(readiness, [
  '# CTOX Sync Engine 9.5/10 Production Readiness Plan',
  'Status: active readiness plan',
  'confirmed journaled writes have RPO 0',
  'LAN replication p95 is at most 2 seconds',
  'WAN replication p95 is at most 5 seconds',
  'reconnect after network, signaling or TURN failure p95 is at most 60 seconds',
  'at least 99.9 percent of writes converge',
  'native off-host backup RPO is at most 15 minutes and RTO is at most 60 minutes',
  'one clean full matrix',
  'at least 40 unique modes',
  '3 cycles x 33 required modes',
  '9 cycles x 33 required modes',
  '72 hour persistent canary',
  '`ctox business-os rxdb status --json`',
  '`productionReadiness`',
  'encrypted off-host snapshot every 15 minutes',
  'signed app packages',
  'declarative schema migrations',
  'TURN-only path',
  'external review is required',
  'Two 30-day pilots are required',
  'Rollout state must be typed and persisted',
  'runbooks exist and have been exercised',
  'docs/ctox-sync-production-readiness-runbooks.md',
  'runtime/build/ctox-sync-production-readiness-95-runbook-exercises.json',
  'runtime/build/ctox-sync-production-readiness-95-wan-turn-matrix.json',
  'runtime/build/ctox-sync-production-readiness-95-browser-recovery-matrix.json',
  'runtime/build/ctox-sync-production-readiness-95-app-runtime-package-gate.json',
  'node src/core/rxdb/tools/print_sync_production_readiness_95_templates.js',
  'node src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_app_runtime_package_gate.js',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_browser_recovery_matrix.js',
  'node src/core/rxdb/tools/print_sync_production_readiness_95_report.js',
  'runtime/build/ctox-sync-production-readiness-95-operator-report.json',
  'template_artifact',
], 'readiness-plan');

requireIncludes(runbooks, [
  '# CTOX Sync Engine Production Readiness Runbooks',
  'Status: active operator contract',
  'ctox.sync.production_readiness_95.runbook_exercises.v1',
  'node src/core/rxdb/tools/print_sync_production_readiness_95_templates.js',
  'template: true',
  'Symptoms:',
  'Status fields:',
  'Immediate containment:',
  'Recovery steps:',
  'Prohibited actions:',
  'Verification after recovery:',
  'Exercise evidence:',
  '`signaling_turn_outage`',
  '`webrtc_backpressure_stall`',
  '`journal_growth_replay`',
  '`quota_exhaustion`',
  '`blocked_indexeddb_primary`',
  '`browser_origin_loss`',
  '`native_sqlite_restore`',
  '`schema_migration_block`',
  '`saga_compensation_failure`',
  '`conflict_flood`',
  '`app_package_revocation`',
  '`key_revocation`',
  '`mcp_access_incident`',
  'Do not add HTTP data fallback',
  'Do not confirm the business write if the journal cannot be written',
  'Do not mark a Saga successful without completed forward steps',
  'Do not mutate database tables directly to repair grants',
], 'production-runbooks');

if (uniqueFullMatrixModeCount < 40) {
  problems.push(`full-matrix.coverage:${uniqueFullMatrixModeCount}`);
}

requireIncludes(hardeningPlan, [
  'docs/ctox-sync-production-readiness-95.md',
], 'hardening-plan-link');

requireIncludes(gitignore, [
  '!/docs/ctox-sync-production-readiness-95.md',
  '!/docs/ctox-sync-production-readiness-runbooks.md',
], 'gitignore-tracking');

requireIncludes(soakWorkflow, [
  'workflow_call:',
  'schedule:',
  'timeout-minutes: 360',
  "github.event_name == 'schedule' && '9'",
  'SOAK_FAIL_ON_RETRY',
  'SOAK_REQUIRED_MODES',
  "SOAK_RESULT_PATH: ${{ github.event_name == 'schedule' && 'runtime/build/ctox-sync-production-readiness-95-nightly-soak.json' || 'rxdb-soak-summary.json' }}",
], 'rxdb-soak-workflow');

requireIncludes(soakGuard, [
  'timeout-minutes: 360',
  "assertAtLeast('runner default modes', runnerModes, 33)",
  'coverage regressed below',
], 'rxdb-soak-guard');

requireIncludes(readinessWorkflow, [
  'name: RxDB Production Readiness 9.5',
  'timeout-minutes: 360',
  'Run full 9.5 Browser/Rust matrix',
  'ctox-sync-production-readiness-95-default-matrix.json',
  'ctox-sync-production-readiness-95-business-os-matrix.json',
  'node src/core/rxdb/tools/audit_sync_production_readiness_95_evidence.js',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_operational_gate.js',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_full_matrix.js',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_runbook_exercises.js',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_wan_turn_matrix.js',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_browser_recovery_matrix.js',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_app_runtime_package_gate.js',
  'node src/core/rxdb/tools/print_sync_production_readiness_95_report.js || true',
  'runtime/build/ctox-sync-production-readiness-95-operator-report.json',
  'Enforce 9.5 readiness evidence',
  'node src/core/rxdb/tools/audit_sync_production_readiness_95_evidence.js --require-complete',
], 'readiness-workflow');

requireIncludes(releaseWorkflow, [
  'rxdb-release-soak',
  'uses: ./.github/workflows/rxdb-soak.yml',
  'fail_on_retry: "true"',
  'require_release_coverage: "true"',
  'business-os-production-gate',
  'needs: rxdb-release-soak',
  'Download RxDB release soak evidence',
  'actions/download-artifact@v7',
  'name: rxdb-soak-summary',
  'node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs --require-signed-off',
  'node src/core/rxdb/tools/audit_sync_production_readiness_95_evidence.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_operational_gate.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_full_matrix.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_runbook_exercises.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_wan_turn_matrix.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_browser_recovery_matrix.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_app_runtime_package_gate.js --self-test',
  'CTOX Sync WAN/TURN matrix',
  'CTOX Sync browser recovery matrix',
  'CTOX Sync app runtime package gate',
  'node src/core/rxdb/tools/audit_sync_production_readiness_95_evidence.js --require-complete',
  'runtime/build/ctox-sync-production-readiness-95-validation.json',
  'business-os-release-production-smoke-evidence',
], 'release-workflow');

requireIncludes(ciWorkflow, [
  'node --check src/core/rxdb/tools/assert_sync_production_readiness_95.js',
  'node src/core/rxdb/tools/assert_sync_production_readiness_95.js',
  'node src/core/rxdb/tools/audit_sync_production_readiness_95_evidence.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_operational_gate.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_full_matrix.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_runbook_exercises.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_wan_turn_matrix.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_browser_recovery_matrix.js --self-test',
  'node src/core/rxdb/tools/run_sync_production_readiness_95_app_runtime_package_gate.js --self-test',
  'node src/core/rxdb/tools/print_sync_production_readiness_95_templates.js --self-test',
  'node src/core/rxdb/tools/print_sync_production_readiness_95_report.js --self-test',
  'node src/core/rxdb/tools/print_sync_production_readiness_95_report.js || true',
], 'ci-workflow');

requireIncludes(productionSignoff, [
  'Schema: ctox.business_os.production_signoff.v1',
  'Status: pending-signoff',
  'docs/business-os-security-privacy-signoff.json',
  'artifact-integrity',
], 'production-signoff');

requireIncludes(businessOsCli, [
  'enrich_rxdb_peer_status_with_production_readiness',
  '"productionReadiness"',
  '"ctox.sync.production_readiness_95.status.v1"',
  '"localSubmitP95Ms": 100',
  '"wanReplicationP95Ms": 5_000',
  '"fullMatrixMinimumModes": 40',
  '"evidenceArtifacts"',
  '"templateCatalog"',
  '"artifactBuilder"',
  '"fullMatrixRunner"',
  '"wanTurnMatrix"',
  '"wanTurnRunner"',
  '"browserRecoveryMatrix"',
  '"browserRecoveryRunner"',
  '"appRuntimePackageGate"',
  '"appRuntimePackageRunner"',
  '"operationalGateRunner"',
  '"runbookExercisesRunner"',
  '"operatorReport"',
  'missing_evidence:canary_72h',
  'Production readiness 9.5',
], 'rxdb-status-production-readiness');

requireIncludes(evidenceAudit, [
  'ctox.sync.production_readiness_95.evidence_audit.v1',
  '--require-complete',
  '--self-test',
  'ctox_sync_production_readiness_95_evidence_self_test=1',
  'template_artifact',
  'auditTemplateMarker',
  'assertHasBlocker',
  'candidate_commit',
  'release_soak_3x33_no_retry',
  'nightly_soak_9x33_no_retry',
  'full_matrix_min_40_no_retry',
  'canary_72h',
  'native_restore_drill',
  'wan_turn_matrix',
  'browser_recovery_matrix',
  'app_runtime_package_gate',
  'security_privacy_signoff',
  'auditSecuritySignoffArtifact',
  'requiredSecurityControls',
  'requiredSecuritySourceHashes',
  'security_signoff_evidence_revision',
  'control_missing:',
  'control_missing_evidence:',
  'source_hash_required_missing:',
  'reviewer_not_set',
  'reviewed_at_not_date',
  'source_hash_mismatch',
  'record_workbench_30_day_pilot',
  'workflow_30_day_pilot',
  'runbook_exercises',
  'missing_release_commit',
  '${label}_mismatch',
  'candidate_commit_unavailable',
  'missing_attempt_evidence',
  'requested_attempts_not_one',
  'prefixBlockers',
  'auditSourceEvidenceFields(defaultMatrix.source)',
  'auditSourceEvidenceFields(businessMatrix.source)',
  'missing_browser_bundle_hash',
  'missing_smoke_binary_hash',
], 'evidence-audit');

requireIncludes(evidenceArtifactBuilder, [
  'ctox_sync_production_readiness_95_artifact_builder_self_test=1',
  '--kind',
  '--input',
  '--output',
  'template artifact',
  'artifact validation failed',
  'browserBundleSha256',
  'smokeBinarySha256',
  'readCurrentGitCommit',
  'gitDirty',
  'canary_72h',
  'native_restore_drill',
  'wan_turn_matrix',
  'browser_recovery_matrix',
  'app_runtime_package_gate',
  'record_workbench_30_day_pilot',
  'workflow_30_day_pilot',
  'runbook_exercises',
], 'evidence-artifact-builder');

requireIncludes(browserRecoveryRunner, [
  'ctox_sync_production_readiness_95_browser_recovery_matrix_self_test=1',
  'ctox_sync_production_readiness_95_browser_recovery_matrix=1',
  'recovery-crypto-smoke.mjs',
  'quota-recovery-smoke.mjs',
  'recovery-registration-nonblocking-smoke.mjs',
  'recovery-journal-browser-smoke.mjs',
  'recovery-primary-reset-browser-smoke.mjs',
  'browser_recovery_matrix',
  '--smoke-binary',
  'journal_crash_matrix_passed',
  'export_import_matrix_passed',
  'quota_matrix_passed',
  'blocked_primary_matrix_passed',
  'primary_reset_policy_passed',
  'lost_confirmed_writes',
  'unexplained_conflicts',
], 'browser-recovery-runner');

requireIncludes(wanTurnRunner, [
  'ctox_sync_production_readiness_95_wan_turn_matrix_self_test=1',
  'ctox_sync_production_readiness_95_wan_turn_matrix=1',
  'browser_rust_smoke_matrix.js',
  'browser-to-rust',
  'network-flap-browser-to-rust',
  'restart-signaling-browser-to-rust',
  '--external-measurements',
  'ctox.sync.production_readiness_95.wan_turn_external_measurements.v1',
  'environment_kind_real_wan_turn_required',
  'simulation_not_accepted_for_turn_evidence',
  'turn_only_passed',
  'turn_rotation_passed',
  'eight_hour_offline_catchup_passed',
  'lost_confirmed_writes',
  'duplicate_effects',
], 'wan-turn-runner');

requireIncludes(fullMatrixRunner, [
  'ctox_sync_production_readiness_95_full_matrix_self_test=1',
  'ctox_sync_production_readiness_95_full_matrix=1',
  'browser_rust_smoke_matrix.js',
  'businessOsProductionSmokeModes',
  'ctox-sync-production-readiness-95-default-matrix.json',
  'ctox-sync-production-readiness-95-business-os-matrix.json',
  'SMOKE_MATRIX_ATTEMPTS',
  'SMOKE_BROWSER_WARNING_BUDGET',
  '--smoke-binary',
  'full matrix unique mode count below 40',
], 'full-matrix-runner');

requireIncludes(runbookExercisesRunner, [
  'ctox_sync_production_readiness_95_runbook_exercises_self_test=1',
  'ctox_sync_production_readiness_95_runbook_exercises=1',
  'ctox.sync.production_readiness_95.runbook_exercise_log.v1',
  '--exercise-log',
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
  'open_followups',
  'missing_runbooks',
], 'runbook-exercises-runner');

requireIncludes(operationalGateRunner, [
  'ctox_sync_production_readiness_95_operational_gate_self_test=1',
  'ctox.sync.production_readiness_95.canary_observation.v1',
  'ctox.sync.production_readiness_95.native_restore_drill_log.v1',
  'ctox.sync.production_readiness_95.pilot_observation.v1',
  'canary_72h',
  'native_restore_drill',
  'record_workbench_30_day_pilot',
  'workflow_30_day_pilot',
  '--gate',
  '--evidence-log',
  'network_partition',
  'daemon_restart',
  'off_host_encrypted_snapshot_true',
  'slo_convergence_percent',
], 'operational-gate-runner');

requireIncludes(appRuntimePackageRunner, [
  'ctox_sync_production_readiness_95_app_runtime_package_gate_self_test=1',
  'ctox_sync_production_readiness_95_app_runtime_package_gate=1',
  'runtime_installed_module_schemas_extend_native_collection_creators',
  'runtime_installed_declarative_migration_is_discovered_and_copied',
  'native_declarative_migration_matches_browser_operations',
  'module_release_command_replay_does_not_duplicate_release_state',
  'peer_revocation_registry_round_trips',
  'business-os-dynamic-apps-ui',
  'business-os-app-release-ui',
  'business-os-app-audience-ui',
  'assert-declarative-migrations.mjs',
  'signed_packages_enforced',
  'revocation_enforced',
  'declarative_migrations_enforced',
  'no_backend_recompile_for_new_schema',
  'no_manual_daemon_restart_for_activation',
  'definition_snapshot_passed',
  'runtime_hash_reconcile_passed',
  'ctox_sync_production_readiness_95_app_runtime_package_gate_step_start',
  'ctox_sync_production_readiness_95_app_runtime_package_gate_step_failed',
], 'app-runtime-package-runner');

requireIncludes(evidenceTemplates, [
  'ctox.sync.production_readiness_95.evidence_templates.v1',
  'ctox_sync_production_readiness_95_templates_self_test=1',
  'security_source_hash_paths',
  'security_source_hashes',
  'template: true',
  'ok: false',
  'canary_72h',
  'native_restore_drill',
  'wan_turn_matrix',
  'browser_recovery_matrix',
  'app_runtime_package_gate',
  'record_workbench_30_day_pilot',
  'workflow_30_day_pilot',
  'runbook_exercises',
  'signaling_turn_outage',
  'smokeBinarySha256',
], 'evidence-templates');

requireIncludes(evidenceReport, [
  'ctox.sync.production_readiness_95.operator_report.v1',
  'ctox_sync_production_readiness_95_report_self_test=1',
  'ctox-sync-production-readiness-95-operator-report.json',
  'ctox_sync_production_readiness_95_report ok=',
  'writeJsonReport',
  'ctox_sync_production_readiness_95_gate',
  'GITHUB_STEP_SUMMARY',
  'securityBlockerCount',
  'missingArtifactGates',
], 'evidence-report');

validateSecuritySignoff(securitySignoff);

if (problems.length) {
  writeValidationArtifact(false);
  console.error(`ctox_sync_production_readiness_95_ok=0 problems=${problems.join(',')}`);
  process.exit(1);
}

writeValidationArtifact(true);
console.log('ctox_sync_production_readiness_95_ok=1');

function validateSecuritySignoff(candidate) {
  const requiredControls = [
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
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
    problems.push('security-signoff.object');
    return;
  }
  if (candidate.schema !== 'ctox.business_os.security_privacy_signoff.v1') {
    problems.push('security-signoff.schema');
  }
  if (!['pending-signoff', 'signed-off'].includes(candidate.status)) {
    problems.push('security-signoff.status');
  }
  for (const control of requiredControls) {
    const entry = candidate.controls?.[control];
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      problems.push(`security-signoff.control.${control}`);
      continue;
    }
    if (!['pending', 'signed-off'].includes(entry.status)) {
      problems.push(`security-signoff.control.${control}.status`);
    }
    if (!Array.isArray(entry.evidence) || entry.evidence.length === 0) {
      problems.push(`security-signoff.control.${control}.evidence`);
    }
  }
  if (!candidate.source_hashes || typeof candidate.source_hashes !== 'object' || Array.isArray(candidate.source_hashes)) {
    problems.push('security-signoff.source_hashes');
    return;
  }
  for (const relativePath of requiredSecuritySourceHashes) {
    const hash = candidate.source_hashes[relativePath];
    if (typeof hash !== 'string' || !/^[0-9a-f]{64}$/i.test(hash)) {
      problems.push(`security-signoff.source_hash.${relativePath}`);
    }
  }
}

function requireIncludes(source, needles, label) {
  for (const needle of needles) {
    if (!source.includes(needle)) {
      problems.push(`${label}.missing:${needle}`);
    }
  }
}

function readRequired(filePath) {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch (error) {
    problems.push(`missing:${path.relative(root, filePath)}`);
    return '';
  }
}

function readJsonRequired(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    problems.push(`missing-or-invalid-json:${path.relative(root, filePath)}`);
    return null;
  }
}

function writeValidationArtifact(ok) {
  const artifact = {
    schema: 'ctox.sync.production_readiness_95.validation.v1',
    ok,
    generated_at: new Date().toISOString(),
    readiness_plan: path.relative(root, readinessPath),
    runbooks: path.relative(root, runbooksPath),
    gates: {
      release_soak_modes: 33,
      release_soak_cycles: 3,
      nightly_soak_cycles: 9,
      nightly_timeout_minutes: 360,
      full_matrix_minimum_modes: 40,
      current_default_matrix_modes: smokeMatrixModes.length,
      current_production_matrix_modes: productionModes.length,
      current_unique_full_matrix_modes: uniqueFullMatrixModeCount,
      canary_hours: 72,
      pilot_days: 30,
    },
    blockers: problems,
  };
  fs.mkdirSync(path.dirname(validationPath), { recursive: true });
  fs.writeFileSync(validationPath, `${JSON.stringify(artifact, null, 2)}\n`);
}

function extractSmokeMatrixDefaultModes(source) {
  const match = source.match(/const defaultModes = \[([\s\S]*?)\];/);
  if (!match) {
    problems.push('smoke-matrix.defaultModes');
    return [];
  }
  return [...match[1].matchAll(/'([^']+)'/g)].map((entry) => entry[1]);
}
