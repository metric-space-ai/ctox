import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const officeRoot = new URL('./', import.meta.url);
const repositoryRoot = new URL('../../../../', officeRoot);
const readJson = async (relative, root = officeRoot) => JSON.parse(await readFile(new URL(relative, root), 'utf8'));
const readText = async (relative, root = officeRoot) => readFile(new URL(relative, root), 'utf8');
const semverParts = (value) => value.split('.').map(Number);
const compareSemver = (left, right) => {
  const a = semverParts(left);
  const b = semverParts(right);
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return 0;
};

const rollout = await readJson('rollout.json');
const matrix = await readJson('features.json');
const restartEvidence = await readJson('oracle/evidence/office.native-peer-restart.json');
const documents = await readText('../modules/documents/index.js');
const spreadsheets = await readText('../modules/spreadsheets/index.js');
const store = await readText('../../../core/business_os/store.rs');
const soakWorkflow = await readText('../../../../.github/workflows/rxdb-soak.yml');
const releaseWorkflow = await readText('../../../../.github/workflows/release.yml');

assert.equal(rollout.schema_version, 'ctox-office-rollout-v2');
assert.deepEqual(rollout.default_engines, {
  document: 'ctox_documents',
  spreadsheet: 'ctox_spreadsheets',
});
assert.match(rollout.pre_switch_baseline.last_published_release_version, /^\d+\.\d+\.\d+$/);
assert.match(rollout.pre_switch_baseline.highest_existing_tag_version, /^\d+\.\d+\.\d+$/);
assert.match(rollout.pre_switch_baseline.last_published_release_git_revision, /^[0-9a-f]{40}$/);
assert.match(rollout.pre_switch_baseline.highest_existing_tag_git_revision, /^[0-9a-f]{40}$/);
assert.equal(rollout.pre_switch_baseline.last_published_release_url,
  `https://github.com/metric-space-ai/ctox/releases/tag/v${rollout.pre_switch_baseline.last_published_release_version}`);
assert.equal(rollout.pre_switch_baseline.highest_existing_tag_url,
  `https://github.com/metric-space-ai/ctox/releases/tag/v${rollout.pre_switch_baseline.highest_existing_tag_version}`);
assert.ok(compareSemver(
  rollout.pre_switch_baseline.highest_existing_tag_version,
  rollout.pre_switch_baseline.last_published_release_version,
) >= 0);
assert.ok(Number.isInteger(rollout.minimum_stable_releases_after_switch));
assert.ok(rollout.minimum_stable_releases_after_switch >= 1);
assert.ok(Array.isArray(rollout.qualifying_releases));

const features = Object.values(matrix.editors).flatMap((editor) => editor.features);
assert.equal(features.length, 24);
const expectedStatus = rollout.legacy_removal_authorized
  ? rollout.feature_status_after_authorization
  : rollout.feature_status_before_authorization;
for (const feature of features) assert.equal(feature.status, expectedStatus, `${feature.id} rollout status`);

assert.match(documents, /officeEngine:\s*'ctox_documents'/);
assert.match(spreadsheets, /officeEngine:\s*'ctox_spreadsheets'/);
for (const source of [documents, spreadsheets]) {
  if (!rollout.legacy_removal_authorized) assert.match(source, /===\s*'legacy'/, 'typed Legacy rollback must remain before authorization');
}
assert.match(store, /"documents_engine":\s*"ctox_documents"/);
assert.match(store, /"spreadsheets_engine":\s*"ctox_spreadsheets"/);
if (!rollout.legacy_removal_authorized) assert.match(store, /\(_,\s*"legacy"\)\s*=>\s*Ok\("legacy"\)/);

assert.equal(restartEvidence.status, 'passed');
assert.deepEqual(restartEvidence.cases.map(({ kind, status }) => ({ kind, status })), [
  { kind: 'document', status: 'completed' },
  { kind: 'spreadsheet', status: 'completed' },
]);
for (const mode of [
  'office-document-midflight-restart-browser-to-rust',
  'office-spreadsheet-midflight-restart-browser-to-rust',
]) {
  assert.match(soakWorkflow, new RegExp(`SMOKE_MODES: ${mode}`));
}
assert.equal((soakWorkflow.match(/SMOKE_MATRIX_ATTEMPTS: "1"/g) || []).length >= 2, true);
assert.match(releaseWorkflow, /capture-release-evidence\.mjs/);
assert.match(releaseWorkflow, /office-document-restart-matrix\.json/);
assert.match(releaseWorkflow, /office-spreadsheet-restart-matrix\.json/);
assert.match(releaseWorkflow, /ctox-office-release-candidate-evidence\.json/);
assert.match(releaseWorkflow, /artifacts\/\*\*\/ctox-office-release-candidate-evidence\.json/);

const requiredFields = rollout.release_evidence_contract.required_fields;
const switchRelease = rollout.default_switch_release;
if (switchRelease !== null) {
  for (const field of rollout.release_evidence_contract.default_switch_release_required_fields) {
    assert.ok(String(switchRelease[field] || '').trim(), `default switch release misses ${field}`);
  }
  assert.match(switchRelease.version, /^\d+\.\d+\.\d+$/);
  assert.ok(compareSemver(switchRelease.version, rollout.pre_switch_baseline.last_published_release_version) > 0);
  assert.ok(compareSemver(switchRelease.version, rollout.pre_switch_baseline.highest_existing_tag_version) > 0);
  assert.equal(switchRelease.release_workflow_status, 'passed');
  assert.equal(switchRelease.office_restart_retry_count, 0);
}
const versions = new Set();
for (const release of rollout.qualifying_releases) {
  for (const field of requiredFields) assert.ok(String(release[field] || '').trim(), `${release.version || 'release'} misses ${field}`);
  assert.match(release.version, /^\d+\.\d+\.\d+$/);
  assert.ok(switchRelease, 'a stable qualifying release requires prior default-switch release evidence');
  assert.ok(compareSemver(release.version, switchRelease.version) > 0, 'the stable observation release must follow the switch release');
  assert.equal(release.release_workflow_status, 'passed');
  assert.equal(release.office_restart_retry_count, 0);
  assert.equal(versions.has(release.version), false, `duplicate qualifying release ${release.version}`);
  versions.add(release.version);
}

const hasStablePeriod = Boolean(switchRelease)
  && rollout.qualifying_releases.length >= rollout.minimum_stable_releases_after_switch;
assert.equal(rollout.legacy_removal_authorized, hasStablePeriod,
  'legacy_removal_authorized must exactly reflect the required stable release evidence');

console.log(`CTOX product rollout OK: documents=${rollout.default_engines.document}, spreadsheets=${rollout.default_engines.spreadsheet}, stable_releases=${rollout.qualifying_releases.length}/${rollout.minimum_stable_releases_after_switch}, legacy_removal_authorized=${rollout.legacy_removal_authorized}`);
