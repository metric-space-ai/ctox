import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import {
  __ticketTestHooks,
  ticketBandOf,
  matchesTicketStatusFilter,
  filterTicketRows,
  countsForTickets,
  ticketOpsFlowActive,
  resolveOpsVisible,
  renderTicketList,
  ticketRowHtml,
} from './index.js';

const read = (rel) => readFileSync(new URL(rel, import.meta.url), 'utf8');
const html = read('./index.html');
const css = read('./index.css');
const indexJs = read('./index.js');

test('tickets: agent record context + command-failure contract', () => {
  const context = __ticketTestHooks.ticketRecordContextForSmoke({
    id: 'ticket-42',
    ticket_key: 'SUP-42',
    title: 'Login unavailable',
  });
  assert.equal(context['data-context-module'], 'tickets');
  assert.equal(context['data-context-submodule'], 'inbox');
  assert.equal(context['data-context-record-type'], 'ticket');
  assert.equal(context['data-context-record-id'], 'SUP-42');
  assert.equal(context['data-context-label'], 'Login unavailable');

  assert.equal(__ticketTestHooks.commandFailureMessage({ status: 'failed', error: 'denied' }), 'denied');
  assert.equal(__ticketTestHooks.isCollectionDiagnosticsReady({ connectionStatus: 'connected' }), true);
});

test('tickets: presentation stays within the kit guard rails', () => {
  const presentationSource = `${css}\n${html}`;
  const forbiddenSurfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'Prem' + 'ium', 'gla' + 'ss'].join('|'), 'i');
  assert.doesNotMatch(presentationSource, forbiddenSurfacePattern);
  assert.doesNotMatch(presentationSource, /border-(?:left|right)\s*:\s*(?:[2-9]|[0-9]{2,})px/);
  assert.doesNotMatch(presentationSource, /border-radius:\s*(?:8|10|12|14|16|18|20|24)px/);
  assert.doesNotMatch(presentationSource, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
});

test('tickets: IA-Karte — left selector, center timeline, on-demand ops pane', () => {
  assert.match(html, /class="ctox-workspace tickets-module[^"]*"/);
  // Operations pane is on-demand: hidden by default via .is-ops-hidden.
  assert.match(html, /is-ops-hidden/);
  assert.match(html, /class="ctox-pane tickets-pane tickets-left"/);
  assert.match(html, /data-ticket-detail/);
  assert.match(html, /data-ticket-ops/);
  // Toggle + collapse for the on-demand ops pane are wired in index.js.
  assert.match(indexJs, /=== 'toggle-ops'/, 'ops toggle handled');
  assert.match(indexJs, /=== 'close-ops'/, 'ops collapse handled');
  // Grid pins + resizers.
  assert.match(css, /--ctox-left-width: 340px/);
  assert.match(css, /--ctox-right-width: 360px/);
  assert.match(css, /@container business-app-window \(max-width: 1160px\)/);
  assert.match(css, /@container business-app-window \(max-width: 640px\)/);
  assert.match(html, /data-resizer-var="--ctox-left-width"/);
  assert.match(html, /data-resizer-var="--ctox-right-width"/);
});

test('tickets: left column carries the canonical grammar markup pins', () => {
  assert.match(html, /data-pg-search/, 'grammar search input');
  assert.match(html, /data-pg-view="cards"/, 'shard view toggle');
  assert.match(html, /data-pg-view="list"/, 'list view toggle');
  assert.match(html, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(html, /data-pg-tray\b/, 'collapsed tray');
  assert.match(html, /data-pg-reset/, 'tray reset control');
  assert.match(html, /data-pg-footer/, 'one-line footer target');
  assert.match(html, /class="ctox-well"|ctox-well/, 'recessed well');
});

test('tickets: counted band has >= 2 real views, each with a count target', () => {
  const bands = html.match(/data-pg-band="[^"]+"/g) || [];
  assert.ok(bands.length >= 2, `expected >= 2 view band tabs, got ${bands.length}`);
  const counts = html.match(/data-pg-count="[^"]+"/g) || [];
  assert.ok(counts.length >= 2, 'each band tab exposes a count target');
});

test('tickets: header carries the standing Neu / Import / Export icon actions', () => {
  assert.match(html, /data-action="new"/, 'primary create action');
  assert.match(html, /data-action="import"/, 'import action');
  assert.match(html, /data-action="export"/, 'export action');
  assert.match(html, /data-action="import"[^>]*aria-label=/, 'import icon has aria-label');
  assert.match(html, /data-action="export"[^>]*aria-label=/, 'export icon has aria-label');
});

test('tickets: import/export handlers are wired (JSON via Blob / file input)', () => {
  assert.match(indexJs, /=== 'import'/, 'import action handled');
  assert.match(indexJs, /=== 'export'/, 'export action handled');
  assert.match(indexJs, /=== 'new'/, 'new action handled');
  assert.match(indexJs, /new Blob\(/, 'export builds a Blob');
  assert.match(indexJs, /URL\.createObjectURL/, 'export uses an object URL');
  assert.match(indexJs, /type = 'file'/, 'import creates a file input');
  // Import goes through the real command — projections stay server-owned.
  assert.match(indexJs, /ctox\.ticket\.local\.create/, 'import dispatches the local-create command');
});

test('tickets: selecting a row is an in-place flip, never a list rebuild', () => {
  // The click handler calls selectRecord (which flips classes), not render().
  assert.match(indexJs, /function selectRecord\(/, 'has a selectRecord path');
  assert.match(indexJs, /function applyListSelection\(/, 'has an in-place selection flip');
  assert.match(indexJs, /classList\.toggle\('is-selected'/, 'flips is-selected in place');
  // selectRecord must not rebuild the list container.
  const body = indexJs.slice(indexJs.indexOf('function selectRecord('), indexJs.indexOf('function applyListSelection('));
  assert.doesNotMatch(body, /renderList\(\)/, 'selection does not rebuild the list');
});

test('tickets: renderTicketList renders selector shards and marks selection', () => {
  const rows = [
    { id: 'a1', key: 'SUP-1', title: 'Login broken', status: 'open', source: 'email', subtitle: 'high' },
    { id: 'b2', key: 'SUP-2', title: 'Refund', status: 'closed', source: 'chat', subtitle: 'low' },
  ];
  const out = renderTicketList(rows, { view: 'cards', selectedId: 'a1' });
  assert.match(out, /data-context-record-id="a1"/);
  assert.match(out, /data-context-record-type="ticket"/);
  assert.match(out, /data-context-label="Login broken"/);
  assert.match(out, /Login broken/);
  assert.match(out, /Refund/);
  assert.match(out, /is-selected/, 'the selected row is marked');
  assert.ok(!/<details/.test(out), 'shards do not expand inline');

  const empty = renderTicketList([], { view: 'cards', emptyText: 'Nichts' });
  assert.match(empty, /ctox-empty/, 'empty state uses the kit empty class');

  const listRow = ticketRowHtml(rows[0], { view: 'list', selected: false });
  assert.match(listRow, /ticket-row--list/);
});

test('tickets: band / status filters and counts derive from remote_status', () => {
  assert.equal(ticketBandOf('closed'), 'closed');
  assert.equal(ticketBandOf('pending_approval'), 'pending');
  assert.equal(ticketBandOf('open'), 'open');
  assert.equal(matchesTicketStatusFilter('blocked', 'blocked'), true);
  assert.equal(matchesTicketStatusFilter('open', 'closed'), false);

  const rows = [
    { id: '1', remote_status: 'open', title: 'a' },
    { id: '2', remote_status: 'pending', title: 'b' },
    { id: '3', remote_status: 'closed', title: 'c' },
    { id: '4', remote_status: 'blocked', title: 'd' },
  ];
  assert.deepEqual(countsForTickets(rows), { all: 4, open: 2, pending: 1, closed: 1 });
  assert.equal(filterTicketRows(rows, { band: 'closed' }).length, 1);
  assert.equal(filterTicketRows(rows, { band: 'pending' }).length, 1);
  assert.equal(filterTicketRows(rows, { search: 'blocked' }).length, 1);
});

test('tickets: ops pane auto-reveal follows mode + operation flow', () => {
  // 'auto' reveals only when a flow needs it.
  assert.equal(resolveOpsVisible('auto', false), false, 'auto + no flow → hidden');
  assert.equal(resolveOpsVisible('auto', true), true, 'auto + flow → revealed');
  // Explicit modes win over the flow.
  assert.equal(resolveOpsVisible('open', false), true, 'user-opened → visible');
  assert.equal(resolveOpsVisible('closed', true), false, 'user-collapsed → hidden');

  // A flow exists when a clarification is open or a case awaits approval.
  assert.equal(ticketOpsFlowActive([], []), false);
  assert.equal(ticketOpsFlowActive([{ state: 'approval_pending' }], []), true);
  assert.equal(ticketOpsFlowActive([], [{ status: 'draft' }]), true);
  assert.equal(ticketOpsFlowActive([], [{ status: 'resolved' }]), false);
});
