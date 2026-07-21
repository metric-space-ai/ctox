import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

import {
  renderList,
  filterRecords,
  bandCounts,
  lifecycleBand,
  computeDetailVisible,
  normalizeSignatureRequest,
} from '../index.js';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');

test('esign: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'esign');
  assert.ok(manifest.collections.includes('signature_requests'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('esign: schema declares its owned collection', () => {
  const schemaSrc = read('schema.js');
  assert.match(schemaSrc, /signature_requests/);
  assert.match(schemaSrc, /export const collections/);
});

test('esign: left column carries the canonical grammar markup pins', () => {
  const html = read('index.html');
  // Two-pane frame with an explicit left resizer.
  assert.match(html, /ctox-workspace--two-pane/);
  assert.match(html, /data-resizer="left"/);
  // Search + collapsed tray + reset + shard/list toggle.
  assert.match(html, /data-pg-search/);
  assert.match(html, /data-pg-tray\b/);
  assert.match(html, /data-pg-reset/);
  assert.match(html, /data-pg-view="cards"/);
  assert.match(html, /data-pg-view="list"/);
  // Footer target.
  assert.match(html, /data-pg-footer/);
  // Counted view band with >= 2 real views (bands with a single tab are a
  // static-check failure).
  const bandTabs = (html.match(/data-pg-band=/g) || []).length;
  assert.ok(bandTabs >= 2, `expected >= 2 band tabs, found ${bandTabs}`);
  const counts = (html.match(/data-pg-count=/g) || []).length;
  assert.ok(counts >= 2, `expected >= 2 band counts, found ${counts}`);
});

test('esign: header carries Neu, Import and Export actions', () => {
  const html = read('index.html');
  assert.match(html, /data-action="new"/);
  assert.match(html, /data-action="import"/);
  assert.match(html, /data-action="export"/);
});

test('esign: import/export handlers are wired in index.js', () => {
  const js = read('index.js');
  // Export via Blob URL download, import via file input + JSON, upsert through
  // the shell-provided collection handle — no HTTP.
  assert.match(js, /new Blob\(/);
  assert.match(js, /URL\.createObjectURL/);
  assert.match(js, /input\.type = 'file'/);
  assert.match(js, /JSON\.parse/);
  assert.match(js, /\.upsert\(/);
  assert.match(js, /\[data-action="import"\]/);
  assert.match(js, /\[data-action="export"\]/);
});

test('esign: renders a shard list from a stub doc array with the context trio', () => {
  const docs = [
    { id: 'r1', document_id: 'DOC-1', subject_kind: 'arbeitsvertrag', status: 'created', signers: [{ id: 's1', state: 'pending' }] },
    { id: 'r2', document_id: 'DOC-2', subject_kind: 'vermittlungsvertrag', status: 'completed', signers: [{ id: 's2', state: 'signed' }] },
  ];
  const cardsHtml = renderList(docs, { view: 'cards', selectedId: 'r2' });
  assert.match(cardsHtml, /DOC-1/);
  assert.match(cardsHtml, /DOC-2/);
  assert.match(cardsHtml, /data-context-record-id="r1"/);
  assert.match(cardsHtml, /data-context-record-type="signature_request"/);
  assert.match(cardsHtml, /data-esign-row="r2"/);
  // Selected row is highlighted via the kit class.
  assert.match(cardsHtml, /data-esign-row="r2"[^>]*is-selected|is-selected[^>]*data-esign-row="r2"/);
  // Compact variant is a distinct rendering behind the toggle.
  const listHtml = renderList(docs, { view: 'list', selectedId: '' });
  assert.match(listHtml, /esign-row-compact/);
  // Empty state.
  assert.match(renderList([], { view: 'cards' }), /ctox-empty/);
});

test('esign: band counts and filtering derive from the status field', () => {
  const docs = [
    { id: 'a', document_id: 'ALPHA', status: 'created', subject_kind: 'arbeitsvertrag' },
    { id: 'b', document_id: 'BRAVO', status: 'sent', subject_kind: 'arbeitsvertrag' },
    { id: 'c', document_id: 'CHARLIE', status: 'completed', subject_kind: 'vermittlungsvertrag' },
    { id: 'd', document_id: 'DELTA', status: 'expired', subject_kind: 'ueberlassungsvertrag' },
  ];
  assert.equal(lifecycleBand('created'), 'open');
  assert.equal(lifecycleBand('completed'), 'done');
  const counts = bandCounts(docs);
  assert.deepEqual(counts, { all: 4, open: 2, done: 2 });
  // Band filter.
  assert.equal(filterRecords(docs, { band: 'open' }).length, 2);
  assert.equal(filterRecords(docs, { band: 'done' }).length, 2);
  // Tray filter (subject_kind) + search.
  assert.equal(filterRecords(docs, { filters: { subject_kind: 'arbeitsvertrag' } }).length, 2);
  assert.equal(filterRecords(docs, { search: 'zzz' }).length, 0);
  assert.equal(filterRecords(docs, { search: 'bravo' }).length, 1);
});

test('esign: auto-reveal follows the outbound idiom', () => {
  assert.equal(computeDetailVisible(false, false), false); // nothing selected -> no detail
  assert.equal(computeDetailVisible(true, false), true); // select -> reveal
  assert.equal(computeDetailVisible(true, true), false); // user collapse wins
});

test('esign: imported records normalize to schema-valid shape', () => {
  const record = normalizeSignatureRequest(
    { document_id: 'DOC-9', signers: ['alice', { id: 'bob', state: 'viewed' }] },
    { nowMs: 1781990000000 },
  );
  assert.equal(record.document_id, 'DOC-9');
  assert.equal(typeof record.id, 'string');
  assert.ok(record.id.length > 0);
  assert.equal(record.updated_at_ms, 1781990000000);
  assert.deepEqual(record.signers[0], { id: 'alice', state: 'pending' });
  assert.equal(record.signers[1].id, 'bob');
});
