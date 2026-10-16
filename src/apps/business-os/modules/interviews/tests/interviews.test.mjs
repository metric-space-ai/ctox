import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

import { partitionMeetings, meetingShard, meetingListBody, isWorkbenchRevealed, meetingBand } from '../index.js';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');

test('interviews: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'interviews');
  assert.ok(manifest.collections.includes('interview_scorecards'));
  assert.ok(manifest.collections.includes('interview_meetings'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('interviews: collections.schema.json is the only schema source (schema.js removed, §16.13)', () => {
  const schema = JSON.parse(read('collections.schema.json'));
  assert.ok(schema.collections.interview_meetings, 'meetings declared');
  assert.ok(schema.collections.interview_scorecards, 'scorecards declared');
  assert.throws(() => read('schema.js'), /ENOENT/, 'schema.js is gone');
});

test('interviews: left column carries the full canonical grammar markup', () => {
  const html = read('index.html');
  // Shell-wired grammar pins.
  assert.match(html, /data-pg-search/, 'search pin');
  assert.match(html, /data-pg-tray-toggle/, 'tray toggle');
  assert.match(html, /data-pg-tray\b/, 'tray');
  assert.match(html, /data-pg-reset/, 'reset');
  assert.doesNotMatch(html, /data-pg-footer/, 'keine Fußzeile, die nur den Reiterzähler wiederholt (Textdiät Regel 5)');
  // Recessed well + one-line footer + view-switch band from the kit.
  assert.match(html, /ctox-well/, 'recessed well');
});

// Operator directive 31.08.2026: the shard/list switch is ONE button that
// toggles the view, not a two-button state pair. The button is an action, so it
// carries a label and no aria-pressed, and the app owns the wiring.
test('interviews: the view switch is a single toggling action button', () => {
  const html = read('index.html');
  const filterbar = html.match(/<div class="ctox-filterbar">[\s\S]*?<\/div>/);
  assert.ok(filterbar, 'filterbar present');
  assert.doesNotMatch(html, /data-pg-view=/, 'no two-button pane-grammar view pair');
  assert.doesNotMatch(html, /ctox-view-toggle/, 'no two-button toggle group');
  const toggles = html.match(/<button[^>]*data-ats-view-toggle="[^"]*"[^>]*>/g) || [];
  assert.equal(toggles.length, 1, 'exactly one view control');
  assert.match(toggles[0], /data-ats-view-toggle="cards"/, 'starts in card view');
  assert.match(toggles[0], /aria-label=/, 'view control has an aria-label');
  assert.match(toggles[0], /title=/, 'view control has a title');
  assert.doesNotMatch(toggles[0], /aria-pressed/, 'an action carries no pressed state');
  const js = read('index.js');
  assert.match(js, /function toggleView/, 'app owns the toggle wiring');
  assert.match(js, /viewAsList/, 'label names the view switched TO');
  assert.match(js, /viewAsCards/, 'label names the view switched TO');
});

// Operator directive 31.08.2026: cards and list must be different shapes.
test('interviews: card and list rows are rendered differently', () => {
  const now = 1_700_000_000_000;
  const row = {
    id: 'imeet_1', candidate_id: 'cand-a', candidate_name: 'Anna Adam', vacancy_id: 'vac-1', state: 'confirmed',
    parties: [{ name: 'x' }, { name: 'y' }], start: now, end: now + 3_600_000,
    location_mode: 'video', video_link: 'https://example.test/x',
  };
  const t = (k) => k;
  const cards = meetingShard(row, { t, locale: 'de', nowMs: now, view: 'cards' });
  const list = meetingShard(row, { t, locale: 'de', nowMs: now, view: 'list' });
  assert.notEqual(cards, list, 'the two views render different markup');
  // Cards: bold title + state badge, then TWO meta lines with the detail fields.
  assert.match(cards, /ats-shard-head/);
  assert.match(cards, /ctox-badge/);
  assert.equal((cards.match(/class="ats-shard-meta"/g) || []).length, 2, 'two meta lines');
  assert.match(cards, /60 minutesShort/, 'duration derived from start/end');
  assert.match(cards, /2 parties/, 'party count');
  // List: exactly ONE line — title plus a single short trailing meta, and the
  // state reduced to a dot that still carries its word for screen readers.
  assert.equal((list.match(/class="ats-shard-line"/g) || []).length, 1, 'one line');
  assert.equal((list.match(/class="ats-shard-meta"/g) || []).length, 0, 'no meta lines');
  assert.equal((list.match(/class="ats-shard-trail"/g) || []).length, 1, 'one short trailing meta');
  assert.doesNotMatch(list, /ctox-badge/, 'no badge in the dense row');
  assert.match(list, /ats-shard-dot" data-status="confirmed"/);
  assert.match(list, /ats-sr">stateConfirmed/);
  // The selector shell is identical in both views.
  for (const html of [cards, list]) {
    assert.match(html, /class="ats-shard" data-ats-select="imeet_1"/);
    assert.match(html, /data-context-record-type="interview_meeting"/);
  }
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
  assert.match(js, /api\.put\(PRIMARY, rec\)/, 'import writes records through api.put');
  assert.match(js, /data-ats-import-input/, 'hidden file input');
});

test('interviews: record list renders shards from a stub doc array', () => {
  const now = 1_700_000_000_000;
  const rows = [
    { id: 'imeet_1', candidate_id: 'cand-a', candidate_name: 'Anna Adam', vacancy_id: 'vac-1', state: 'proposed', parties: [{ name: 'x' }], start: now + 3_600_000 },
    { id: 'imeet_2', candidate_id: 'cand-b', candidate_name: 'Bernd Bauer', state: 'completed', parties: [], attended: true, start: now - 3_600_000, end: now - 1_800_000 },
  ];
  const t = (k) => k;
  const html = rows.map((r) => meetingShard(r, { t, locale: 'de', nowMs: now, selectedId: 'imeet_2' })).join('');
  // Both records render with the mandatory agent-context trio.
  assert.match(html, /data-context-record-id="imeet_1"/);
  assert.match(html, /data-context-record-type="interview_meeting"/);
  assert.match(html, /data-context-label="Anna Adam"/);
  assert.match(html, /Anna Adam/);
  assert.match(html, /Bernd Bauer/);
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

test('interviews: list body renders rows only, an empty list renders nothing (App-API 4.0)', () => {
  const t = (k) => k;
  assert.equal(meetingListBody([], { t }), '', 'no syncing shell, no empty copy');
  const now = 1_700_000_000_000;
  const rows = meetingListBody([{ id: 'imeet_1', candidate_id: 'cand-a', candidate_name: 'Anna Adam', state: 'proposed' }], { t, nowMs: now });
  assert.match(rows, /data-ats-select="imeet_1"/);
  const js = read('index.js');
  assert.doesNotMatch(js, /ctox-syncing|renderListOrState|collectionReadiness|ctx\.db/, 'no v3 data/sync paths left');
});

test('interviews: auto-reveal follows selection / create / collapse', () => {
  assert.equal(isWorkbenchRevealed({ selectedId: null, creating: false, collapsed: false }), false);
  assert.equal(isWorkbenchRevealed({ selectedId: 'imeet_1', creating: false, collapsed: false }), true);
  assert.equal(isWorkbenchRevealed({ selectedId: null, creating: true, collapsed: false }), true);
  assert.equal(isWorkbenchRevealed({ selectedId: 'imeet_1', creating: false, collapsed: true }), false);
});
