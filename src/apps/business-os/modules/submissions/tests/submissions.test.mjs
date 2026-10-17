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

test('submissions: collections.schema.json is the only schema source (schema.js removed, §16.13)', () => {
  const schema = JSON.parse(read('collections.schema.json'));
  assert.ok(schema.collections.submissions, 'submissions declared');
  assert.throws(() => read('schema.js'), /ENOENT/, 'schema.js is gone');
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
  // Ein-Knopf-Umschalter (Betreiber-Direktive 31.08.2026): the shard/list
  // toggle is ONE module-wired action button, not the former aria-pressed pair
  // of two [data-pg-view] buttons in a .ctox-view-toggle group. The pane
  // publishes the live view through data-pg-default-view, which is what the
  // shell grammar reads.
  const markup = html.replace(/<!--[\s\S]*?-->/g, '');
  assert.equal((markup.match(/data-subs-view-toggle/g) || []).length, 1);
  assert.match(markup, /data-pg-default-view="cards"/);
  assert.doesNotMatch(markup, /data-pg-view=/);
  assert.doesNotMatch(markup, /ctox-view-toggle/);
  // Footer target.
  assert.doesNotMatch(html, /data-pg-footer/, 'keine Fußzeile, die nur den Reiterzähler wiederholt (Textdiät Regel 5)');
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
  // Export via Blob URL download, import via file input + JSON, writes through
  // api.put (App-API 4.0) — no HTTP.
  assert.match(js, /new Blob\(/);
  assert.match(js, /URL\.createObjectURL/);
  assert.match(js, /input\.type = 'file'/);
  assert.match(js, /JSON\.parse/);
  assert.match(js, /api\.put\(PRIMARY, record\)/);
  assert.match(js, /\[data-action="import"\]/);
  assert.match(js, /\[data-action="export"\]/);
});

test('submissions: present command type + payload are unchanged', () => {
  const js = read('index.js');
  // The single native command and its payload keys must stay exactly as-is.
  assert.match(js, /ats\.submission\.present/);
  assert.match(js, /sendCommand\(PRESENT_COMMAND, payload,/, 'v4: api.command(type, payload) über sendCommand');
  assert.match(js, /vacancy_id: String\(f\.vacancy_id \|\| ''\)\.trim\(\) \|\| null/);
  assert.match(js, /client_contact_id: String\(f\.client_contact_id \|\| ''\)\.trim\(\) \|\| null/);
});

test('submissions: renders a shard list from a stub doc array with the context trio', () => {
  const docs = [
    { id: 's1', candidate_id: 'CAND-1', candidate_name: 'Anna Eins', client_account_id: 'ACC-1', status: 'sent' },
    { id: 's2', candidate_id: 'CAND-2', candidate_name: 'Bernd Zwei', client_account_id: 'ACC-2', status: 'hired', feedback: { outcome: 'accepted' } },
  ];
  const cardsHtml = renderList(docs, { view: 'cards', selectedId: 's2' });
  assert.match(cardsHtml, /Anna Eins/);
  assert.match(cardsHtml, /Bernd Zwei/);
  assert.match(cardsHtml, /data-context-record-id="s1"/);
  assert.match(cardsHtml, /data-context-record-type="submission"/);
  assert.match(cardsHtml, /data-subs-row="s2"/);
  // Selected row is highlighted via the kit class.
  assert.match(cardsHtml, /data-subs-row="s2"[^>]*is-selected|is-selected[^>]*data-subs-row="s2"/);
  // Compact variant is a distinct rendering behind the toggle.
  const listHtml = renderList(docs, { view: 'list', selectedId: '' });
  assert.match(listHtml, /subs-row-compact/);
  // An empty list renders nothing (App-API 4.0).
  assert.equal(renderList([], { view: 'cards' }), '');
});

test('submissions: no readiness / syncing state in the content (App-API 4.0, R4/R11)', () => {
  assert.equal(renderList([], { view: 'cards', sourceEmpty: true, readiness: { ready: false } }), '');
  const rowsHtml = renderList([{ id: 's1', candidate_id: 'CAND-1', candidate_name: 'Anna Eins', client_account_id: 'ACC-1', status: 'sent' }], { view: 'cards' });
  assert.match(rowsHtml, /Anna Eins/);
  const js = read('index.js');
  assert.doesNotMatch(js, /ctox-syncing|renderListOrState|collectionReadiness|commandBus|ctx\.db/, 'no v3 data/sync paths left');
  assert.match(js, /api\.command\(type, payload\)/, 'commands go through api.command without waiting');
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
