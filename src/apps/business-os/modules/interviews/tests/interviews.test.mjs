import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

import { partitionMeetings, meetingShard, isWorkbenchRevealed, meetingBand } from '../index.js';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');

test('interviews: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'interviews');
  assert.ok(manifest.collections.includes('interview_scorecards'));
  assert.ok(manifest.collections.includes('interview_meetings'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('interviews: schema declares its owned collection', () => {
  const schemaSrc = read('schema.js');
  assert.match(schemaSrc, /interview_scorecards/);
  assert.match(schemaSrc, /export const collections/);
});

test('interviews: left column carries the full canonical grammar markup', () => {
  const html = read('index.html');
  // Shell-wired grammar pins.
  assert.match(html, /data-pg-search/, 'search pin');
  assert.match(html, /data-pg-view="cards"/, 'shard view toggle');
  assert.match(html, /data-pg-view="list"/, 'list view toggle');
  assert.match(html, /data-pg-tray-toggle/, 'tray toggle');
  assert.match(html, /data-pg-tray\b/, 'tray');
  assert.match(html, /data-pg-reset/, 'reset');
  assert.match(html, /data-pg-footer/, 'footer target');
  // Recessed well + one-line footer + view-switch band from the kit.
  assert.match(html, /ctox-well/, 'recessed well');
  assert.match(html, /ctox-pane-footer/, 'pane footer');
});

test('interviews: view band has >= 2 real counted views', () => {
  const html = read('index.html');
  const band = html.match(/<nav[^>]*ctox-view-switch[\s\S]*?<\/nav>/);
  assert.ok(band, 'view-switch band present');
  const tabs = band[0].match(/class="[^"]*ctox-pane-tab[^"]*"/g) || [];
  assert.ok(tabs.length >= 2, `band needs >= 2 tabs, found ${tabs.length}`);
  assert.match(band[0], /data-pg-band="all"/);
  assert.match(band[0], /data-pg-count="all"/);
  assert.match(band[0], /data-pg-band="open"/);
  assert.match(band[0], /data-pg-band="done"/);
});

test('interviews: header carries Neu / Import / Export actions with labels', () => {
  const html = read('index.html');
  for (const action of ['new', 'import', 'export']) {
    const re = new RegExp(`data-action="${action}"[^>]*`);
    const tag = html.match(new RegExp(`<button[^>]*data-action="${action}"[^>]*>`));
    assert.ok(tag, `header action ${action} present`);
    assert.match(tag[0], /aria-label=/, `${action} has aria-label`);
    assert.match(tag[0], /title=/, `${action} has title`);
  }
});

test('interviews: import/export handlers are wired', () => {
  const js = read('index.js');
  // Export = JSON download via a Blob URL (no HTTP).
  assert.match(js, /createObjectURL/, 'export uses Blob URL');
  assert.match(js, /function exportVisible/, 'export handler');
  // Import = read JSON from a file input and upsert via the record helper.
  assert.match(js, /\.text\(\)/, 'import reads file text');
  assert.match(js, /function normalizeImported/, 'import normalizer');
  assert.match(js, /\.upsert\(/, 'import upserts records');
  assert.match(js, /data-ats-import-input/, 'hidden file input');
});

test('interviews: record list renders shards from a stub doc array', () => {
  const now = 1_700_000_000_000;
  const rows = [
    { id: 'imeet_1', candidate_id: 'cand-a', vacancy_id: 'vac-1', state: 'proposed', parties: [{ name: 'x' }], start: now + 3_600_000 },
    { id: 'imeet_2', candidate_id: 'cand-b', state: 'completed', parties: [], attended: true, start: now - 3_600_000, end: now - 1_800_000 },
  ];
  const t = (k) => k;
  const html = rows.map((r) => meetingShard(r, { t, locale: 'de', nowMs: now, selectedId: 'imeet_2' })).join('');
  // Both records render with the mandatory agent-context trio.
  assert.match(html, /data-context-record-id="imeet_1"/);
  assert.match(html, /data-context-record-type="interview_meeting"/);
  assert.match(html, /data-context-label="cand-a"/);
  assert.match(html, /cand-a/);
  assert.match(html, /cand-b/);
  // The selected record is marked; every shard is a pure selector button.
  assert.match(html, /class="ats-shard is-selected" data-ats-select="imeet_2"/);
  assert.equal((html.match(/data-ats-select=/g) || []).length, 2);
});

test('interviews: partitionMeetings derives band counts and honours filters', () => {
  const now = 1_700_000_000_000;
  const rows = [
    { id: 'a', candidate_id: 'alice', state: 'proposed' },
    { id: 'b', candidate_id: 'bob', state: 'confirmed' },
    { id: 'c', candidate_id: 'carol', state: 'completed' },
  ];
  const all = partitionMeetings(rows, { band: 'all', nowMs: now });
  assert.deepEqual(all.counts, { all: 3, open: 2, done: 1 });
  assert.equal(all.visible.length, 3);

  const openOnly = partitionMeetings(rows, { band: 'open', nowMs: now });
  assert.equal(openOnly.visible.length, 2);
  assert.ok(openOnly.visible.every((r) => meetingBand(r.state) === 'open'));

  const searched = partitionMeetings(rows, { band: 'all', search: 'carol', nowMs: now });
  assert.equal(searched.visible.length, 1);
  assert.equal(searched.visible[0].id, 'c');

  const statusFiltered = partitionMeetings(rows, { band: 'all', status: 'completed', nowMs: now });
  assert.equal(statusFiltered.counts.all, 1);
  assert.equal(statusFiltered.visible[0].id, 'c');
});

test('interviews: auto-reveal follows selection / create / collapse', () => {
  assert.equal(isWorkbenchRevealed({ selectedId: null, creating: false, collapsed: false }), false);
  assert.equal(isWorkbenchRevealed({ selectedId: 'imeet_1', creating: false, collapsed: false }), true);
  assert.equal(isWorkbenchRevealed({ selectedId: null, creating: true, collapsed: false }), true);
  assert.equal(isWorkbenchRevealed({ selectedId: 'imeet_1', creating: false, collapsed: true }), false);
});
