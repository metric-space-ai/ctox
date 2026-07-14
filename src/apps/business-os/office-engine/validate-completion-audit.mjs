import assert from 'node:assert/strict';
import { access, readFile } from 'node:fs/promises';

const root = new URL('./', import.meta.url);
const repo = new URL('../../../../', root);
const readJson = async (path, base = root) => JSON.parse(await readFile(new URL(path, base), 'utf8'));
const readText = (path, base = root) => readFile(new URL(path, base), 'utf8');

const audit = await readJson('completion-audit.json');
const matrix = await readJson('features.json');
const rollout = await readJson('rollout.json');
const securitySignoff = await readJson('../../../../docs/business-os-security-privacy-signoff.json');
const pin = await readJson('upstream/euro-office-v9.3.1.json');
const provenance = await readJson('../vendor/ctox-office/provenance.json');
const ui = await readJson('oracle/evidence/office.fork-business-os-ui.json');
const docCorpus = await readJson('oracle/evidence/document.docx-roundtrip-corpus.json');
const sheetCorpus = await readJson('oracle/evidence/spreadsheet.xlsx-roundtrip-corpus.json');
const documentsManifest = await readJson('../modules/documents/module.json');
const spreadsheetsManifest = await readJson('../modules/spreadsheets/module.json');
const documents = await readText('../modules/documents/index.js');
const spreadsheets = await readText('../modules/spreadsheets/index.js');
const documentsBundle = await readText('../vendor/ctox-office/ctox-office-document.mjs');
const spreadsheetsBundle = await readText('../vendor/ctox-office/ctox-office-spreadsheet.mjs');
const rustEngine = await readText('../../../core/business_os/office_engine.rs');
const rustStore = await readText('../../../core/business_os/store.rs');
const plan = await readText('../../../../docs/ctox-office-port-plan.md', root);

assert.equal(audit.schema_version, 'ctox-office-completion-audit-v1');
assert.equal(audit.overall_status, 'release_observation_pending');
assert.equal(audit.verified_workstreams, 9);
assert.equal(audit.total_workstreams, 10);
assert.deepEqual(audit.products, ['ctox-documents', 'ctox-spreadsheets']);
assert.deepEqual(audit.requirements.map(({ id }) => id), ['A1', 'A2', 'A3', 'A3b', 'A4', 'A5', 'A6', 'A7', 'A8', 'A9', 'A10']);
assert.ok(audit.requirements.slice(0, -1).every(({ status }) => status === 'verified'));
assert.equal(audit.requirements.at(-1).status, 'release_observation_pending');
for (const requirement of audit.requirements) {
  for (const evidence of requirement.evidence) await access(new URL(evidence, root));
}
assert.equal(audit.release_prerequisites.technical_office_matrix.status, 'verified');
assert.match(audit.release_prerequisites.technical_office_matrix.evidence_run, /^https:\/\/github\.com\/metric-space-ai\/ctox\/actions\/runs\/\d+$/);
assert.equal(audit.release_prerequisites.operational_evidence.status, 'pending');
assert.deepEqual(audit.release_prerequisites.operational_evidence.required, [
  'nightly_soak_9x33_no_retry',
  'canary_72h',
  'wan_turn_matrix',
  'native_restore_drill',
  'runbook_exercises',
  'record_workbench_30_day_pilot',
  'workflow_30_day_pilot',
]);
assert.equal(audit.release_prerequisites.security_privacy_signoff.status, securitySignoff.status);
assert.equal(audit.release_prerequisites.security_privacy_signoff.required_controls, Object.keys(securitySignoff.controls).length);
assert.equal(audit.release_prerequisites.release_observation.status, 'pending');
assert.equal(audit.release_prerequisites.release_observation.minimum_switch_release, '0.3.32');
assert.equal(audit.release_prerequisites.release_observation.minimum_stable_followup_releases, rollout.minimum_stable_releases_after_switch);

assert.equal(pin.release, 'v9.3.1');
assert.match(pin.commit_sha, /^[0-9a-f]{40}$/);
for (const sha of Object.values(pin.submodules)) assert.match(sha, /^[0-9a-f]{40}$/);
assert.match(pin.oracle_image.index_digest, /^sha256:[0-9a-f]{64}$/);
assert.deepEqual(provenance.license_inventory, [
  { component: 'CTOX Documents fork', license: 'AGPL-3.0-only', origin: 'CTOX' },
  { component: 'CTOX Spreadsheets fork', license: 'AGPL-3.0-only', origin: 'CTOX' },
  { component: 'Euro-Office upstream ancestry', license: pin.license, origin: pin.release_url },
]);

assert.deepEqual(provenance.fork_products.map(({ product_id }) => product_id), ['ctox-documents', 'ctox-spreadsheets']);
const artifactPaths = provenance.artifacts.map(({ path }) => path);
for (const suffix of [
  '/runtime/ctox-documents.mjs',
  '/runtime/ctox-spreadsheets.mjs',
  '/runtime/ctox-fork-core.mjs',
  '/forks/ctox-documents/manifest.json',
  '/forks/ctox-spreadsheets/manifest.json',
  '/forks/shared/business-os.css',
]) assert.ok(artifactPaths.some((path) => path.endsWith(suffix)), `missing product artifact ${suffix}`);
assert.equal(artifactPaths.some((path) => path.endsWith('/runtime/document.mjs')), false);
assert.equal(artifactPaths.some((path) => path.endsWith('/runtime/spreadsheet.mjs')), false);

const features = Object.values(matrix.editors).flatMap(({ features }) => features);
assert.equal(features.length, 24);
assert.ok(features.every(({ status }) => status === 'differential_passed'));
for (const feature of features) {
  await access(new URL(feature.evidence, root));
  await access(new URL(feature.flow, root));
}

assert.equal(ui.status, 'passed');
assert.deepEqual(ui.products, ['ctox-documents', 'ctox-spreadsheets']);
assert.deepEqual(ui.required_locales, ['de', 'en']);
assert.deepEqual(ui.required_themes, ['light', 'dark']);
assert.deepEqual(ui.required_widths, [360, 640, 1600]);
assert.equal(ui.results.length, 8);
assert.ok(ui.results.every(({ status }) => status === 'passed'));
assert.equal(ui.assertions.visible_foreign_brand, false);
assert.equal(ui.assertions.browser_errors, 0);

assert.match(documents, /vendor\/ctox-office\/ctox-office-document\.mjs/);
assert.match(documents, /createCtoxDocumentsEditor/);
assert.match(documents, /kind:\s*'ctox-documents'/);
assert.match(documents, /officeEngine:\s*'ctox_documents'/);
assert.match(spreadsheets, /vendor\/ctox-office\/ctox-office-spreadsheet\.mjs/);
assert.match(spreadsheets, /createCtoxSpreadsheetsEditor/);
assert.match(spreadsheets, /kind:\s*'ctox-spreadsheets'/);
assert.match(spreadsheets, /officeEngine:\s*'ctox_spreadsheets'/);
assert.equal(documentsManifest.title, 'CTOX Documents');
assert.equal(spreadsheetsManifest.title, 'CTOX Spreadsheets');
assert.match(documentsBundle, /createCtoxDocumentsEditor/);
assert.match(documentsBundle, /CTOX_DOCUMENTS_PRODUCT_ID/);
assert.match(spreadsheetsBundle, /createCtoxSpreadsheetsEditor/);
assert.match(spreadsheetsBundle, /CTOX_SPREADSHEETS_PRODUCT_ID/);
for (const source of [documents, spreadsheets]) {
  assert.match(source, /ctx\.db/);
  assert.match(source, /ctx\.commandBus/);
}

for (const operation of ['prepare', 'apply_changes', 'export', 'inspect']) {
  assert.match(rustEngine, new RegExp(`pub fn ${operation}\\b`));
}
for (const command of [
  'office.document.prepare', 'office.document.commit', 'office.document.export',
  'office.spreadsheet.prepare', 'office.spreadsheet.commit', 'office.spreadsheet.export',
]) assert.ok(rustStore.includes(command), `missing native command ${command}`);

assert.equal(docCorpus.status, 'differential_passed');
assert.equal(docCorpus.corpus.fixtures, 11);
assert.equal(docCorpus.corpus.declared_package_parts, 204);
assert.equal(sheetCorpus.status, 'differential_passed');
assert.equal(sheetCorpus.fixture.entries, 11);
assert.equal(sheetCorpus.fixture.package_parts, 147);
assert.equal(sheetCorpus.ctox.native_identity_roundtrip.all_original_parts_byte_identical, true);

assert.deepEqual(rollout.default_engines, { document: 'ctox_documents', spreadsheet: 'ctox_spreadsheets' });
assert.equal(rollout.default_switch_release, null);
assert.deepEqual(rollout.qualifying_releases, []);
assert.equal(rollout.legacy_removal_authorized, false);
assert.equal(audit.external_release_observation.latest_published_release, rollout.pre_switch_baseline.last_published_release_version);
assert.equal(audit.external_release_observation.legacy_removal_authorized, rollout.legacy_removal_authorized);
assert.match(plan, /Gesamtfortschritt: 9 von 10 Arbeitsstroemen/);
assert.match(plan, /Die technischen Arbeitsstroeme A1 bis A9 sind abgenommen/);
assert.match(plan, /72-Stunden-Canary/);
assert.match(plan, /zwei 30-Tage-Piloten/);
assert.match(plan, /Security-\/Privacy-Freigabe/);

console.log('CTOX Documents/Spreadsheets completion audit OK: A1-A9 verified, A10 release observation pending');
