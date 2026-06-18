#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, '../../../..');

const operatorGuidePath = path.join(root, 'docs/business-os-roles-permissions-operator-guide.md');
const customerGuidePath = path.join(root, 'docs/business-os-app-access-and-roles-guide.md');
const rolloutGuidePath = path.join(root, 'docs/business-os-roles-permissions-rollout.md');
const humanSignoffPath = path.join(root, 'docs/business-os-production-release-signoff.md');
const securitySignoffPath = path.join(root, 'docs/business-os-security-privacy-signoff.json');
const releaseWorkflowPath = path.join(root, '.github/workflows/release.yml');
const dryRunEvidencePath = path.join(root, 'runtime/build/business-os-release-docs-dry-run.json');
const smokeSummaryPath = path.join(root, 'runtime/build/business-os-smoke-matrix-summary.json');

const uiSourceChecks = [
  {
    id: 'shell-lifecycle-badge',
    path: 'src/apps/business-os/app.js',
    includes: ['data-module-lifecycle', 'data-app-lifecycle-badge', 'data-edit-lifecycle-app'],
  },
  {
    id: 'shell-lifecycle-governance-note',
    path: 'src/apps/business-os/app.js',
    includes: ['App-Sichtbarkeit entscheidet', 'Daten bleiben separat'],
  },
  {
    id: 'app-store-release-flow',
    path: 'src/apps/business-os/modules/app-store/index.js',
    includes: ['Freigeben', 'target_version', 'source_version_id', 'rollback_version_id'],
  },
  {
    id: 'settings-diagnostics',
    path: 'src/apps/business-os/shared/react-settings.js',
    includes: ['Warum?', 'Support-Paket', 'ctox.business_os.support_diagnostics.v1'],
  },
  {
    id: 'why-diagnostics-model',
    path: 'src/apps/business-os/shared/shell-permissions-ui.js',
    includes: ['Warum?', 'lifecycle_state', 'declared_collections', 'data-why-data-decisions'],
  },
  {
    id: 'production-smoke-registry',
    path: 'src/core/rxdb/tools/business_os_production_smoke_registry.js',
    includes: [
      'business-os-app-release-ui',
      'business-os-app-audience-ui',
      'business-os-agent-scope-ui',
      'business-os-auth-scope-ui',
      'business-os-fresh-profile-ui',
    ],
  },
];

const problems = [];

const operatorGuide = readRequired(operatorGuidePath);
const customerGuide = readRequired(customerGuidePath);
const rolloutGuide = readRequired(rolloutGuidePath);
const humanSignoff = readRequired(humanSignoffPath);
const securitySignoff = readJsonRequired(securitySignoffPath);
const releaseWorkflow = readRequired(releaseWorkflowPath);

requireIncludes(operatorGuide, [
  '# Business OS Roles, App Lifecycle And Operator Guide',
  'Owner',
  'Admin',
  'App-Verantwortliche:r',
  'Teammitglied',
  'Privat',
  'Vorschau',
  'Team',
  'Eingeschraenkt',
  '0.x.y',
  '1.0.0',
  'App aendern',
  'apps.source.view',
  'Freigeben',
  'Rollback',
  'locked data areas',
  'MCP external effects remain disabled',
  'Warum?',
  'Support-Paket',
  'docs/business-os-production-release-signoff.md',
  'docs/business-os-security-privacy-signoff.json',
], 'operator-guide');

requireIncludes(customerGuide, [
  '# Business OS App Access And Roles Guide',
  'Owner',
  'Admin',
  'App-Verantwortliche:r',
  'Teammitglied',
  '0.x.y',
  'Privat',
  'Vorschau',
  '1.0.0+',
  'Team',
  'Eingeschraenkt',
  'App aendern',
  'Freigeben',
  'Settings shows diagnostics and read-only release state',
  'External effects are disabled for this rollout',
  'Warum?',
  'Support-Paket',
], 'customer-guide');

requireIncludes(rolloutGuide, [
  'docs/business-os-app-access-and-roles-guide.md',
  'docs/business-os-roles-permissions-operator-guide.md',
  'docs/business-os-production-release-signoff.md',
  'docs/business-os-security-privacy-signoff.json',
  'node src/apps/business-os/scripts/assert-production-release-docs.mjs',
  'node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs',
], 'rollout-guide');

requireIncludes(humanSignoff, [
  'Schema: ctox.business_os.production_signoff.v1',
  'docs/business-os-security-privacy-signoff.json',
  '## Required Checklist',
  '## Evidence To Review',
], 'signoff');

if (securitySignoff.schema !== 'ctox.business_os.security_privacy_signoff.v1') {
  problems.push('security-signoff.schema');
}
if (!['pending-signoff', 'signed-off'].includes(securitySignoff.status)) {
  problems.push('security-signoff.status');
}

requireIncludes(releaseWorkflow, [
  'business-os-production-gate',
  'business-os-release-production-smoke-evidence',
  'business-os-release-docs-dry-run.json',
  'node src/apps/business-os/scripts/assert-production-release-docs.mjs',
  'node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs --require-signed-off',
], 'release-workflow');

const uiEvidence = uiSourceChecks.map((check) => sourceCheckEvidence(check));

for (const check of uiEvidence) {
  if (!check.ok) {
    problems.push(`ui-source:${check.id}:${check.missing.join('|')}`);
  }
}

if (problems.length) {
  writeDryRunEvidence({
    ok: false,
    problems,
    uiEvidence,
  });
  console.error(`business_os_release_docs_ok=0 problems=${problems.join(',')}`);
  process.exit(1);
}

writeDryRunEvidence({
  ok: true,
  problems: [],
  uiEvidence,
});
console.log(`business_os_release_docs_ok=1 status=${securitySignoff.status}`);

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
    return {};
  }
}

function requireIncludes(text, needles, label) {
  for (const needle of needles) {
    if (!text.includes(needle)) problems.push(`${label}:${needle}`);
  }
}

function sourceCheckEvidence(check) {
  const filePath = path.join(root, check.path);
  const text = readRequired(filePath);
  const missing = check.includes.filter((needle) => !text.includes(needle));
  return {
    id: check.id,
    path: check.path,
    ok: missing.length === 0,
    required_strings: check.includes,
    missing,
  };
}

function writeDryRunEvidence({ ok, problems: evidenceProblems, uiEvidence }) {
  const smokeSummary = readSmokeSummary();
  const artifact = {
    schema: 'ctox.business_os.release_docs_dry_run.v1',
    generated_at: new Date().toISOString(),
    ok,
    status: securitySignoff.status || 'unknown',
    problems: evidenceProblems,
    docs: {
      customer_guide: path.relative(root, customerGuidePath),
      operator_guide: path.relative(root, operatorGuidePath),
      rollout_guide: path.relative(root, rolloutGuidePath),
      production_signoff: path.relative(root, humanSignoffPath),
      security_privacy_signoff: path.relative(root, securitySignoffPath),
    },
    ui_source_evidence: uiEvidence,
    smoke_summary: smokeSummary,
    release_boundary: {
      production_ready_claim_allowed: false,
      reason: securitySignoff.status === 'signed-off'
        ? 'customer/operator dry-run evidence still requires release review'
        : 'security/privacy signoff is not signed off',
    },
  };
  fs.mkdirSync(path.dirname(dryRunEvidencePath), { recursive: true });
  fs.writeFileSync(dryRunEvidencePath, `${JSON.stringify(artifact, null, 2)}\n`);
}

function readSmokeSummary() {
  try {
    const summary = JSON.parse(fs.readFileSync(smokeSummaryPath, 'utf8'));
    return {
      exists: true,
      path: path.relative(root, smokeSummaryPath),
      schema: summary.schema || '',
      git_revision: summary.gitRevision || '',
      requested_modes: Array.isArray(summary.requestedModes) ? summary.requestedModes : [],
      started_at: summary.startedAt || '',
      ended_at: summary.endedAt || '',
    };
  } catch (error) {
    return {
      exists: false,
      path: path.relative(root, smokeSummaryPath),
      reason: 'not available for this local/CI step',
    };
  }
}
