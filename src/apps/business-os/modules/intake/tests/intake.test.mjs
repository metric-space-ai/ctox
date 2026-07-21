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

test('intake: schema declares its owned collection', () => {
  const schemaSrc = read('schema.js');
  assert.match(schemaSrc, /applications/);
  assert.match(schemaSrc, /export const collections/);
});

test('intake: left column carries the canonical grammar markup pins', () => {
  // Search + shard/list toggle + collapsed tray with reset + footer target.
  assert.match(html, /data-pg-search/, 'grammar search input');
  assert.match(html, /data-pg-view="cards"/, 'shard view toggle');
  assert.match(html, /data-pg-view="list"/, 'list view toggle');
  assert.match(html, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(html, /data-pg-tray\b/, 'collapsed tray');
  assert.match(html, /data-pg-reset/, 'tray reset control');
  assert.match(html, /data-pg-footer/, 'one-line footer target');
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
  assert.match(indexJs, /\.upsert\(/, 'import upserts records');
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

  const empty = mod.renderRecordList([], { view: 'cards' });
  assert.match(empty, /ctox-empty/, 'empty state renders the kit empty class');
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
