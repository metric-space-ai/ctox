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
// v4: nur noch Live-Abos (keine Readiness-Liste); desktop_file_chunks wird nie abonniert.
const liveCollections = sourceArray('LIVE_COLLECTIONS');
assert.match(markup, /multiple\s+data-cv-upload/);
assert.match(markup, /data-pg-search/);
assert.match(markup, /data-pg-filter\s+data-pg-name="sort"/);
assert.match(markup, /data-cv-reparse-all/);
assert.match(source, /label:\s*'Minimal'/);
assert.match(source, /label:\s*'Klassisch'/);
assert.match(source, /label:\s*'Modern'/);
assert.match(source, /async function importPdfs/);
assert.match(source, /title:\s*tr\('Freigeben',\s*'Approve'\)/);
assert.match(source, /phase:\s*'approved'/);
assert.match(source, /view_mode:\s*'print'/);
assert.match(source, /applyLiveWorkflowState/);
assert.match(source, /buildLiveStatusIndex/);
// v4 (App-API 4.0): Parsing ist der eigene Befehl ctox.cv_print.apply_parse (Handler fertig), gesendet über
// api.command ohne Warten; kein business_os.chat.task, kein commandBus, kein Chat-Dokument.
assert.match(source, /const PARSE_COMMAND = 'ctox\.cv_print\.apply_parse'/);
assert.match(source, /submitCommand\(state, PARSE_COMMAND, \{/);
assert.match(source, /state\.api\.command\(type, payload, payload\?\.record_id \? \{ record_id: payload\.record_id \} : undefined\)/);
assert.equal((source.match(/state\.api\.command\(/g) || []).length, 1, 'exactly one command path');
const submitCommandSource = source.slice(source.indexOf('function submitCommand('), source.indexOf('function submitCommand(') + 1200);
assert.match(submitCommandSource, /handle\.status\.subscribe[\s\S]*?'failed'[\s\S]*?status\.report[\s\S]*?'done'[\s\S]*?status\.clear/);
assert.doesNotMatch(submitCommandSource, /await /, 'a command is never awaited (R5)');
assert.doesNotMatch(source.replace(/^\s*\/\/.*$/gm, ''), /business_os\.chat\.task|commandBus|upsertBusinessChat|wait_timeout_ms/);
assert.match(source, /command_id: commandId/);
assert.match(source, /const commandId = handle\.id/);
assert.match(source, /task_status:\s*'queued'/);
assert.match(source, /Flow öffnen/);
assert.match(source, /record_snapshot:\s*parserRecordSnapshot\(item\)/);
assert.doesNotMatch(source, /attachments:\s*\[/);
assert.doesNotMatch(source, /async function insertBusinessCommand/);
assert.doesNotMatch(source, /sync_collections:\s*\[/);
assert.doesNotMatch(liveCollections, /'desktop_file_chunks'/);
assert.doesNotMatch(liveCollections, /'business_chats'/);
assert.match(source, /'ctox_queue_tasks'/);
assert.match(source, /data-cv-open-task/);
assert.doesNotMatch(source, /ctox-cv-print-parser/);
assert.match(source, /Skill: `cv-print-builder\/parser`/);
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
// v4: das Original-PDF geht über api.files (Ref = sha256), nicht mehr als Dokumente in die fremden
// Collections desktop_files/desktop_file_chunks; ältere Profile werden aus den lokalen Chunks gelesen.
assert.match(source, /state\.api\.files\.put\(file, \{ id: fileId, name: file\.name, mime: mimeType \}\)/);
assert.match(source, /const fileRef = await upload\.ref/);
assert.match(source, /file_ref: input\.fileRef/);
assert.match(source, /state\.api\.files\.get\(source\.file_ref\)/);
assert.doesNotMatch(source, /api\.(?:put|patch)\('desktop_file/, 'no writes into desktop_files/desktop_file_chunks');
assert.doesNotMatch(source, /saveDesktopFile|writeChunkDocuments|canonicalDesktopFileChunkId|bulkUpsert|CHUNK_SIZE/);
assert.match(source, /CONTENT_HASH_SCHEME\s*=\s*'sha256-bytes-v1'/);
assert.match(source, /readStoredFileFromChunks\(chunks, source\.desktop_file_id, 'application\/pdf'/);
assert.match(source, /state\.api\.query\('desktop_file_chunks', \{ selector: \{ file_id: source\.desktop_file_id \} \}\)\.get\(\)/);
// Keine Sync-Kopplung, kein Readiness-Warten, keine Zeitüberschreitungs-Ketten (R4/R12).
assert.doesNotMatch(source.replace(/^\s*\/\/.*$/gm, ''), /ctx\.sync|leaseCollection|startCollection|awaitInSync|pushToRemotePeers|DEMAND_ONLY_SYNC_COLLECTIONS|timeoutAfter|waitFor\w*Ready|collectionReadiness|setInterval/);
assert.match(source, /function reparseCandidates\(state\)/);
assert.match(source, /source\.file_ref \|\| source\.desktop_file_id\)\)/);
assert.doesNotMatch(source, /source\.desktop_file_id && source\.generation_id/);
assert.match(source, /state\.importing \|\| state\.bulkParsing/);
assert.doesNotMatch(source, /state\.ready/);
assert.match(source, /async function reparseAllPdfs\(state,\s*candidates = reparseCandidates\(state\)\)/);
assert.match(source, /originalErrors:\s*new Map\(\)/);
// Fehler beim Öffnen des Original-PDF gehen in die Zustandszone, nicht als Kasten in den Inhalt (R11).
assert.match(source, /notify\(state, 'error', 'original', tr\('Original-PDF konnte nicht geöffnet werden'/);
assert.match(source, /\.catch\(\(error\) =>/);
assert.doesNotMatch(source, /Parser-Task wird trotzdem gestartet/);
assert.doesNotMatch(source, /PDF wird geladen|noch nicht lokal materialisiert/);
// Bestätigung und Druck bleiben im App-Fenster (api.shell.dialog, iframe in root), Flow öffnen über die Route.
assert.match(source, /await state\.api\.shell\.dialog\(\{\s*kind: 'confirm'/);
assert.match(source, /default: false/);
assert.doesNotMatch(source.replace(/^\s*\/\/.*$/gm, ''), /window\.(?:confirm|alert|prompt|print)\(|sessionStorage|location\.hash/);
assert.match(source, /function printApprovedSheet\(state, item\)/);
assert.match(source, /state\.api\.shell\.navigate\('ctox', \{/);

// ---------------------------------------------------------------------------
// IA-Karte: candidate selector (left) + CV stage (main) on the shell-owned
// canonical column grammar.
// ---------------------------------------------------------------------------
const css = read('index.css');

// Canonical column grammar — shell-wired data-pg-* chrome, no bespoke chrome.
assert.match(markup, /data-pg-search/);
assert.match(markup, /data-pg-tray-toggle/);
assert.match(markup, /data-pg-tray\b/);
assert.match(markup, /data-pg-reset/);
assert.match(markup, /class="ctox-filterbar"/);
assert.match(markup, /class="ctox-view-switch"/);
assert.match(markup, /ctox-pane-body ctox-well/);
assert.doesNotMatch(markup, /data-pg-footer/, 'keine Fußzeile, die nur den Reiterzähler wiederholt (Textdiät Regel 5)');

// Card/list switch = ONE control, an action and not a state (operator directive
// 31.08.2026). It replaces the shell's two-button [data-pg-view] group, so the
// module owns the flip: the icon/labels name the view it switches TO and there
// is no aria-pressed anywhere on it.
const viewToggles = markup.match(/data-cv-view-toggle/g) || [];
assert.equal(viewToggles.length, 1, 'exactly one card/list control');
assert.doesNotMatch(markup, /data-pg-view/);
assert.doesNotMatch(markup, /class="ctox-view-toggle"/);
const toggleTag = markup.match(/<button[^>]*data-cv-view-toggle[^>]*>/)[0];
assert.doesNotMatch(toggleTag, /aria-pressed/);
assert.match(toggleTag, /data-cv-list-view="cards"/);
assert.match(toggleTag, /aria-label="Als Liste anzeigen"/);
assert.match(source, /function setViewToggle\(state,\s*view\)/);
assert.match(source, /function toggleListView\(state,\s*button\)/);
assert.match(source, /button\.removeAttribute\('aria-pressed'\)/);
assert.match(source, /Als Liste anzeigen/);
assert.match(source, /Als Karten anzeigen/);
assert.match(source, /\[data-cv-view-toggle\]'\)\?\.dataset\.cvListView === 'list' \? 'list' : 'cards'/);

// The two views are different SHAPES, not two paddings of the same row:
// cards carry a second meta line, the list row stays a single line.
assert.match(source, /class="cv-card-meta cv-card-meta--phase"/);
assert.match(source, /class="cv-card-meta cv-card-meta--sub"/);
assert.match(source, /function shortStamp\(ms\)/);
const rowMarkup = source.slice(source.indexOf('class="cv-row'), source.indexOf('class="cv-row') + 260);
assert.equal((rowMarkup.match(/<span/g) || []).length, 2, 'list row is title + one short meta');
assert.match(css, /\.cv-candidate-list\.is-compact \{\n\s*gap: 0;/);

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

// v4: Markup relativ zum Modul ohne Cache-Buster; CSS lädt die Shell aus dem Manifest (R10).
assert.match(source, /fetch\(new URL\('\.\/index\.html', import\.meta\.url\)\)/);
assert.doesNotMatch(source, /markupVersion|cssVersion|ensureStyles|BUILD|[?]v=/);
assert.match(source, /export async function mount\(root, api, \{ restored \} = \{\}\)/);
assert.match(source, /api\.shell\.setTitle\(/);
assert.match(source, /autoWirePaneGrammar\(root\)/);
assert.match(source, /from 'ctox:shared\/file-integrity\.js'/);

// Leere Liste und leere Bühne rendern keinen Zustandstext (R11); keine Readiness-Anzeige.
assert.match(source, /function renderListEmptyState\(\)/);
assert.doesNotMatch(source, /ctox-syncing|Daten werden synchronisiert|Syncing data|listReadiness/);
assert.doesNotMatch(source, /Keine Kandidaten für diesen Filter\.|Kein CV ausgewählt/);

console.log('cv-print-builder module contract OK');
