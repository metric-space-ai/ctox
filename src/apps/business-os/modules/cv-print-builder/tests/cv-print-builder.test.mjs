import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');

const manifest = JSON.parse(read('module.json'));
const registry = JSON.parse(read('../registry.json'));
const registryEntry = registry.modules.find((item) => item.id === 'cv-print-builder');

assert.equal(manifest.id, 'cv-print-builder');
assert.equal(manifest.entry, 'modules/cv-print-builder/index.html');
assert.equal(manifest.install_scope, 'store');
assert.equal(manifest.default_installed, false);
assert.ok(manifest.collections.includes('documents'));
assert.ok(manifest.collections.includes('document_versions'));
assert.ok(manifest.collections.includes('desktop_files'));
assert.ok(manifest.collections.includes('business_commands'));
assert.ok(manifest.collections.includes('ctox_queue_tasks'));

assert.ok(registryEntry, 'registry entry exists');
assert.equal(registryEntry.entry, manifest.entry);
assert.equal(registryEntry.install_scope, manifest.install_scope);
assert.deepEqual(registryEntry.collections, manifest.collections);

const source = read('index.js');
const markup = read('index.html');
const sourceArray = (name) => {
  const marker = `const ${name} = [`;
  const start = source.indexOf(marker);
  assert.notEqual(start, -1, `${name} is declared`);
  const end = source.indexOf('];', start);
  assert.notEqual(end, -1, `${name} array is closed`);
  return source.slice(start + marker.length, end);
};
const requiredCollections = sourceArray('REQUIRED_COLLECTIONS');
const liveCollections = sourceArray('LIVE_COLLECTIONS');
assert.match(markup, /multiple\s+data-cv-upload/);
assert.match(markup, /data-pg-search/);
assert.match(markup, /data-pg-filter\s+data-pg-name="sort"/);
assert.match(markup, /data-cv-reparse-all/);
assert.match(source, /label:\s*'Minimal'/);
assert.match(source, /label:\s*'Klassisch'/);
assert.match(source, /label:\s*'Modern'/);
assert.match(source, /async function importPdfs/);
assert.match(source, /title:\s*tr\('Print freigeben',\s*'Approve print'\)/);
assert.match(source, /phase:\s*'approved'/);
assert.match(source, /view_mode:\s*'print'/);
assert.match(source, /applyLiveWorkflowState/);
assert.match(source, /buildLiveStatusIndex/);
assert.match(source, /const commandId = `cmd_\$\{crypto\.randomUUID\(\)\}`/);
assert.match(source, /id:\s*commandId/);
assert.match(source, /command_type:\s*'business_os\.chat\.task'/);
assert.match(source, /command_type:\s*'business_os\.chat\.task'/);
assert.match(source, /task_status:\s*'queued'/);
assert.match(source, /ctx\.commandBus\.dispatch/);
assert.match(source, /Flow öffnen/);
assert.match(source, /record_snapshot:\s*parserRecordSnapshot\(item\)/);
assert.doesNotMatch(source, /attachments:\s*\[/);
assert.doesNotMatch(source, /async function insertBusinessCommand/);
assert.match(source, /sync_collections:\s*\[/);
assert.match(source, /'desktop_file_chunks'/);
assert.doesNotMatch(requiredCollections, /'desktop_file_chunks'/);
assert.doesNotMatch(liveCollections, /'desktop_file_chunks'/);
assert.match(source, /'business_chats'/);
assert.match(source, /'ctox_queue_tasks'/);
assert.match(source, /data-cv-open-task/);
assert.match(source, /skill:\s*'ctox-cv-print-parser'/);
assert.match(source, /command_type:\s*'ctox\.cv_print\.apply_parse'/);
assert.match(source, /const taskInstruction = buildParserTaskInstruction\(item\)/);
assert.match(source, /instruction:\s*taskInstruction/);
assert.doesNotMatch(source, /instruction:\s*prompt,\s*\n\s*prompt,/);
assert.match(source, /Keine Tools, keine Shell/);
assert.match(source, /function renderPrintPane/);
assert.match(source, /function renderPrintSheet/);
assert.match(source, /reference:\s*'NinjaWorkflowTool_Extension\/find-job-for-candidate qualification profile'/);
assert.match(source, /expected_model_schema:\s*'ctox\.cv_print_profile\.v1'/);
assert.match(source, /QUALIFIKATIONSPROFIL/);
assert.match(source, /METHODEN- \/ SYSTEMKOMPETENZ \/ SPRACHKENNTNISSE/);
assert.match(source, /renderFieldEditor/);
assert.match(source, /data-cv-field-editor/);
assert.match(source, /function safePathParts\(path\)/);
assert.match(source, /editable field path contains unsafe prototype segment/);
assert.match(source, /segment === '__proto__' \|\| segment === 'prototype' \|\| segment === 'constructor'/);
assert.match(source, /cv\.experience: Array mit \{job_title,employer,location,start_date,end_date,job_description\[\]\}/);
assert.match(source, /cv\.education: Array mit \{degree,institution,major,specialization,location,start_date,end_date,details\[\]\}/);
assert.match(source, /cv\.skills: Objekt mit Gruppen/);
assert.doesNotMatch(source, /Limits: max\./);
assert.match(source, /importPdf\(state, file, \{ refresh: false, select: false \}\)/);
assert.match(source, /return \{ documentId, fileId \}/);
assert.match(source, /CONTENT_HASH_SCHEME\s*=\s*'sha256-bytes-v1'/);
assert.match(source, /CHUNK_HASH_SCHEME\s*=\s*'sha256-base64-chunk-v1'/);
assert.match(source, /CHUNK_SIZE\s*=\s*16\s*\*\s*1024/);
assert.match(source, /chunk_hash:\s*chunkHash/);
assert.match(source, /id:\s*canonicalDesktopFileChunkId\(input\.fileId,\s*input\.generationId,\s*idx\)/);
assert.match(source, /function canonicalDesktopFileChunkId\(fileId,\s*generationId,\s*idx\)/);
assert.match(source, /readStoredFileFromDemandChunks/);
assert.doesNotMatch(source, /findAll\(state\.ctx,\s*'desktop_file_chunks'\)/);
assert.match(source, /ensureParseSourceReady\(state,\s*item\)/);
assert.match(source, /verifyDesktopFileSourceAvailable/);
assert.match(source, /readDesktopFileFromDemand\(ctx,\s*fileId,\s*'application\/pdf'/);
assert.doesNotMatch(source, /ensureCanonicalDesktopFileChunks/);
assert.doesNotMatch(source, /readDesktopFileForCanonicalRepair/);
assert.doesNotMatch(source, /fetchDesktopFileChunks\(ctx,\s*fileId,\s*generationId\)/);
assert.match(source, /flushFileCollectionsForDispatch/);
assert.doesNotMatch(source, /const blob = await readDesktopFileForCanonicalRepair\(ctx,\s*fileId,\s*generationId,\s*contentHash\)/);
assert.doesNotMatch(source, /await writeChunkDocuments\(chunksCol,\s*chunkRows\)/);
assert.doesNotMatch(source, /String\(chunk\.bytesBase64 \?\? chunk\.bytes_base64 \?\? ''\)/);
assert.match(source, /source_prepare:\s*sourcePrepare/);
assert.match(source, /source_prepare_ready:\s*sourcePrepare\.ready/);
assert.match(source, /timeoutAfter\(60000/);
assert.match(source, /pushToRemotePeers/);
assert.match(source, /DEMAND_ONLY_SYNC_COLLECTIONS/);
assert.match(source, /sync\.leaseCollection\(collection,\s*reason\)/);
assert.match(source, /releaseSyncLeases/);
assert.match(source, /bulkUpsert\(docs\)/);
assert.match(source, /function reparseCandidates\(state\)/);
assert.match(source, /source\.desktop_file_id\)/);
assert.doesNotMatch(source, /source\.desktop_file_id && source\.generation_id/);
assert.match(source, /state\.importing \|\| state\.bulkParsing/);
assert.doesNotMatch(source, /Boolean\(busy\) \|\| !state\.ready \|\| !reparseCandidates/);
assert.match(source, /async function reparseAllPdfs\(state,\s*candidates = reparseCandidates\(state\)\)/);
assert.match(source, /Alle CV-PDFs wurden erneut an CTOX uebergeben/);
assert.match(source, /originalErrors:\s*new Map\(\)/);
assert.match(source, /Original-PDF konnte nicht geladen werden/);
assert.match(source, /\.catch\(\(error\) =>/);
assert.match(source, /PDF-Daten konnten nicht lokal vorbereitet werden/);
assert.match(source, /!sourcePrepare\.generation_id/);
assert.match(source, /dispatching parser task with native source fallback/);
assert.match(source, /timeoutAfter\(60000/);
assert.doesNotMatch(source, /Parser-Task wird trotzdem gestartet/);
assert.doesNotMatch(source, /ctx\.sync\.startCollection\('desktop_file_chunks'\)/);

// ---------------------------------------------------------------------------
// IA-Karte: candidate selector (left) + CV stage (main) on the shell-owned
// canonical column grammar.
// ---------------------------------------------------------------------------
const css = read('index.css');

// Canonical column grammar — shell-wired data-pg-* chrome, no bespoke chrome.
assert.match(markup, /data-pg-search/);
assert.match(markup, /data-pg-view="cards"/);
assert.match(markup, /data-pg-view="list"/);
assert.match(markup, /data-pg-tray-toggle/);
assert.match(markup, /data-pg-tray\b/);
assert.match(markup, /data-pg-reset/);
assert.match(markup, /class="ctox-filterbar"/);
assert.match(markup, /class="ctox-view-switch"/);
assert.match(markup, /ctox-pane-body ctox-well/);
assert.match(markup, /class="ctox-pane-footer"/);
assert.match(markup, /data-pg-footer/);

// Standing header actions include create + import + export (JSON round-trip).
assert.match(markup, /data-action="new"/);
assert.match(markup, /data-action="import"/);
assert.match(markup, /data-action="export"/);

// Counted view band with >= 2 real views (status partition), zeros included.
for (const band of ['all', 'progress', 'review', 'approved']) {
  assert.match(markup, new RegExp(`data-pg-band="${band}"`), `band ${band} present`);
  assert.match(markup, new RegExp(`data-pg-count="${band}"`), `band count ${band} present`);
}
const bandTabCount = (markup.match(/data-pg-band="/g) || []).length;
assert.ok(bandTabCount >= 2, 'view band has at least two real views');
assert.match(source, /function bandOf\(model\)/);
assert.match(source, /function bandCounts\(state\)/);
assert.match(source, /writeCounts\(state,\s*bandCounts\(state\)\)/);

// No standing status badge / legacy bespoke chrome anymore.
assert.doesNotMatch(markup, /data-cv-count/);
assert.doesNotMatch(markup, /data-toggle-bulk/);
assert.doesNotMatch(markup, /cv-new-card/);
assert.doesNotMatch(css, /box-shadow:\s*inset/);

// Import / export handlers (JSON via Blob / file-input, honest and small).
assert.match(markup, /data-cv-import/);
assert.match(source, /function exportCandidates\(state\)/);
assert.match(source, /function importCandidates\(state,\s*file\)/);
assert.match(source, /function persistImportedProfile\(state,\s*entry\)/);
assert.match(source, /new Blob\(\[JSON\.stringify\(payload/);
assert.match(source, /source_kind:\s*'cv_profile_import'/);

// In-place selection flip — selecting a candidate must NOT rebuild the list.
assert.match(source, /function applyListSelection\(state\)/);
assert.match(source, /row\.classList\.toggle\('is-selected'/);
assert.match(source, /function selectCandidate\(state,\s*id\)/);
const selectBody = source.slice(source.indexOf('function selectCandidate(state, id)'), source.indexOf('function selectCandidate(state, id)') + 500);
assert.match(selectBody, /applyListSelection\(state\)/);
assert.match(selectBody, /renderStage\(state\)/);
assert.doesNotMatch(selectBody, /renderList\(state\)/);

// Auto-reveal stage on select: visible = hasSelection && !userCollapsed.
assert.match(source, /function stageVisible\(state\)/);
assert.match(source, /Boolean\(getSelectedItem\(state\)\)\s*&&\s*!state\.userCollapsed/);
assert.match(source, /data-toggle-stage/);

// Markup + CSS carry the JS cache-buster (fresh JS over stale cached assets).
assert.match(source, /index\.html'\s*,\s*import\.meta\.url\)\.pathname\}\?v=\$\{BUILD\}/);
assert.match(source, /index\.css'\s*,\s*import\.meta\.url\)\.pathname\}\?v=\$\{BUILD\}/);

console.log('cv-print-builder module contract OK');
