import assert from 'node:assert/strict';
import {
  THREAD_COLLECTIONS,
  buildApprovalRequestPayload,
  buildNotePayload,
  buildThreadsCommand,
  splitUserIds,
} from '../commands.js';
import { collections } from '../schema.js';
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
const complete = await collectUniquePages(({ skip, limit }) => {
  requestedOffsets.push(skip);
  return Promise.resolve(manyRecords.slice(skip, skip + limit));
});
assert.equal(complete.length, 235);
assert.deepEqual(requestedOffsets, [0, 100, 200]);
await assert.rejects(
  collectUniquePages(({ skip }) => Promise.resolve(skip ? manyRecords.slice(0, 100) : manyRecords.slice(0, 100))),
  /doppelten Datensatz/,
);

assert.ok(THREAD_COLLECTIONS.includes('user_threads'));
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

for (const attr of ['data-pg-search', 'data-pg-view', 'data-pg-tray-toggle', 'data-pg-tray', 'data-pg-reset', 'data-pg-filter', 'data-pg-band', 'data-pg-count', 'data-pg-footer']) {
  assert.match(html, new RegExp(attr), `index.html carries ${attr}`);
}
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
  ['notification item', js.match(/<div class="ctox-callout threads-notification-item"[^>]*>/)?.[0] || '', 'thread_notification'],
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
// Header actions exist (create + export); markup is fetched with the JS
// cache-buster; no standing briefing row; module UI state uses storageScope.
assert.match(html, /data-action="create-note"/);
assert.match(html, /data-action="export-threads"/);
assert.match(js, /loadModuleMarkup/);
assert.match(js, /\?v=\$\{version\}/);
assert.match(js, /storageScope/);
assert.doesNotMatch(html, /data-personal-briefing|threads-briefing/);
assert.doesNotMatch(css, /\.threads-briefing/);
// Kit tokens are owned by the kit (shared/base.css), never re-defined here.
assert.doesNotMatch(css, /--kit-fill:\s|--kit-hover:\s|--kit-fill-strong:\s|--focus-ring:\s/);

// Personal counts and empty state must wait for the thread, state and approval
// collections. A partial replication cannot claim the inbox is complete.
// Selection/filter empties stay ungated ctox-empty.
assert.match(js, /collectionReadiness/);
assert.match(js, /subscribeCollectionReadiness/);
assert.match(js, /subscribe\.call\(state\.ctx\.sync, 'user_threads'/);
assert.match(js, /state\.cleanup\.push\(wireReadiness\(\)\)/);
assert.match(js, /ctox-syncing" role="status" aria-live="polite"/);
assert.match(js, /syncingThreads/);
assert.match(js, /!state\.data\.threads\.length && \(readiness\?\.ready === false \|\| !personalCollectionsReady\(\)\)/);
assert.match(js, /loadPersonalPages\('user_thread_states'/);
assert.match(js, /loadPersonalPages\('ctox_task_approval_requests'/);
assert.match(js, /filter === 'inbox' \? personalCollectionsReady\(\)/);
for (const locale of ['de', 'en']) {
  const messages = JSON.parse(await readFile(fileURLToPath(new URL(`../locales/${locale}.json`, import.meta.url)), 'utf8'));
  assert.ok(messages.syncingThreads, `locales/${locale}.json carries syncingThreads`);
}

console.log('threads module smoke ok');
