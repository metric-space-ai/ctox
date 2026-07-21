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
  normalizeSubmission,
} from '../index.js';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');

test('submissions: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'submissions');
  assert.ok(manifest.collections.includes('submissions'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('submissions: schema declares its owned collection', () => {
  const schemaSrc = read('schema.js');
  assert.match(schemaSrc, /submissions/);
  assert.match(schemaSrc, /export const collections/);
});

test('submissions: left column carries the canonical grammar markup pins', () => {
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

test('submissions: header carries Neu, Import and Export actions', () => {
  const html = read('index.html');
  assert.match(html, /data-action="new"/);
  assert.match(html, /data-action="import"/);
  assert.match(html, /data-action="export"/);
});

test('submissions: import/export handlers are wired in index.js', () => {
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

test('submissions: present command type + payload are unchanged', () => {
  const js = read('index.js');
  // The single native command and its payload keys must stay exactly as-is.
  assert.match(js, /ats\.submission\.present/);
  assert.match(js, /command_type: PRESENT_COMMAND/);
  assert.match(js, /vacancy_id: String\(f\.vacancy_id \|\| ''\)\.trim\(\) \|\| null/);
  assert.match(js, /client_contact_id: String\(f\.client_contact_id \|\| ''\)\.trim\(\) \|\| null/);
});

test('submissions: renders a shard list from a stub doc array with the context trio', () => {
  const docs = [
    { id: 's1', candidate_id: 'CAND-1', client_account_id: 'ACC-1', status: 'sent' },
    { id: 's2', candidate_id: 'CAND-2', client_account_id: 'ACC-2', status: 'hired', feedback: { outcome: 'accepted' } },
  ];
  const cardsHtml = renderList(docs, { view: 'cards', selectedId: 's2' });
  assert.match(cardsHtml, /CAND-1/);
  assert.match(cardsHtml, /CAND-2/);
  assert.match(cardsHtml, /data-context-record-id="s1"/);
  assert.match(cardsHtml, /data-context-record-type="submission"/);
  assert.match(cardsHtml, /data-subs-row="s2"/);
  // Selected row is highlighted via the kit class.
  assert.match(cardsHtml, /data-subs-row="s2"[^>]*is-selected|is-selected[^>]*data-subs-row="s2"/);
  // Compact variant is a distinct rendering behind the toggle.
  const listHtml = renderList(docs, { view: 'list', selectedId: '' });
  assert.match(listHtml, /subs-row-compact/);
  // Empty state.
  assert.match(renderList([], { view: 'cards' }), /ctox-empty/);
});

test('submissions: band counts and filtering derive from the status field', () => {
  const docs = [
    { id: 'a', candidate_id: 'ALPHA', client_account_id: 'ACC', status: 'sent' },
    { id: 'b', candidate_id: 'BRAVO', client_account_id: 'ACC', status: 'sent' },
    { id: 'c', candidate_id: 'CHARLIE', client_account_id: 'ACC', status: 'hired' },
    { id: 'd', candidate_id: 'DELTA', client_account_id: 'ACC', status: 'rejected' },
  ];
  assert.equal(lifecycleBand('sent'), 'open');
  assert.equal(lifecycleBand('hired'), 'done');
  assert.equal(lifecycleBand('rejected'), 'done');
  const counts = bandCounts(docs);
  assert.deepEqual(counts, { all: 4, open: 2, done: 2 });
  // Band filter.
  assert.equal(filterRecords(docs, { band: 'open' }).length, 2);
  assert.equal(filterRecords(docs, { band: 'done' }).length, 2);
  // Tray filter (status) + search.
  assert.equal(filterRecords(docs, { filters: { status: 'hired' } }).length, 1);
  assert.equal(filterRecords(docs, { search: 'zzz' }).length, 0);
  assert.equal(filterRecords(docs, { search: 'bravo' }).length, 1);
});

test('submissions: auto-reveal follows the outbound idiom', () => {
  assert.equal(computeDetailVisible(false, false), false); // nothing selected -> no detail
  assert.equal(computeDetailVisible(true, false), true); // select -> reveal
  assert.equal(computeDetailVisible(true, true), false); // user collapse wins
});

test('submissions: imported records normalize to schema-valid shape', () => {
  const record = normalizeSubmission(
    { candidate_id: 'CAND-9', client_account_id: 'ACC-9', vacancy_id: 'VAC-1' },
    { nowMs: 1781990000000 },
  );
  assert.equal(record.candidate_id, 'CAND-9');
  assert.equal(record.client_account_id, 'ACC-9');
  assert.equal(record.vacancy_id, 'VAC-1');
  assert.equal(typeof record.id, 'string');
  assert.ok(record.id.length > 0);
  assert.equal(record.updated_at_ms, 1781990000000);
  assert.equal(record.status, 'sent');
  // Records missing the required identity fields carry empty strings so the
  // importer can drop them before upsert.
  const bare = normalizeSubmission({}, { nowMs: 1781990000000 });
  assert.equal(bare.candidate_id, '');
  assert.equal(bare.client_account_id, '');
});
