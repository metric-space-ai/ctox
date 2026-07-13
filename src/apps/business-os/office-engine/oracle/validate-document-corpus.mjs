import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../../../../..');
const corpusPath = resolve(root, 'tests/fixtures/office/document/corpus.json');
const matrixPath = resolve(here, '../features.json');
const corpus = JSON.parse(await readFile(corpusPath, 'utf8'));
const matrix = JSON.parse(await readFile(matrixPath, 'utf8'));
const matrixById = new Map(matrix.editors.document.features.map((feature) => [feature.id, feature]));

const hashFile = async (path) => createHash('sha256').update(await readFile(path)).digest('hex');
const conventionalCanonicalPath = (featureId) =>
  resolve(root, `output/playwright/ctox-office/ctox/${featureId}/ctox-canonical.docx`);
const failures = [];
for (const entry of corpus.entries) {
  const feature = matrixById.get(entry.feature_id);
  if (!feature || feature.status !== 'differential_passed') {
    failures.push(`${entry.feature_id}: feature is not differential_passed`);
    continue;
  }
  const fixturePath = resolve(corpusPath, '..', entry.file);
  const fixture = await readFile(fixturePath);
  if (fixture.byteLength !== entry.bytes) failures.push(`${entry.feature_id}: fixture byte count drifted`);
  if (await hashFile(fixturePath) !== entry.sha256) failures.push(`${entry.feature_id}: fixture sha256 drifted`);
  const evidencePath = resolve(here, '..', feature.evidence);
  const evidence = JSON.parse(await readFile(evidencePath, 'utf8'));
  if (evidence.status !== 'differential_passed') failures.push(`${entry.feature_id}: evidence status is not differential_passed`);
  if (evidence.fixture?.sha256 !== entry.sha256) failures.push(`${entry.feature_id}: evidence fixture hash differs from corpus`);
  const canonical = evidence.ctox?.canonical_export ?? evidence.ctox?.export;
  const canonicalPath = canonical?.path
    ? resolve(root, canonical.path)
    : entry.feature_id === 'document.open-render-zoom'
      ? fixturePath
      : conventionalCanonicalPath(entry.feature_id);
  if (!canonical?.sha256) {
    failures.push(`${entry.feature_id}: canonical export evidence is missing`);
  } else {
    if (await hashFile(canonicalPath) !== canonical.sha256) failures.push(`${entry.feature_id}: canonical export hash drifted`);
    const oracleReopen = canonical.oracle_reopen ?? canonical.reopen_in_oracle;
    if (oracleReopen !== 'passed') failures.push(`${entry.feature_id}: Oracle reopen did not pass`);
    const preservation = evidence.ctox?.required_preservation ?? {};
    if ((canonical.missing_original_parts ?? preservation.missing_original_parts)?.length) {
      failures.push(`${entry.feature_id}: original parts are missing`);
    }
    if ((canonical.unintended_added_parts ?? preservation.unintended_added_parts ?? canonical.added_parts)?.length) {
      failures.push(`${entry.feature_id}: unintended parts were added`);
    }
  }
  const semantic = evidence.ctox?.semantic_verification;
  const ctoxReopen = semantic?.save_reopen_in_ctox ?? canonical?.reopen_in_ctox;
  const identityOpenFeature = entry.feature_id === 'document.open-render-zoom'
    && canonical?.byte_identical_to_fixture === true;
  if (ctoxReopen !== 'passed' && !identityOpenFeature) failures.push(`${entry.feature_id}: CTOX save/reopen did not pass`);
  if (semantic?.clean_profile_reopen !== 'passed' && evidence.browser_health?.clean_profile_flow_passed !== true) {
    failures.push(`${entry.feature_id}: clean-profile evidence is missing`);
  }
  if (evidence.browser_health?.console_errors !== 0 || evidence.browser_health?.console_warnings !== 0) {
    failures.push(`${entry.feature_id}: browser health is not clean`);
  }
  if (evidence.browser_health?.http_business_data_routes !== false) {
    failures.push(`${entry.feature_id}: HTTP business-data boundary is not proven`);
  }
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'));
  process.exitCode = 1;
} else {
  console.log(`CTOX document corpus OK (${corpus.entries.length} fixtures, ${corpus.entries.reduce((sum, entry) => sum + entry.parts, 0)} package parts declared)`);
}
