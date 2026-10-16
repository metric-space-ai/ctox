import assert from 'node:assert/strict';
import {
  THREAD_COLLECTIONS,
  buildApprovalRequestPayload,
  buildNotePayload,
  buildThreadsCommand,
  splitUserIds,
} from '../commands.js';
import { readFileSync } from 'node:fs';
import { threadName, isIdentifier, namingLookupIds, commandTypeName } from '../naming.js';
import { opsFixture } from './fixtures/ops-fixture.mjs';
import { collectUniquePages } from '../paging.js';
import { normalizeInternalDeepLink, sourceDeepLinkFor, sourceFocusSupported } from '../links.js';

assert.equal(
  sourceDeepLinkFor({ source_module: 'tickets', source_record_type: 'ticket_case', source_record_id: 'case 1' }, 'thread-1'),
  '#tickets?record=case+1&record_type=ticket_case&return_thread_id=thread-1',
);
assert.equal(
  sourceDeepLinkFor({ source_module: 'ctox', source_record_type: 'task', source_record_id: 'task-1' }, 'thread-1'),
  '#ctox?task_id=task-1&record_type=task&return_thread_id=thread-1',
);
assert.equal(
  sourceDeepLinkFor({ source_module: 'mail', source_record_type: 'conversation', source_record_id: 'mail-1' }, 'thread-1'),
  '#mail?thread_key=mail-1&record_type=conversation&return_thread_id=thread-1',
);
assert.equal(
  sourceDeepLinkFor({ source_module: 'mail', source_record_type: 'message', source_deep_link: '#mail?record_id=mail-2' }, 'thread-1'),
  '#mail?message_id=mail-2&record_type=message&return_thread_id=thread-1',
);
assert.equal(
  sourceDeepLinkFor({ source_module: 'documents', source_record_type: 'document', source_record_id: 'doc-1' }, 'thread-1'),
  '#documents?record=doc-1&record_type=document&return_thread_id=thread-1',
);
assert.equal(
  sourceDeepLinkFor({ source_module: 'outbound', source_record_type: 'research_run', source_record_id: 'run-1' }, 'thread-1'),
  '#outbound?record=run-1&record_type=research_run&return_thread_id=thread-1',
);
assert.equal(sourceFocusSupported({ source_module: 'outbound', source_record_type: 'research_run', source_record_id: 'run-1' }), true);
assert.equal(sourceFocusSupported({ source_module: 'outbound', source_record_type: 'unknown', source_record_id: 'run-1' }), false);
assert.equal(sourceFocusSupported({ source_module: 'outbound', source_record_type: 'research_run', source_record_id: 'run-1', source_deep_link: '#tickets?record=run-1' }), false);
assert.equal(normalizeInternalDeepLink('javascript:alert(1)', 'thread-1'), '');
assert.equal(normalizeInternalDeepLink('https://example.org/', 'thread-1'), '');
assert.equal(normalizeInternalDeepLink('#tickets?record=1', 'thread-1', new Set(['mail'])), '');

const manyRecords = Array.from({ length: 235 }, (_, index) => ({ id: `thread-${index}` }));
const requestedOffsets = [];
const pageSizes = [];
const complete = await collectUniquePages(({ skip, limit }) => {
  requestedOffsets.push(skip);
  return Promise.resolve(manyRecords.slice(skip, skip + limit));
}, { onPage: (page) => pageSizes.push(page.length) });
assert.equal(complete.records.length, 235);
assert.equal(complete.complete, true);
assert.deepEqual(requestedOffsets, [0, 100, 200]);
assert.deepEqual(pageSizes, [100, 100, 35]);
let deliveredPages = 0;
const cancelled = await collectUniquePages(({ skip, limit }) => Promise.resolve(manyRecords.slice(skip, skip + limit)), {
  onPage: () => { deliveredPages += 1; },
  shouldContinue: () => deliveredPages < 1,
});
assert.equal(cancelled.records.length, 100);
assert.equal(cancelled.complete, false);
await assert.rejects(
  collectUniquePages(({ skip }) => Promise.resolve(skip ? manyRecords.slice(0, 100) : manyRecords.slice(0, 100))),
  /doppelten Datensatz/,
);

// schema.js ist gelöscht (R17): einzige Schemaquelle ist collections.schema.json.
const { collections: schemaCollections } = JSON.parse(readFileSync(new URL('../collections.schema.json', import.meta.url), 'utf8'));
const collections = Object.fromEntries(Object.entries(schemaCollections).map(([name, entry]) => [name, entry.schema || entry]));
assert.ok(THREAD_COLLECTIONS.includes('user_threads'));
for (const name of THREAD_COLLECTIONS) assert.ok(collections[name], `collections.schema.json trägt ${name}`);
assert.ok(collections.user_threads);
assert.ok(collections.user_thread_states);
assert.ok(collections.user_thread_messages);
assert.ok(collections.ctox_task_approval_requests);

assert.deepEqual(splitUserIds('alice, bob  alice\ncarol'), ['alice', 'bob', 'carol']);

const notePayload = buildNotePayload({
  body: ' Bitte pruefen ',
  targetUserIds: 'alice,bob',
  sourceContext: { module: 'tickets', record_id: 'T-1', label: 'Ticket 1', deep_link: '#tickets?record=T-1' },
});
assert.equal(notePayload.body, 'Bitte pruefen');
assert.deepEqual(notePayload.target_user_ids, ['alice', 'bob']);
assert.equal(notePayload.source_context.module, 'tickets');
assert.equal(notePayload.source_context.deep_link, '#tickets?record=T-1');

const approvalPayload = buildApprovalRequestPayload({
  prompt: 'CTOX soll das Ticket beantworten',
  reviewerUserId: 'lead',
  sourceContext: { module: 'support', record_id: 'conv-1', label: 'Kunde A' },
});
assert.equal(approvalPayload.reviewer_user_id, 'lead');
assert.equal(approvalPayload.target_module, 'support');
assert.equal(approvalPayload.target_record_id, 'conv-1');
assert.equal(approvalPayload.target_command_type, 'business_os.chat.task');

const command = buildThreadsCommand({
  commandType: 'threads.note.create',
  payload: notePayload,
  sourceModule: 'tickets',
});
assert.match(command.id, /^cmd_[0-9a-f-]{36}$/);
assert.equal(command.module, 'threads');
assert.equal(command.command_type, 'threads.note.create');
assert.equal(command.inbound_channel, 'tickets');
assert.equal(command.client_context.module_id, 'threads');

const claim = buildThreadsCommand({
  commandType: 'threads.thread.claim',
  recordId: 'thread-1',
  payload: { thread_id: 'thread-1', expected_updated_at_ms: 42 },
});
assert.equal(claim.command_type, 'threads.thread.claim');
assert.equal(claim.payload.expected_updated_at_ms, 42);

// Pane-chrome contract: canonical data-pg-* grammar, in-place selection,
// canonical context-record trio, no hand-rolled chrome or module localStorage.
const { readFile } = await import('node:fs/promises');
const { fileURLToPath } = await import('node:url');
const js = await readFile(fileURLToPath(new URL('../index.js', import.meta.url)), 'utf8');
const html = await readFile(fileURLToPath(new URL('../index.html', import.meta.url)), 'utf8');
const css = await readFile(fileURLToPath(new URL('../index.css', import.meta.url)), 'utf8');

for (const attr of ['data-pg-search', 'data-pg-view', 'data-pg-tray-toggle', 'data-pg-tray', 'data-pg-reset', 'data-pg-filter', 'data-pg-band', 'data-pg-count']) {
  assert.match(html, new RegExp(attr), `index.html carries ${attr}`);
}
// Owner-Regel: keine Fußzeile, keine Zähl-/Zustandszeile (Tab-Zähler im Band bleiben – Navigation).
assert.doesNotMatch(html, /data-pg-footer|threads-footer/);
assert.doesNotMatch(js, /setFooter|data-pg-footer|FILTER_LABELS| · als /);
// Reiter/Ansichten im Originalwortlaut, gekennzeichnet für den Wächter.
assert.match(html, /<!-- v4-allow: label -->\s*<button[^>]*data-pg-band="waiting"[^>]*>Wartet</);
assert.match(html, /<!-- v4-allow: label -->\s*<option value="delegated">Wartet auf andere</);
assert.match(js, /ctox-pane-grammar-change/);
assert.match(js, /__ctoxPaneGrammar/);
assert.doesNotMatch(js, /data-toggle-filters|data-reset-filters|data-filter-select|\[data-view-mode\]|\[data-center-view\]/);
// The counted band covers all four primary queues, zeros included.
for (const band of ['inbox', 'waiting', 'running', 'archived']) {
  assert.match(html, new RegExp(`data-pg-band="${band}"`), `band tab ${band}`);
  assert.match(html, new RegExp(`data-pg-count="${band}"`), `count for ${band}`);
}
// Every thread row carries the canonical context-record trio; the legacy
// data-record-id/data-record-type/data-title attributes are gone.
assert.match(js, /data-context-record-id/);
assert.match(js, /data-context-record-type="thread"/);
assert.match(js, /data-context-label/);
assert.doesNotMatch(js, /data-record-id|data-record-type|data-title=/);
// Secondary message, approval, and notification records expose the same trio
// without replacing their existing action/selection ids.
const secondarySurfaces = [
  ['event message', js.match(/<div class="threads-message is-event"[^>]*>/)?.[0] || '', 'thread_message'],
  ['conversation message', js.match(/<article class="threads-message[^>]*>/)?.[0] || '', 'thread_message'],
  ['approval card', js.match(/<article class="threads-approval-card"[^>]*>/)?.[0] || '', 'thread_approval'],
  ['notification item', js.match(/<div class="threads-notification-item"[^>]*>/)?.[0] || '', 'thread_notification'],
];
for (const [surface, openingTag, recordType] of secondarySurfaces) {
  for (const attr of ['data-context-record-id', 'data-context-record-type', 'data-context-label']) {
    assert.match(openingTag, new RegExp(attr), `${surface} carries ${attr}`);
  }
  assert.match(openingTag, new RegExp(`data-context-record-type="${recordType}"`), `${surface} uses ${recordType}`);
}
assert.match(secondarySurfaces[0][1], /data-message-id/);
assert.match(secondarySurfaces[1][1], /data-message-id/);
assert.match(secondarySurfaces[2][1], /data-approval-id/);
// Selection is an in-place is-selected/aria-selected flip, never a rebuild.
assert.match(js, /applyThreadSelection/);
assert.match(js, /aria-selected/);
// Header actions exist (create + export); no standing briefing row.
assert.match(html, /data-action="create-note"/);
assert.match(html, /data-action="export-threads"/);
assert.doesNotMatch(html, /data-personal-briefing|threads-briefing/);
assert.doesNotMatch(css, /\.threads-briefing/);
// Kit tokens are owned by the kit (shared/base.css), never re-defined here.
assert.doesNotMatch(css, /--kit-fill:\s|--kit-hover:\s|--kit-fill-strong:\s|--focus-ring:\s/);

// App-API 4.0 (PORTING.md): mount(root, api, {restored}) → {unmount, snapshot}; Daten nur über Live-Abfragen;
// keine Readiness, keine Schleifen, kein Cache-Buster, kein HTTP-Datenweg, kein Speicher außerhalb snapshot().
assert.match(js, /export async function mount\(root, api, \{ restored \} = \{\}\)/);
assert.match(js, /snapshot,\n\s*\};/);
assert.match(js, /api\.query\('user_threads'|live\('user_threads'/);
assert.match(js, /autoWirePaneGrammar\(root\)/);
assert.match(js, /from 'ctox:shared\/stable-dom\.js'/);
assert.match(js, /fetch\(new URL\('\.\/index\.html', import\.meta\.url\)\)/);
assert.doesNotMatch(js, /\bctx\.|collectionReadiness|subscribeCollectionReadiness|setInterval|storageScope|sessionStorage|localStorage|\?v=|location\.hash|window\.(confirm|prompt|alert)|new Notification/);
assert.doesNotMatch(js, /ctox-syncing|syncingThreads|partialSearchResults|commandReceipt/);
assert.doesNotMatch(html, /role="status"|data-sync-state|data-thread-command-receipt/);
for (const locale of ['de', 'en']) {
  const messages = JSON.parse(await readFile(fileURLToPath(new URL(`../locales/${locale}.json`, import.meta.url)), 'utf8'));
  for (const stateKey of ['syncingThreads', 'noThreads', 'noSelection']) assert.equal(messages[stateKey], undefined, `${locale}: kein Zustandstext ${stateKey}`);
}

// workjet#594: Name nach Projekt und Aufgabe, nie eine Kennung (naming.js, Rückfall-Reihenfolge in V4-PORT.md).
assert.equal(isIdentifier('a0796d32-2ca9-4f1e-9b7a-3c5d8e2f1a46'), true);
assert.equal(isIdentifier('CTOX Arbeit 5c1e8a27-b4d6-4c93-a0f5-8e7d2b1c9a60'), true);
assert.equal(isIdentifier('cmd_42'), true);
assert.equal(isIdentifier('Angebot Müller GmbH prüfen'), false);
assert.equal(isIdentifier('ctox.workjet.project.list'), false);
const fx = opsFixture();
const pick = (rows) => (id) => rows.find((row) => row.id === id) || null;
const lookups = {
  command: pick(fx.collections.business_commands), task: pick(fx.collections.ctox_queue_tasks),
  project: pick(fx.collections.workjet_projects), appTitle: (module) => pick(fx.collections.business_apps)(module)?.title,
};
const names = Object.fromEntries(fx.collections.user_threads.map((thread) => [thread.id, threadName(thread, lookups)]));
assert.deepEqual([names[fx.ids.t1].project, names[fx.ids.t1].task], ['CTOX Shell v4', 'Login-Seite auf App-API 4.0 umstellen und Abnahme fahren']);
// Stufe 8: Befehlstyp nie roh, sondern Quelle + Ableitung aus der festen Tabelle (COMMAND_TYPE_NAMES).
assert.equal(names[fx.ids.t2].title, 'Workjet · Projektliste');
assert.deepEqual(commandTypeName('ctox.workjet.project.list'), { source: 'Workjet', action: 'Projektliste' });
assert.deepEqual(commandTypeName('ctox.workjet.project.list', 'en'), { source: 'Workjet', action: 'Project list' });
assert.deepEqual(commandTypeName('ctox.workjet.project.supervisor.turn.submit'), { source: 'Workjet', action: 'Supervisor-Zug' });
assert.deepEqual(commandTypeName('ctox.workjet.exit_model.refresh'), { source: 'Workjet', action: '' });
assert.equal(commandTypeName('ctox.workjetx.foo'), null, 'nur an Segmentgrenzen');
assert.equal(commandTypeName('crm.lead.update'), null, 'ohne Tabellentreffer nichts erfunden');
{
  const t2 = fx.collections.user_threads.find((thread) => thread.id === fx.ids.t2);
  const unknown = { ...lookups, command: (id) => ({ ...lookups.command(id), command_type: 'crm.lead.update' }) };
  assert.doesNotMatch(threadName(t2, unknown).title, /crm\.lead\.update/, 'unbekannter Befehlstyp nie roh');
}
assert.equal(names[fx.ids.t3].title, 'CTOX Shell v4 · Nachtlauf: Abnahme Kundenpipeline auf gpu3');
assert.equal(names[fx.ids.t4].title, 'Workjet Desktop · Supervisor');
assert.equal(names[fx.ids.t5].title, 'Angebot Müller GmbH prüfen');
for (const name of Object.values(names)) assert.doesNotMatch(name.title, /[0-9a-f]{8}-[0-9a-f]{4}/);
// Ohne Nachschlagedaten (z. B. workjet_projects nicht freigegeben) bleibt es bei lesbaren Rückfallstufen.
assert.equal(threadName(fx.collections.user_threads[0], { appTitle: () => 'CTOX' }).title, 'CTOX Arbeit');
assert.deepEqual(namingLookupIds(fx.collections.user_threads).commandIds, [fx.ids.C1, fx.ids.C2]);

console.log('threads module smoke ok');
