import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../../../../..');
const corpusPath = resolve(root, 'tests/fixtures/office/spreadsheet/corpus.json');
const matrix = JSON.parse(await readFile(resolve(here, '../features.json'), 'utf8'));
const corpus = JSON.parse(await readFile(corpusPath, 'utf8'));
const features = new Map(matrix.editors.spreadsheet.features.map((feature) => [feature.id, feature]));
const hashFile = async (path) => createHash('sha256').update(await readFile(path)).digest('hex');
const failures = [];
for (const entry of corpus.entries) {
  const feature = features.get(entry.feature_id);
  if (!feature || feature.status !== 'differential_passed') { failures.push(`${entry.feature_id}: dependency is not differential_passed`); continue; }
  const fixturePath = resolve(corpusPath, '..', entry.file);
  const fixture = await readFile(fixturePath);
  if (fixture.byteLength !== entry.bytes) failures.push(`${entry.feature_id}: fixture byte count drifted`);
  if (await hashFile(fixturePath) !== entry.sha256) failures.push(`${entry.feature_id}: fixture hash drifted`);
  const evidence = JSON.parse(await readFile(resolve(here, '..', feature.evidence), 'utf8'));
  if (evidence.status !== 'differential_passed') failures.push(`${entry.feature_id}: evidence status is not differential_passed`);
  if (evidence.fixture?.sha256 !== entry.sha256) failures.push(`${entry.feature_id}: evidence fixture hash differs from corpus`);
  const runtime = evidence.ctox?.runtime ?? evidence.ctox_frontend_port?.runtime;
  const runtimeGate = evidence.side_by_side?.runtime_provenance_gate;
  if (runtime !== 'ctox-spreadsheets-fork' && runtimeGate !== 'passed') failures.push(`${entry.feature_id}: CTOX Spreadsheets fork provenance is missing`);
  const reopen = evidence.ctox?.reopen;
  const canonical = evidence.ctox?.canonical_export ?? evidence.ctox?.export;
  const legacyExport = evidence.rust_export;
  const ctoxReady = reopen?.ctox === 'document-ready' || canonical?.reopen_in_ctox === 'passed' || canonical?.ctox_clean_profile_reopen === 'passed' || legacyExport?.ctox_reopen === 'passed' || evidence.side_by_side?.ctox_export_reopen?.ctox === 'passed';
  const oracleReady = reopen?.oracle === 'document-ready' || canonical?.oracle_reopen === 'passed' || canonical?.reopen_in_oracle === 'passed' || canonical?.oracle_clean_profile_reopen === 'passed' || legacyExport?.oracle_reopen === 'passed' || evidence.side_by_side?.ctox_export_reopen?.oracle === 'passed';
  if (!ctoxReady && entry.feature_id !== 'spreadsheet.open-render-sheets') failures.push(`${entry.feature_id}: CTOX reopen evidence missing`);
  if (!oracleReady && entry.feature_id !== 'spreadsheet.open-render-sheets') failures.push(`${entry.feature_id}: Oracle reopen evidence missing`);
  const businessOs = evidence.ctox?.business_os_mount ?? evidence.business_os_wrapper;
  const businessTransport = businessOs?.transport ?? businessOs?.command_transport;
  const businessScreenshot = evidence.screenshots?.some(({ path }) => path.includes('business-os-mount'));
  if (businessTransport !== 'rxdb-webrtc' && !businessScreenshot) failures.push(`${entry.feature_id}: Business OS RxDB/WebRTC mount evidence missing`);
  if (evidence.browser_health && (evidence.browser_health.console_errors !== 0 || evidence.browser_health.http_business_data_routes !== false)) failures.push(`${entry.feature_id}: browser health or HTTP data boundary failed`);
}
if (failures.length) { console.error(failures.map((failure) => `- ${failure}`).join('\n')); process.exitCode = 1; }
else console.log(`CTOX spreadsheet corpus OK (${corpus.entries.length} fixtures, ${corpus.entries.reduce((sum, entry) => sum + entry.parts, 0)} package parts declared)`);
