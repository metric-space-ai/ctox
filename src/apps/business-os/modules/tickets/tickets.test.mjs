import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createRxDatabase } from '../../rxdb/src/index.mjs';
import {
  __ticketTestHooks,
  ticketBandOf,
  matchesTicketStatusFilter,
  filterTicketRows,
  countsForTickets,
  ticketOpsFlowActive,
  resolveOpsVisible,
  resolveTicketListState,
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
});

test('tickets: canonical collection readiness separates syncing from true empty', () => {
  assert.equal(
    resolveTicketListState({ sourceCount: 0, readiness: { ready: false, state: 'catching-up' } }),
    'syncing',
    'an empty unready source remains in the syncing state',
  );
  assert.equal(
    resolveTicketListState({ sourceCount: 0, readiness: { ready: true, state: 'live' } }),
    'empty',
    'an empty ready source is a true empty state',
  );
  assert.equal(
    resolveTicketListState({ sourceCount: 2, readiness: { ready: false, state: 'catching-up' } }),
    'content',
    'readiness does not gate filter-empty rendering once source tickets exist',
  );
  assert.match(indexJs, /sync\?\.subscribeCollectionReadiness/, 'canonical readiness changes are subscribed');
  assert.match(indexJs, /subscribe\.call\(state\.ctx\.sync, TICKET_PRIMARY_COLLECTION/, 'primary collection is wired to readiness');
  assert.match(indexJs, /stopReadiness\(\)/, 'readiness subscription is cleaned up');
  assert.match(indexJs, /ctox-empty ctox-syncing/, 'syncing state uses the canonical kit class');
  assert.doesNotMatch(indexJs, /sync\.diagnostics|frameTransport|initialReplicationState/, 'private diagnostics heuristics are removed');
  const detailRenderer = indexJs.slice(indexJs.indexOf('function renderDetail('), indexJs.indexOf('function renderOps('));
  assert.doesNotMatch(detailRenderer, /ticketReadiness|resolveTicketListState/, 'selection empty is not readiness-gated');
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
  assert.match(css, /@container business-app-window \(max-width: 1024px\)/);
  assert.match(css, /@container business-app-window \(max-width: 768px\)/);
  assert.match(html, /data-resizer-var="--ctox-left-width"/);
  assert.match(html, /data-resizer-var="--ctox-right-width"/);
});

test('tickets: left column carries the canonical grammar markup pins', () => {
  assert.match(html, /data-pg-search/, 'grammar search input');
  assert.match(html, /data-pg-view="cards"/, 'view toggle seeds the cards view');
  assert.match(html, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(html, /data-pg-tray\b/, 'collapsed tray');
  assert.match(html, /data-pg-reset/, 'tray reset control');
  assert.match(html, /data-pg-footer/, 'one-line footer target');
  assert.match(html, /class="ctox-well"|ctox-well/, 'recessed well');
});

// Betreiber-Direktive 31.08.: the shard/list switch is ONE button that toggles
// the view. Icon and label name the view the click switches TO, so it carries
// no aria-pressed — it is an action, not a state. `data-pg-view` stays on it as
// the CURRENT view so the shell-wired grammar keeps reading a truthful value.
test('tickets: the shard/list switch is one action button, not a pressed pair', () => {
  const htmlNoComments = html.replace(/<!--[\s\S]*?-->/g, '');
  assert.equal((htmlNoComments.match(/data-pg-view=/g) || []).length, 1, 'exactly one view control');
  assert.doesNotMatch(htmlNoComments, /<button[^>]*data-pg-view[^>]*aria-pressed/, 'an action carries no state');
  assert.doesNotMatch(htmlNoComments, /ctox-view-toggle/, 'no two-button toggle group left');
  assert.match(htmlNoComments, /data-tickets-view-toggle[^>]*data-pg-view="cards"/, 'seeded with the cards view');
  assert.match(htmlNoComments, /data-view-icon="list"/, 'carries the list glyph');
  assert.match(htmlNoComments, /data-view-icon="cards"/, 'carries the cards glyph');
  // CSS paints exactly the glyph of the view the click switches TO.
  assert.match(css, /\.tickets-view-toggle\[data-pg-view="cards"\] > \[data-view-icon="list"\]/);
  assert.match(css, /\.tickets-view-toggle\[data-pg-view="list"\] > \[data-view-icon="cards"\]/);
  // JS flips it in the capture phase and strips the grammar's aria-pressed.
  assert.match(indexJs, /addEventListener\('click', onViewToggleCapture, true\)/, 'capture-phase flip');
  assert.match(indexJs, /button\.dataset\.pgView = button\.dataset\.pgView === 'list' \? 'cards' : 'list'/);
  assert.match(indexJs, /button\.removeAttribute\('aria-pressed'\)/);
  assert.match(indexJs, /showAsList/, 'label for the switch to list');
  assert.match(indexJs, /showAsCards/, 'label for the switch to cards');
});

test('tickets: cards and list are two densities, not two paddings', () => {
  const row = {
    id: 'a1', key: 'SUP-1', title: 'Login broken', status: 'open',
    source: 'email', subtitle: 'high', updated: '30. Aug. 2026, 09:12',
  };
  const cards = ticketRowHtml(row, { view: 'cards' });
  const list = ticketRowHtml(row, { view: 'list' });

  // KARTEN: title line + two meta lines carrying the real detail fields.
  assert.match(cards, /ticket-row--cards/);
  assert.equal((cards.match(/ticket-row-meta/g) || []).length, 3, 'two meta lines (one carries two classes)');
  assert.match(cards, /SUP-1 · email/, 'key + source meta line');
  assert.match(cards, /high · 30\. Aug\. 2026, 09:12/, 'assignment + updated meta line');

  // LISTE: exactly one line — title plus the badge as the single short meta.
  assert.match(list, /ticket-row--list/);
  assert.doesNotMatch(list, /ticket-row-meta/, 'no meta lines in the dense list row');
  assert.doesNotMatch(list, /ticket-row-head/, 'no nested head row in the dense list row');
  assert.equal((list.match(/<div/g) || []).length, 1, 'a list row is a single flat row');

  assert.match(css, /\.ticket-row--list[\s\S]*?min-height: 26px/, 'list row is pinned tight');
  assert.match(css, /\.ticket-row--cards[\s\S]*?padding-top: 11px/, 'card row pads generously');
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


test('tickets realtime refresh hints do not start an unused denied queue snapshot', async () => {
  const names = ['ctox_ticket_items', 'ctox_queue_tasks', 'ctox_crew_members'];
  const databases = [];
  const handles = new Map();
  const observers = new Map();
  const reads = { demand: 0, storage: 0 };
  const denied = Object.assign(new Error('peer is not authorized for collection ctox_queue_tasks'), {
    code: 'REMOTE_ERROR',
  });
  let refreshes = 0;
  let cleanup;
  const wait = () => new Promise(resolve => setTimeout(resolve, 120));
  try {
    for (const name of names) {
      const listeners = new Set();
      observers.set(name, listeners);
      const storage = {
        primaryPath: 'id', schemaIndexes: () => [],
        observe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
        async queryDocuments() { reads.storage++; return []; },
        async allDocuments() { reads.storage++; return []; },
        async findDocumentsById() { reads.storage++; return {}; },
      };
      const db = await createRxDatabase({
        name: 'tickets-realtime-' + name,
        storage: { nativeStorage: { collection: () => storage, close() {} } },
      });
      databases.push(db);
      await db.addCollections({ [name]: { schema: {
        version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string' } },
      } } });
      const collection = db.collection(name);
      collection.setDemandLoader({ async resolveQuery() {
        reads.demand++;
        if (name === 'ctox_queue_tasks') throw denied;
        return [{ id: 'visible-record', _rev: '1-record', _deleted: false }];
      } });
      handles.set(name, collection);
    }
    const declarationStart = indexJs.indexOf('const collectionNames = [');
    const declarationEnd = indexJs.indexOf('];', declarationStart) + 2;
    const functionStart = indexJs.indexOf('function wireRealtime() {');
    const functionEnd = indexJs.indexOf('\nfunction readPrimaryTicketReadiness(', functionStart);
    assert(declarationStart >= 0 && declarationEnd > declarationStart && functionStart >= 0 && functionEnd > functionStart);
    // Run the production function and collection list, with actual RxDB handles.
    // Its callback only schedules the existing bounded refresh; no snapshot is consumed.
    const wire = new Function('ticketCollection', 'scheduleRefresh',
      indexJs.slice(declarationStart, declarationEnd) + '\n'
      + indexJs.slice(functionStart, functionEnd) + '\nreturn wireRealtime;')(
      name => handles.get(name), () => { refreshes++; },
    );
    cleanup = wire();
    await wait();
    assert.equal(refreshes, 3, 'initial invalidation reaches each present source');
    assert.deepEqual(reads, { demand: 0, storage: 0 }, 'subscribing does not issue implicit native reads');
    for (const listener of observers.get('ctox_ticket_items')) listener({ external: true, ids: ['visible-record'] });
    await wait();
    assert.equal(refreshes, 4, 'a changed ticket still schedules refresh');
    assert.deepEqual(reads, { demand: 0, storage: 0 });
    assert.equal((await handles.get('ctox_ticket_items').find().exec()).length, 1);
    await assert.rejects(handles.get('ctox_queue_tasks').find().exec(), error => error === denied);
    assert.equal(reads.demand, 2, 'explicit reads retain both authorized data and native denial');
    cleanup();
    cleanup();
    assert.equal([...observers.values()].reduce((total, listeners) => total + listeners.size, 0), 0);
    for (const handle of handles.values()) handle.notifyQueryWindowChange();
    await wait();
    assert.equal(refreshes, 4, 'closing Tickets detaches every realtime listener');
  } finally {
    cleanup?.();
    await Promise.all(databases.map(db => db.close()));
  }
});
