import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

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

const testDir = dirname(fileURLToPath(import.meta.url));
const moduleDir = resolve(testDir, '..');
// App-API 4.0: shared helpers are imported as 'ctox:shared/x.js' (rewritten by the Service Worker,
// PORTING.md R14 / §4 Falle 2). Node needs the same mapping onto the shell's shared/ directory.
const sharedDir = resolve(moduleDir, '../../shared');
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('ctox:shared/')) {
      return nextResolve(pathToFileURL(resolve(sharedDir, specifier.slice('ctox:shared/'.length))).href, context);
    }
    return nextResolve(specifier, context);
  },
});
const { __supportTestHooks: hooks } = await import('../index.js');
const manifest = JSON.parse(readFileSync(resolve(moduleDir, 'module.json'), 'utf8'));
const schemaDocument = JSON.parse(readFileSync(resolve(moduleDir, 'collections.schema.json'), 'utf8'));
const collections = schemaDocument.collections || {};
const css = readFileSync(resolve(moduleDir, 'index.css'), 'utf8');
const html = readFileSync(resolve(moduleDir, 'index.html'), 'utf8');
const indexJs = readFileSync(resolve(moduleDir, 'index.js'), 'utf8');
const de = JSON.parse(readFileSync(resolve(moduleDir, 'locales/de.json'), 'utf8'));
const en = JSON.parse(readFileSync(resolve(moduleDir, 'locales/en.json'), 'utf8'));
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
assert.match(css, /@container business-app-window \(max-width: 1024px\)/);
assert.match(css, /@container business-app-window \(max-width: 768px\)/);
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

// „CTOX fragen“ (Strang SKL-A1): App-Befehl support.agent.task statt Kernbefehl business_os.chat.task; Anweisung und
// Skill setzt der Handler (prompts/agent-task.md, skills/support-workflow), nicht die UI.
const taskCommand = buildSupportAgentTaskCommand({
  conversationId: 'conv_1',
  title: 'Support summary',
  status: 'open',
  priority: 'high',
  recordSnapshot: { conversation: { id: 'conv_1' } },
});
assert.equal(taskCommand.command_type, 'support.agent.task');
assert.equal(taskCommand.record_id, 'conv_1');
assert.deepEqual(taskCommand.payload, { conversation_id: 'conv_1', request_kind: 'summary', record_snapshot: { conversation: { id: 'conv_1' } }, title: 'Support summary', status: 'open', priority: 'high' });
assert.equal(Object.hasOwn(taskCommand.payload, 'instruction'), false);
assert.equal(Object.hasOwn(taskCommand.payload, 'required_skills'), false);
assert.doesNotMatch(indexJs, /business_os\.chat\.task|business-os-support-workflow|buildAgentInstruction/);

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
  // Row 2: search + ONE shard/list toggle + collapsed tray with reset.
  assert.match(html, /class="ctox-filterbar"/);
  assert.match(html, /data-pg-search/);
  // Betreiber-Direktive 31.08.: one action button, not a two-button state pair.
  assert.match(html, /data-support-view-toggle/);
  assert.equal((html.match(/data-support-view-toggle/g) || []).length, 1, 'exactly one view toggle button');
  assert.equal((html.match(/data-pg-view=/g) || []).length, 0, 'no two-button view toggle left');
  assert.doesNotMatch(html, /ctox-view-toggle/);
  // The current view lives on the pane, where the shell's wirePaneGrammar()
  // reads it when no [data-pg-view] pair exists.
  assert.match(html, /data-pg-default-view="cards"/);
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
  // Einzige erlaubte Ausnahme (VIS3, 8086cc80f): die gezählte Leiste der Queue-Spalte bricht um, statt den letzten
  // Reiter am Spaltenrand abzuschneiden. Sonst keine eigene Reiterleisten-Regel.
  const vis3Wrap = /\[data-app="support"\] \.support-module \.support-left > \.ctox-view-switch > \.ctox-pane-tabs \{\s*flex-wrap: wrap;\s*min-width: 0;\s*max-width: 100%;\s*\}/;
  assert.match(css, vis3Wrap);
  assert.doesNotMatch(css.replace(vis3Wrap, ''), /\.ctox-pane-tabs\s*\{/);
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
  const importFn = indexJs.match(/function importSupportText\(text\)\s*\{[\s\S]*?\n\}/);
  assert.ok(importFn, 'importSupportText present');
  assert.match(importFn[0], /support\.conversation\.open_from_thread/);
  assert.doesNotMatch(importFn[0], /api\.(?:put|patch|remove)\(/);
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

test('the view toggle is one action button, never a state control', () => {
  // Label/icon name the view the click switches TO; no aria-pressed anywhere
  // on it, and the flip writes [data-pg-default-view] back onto the pane.
  const apply = indexJs.match(/function applyViewToggle\(\)\s*\{[\s\S]*?\n\}/);
  assert.ok(apply, 'applyViewToggle present');
  assert.match(apply[0], /removeAttribute\('aria-pressed'\)/);
  assert.match(apply[0], /showAsList/);
  assert.match(apply[0], /showAsCards/);
  assert.match(apply[0], /setAttribute\('aria-label'/);
  assert.match(apply[0], /setAttribute\('title'/);
  const flip = indexJs.match(/function toggleViewMode\(\)\s*\{[\s\S]*?\n\}/);
  assert.ok(flip, 'toggleViewMode present');
  assert.match(flip[0], /pgDefaultView/);
  assert.match(flip[0], /ctox-pane-grammar-change/);
  assert.doesNotMatch(html, /data-support-view-toggle[^>]*aria-pressed/);
  for (const key of ['showAsList', 'showAsCards']) {
    assert.ok(Object.hasOwn(de, key), `de locale carries ${key}`);
    assert.ok(Object.hasOwn(en, key), `en locale carries ${key}`);
  }
});

test('cards and list are two different row shapes, not two paddings', () => {
  const row = indexJs.match(/function renderConversationRow\(item, viewMode[^)]*\)\s*\{[\s\S]*?\n\}/);
  assert.ok(row, 'renderConversationRow present');
  const body = row[0];
  // Row class carries the variant, and the two branches are separate shapes.
  assert.match(body, /support-conversation-row--\$\{view\}/);
  assert.match(body, /if \(view === 'list'\)/);
  // List: exactly one line — title + a single short meta, nothing else.
  assert.match(body, /support-row-tail/);
  assert.doesNotMatch(body.split("if (view === 'list')")[1].split('}')[0], /support-row-meta/);
  // Cards: bold title + badges, then two meta lines built from fields the
  // module already loads (status/priority/inbox, assignee/last activity).
  assert.match(body, /support-row-head/);
  assert.match(body, /support-row-badges/);
  assert.match(body, /const primary = \[/);
  assert.match(body, /const secondary = \[/);
  assert.match(body, /assignee_id/);
  assert.match(body, /last_activity_at_ms/);
  // The variants must be styled apart, and the old container-class view is gone.
  assert.match(css, /\.support-conversation-row--cards\s*\{/);
  assert.match(css, /\.support-conversation-row--list\s*\{/);
  assert.doesNotMatch(css, /is-list-view/);
  assert.doesNotMatch(indexJs, /is-list-view/);
});

test('stacked view keeps the shell icon zone free (contract §7 iconZone)', () => {
  // At <=768px the shell-owned 6px workspace inset would start the queue pane
  // INSIDE the 80x74 icon zone; padding: 0 puts it back at the content origin.
  const narrow = css.match(/@container business-app-window \(max-width: 768px\)\s*\{[\s\S]*/);
  assert.ok(narrow, 'narrow container block present');
  assert.match(narrow[0], /padding:\s*0;/);
  assert.match(narrow[0], /overflow-x:\s*hidden;/);
});

test('v4: an empty queue renders nothing — no loading/syncing copy, no readiness wiring', () => {
  // App-API 4.0 (PORTING.md R4/R11): queries are always local and complete; state goes only to
  // the shell's status zone via api.status.report.
  assert.doesNotMatch(indexJs, /ctox-syncing/);
  assert.doesNotMatch(indexJs, /subscribeCollectionReadiness|collectionReadiness|startCollection/);
  assert.doesNotMatch(indexJs, /commandBus|ctx\.host|ctx\.sync/);
  const listFn = indexJs.match(/function renderConversationList\(\)\s*\{[\s\S]*?\n\}/);
  assert.ok(listFn, 'renderConversationList present');
  assert.match(listFn[0], /renderHtmlIfChanged\(container, '',/);
  assert.match(indexJs, /api\.status\.report\(/);
  for (const key of ['loadingTitle', 'loadingBody', 'emptyListTitle', 'commandPending']) {
    assert.doesNotMatch(indexJs, new RegExp(`\\b${key}\\b`), `${key} state copy is gone`);
  }
});

console.log('support module phase-0 smoke OK');
