import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  SUPPORT_AGENT_SUGGESTION_KINDS,
  buildAgentWritebackCommand,
  buildSupportAgentTaskCommand,
  buildSupportCommand,
} from '../support-commands.mjs';
import {
  filterSupportConversations,
  mergeSupportTimeline,
  supportQueueCounts,
} from '../support-reducers.mjs';
import { __supportTestHooks as hooks } from '../index.js';

const testDir = dirname(fileURLToPath(import.meta.url));
const moduleDir = resolve(testDir, '..');
const manifest = JSON.parse(readFileSync(resolve(moduleDir, 'module.json'), 'utf8'));
const schemaDocument = JSON.parse(readFileSync(resolve(moduleDir, 'collections.schema.json'), 'utf8'));
const collections = schemaDocument.collections || {};
const css = readFileSync(resolve(moduleDir, 'index.css'), 'utf8');
const html = readFileSync(resolve(moduleDir, 'index.html'), 'utf8');
const indexJs = readFileSync(resolve(moduleDir, 'index.js'), 'utf8');
const source = `${css}\n${html}`;
const forbiddenSurfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'Prem' + 'ium', 'gla' + 'ss'].join('|'), 'i');

for (const name of manifest.collections) {
  if (name === 'business_commands' || name === 'ctox_queue_tasks') continue;
  assert.ok(collections[name], `${name} is declared by support/collections.schema.json`);
}

assert.ok(collections.business_chats, 'support imports canonical business_chats for CTOX Harness replies');
assert.ok(collections.communication_messages, 'support imports canonical communication messages for the timeline');
assert.ok(collections.ctox_ticket_cases, 'support imports canonical ticket cases for context');
assert.ok(collections.customer_accounts, 'support imports canonical customer accounts for context');
assert.ok(collections.customer_contacts, 'support imports canonical customer contacts for context');
assert.ok(collections.desktop_files, 'support imports canonical desktop files for reply attachments');
assert.ok(collections.desktop_file_chunks, 'support imports canonical desktop file chunks for attachment content');
assert.ok(collections.support_agent_suggestions, 'agent suggestions collection exists');
assert.ok(SUPPORT_AGENT_SUGGESTION_KINDS.includes('draft_reply'), 'draft reply suggestions are allowed');
assert.doesNotMatch(source, forbiddenSurfacePattern);
assert.doesNotMatch(source, /border-(?:left|right)\s*:\s*(?:[2-9]|[0-9]{2,})px/);
assert.doesNotMatch(source, /border-radius:\s*(?:10|12|14|16|18|20|24)px/);
assert.doesNotMatch(source, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
assert.match(css, /@container business-app-window \(max-width: 1180px\)/);
assert.match(css, /@container business-app-window \(max-width: 760px\)/);
assert.match(html, /ctox-workspace[^"]*support-module/);
assert.match(html, /data-resize-frame/);
assert.match(html, /ctox-column-resizer[^>]*data-resizer-var="--ctox-left-width"/);
assert.match(html, /ctox-column-resizer[^>]*data-resizer-var="--ctox-right-width"/);
assert.doesNotMatch(css, /--support-left-width|--support-right-width/);

const command = buildSupportCommand({
  commandType: 'support.conversation.claim',
  recordId: 'conv_1',
  payload: { conversation_id: 'conv_1' },
});
assert.equal(command.module, 'support');
assert.equal(command.command_type, 'support.conversation.claim');
assert.equal(command.command_type, 'support.conversation.claim');
assert.equal(command.payload.conversation_id, 'conv_1');
assert.equal(Object.hasOwn(command.client_context, 'actor'), false);

const taskCommand = buildSupportAgentTaskCommand({
  conversationId: 'conv_1',
  title: 'Support summary',
  instruction: 'Summarize',
});
assert.equal(taskCommand.command_type, 'business_os.chat.task');
assert.equal(taskCommand.command_type, 'business_os.chat.task');
assert.equal(taskCommand.payload.thread_key, 'business-os/support/conv_1');
assert.equal(taskCommand.payload.writeback_contract.command_type, 'support.agent.writeback');
assert.equal(taskCommand.payload.writeback_contract.collection, 'support_agent_suggestions');

const writeback = buildAgentWritebackCommand({
  conversationId: 'conv_1',
  sourceCommandId: taskCommand.id,
  suggestionKind: 'summary',
  payload: { summary: 'Customer waits for contract data.' },
});
assert.equal(writeback.command_type, 'support.agent.writeback');
assert.equal(writeback.payload.source_command_id, taskCommand.id);
assert.equal(writeback.payload.required_human_action, 'review');

const bulkCommand = buildSupportCommand({
  commandType: 'support.bulk.resolve',
  payload: { conversation_ids: ['conv_1', 'conv_2'] },
});
assert.equal(bulkCommand.command_type, 'support.bulk.resolve');

const reportingCommand = buildSupportCommand({
  commandType: 'support.reporting.rebuild_rollups',
});
assert.equal(reportingCommand.command_type, 'support.reporting.rebuild_rollups');

const conversations = [
  { id: 'conv_a', status: 'open', assignee_id: '', priority: 'high', unread_count: 1, updated_at_ms: 1, last_activity_at_ms: 20, search_text: 'alpha contract' },
  { id: 'conv_b', status: 'resolved', assignee_id: 'user-1', priority: 'low', updated_at_ms: 2, last_activity_at_ms: 10, search_text: 'beta' },
  { id: 'conv_c', status: 'open', assignee_id: 'user-1', priority: 'normal', updated_at_ms: 3, last_activity_at_ms: 30, search_text: 'gamma' },
  { id: 'conv_d', status: 'waiting', assignee_id: 'user-1', priority: 'normal', updated_at_ms: 4, last_activity_at_ms: 40, search_text: 'delta' },
];
assert.deepEqual(filterSupportConversations(conversations, { status: 'open', query: 'alpha' }).map((item) => item.id), ['conv_a']);
assert.deepEqual(filterSupportConversations(conversations, { status: 'open' }).map((item) => item.id), ['conv_d', 'conv_c', 'conv_a']);
assert.equal(supportQueueCounts(conversations, 1000, 'user-1').open, 3);
assert.equal(supportQueueCounts(conversations, 1000, 'user-1').mine, 2);
assert.equal(supportQueueCounts(conversations, 1000, 'user-1').needsReply, 1);

const timeline = mergeSupportTimeline({
  notes: [{ id: 'note_1', created_at_ms: 30, body: 'internal' }],
  events: [{ id: 'event_1', occurred_at_ms: 20, event_type: 'claimed' }],
  suggestions: [{ id: 'suggestion_1', created_at_ms: 40, suggestion_kind: 'summary' }],
});
assert.deepEqual(timeline.map((row) => row.id), ['event_1', 'note_1', 'suggestion_1']);

// --- Canonical column grammar / IA (queues + conversation + customer context) ---

test('left column carries the full shell-wired grammar (data-pg-*)', () => {
  // Row 1: header actions collected top-right — Import + Export are standing.
  assert.match(html, /data-support-import/);
  assert.match(html, /data-support-export/);
  // Row 2: search + shard/list toggle + collapsed tray with reset.
  assert.match(html, /class="ctox-filterbar"/);
  assert.match(html, /data-pg-search/);
  assert.match(html, /data-pg-view="cards"/);
  assert.match(html, /data-pg-view="list"/);
  assert.match(html, /data-pg-tray-toggle/);
  assert.match(html, /data-pg-tray\b/);
  assert.match(html, /data-pg-reset/);
  // Status / priority / focus refinements live in the tray as dropdowns.
  assert.match(html, /data-pg-filter[^>]*data-pg-name="status"/);
  assert.match(html, /data-pg-filter[^>]*data-pg-name="priority"/);
  assert.match(html, /data-pg-filter[^>]*data-pg-name="focus"/);
  // Recessed well + one-line footer.
  assert.match(html, /ctox-pane-body ctox-well/);
  assert.match(html, /class="ctox-pane-footer"[^>]*>\s*<span data-pg-footer>/);
  // Module writes NO chrome CSS: no per-app filterbar/tray/switch/band/well rules.
  assert.doesNotMatch(css, /\.ctox-filterbar\s*\{/);
  assert.doesNotMatch(css, /\.ctox-filter-tray\s*\{/);
  assert.doesNotMatch(css, /\.ctox-view-switch\s*\{/);
  assert.doesNotMatch(css, /\.ctox-pane-tabs\s*\{/);
  assert.doesNotMatch(css, /\.ctox-well\s*\{/);
});

test('the counted queue band has >= 2 real views with counts (zeros included)', () => {
  const band = html.match(/class="[^"]*ctox-pane-tabs[^"]*"[\s\S]*?<\/nav>/);
  assert.ok(band, 'ctox-pane-tabs band present');
  const tabs = band[0].match(/class="[^"]*ctox-pane-tab[^"]*"/g) || [];
  assert.ok(tabs.length >= 2, `band needs >= 2 tabs, found ${tabs.length}`);
  for (const key of ['open', 'mine', 'unassigned', 'slaRisk']) {
    assert.match(html, new RegExp(`data-pg-band="${key}"`));
    assert.match(html, new RegExp(`data-pg-count="${key}"`));
  }
});

test('workspace pins panes to explicit grid tracks with a hard center minimum', () => {
  assert.match(css, /\.support-center\s*\{\s*grid-column:\s*3;\s*\}/);
  assert.match(css, /\.support-left\s*\{\s*grid-column:\s*1;\s*\}/);
  assert.match(css, /minmax\(320px,\s*1fr\)/);
  // Left grammar column declares its explicit rows (header/band/well/footer).
  assert.match(css, /\.ctox-pane\.support-left\s*\{[\s\S]*?grid-template-rows:\s*auto auto minmax\(0, 1fr\) auto/);
});

test('band counts derive from the reducer, include zeros, honour the queue predicates', () => {
  const nowMs = 1000;
  const convs = [
    { id: 'a', status: 'open', assignee_id: '' },
    { id: 'b', status: 'open', assignee_id: 'user-1' },
    { id: 'c', status: 'resolved', assignee_id: 'user-1' },
    { id: 'd', status: 'open', assignee_id: '', sla_due_at_ms: nowMs + 30 * 60 * 1000 },
  ];
  assert.deepEqual(hooks.bandCountsFor(convs, nowMs, 'user-1'), { open: 3, mine: 1, unassigned: 2, slaRisk: 1 });
  // Zeros are rendered, never hidden.
  assert.deepEqual(hooks.bandCountsFor([], nowMs, ''), { open: 0, mine: 0, unassigned: 0, slaRisk: 0 });
});

test('export is an honest, small JSON snapshot of the visible queue', () => {
  const rows = hooks.buildSupportExport([
    { id: 'c1', status: 'open', priority: 'high', assignee_id: 'u1', inbox_id: 'inbox-1', primary_thread_key: 't/1', updated_at_ms: 5 },
  ]);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].id, 'c1');
  assert.equal(rows[0].priority, 'high');
  assert.equal(rows[0].inbox_id, 'inbox-1');
  assert.equal(rows[0].primary_thread_key, 't/1');
  assert.equal(rows[0].updated_at_ms, 5);
});

test('import keeps only thread-linked entries and accepts an array or a single object', () => {
  const parsed = hooks.parseSupportImport([
    { primary_thread_key: 't/1', inbox_id: 'inbox-1' },
    { thread_key: 't/2' },
    { id: 'no-thread' },      // dropped: no thread key
    { primary_thread_key: '   ' }, // dropped: blank thread key
  ]);
  assert.equal(parsed.length, 2);
  assert.equal(parsed[0].thread_key, 't/1');
  assert.equal(parsed[0].inbox_id, 'inbox-1');
  assert.equal(parsed[1].thread_key, 't/2');
  assert.equal(parsed[1].inbox_id, '');
  // A single object is accepted too; garbage yields nothing.
  assert.equal(hooks.parseSupportImport({ thread_key: 't/9' }).length, 1);
  assert.deepEqual(hooks.parseSupportImport('nonsense'), []);
});

test('import dispatches only the existing open_from_thread command (schemas untouched)', () => {
  // The import path re-opens conversations from their threads via a real,
  // already-declared command — it never writes a record directly.
  const importFn = indexJs.match(/async function handleImport\(\)\s*\{[\s\S]*?\n\}/);
  assert.ok(importFn, 'handleImport present');
  assert.match(importFn[0], /support\.conversation\.open_from_thread/);
});

test('selecting a conversation is an in-place flip, never a list rebuild', () => {
  const selectFn = indexJs.match(/function selectConversation\(id\)\s*\{[\s\S]*?\n\}/);
  assert.ok(selectFn, 'selectConversation present');
  const body = selectFn[0];
  // Flip + main/context surfaces only — no queue-list innerHTML rebuild.
  assert.match(body, /applyListSelection\(\)/);
  assert.match(body, /renderTimeline\(\)/);
  assert.doesNotMatch(body, /renderConversationList\(\)/);
  // applyListSelection toggles the class across existing rows in place.
  const flip = indexJs.match(/function applyListSelection\(\)\s*\{[\s\S]*?\n\}/);
  assert.ok(flip, 'applyListSelection present');
  assert.match(flip[0], /classList\.toggle\('is-selected'/);
  assert.doesNotMatch(flip[0], /innerHTML/);
  // The list click handler routes to the in-place selectConversation, not render.
  const wire = indexJs.match(/list\?\.addEventListener\('click'[\s\S]*?\}\);/);
  assert.ok(wire, 'list click handler present');
  assert.match(wire[0], /selectConversation\(/);
});

test('primary and secondary support records expose the full context trio', () => {
  assert.match(indexJs, /support-conversation-row[^\n]+data-context-record-id=[^\n]+data-context-record-type="support_conversation"[^\n]+data-context-label=/);
  assert.match(indexJs, /support-timeline-item[^\n]+data-context-record-id=[^\n]+data-context-record-type="support_/);
  assert.match(indexJs, /const relatedCustomerType = account \? 'customer_account' : 'customer_contact'/);
  assert.match(indexJs, /data-context-record-type="\$\{relatedCustomerType\}"/);
  for (const recordType of ['ticket', 'support_thread_link', 'business_command', 'ctox_queue_task', 'support_agent_suggestion']) {
    assert.match(indexJs, new RegExp(`data-context-record-type="${recordType}"`), `${recordType} context type is present`);
  }
});

test('auto-reveal: context visible only with a selection that is not collapsed', () => {
  assert.equal(hooks.computeContextVisible({ hasSelection: true, userCollapsed: false }), true);
  assert.equal(hooks.computeContextVisible({ hasSelection: false, userCollapsed: false }), false);
  assert.equal(hooks.computeContextVisible({ hasSelection: true, userCollapsed: true }), false);
  // The module keys visibility off .is-context-hidden and seeds collapsed=true
  // (hidden by default until an explicit selection reveals it).
  assert.match(indexJs, /is-context-hidden/);
  assert.match(indexJs, /contextCollapsed:\s*true/);
});

console.log('support module phase-0 smoke OK');
