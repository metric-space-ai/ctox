import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');
const html = read('index.html');
const indexJs = read('index.js');

test('intake: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'intake');
  assert.ok(manifest.collections.includes('applications'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('intake: collections.schema.json is the only schema source (schema.js removed, §16.13)', () => {
  const schema = JSON.parse(read('collections.schema.json'));
  assert.ok(schema.collections.applications, 'applications collection declared');
  assert.deepEqual(schema.collections.applications.required, ['id', 'channel', 'updated_at_ms']);
  assert.throws(() => read('schema.js'), /ENOENT/, 'schema.js is gone');
});

test('intake: left column carries the canonical grammar markup pins', () => {
  // Search + shard/list toggle + collapsed tray with reset + footer target.
  assert.match(html, /data-pg-search/, 'grammar search input');
  assert.match(html, /data-pg-view="cards"/, 'view toggle starts in the card view');
  assert.match(html, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(html, /data-pg-tray\b/, 'collapsed tray');
  assert.match(html, /data-pg-reset/, 'tray reset control');
  assert.doesNotMatch(html, /data-pg-footer/, 'keine Fußzeile, die nur den Reiterzähler wiederholt (Textdiät Regel 5)');
});

test('intake: the view toggle is ONE action button, not a pressed-state pair', () => {
  // Ein-Knopf-Umschalter (Betreiber-Direktive 31.08.): der einzige Knopf
  // togglet; `data-pg-view` traegt die AKTUELLE, `data-pg-view-alt` die
  // Gegenansicht. Ein Aktionsknopf hat keinen Gedrueckt-Zustand.
  const viewControls = (html.match(/data-pg-view=/g) || []).length;
  assert.equal(viewControls, 1, `expected exactly ONE view-toggle control, got ${viewControls}`);
  assert.match(html, /data-pg-view="cards"[^>]*data-pg-view-alt="list"/, 'toggle declares current + alternate view');
  assert.doesNotMatch(html, /<button[^>]*data-pg-view[^>]*aria-pressed/, 'the toggle carries no aria-pressed');
  assert.match(html, /class="[^"]*intake-view-toggle/, 'toggle is module-wired via its own class');
  // Icon + Beschriftung benennen das ZIEL des Klicks, nicht den Zustand.
  assert.match(html, /data-pg-view="cards"[^>]*aria-label="Als Liste anzeigen"/, 'label names the click target');
  assert.match(indexJs, /viewToList: '[^']+'/, 'de copy carries the "to list" action label');
  assert.match(indexJs, /viewToCards: '[^']+'/, 'de copy carries the "to cards" action label');
  assert.match(indexJs, /function onViewToggle\(/, 'module owns the toggle behaviour');
  assert.match(indexJs, /removeAttribute\('aria-pressed'\)/, 'shell-set aria-pressed is stripped');
});

test('intake: cards and list are different densities, not the same row twice', () => {
  // KARTE = drei Zeilen (Name + Status, Zuordnung, Eckdaten), LISTE = genau
  // eine Zeile (Name + ein Kurz-Meta rechts). Betreiber-Direktive 31.08.
  const css = read('index.css');
  assert.match(css, /\.intake-row--cards\b/, 'card density rule exists');
  assert.match(css, /\.intake-row--list\b/, 'list density rule exists');
  assert.match(css, /\.intake-row--list[\s\S]*?line-height: 1\.25/, 'list rows use the tight line height');
  assert.match(css, /\.intake-row--cards[\s\S]*?line-height: 1\.45/, 'card rows use the roomy line height');
});

test('intake: card rows carry meta lines, list rows carry exactly one', async () => {
  const mod = await import('../index.js');
  const rec = {
    id: 'a1', channel: 'job_board', status: 'screening', vacancy_id: 'VAC-7', vacancy_title: 'Lagerist Nord',
    candidate: { name: 'Alice Ng', email: 'a@example.org' },
    documents: ['cv.pdf'], received_at_ms: Date.UTC(2026, 7, 30),
  };
  const card = mod.applicationRow(rec, { view: 'cards' });
  assert.equal((card.match(/class="intake-row-meta/g) || []).length, 2, 'card renders two meta lines below the title');
  assert.equal((card.match(/class="intake-row-head"/g) || []).length, 1, 'card renders a title/status head line');
  assert.match(card, /Lagerist Nord/, 'card shows the vacancy assignment');
  assert.match(card, /a@example\.org/, 'card shows the contact');
  assert.match(card, /Jobbörse/, 'channel renders as a label, not a schema key');

  const row = mod.applicationRow(rec, { view: 'list' });
  assert.doesNotMatch(row, /intake-row-meta/, 'list row carries no meta lines');
  assert.match(row, /intake-row-side/, 'list row carries exactly one short meta on the right');
  assert.doesNotMatch(row, /Lagerist Nord|a@example\.org/, 'list row stays a single dense line');
});

test('intake: channel keys render as translated labels', async () => {
  const mod = await import('../index.js');
  assert.equal(mod.channelLabel('career_site'), 'Karriereseite');
  assert.equal(mod.channelLabel('walk_in'), 'Walk-in');
  assert.equal(mod.channelLabel(''), '—', 'a missing channel does not render an empty cell');
  assert.equal(mod.channelLabel('some_future_channel'), 'some_future_channel', 'unknown keys fall back to the raw value');
});

test('intake: counted band has >= 2 real views (no stray single-tab chip)', () => {
  const bands = html.match(/data-pg-band="[^"]+"/g) || [];
  assert.ok(bands.length >= 2, `expected >= 2 view band tabs, got ${bands.length}`);
  const counts = html.match(/data-pg-count="[^"]+"/g) || [];
  assert.ok(counts.length >= 2, 'each band tab exposes a count target');
});

test('intake: header carries the standing Neu / Import / Export icon actions', () => {
  assert.match(html, /data-action="new"/, 'primary create action');
  assert.match(html, /data-action="import"/, 'import action');
  assert.match(html, /data-action="export"/, 'export action');
  // Icon buttons must be labelled.
  assert.match(html, /data-action="import"[^>]*aria-label=/, 'import icon has aria-label');
  assert.match(html, /data-action="export"[^>]*aria-label=/, 'export icon has aria-label');
});

test('intake: import/export handlers are wired in index.js', () => {
  assert.match(indexJs, /=== 'import'/, 'import action handled');
  assert.match(indexJs, /=== 'export'/, 'export action handled');
  assert.match(indexJs, /=== 'new'/, 'new action handled');
  assert.match(indexJs, /=== 'collapse-detail'/, 'detail collapse handled');
  // Export = JSON download via Blob URL (no HTTP).
  assert.match(indexJs, /new Blob\(/, 'export builds a Blob');
  assert.match(indexJs, /URL\.createObjectURL/, 'export uses an object URL');
  // Import = file input reading JSON, upserting via the record helpers.
  assert.match(indexJs, /type = 'file'/, 'import creates a file input');
  assert.match(indexJs, /api\.put\(PRIMARY, prepareImport\(/, 'import writes records through api.put');
  assert.match(indexJs, /prepareImport\(/, 'import normalizes via the record helper');
});

test('intake: auto-reveal follows hasSelection && !userCollapsed', async () => {
  const mod = await import('../index.js');
  assert.equal(mod.shouldRevealRecord(true, false), true, 'selected + not collapsed → shown');
  assert.equal(mod.shouldRevealRecord(false, false), false, 'no selection → hidden');
  assert.equal(mod.shouldRevealRecord(true, true), false, 'user collapsed → hidden');
});

test('intake: record list renders selector rows from a stub doc array', async () => {
  const mod = await import('../index.js');
  const rows = [
    { id: 'a1', channel: 'email', status: 'new', candidate: { name: 'Alice Ng' }, received_at_ms: 2 },
    { id: 'b2', channel: 'referral', status: 'hired', candidate: { name: 'Bob Lee' }, received_at_ms: 1 },
  ];
  const out = mod.renderRecordList(rows, { view: 'cards', selectedId: 'a1' });
  assert.match(out, /data-context-record-id="a1"/, 'row carries the record id');
  assert.match(out, /data-context-record-type="application"/, 'row carries the record type');
  assert.match(out, /data-context-label="Alice Ng"/, 'row carries the record label');
  assert.match(out, /Alice Ng/);
  assert.match(out, /Bob Lee/);
  assert.match(out, /is-selected/, 'the selected row is marked');
  // No inline expansion inside the selection list.
  assert.ok(!/<details/.test(out), 'shards do not expand inline');

  assert.equal(mod.renderRecordList([], { view: 'cards' }), '', 'an empty list renders nothing (App-API 4.0)');
});

test('intake: no readiness / syncing state in the content (App-API 4.0, R4/R11)', async () => {
  const mod = await import('../index.js');
  assert.equal(mod.renderRecordList([], { view: 'cards', readiness: { ready: false } }), '', 'no syncing shell, no empty copy');
  const rows = [{ id: 'a1', channel: 'email', status: 'new', candidate: { name: 'Alice Ng' } }];
  assert.match(mod.renderRecordList(rows, { view: 'cards' }), /Alice Ng/, 'rows render');
  assert.doesNotMatch(indexJs, /ctox-syncing|collectionReadiness|commandBus|ctx\.db/, 'no v3 data/sync paths left');
  assert.match(indexJs, /api\.status\.report\(/, 'state goes to the shell status zone');
});

test('intake: capture result blockers are derived from the pushed command result', async () => {
  const mod = await import('../index.js');
  assert.equal(mod.captureBlockers({ ok: true, application_id: 'appl_1', dedupe_key: 'email:a' }), null);
  assert.deepEqual(mod.captureBlockers({ ok: false, blockers: ['dup', { message: 'kein Kanal' }] }), ['dup', 'kein Kanal']);
  assert.deepEqual(mod.captureBlockers({ status: 'blocked' }), []);
});

test('intake: band + counts derive from the record status field', async () => {
  const mod = await import('../index.js');
  const rows = [
    { id: '1', status: 'new' },
    { id: '2', status: 'screening' },
    { id: '3', status: 'hired' },
    { id: '4', status: 'rejected' },
  ];
  assert.deepEqual(mod.countsFor(rows), { all: 4, open: 2, closed: 2 });
  assert.equal(mod.bandOf('hired'), 'closed');
  assert.equal(mod.bandOf('new'), 'open');
  assert.equal(mod.filterRows(rows, { band: 'closed' }).length, 2);
  assert.equal(mod.filterRows(rows, { band: 'open' }).length, 2);
  assert.equal(mod.filterRows(rows, { status: 'hired' }).length, 1);
});
