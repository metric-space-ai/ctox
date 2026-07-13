#!/usr/bin/env node
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, '../../../..');

const SIGNOFF_SCHEMA = 'ctox.business_os.security_privacy_signoff.v1';
const VALIDATION_SCHEMA = 'ctox.business_os.security_privacy_signoff_validation.v1';
const signoffPath = path.join(root, 'docs/business-os-security-privacy-signoff.json');
const humanSignoffPath = path.join(root, 'docs/business-os-production-release-signoff.md');
const workflowPath = path.join(root, '.github/workflows/release.yml');
const validationPath = path.join(root, 'runtime/build/business-os-security-privacy-signoff-validation.json');
const requireSignedOff = process.argv.includes('--require-signed-off');
const selfTest = process.argv.includes('--self-test');

const requiredControls = Object.freeze([
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
]);
const requiredSourceHashes = Object.freeze([
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
]);

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

const signoff = readJson(signoffPath);
const problems = validateSecurityPrivacySignoff(signoff, { requireSignedOff });
writeValidationArtifact(signoff, problems);

if (problems.length) {
  console.error(`business_os_security_privacy_signoff_ok=0 require_signed_off=${requireSignedOff ? 1 : 0} problems=${problems.join(',')}`);
  process.exit(1);
}

console.log(`business_os_security_privacy_signoff_ok=1 require_signed_off=${requireSignedOff ? 1 : 0} status=${signoff.status}`);

function validateSecurityPrivacySignoff(candidate, options = {}) {
  const problems = [];
  const require = (condition, message) => {
    if (!condition) problems.push(message);
  };
  require(candidate && typeof candidate === 'object' && !Array.isArray(candidate), 'signoff.object');
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) return problems;

  require(candidate.schema === SIGNOFF_SCHEMA, 'signoff.schema');
  require(['pending-signoff', 'signed-off'].includes(candidate.status), 'signoff.status');
  require(typeof candidate.reviewer === 'string' && candidate.reviewer.length > 0, 'signoff.reviewer');
  require(typeof candidate.reviewed_at === 'string' && candidate.reviewed_at.length > 0, 'signoff.reviewed_at');
  require(typeof candidate.evidence_revision === 'string' && candidate.evidence_revision.length > 0, 'signoff.evidence_revision');
  require(candidate.controls && typeof candidate.controls === 'object' && !Array.isArray(candidate.controls), 'signoff.controls');
  require(candidate.source_hashes && typeof candidate.source_hashes === 'object' && !Array.isArray(candidate.source_hashes), 'signoff.source_hashes');
  require(Array.isArray(candidate.notes), 'signoff.notes');

  for (const control of requiredControls) {
    const entry = candidate.controls?.[control];
    require(entry && typeof entry === 'object' && !Array.isArray(entry), `control.${control}`);
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) continue;
    require(['pending', 'signed-off'].includes(entry.status), `control.${control}.status`);
    require(Array.isArray(entry.evidence) && entry.evidence.length > 0, `control.${control}.evidence`);
    for (const evidencePath of entry.evidence || []) {
      require(typeof evidencePath === 'string' && evidencePath.length > 0, `control.${control}.evidence.path`);
      if (typeof evidencePath === 'string' && !evidencePath.includes('#') && !evidencePath.startsWith('runtime/')) {
        require(fs.existsSync(path.join(root, evidencePath)), `control.${control}.evidence.exists:${evidencePath}`);
      }
    }
    if (options.requireSignedOff) {
      require(entry.status === 'signed-off', `control.${control}.signed-off`);
    }
  }

  for (const relativePath of requiredSourceHashes) {
    require(Object.hasOwn(candidate.source_hashes || {}, relativePath), `source_hash.${relativePath}.required`);
  }

  for (const [relativePath, expectedHash] of Object.entries(candidate.source_hashes || {})) {
    require(typeof expectedHash === 'string' && expectedHash.length > 0, `source_hash.${relativePath}.value`);
    const absolutePath = path.join(root, relativePath);
    require(fs.existsSync(absolutePath), `source_hash.${relativePath}.exists`);
    if (options.requireSignedOff && fs.existsSync(absolutePath)) {
      require(/^[0-9a-f]{64}$/i.test(expectedHash), `source_hash.${relativePath}.sha256`);
      if (/^[0-9a-f]{64}$/i.test(expectedHash)) {
        const actualHash = sha256File(absolutePath);
        require(actualHash === expectedHash.toLowerCase(), `source_hash.${relativePath}.match`);
      }
    }
  }

  const workflow = readText(workflowPath);
  const humanSignoff = readText(humanSignoffPath);
  for (const control of requiredControls) {
    require(humanSignoff.includes(`\`${control}\``), `human_signoff.control.${control}`);
  }
  require(workflow.includes('node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs --require-signed-off'), 'release.workflow.require_signoff');
  require(workflow.includes('business-os-security-privacy-signoff-validation'), 'release.workflow.upload_validation');

  if (options.requireSignedOff) {
    require(candidate.status === 'signed-off', 'signoff.status.signed-off');
    require(candidate.reviewer !== 'TBD', 'signoff.reviewer.signed-off');
    require(/^\d{4}-\d{2}-\d{2}$/.test(candidate.reviewed_at), 'signoff.reviewed_at.date');
    require(/^[0-9a-f]{7,40}$/i.test(candidate.evidence_revision), 'signoff.evidence_revision.git');
  }

  return problems;
}

function writeValidationArtifact(signoff, problems) {
  const validation = {
    schema: VALIDATION_SCHEMA,
    signoff_schema: SIGNOFF_SCHEMA,
    require_signed_off: requireSignedOff,
    ok: problems.length === 0,
    status: signoff?.status || null,
    reviewer: signoff?.reviewer || null,
    reviewed_at: signoff?.reviewed_at || null,
    evidence_revision: signoff?.evidence_revision || null,
    required_controls: requiredControls,
    required_source_hashes: requiredSourceHashes,
    source_hashes_checked: Object.keys(signoff?.source_hashes || {}).sort(),
    problems,
    generated_at: new Date().toISOString(),
  };
  fs.mkdirSync(path.dirname(validationPath), { recursive: true });
  fs.writeFileSync(validationPath, `${JSON.stringify(validation, null, 2)}\n`);
}

function readJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    return { schema: '', status: '', controls: {}, source_hashes: {}, notes: [], read_error: error.message };
  }
}

function readText(filePath) {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch {
    return '';
  }
}

function sha256File(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function runSelfTest() {
  const fixture = {
    schema: SIGNOFF_SCHEMA,
    status: 'signed-off',
    reviewer: 'Reviewer',
    reviewed_at: '2026-06-18',
    evidence_revision: '0123456789abcdef',
    controls: Object.fromEntries(requiredControls.map((control) => [
      control,
      { status: 'signed-off', evidence: ['docs/business-os-roles-permissions-plan.md'] },
    ])),
    source_hashes: Object.fromEntries(requiredSourceHashes.map((relativePath) => [
      relativePath,
      sha256File(path.join(root, relativePath)),
    ])),
    notes: [],
  };
  const validProblems = validateSecurityPrivacySignoff(fixture, { requireSignedOff: true });
  if (validProblems.length) {
    throw new Error(`valid signoff fixture failed: ${validProblems.join(',')}`);
  }
  assertThrows('missing control', () => {
    const broken = JSON.parse(JSON.stringify(fixture));
    delete broken.controls.dynamic_app_runtime_boundary;
    const problems = validateSecurityPrivacySignoff(broken, { requireSignedOff: false });
    if (!problems.includes('control.dynamic_app_runtime_boundary')) {
      throw new Error(`unexpected problems: ${problems.join(',')}`);
    }
    throw new Error('expected');
  });
  assertThrows('pending release signoff', () => {
    const broken = JSON.parse(JSON.stringify(fixture));
    broken.status = 'pending-signoff';
    broken.controls.dynamic_app_runtime_boundary.status = 'pending';
    const problems = validateSecurityPrivacySignoff(broken, { requireSignedOff: true });
    if (!problems.includes('signoff.status.signed-off') || !problems.includes('control.dynamic_app_runtime_boundary.signed-off')) {
      throw new Error(`unexpected problems: ${problems.join(',')}`);
    }
    throw new Error('expected');
  });
  assertThrows('source hash mismatch', () => {
    const broken = JSON.parse(JSON.stringify(fixture));
    broken.source_hashes['.github/workflows/release.yml'] = '0'.repeat(64);
    const problems = validateSecurityPrivacySignoff(broken, { requireSignedOff: true });
    if (!problems.includes('source_hash..github/workflows/release.yml.match')) {
      throw new Error(`unexpected problems: ${problems.join(',')}`);
    }
    throw new Error('expected');
  });
  assertThrows('missing required source hash', () => {
    const broken = JSON.parse(JSON.stringify(fixture));
    delete broken.source_hashes['.github/workflows/release.yml'];
    const problems = validateSecurityPrivacySignoff(broken, { requireSignedOff: false });
    if (!problems.includes('source_hash..github/workflows/release.yml.required')) {
      throw new Error(`unexpected problems: ${problems.join(',')}`);
    }
    throw new Error('expected');
  });
  console.log(`business_os_security_privacy_signoff_self_test=1 controls=${requiredControls.join(',')}`);
}

function assertThrows(label, fn) {
  try {
    fn();
  } catch (error) {
    if (error.message === 'expected') return;
    throw error;
  }
  throw new Error(`Expected self-test failure for ${label}`);
}
