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
  placementTypeKey,
  computeDetailVisible,
  normalizePlacement,
} from '../index.js';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');

test('placements: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'placements');
  assert.ok(manifest.collections.includes('offers'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('placements: schema declares its owned collection', () => {
  const schemaSrc = read('schema.js');
  assert.match(schemaSrc, /offers/);
  assert.match(schemaSrc, /export const collections/);
});

test('placements: left column carries the canonical grammar markup pins', () => {
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

test('placements: header carries Neu, Import and Export actions', () => {
  const html = read('index.html');
  assert.match(html, /data-action="new"/);
  assert.match(html, /data-action="import"/);
  assert.match(html, /data-action="export"/);
});

test('placements: import/export handlers are wired in index.js', () => {
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

test('placements: existing command flows are preserved unchanged', () => {
  const js = read('index.js');
  // Both ATS command types and their payloads stay exactly as shipped.
  assert.match(js, /ats\.placement\.create/);
  assert.match(js, /ats\.placement\.early_leave/);
  assert.match(js, /placement_id: placementId, left_at_ms: Date\.now\(\)/);
});

test('placements: renders a shard list from a stub doc array with the context trio', () => {
  const docs = [
    { id: 'p1', candidate_id: 'CAND-1', client_account_id: 'ACME', placement_type: '', status: 'confirmed', fee: 5000 },
    { id: 'p2', candidate_id: 'CAND-2', client_account_id: 'GLOBEX', placement_type: 'arbeitnehmerueberlassung', status: 'early_leave' },
  ];
  const cardsHtml = renderList(docs, { view: 'cards', selectedId: 'p2' });
  assert.match(cardsHtml, /CAND-1/);
  assert.match(cardsHtml, /CAND-2/);
  assert.match(cardsHtml, /data-context-record-id="p1"/);
  assert.match(cardsHtml, /data-context-record-type="placement"/);
  assert.match(cardsHtml, /data-ats-row="p2"/);
  // Selected row is highlighted via the kit class.
  assert.match(cardsHtml, /data-ats-row="p2"[^>]*is-selected|is-selected[^>]*data-ats-row="p2"/);
  // Compact variant is a distinct rendering behind the toggle.
  const listHtml = renderList(docs, { view: 'list', selectedId: '' });
  assert.match(listHtml, /placements-row-compact/);
  // Empty state.
  assert.match(renderList([], { view: 'cards' }), /ctox-empty/);
});

test('placements: band counts and filtering derive from the status field', () => {
  const docs = [
    { id: 'a', candidate_id: 'ALPHA', status: 'confirmed', placement_type: '' },
    { id: 'b', candidate_id: 'BRAVO', status: 'confirmed', placement_type: 'arbeitnehmerueberlassung' },
    { id: 'c', candidate_id: 'CHARLIE', status: 'early_leave', placement_type: 'arbeitnehmerueberlassung' },
    { id: 'd', candidate_id: 'DELTA', status: 'cancelled', placement_type: '' },
  ];
  assert.equal(lifecycleBand('confirmed'), 'active');
  assert.equal(lifecycleBand('early_leave'), 'ended');
  assert.equal(lifecycleBand('cancelled'), 'ended');
  const counts = bandCounts(docs);
  assert.deepEqual(counts, { all: 4, active: 2, ended: 2 });
  // Band filter.
  assert.equal(filterRecords(docs, { band: 'active' }).length, 2);
  assert.equal(filterRecords(docs, { band: 'ended' }).length, 2);
  // Tray filter (placement_type) — '' / null map to the direct-hire key.
  assert.equal(placementTypeKey(''), 'direct');
  assert.equal(placementTypeKey('arbeitnehmerueberlassung'), 'arbeitnehmerueberlassung');
  assert.equal(filterRecords(docs, { filters: { placement_type: 'direct' } }).length, 2);
  assert.equal(filterRecords(docs, { filters: { placement_type: 'arbeitnehmerueberlassung' } }).length, 2);
  // Search.
  assert.equal(filterRecords(docs, { search: 'zzz' }).length, 0);
  assert.equal(filterRecords(docs, { search: 'bravo' }).length, 1);
});

test('placements: auto-reveal follows the outbound idiom', () => {
  assert.equal(computeDetailVisible(false, false), false); // nothing selected -> no detail
  assert.equal(computeDetailVisible(true, false), true); // select -> reveal
  assert.equal(computeDetailVisible(true, true), false); // user collapse wins
});

test('placements: imported records normalize to a schema-valid shape', () => {
  const record = normalizePlacement(
    { candidate_id: 'CAND-9', client_account_id: 'ACME', fee: '4200', guarantee_days: '90', placement_type: 'arbeitnehmerueberlassung' },
    { nowMs: 1781990000000 },
  );
  assert.equal(record.candidate_id, 'CAND-9');
  assert.equal(typeof record.id, 'string');
  assert.ok(record.id.length > 0);
  assert.equal(record.updated_at_ms, 1781990000000);
  assert.equal(record.fee, 4200);
  assert.equal(record.guarantee_days, 90);
  assert.equal(record.placement_type, 'arbeitnehmerueberlassung');
  // A record without a candidate id is skipped by the importer (required field).
  assert.equal(normalizePlacement({}, { nowMs: 1 }).candidate_id, '');
});
