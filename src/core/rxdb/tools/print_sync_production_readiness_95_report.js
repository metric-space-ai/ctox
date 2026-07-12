#!/usr/bin/env node
'use strict';

/*
 * Print a compact operator summary for the CTOX Sync Engine 9.5/10 evidence
 * audit. This tool is read-only and never turns a red audit green.
 */

const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '../../../..');
const selfTest = process.argv.includes('--self-test');
const defaultAuditPath = path.join(root, 'runtime/build/ctox-sync-production-readiness-95-evidence-audit.json');
const defaultOutputPath = path.join(root, 'runtime/build/ctox-sync-production-readiness-95-operator-report.json');
const auditPath = process.argv.find((arg, index) => index > 1 && !arg.startsWith('--'))
  || process.env.CTOX_SYNC_READINESS_95_AUDIT_PATH
  || defaultAuditPath;
const outputPath = flagValue('--output')
  || process.env.CTOX_SYNC_READINESS_95_REPORT_PATH
  || defaultOutputPath;

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

if (!fs.existsSync(auditPath)) {
  console.warn(`ctox_sync_production_readiness_95_report missing_audit=${path.relative(root, auditPath)}`);
  process.exit(0);
}

const audit = JSON.parse(fs.readFileSync(auditPath, 'utf8'));
const report = buildReport(audit);
writeJsonReport(report);
printReport(report);
writeGitHubStepSummary(report);

if (!audit.ok) process.exitCode = 1;

function buildReport(audit) {
  const gates = Array.isArray(audit.gates) ? audit.gates : [];
  const failedGates = gates.filter((gate) => gate?.ok !== true);
  const missingArtifactGates = failedGates
    .filter((gate) => Array.isArray(gate.blockers) && gate.blockers.includes('missing_artifact'))
    .map((gate) => gate.id);
  const securityGate = gates.find((gate) => gate.id === 'security_privacy_signoff');
  const securityBlockerCount = Array.isArray(securityGate?.blockers) ? securityGate.blockers.length : 0;
  return {
    schema: 'ctox.sync.production_readiness_95.operator_report.v1',
    ok: audit.ok === true,
    generatedAt: new Date().toISOString(),
    sourceAudit: path.relative(root, auditPath),
    candidateCommit: audit.candidate_commit || null,
    totalGates: gates.length,
    failedGates: failedGates.map((gate) => ({
      id: gate.id,
      artifact: gate.artifact,
      blockers: Array.isArray(gate.blockers) ? gate.blockers : [],
    })),
    blockerCount: Array.isArray(audit.blockers) ? audit.blockers.length : failedGates.reduce((sum, gate) => sum + (gate.blockers?.length || 0), 0),
    missingArtifactGates,
    securityBlockerCount,
  };
}

function writeJsonReport(report) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
}

function printReport(report) {
  console.log([
    `ctox_sync_production_readiness_95_report ok=${report.ok ? 1 : 0}`,
    `candidate=${report.candidateCommit || ''}`,
    `gates=${report.totalGates}`,
    `failed=${report.failedGates.length}`,
    `blockers=${report.blockerCount}`,
    `missing_artifacts=${report.missingArtifactGates.length}`,
    `security_blockers=${report.securityBlockerCount}`,
  ].join(' '));
  for (const gate of report.failedGates) {
    const blockers = gate.blockers.join('|') || 'none';
    const artifact = Array.isArray(gate.artifact) ? gate.artifact.join(',') : gate.artifact;
    console.log(`ctox_sync_production_readiness_95_gate id=${gate.id} blockers=${formatValue(blockers)} artifact=${formatValue(artifact || '')}`);
  }
}

function writeGitHubStepSummary(report) {
  const target = process.env.GITHUB_STEP_SUMMARY;
  if (!target) return;
  const lines = [
    '## CTOX Sync Engine 9.5 Production Readiness',
    '',
    `- Result: ${report.ok ? 'passed' : 'blocked'}`,
    `- Candidate: ${report.candidateCommit || 'unknown'}`,
    `- Failed gates: ${report.failedGates.length}/${report.totalGates}`,
    `- Blockers: ${report.blockerCount}`,
    '',
    '| Gate | Blockers | Artifact |',
    '|---|---:|---|',
  ];
  for (const gate of report.failedGates) {
    lines.push([
      gate.id,
      gate.blockers.length,
      Array.isArray(gate.artifact) ? gate.artifact.join(', ') : (gate.artifact || ''),
    ].map(markdownCell).join('|').replace(/^/, '|').replace(/$/, '|'));
  }
  fs.appendFileSync(target, `${lines.join('\n')}\n`, 'utf8');
}

function formatValue(value) {
  const raw = String(value ?? '');
  if (!raw.includes(' ') && raw.length <= 240) return raw;
  return JSON.stringify(raw.length > 480 ? `${raw.slice(0, 477)}...` : raw);
}

function markdownCell(value) {
  return ` ${String(value ?? '').replace(/\r?\n/g, ' ').replace(/\|/g, '\\|')} `;
}

function runSelfTest() {
  const fixture = {
    schema: 'ctox.sync.production_readiness_95.evidence_audit.v1',
    ok: false,
    candidate_commit: '0123456789abcdef0123456789abcdef01234567',
    gates: [
      {
        id: 'release_soak_3x33_no_retry',
        ok: false,
        artifact: 'rxdb-soak-summary.json',
        blockers: ['missing_artifact'],
      },
      {
        id: 'security_privacy_signoff',
        ok: false,
        artifact: 'docs/business-os-security-privacy-signoff.json',
        blockers: ['status_pending-signoff', 'reviewer_not_set'],
      },
      {
        id: 'runbook_exercises',
        ok: true,
        artifact: 'runtime/build/ctox-sync-production-readiness-95-runbook-exercises.json',
        blockers: [],
      },
    ],
    blockers: [
      'release_soak_3x33_no_retry:missing_artifact',
      'security_privacy_signoff:status_pending-signoff',
      'security_privacy_signoff:reviewer_not_set',
    ],
  };
  const report = buildReport(fixture);
  if (report.ok !== false) throw new Error('self-test expected blocked report');
  if (report.failedGates.length !== 2) throw new Error(`failed gate count ${report.failedGates.length}`);
  if (report.blockerCount !== 3) throw new Error(`blocker count ${report.blockerCount}`);
  if (report.missingArtifactGates[0] !== 'release_soak_3x33_no_retry') throw new Error('missing artifact summary');
  if (report.securityBlockerCount !== 2) throw new Error(`security blocker count ${report.securityBlockerCount}`);
  if (report.sourceAudit !== path.relative(root, auditPath)) throw new Error('source audit path');
  console.log('ctox_sync_production_readiness_95_report_self_test=1');
}

function flagValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return '';
  return process.argv[index + 1] || '';
}
