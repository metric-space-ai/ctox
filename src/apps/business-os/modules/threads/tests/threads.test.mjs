import assert from 'node:assert/strict';
import {
  THREAD_COLLECTIONS,
  buildApprovalRequestPayload,
  buildNotePayload,
  buildThreadsCommand,
  splitUserIds,
} from '../commands.js';
import { collections } from '../schema.js';

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

console.log('threads module smoke ok');
