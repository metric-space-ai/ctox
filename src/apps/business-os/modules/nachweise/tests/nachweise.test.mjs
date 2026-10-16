import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');
const html = read('index.html');
const indexJs = read('index.js');

test('credentials: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'nachweise');
  assert.ok(manifest.collections.includes('business_credentials'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('credentials: collections.schema.json is the only schema source (schema.js removed, §16.13)', () => {
  const schema = JSON.parse(read('collections.schema.json'));
  assert.ok(schema.collections.business_credentials, 'business_credentials declared');
  assert.throws(() => read('schema.js'), /ENOENT/, 'schema.js is gone');
});

test('credentials: left column carries the canonical grammar markup pins', () => {
  // Search + shard/list toggle + collapsed tray with reset + footer target.
  assert.match(html, /data-pg-search/, 'grammar search input');
  // Shard/list switching is ONE button, not a pressed/unpressed pair
  // (Betreiber-Direktive 31.08.): the current view lives on the pane as
  // [data-pg-default-view] — the attribute the shell's own wirePaneGrammar()
  // falls back to — and the button is an action, so it carries no
  // aria-pressed and no per-view data-pg-view twin.
  assert.match(html, /data-pg-default-view="cards"/, 'pane carries the current view');
  assert.equal((html.match(/data-ats-view-toggle/g) || []).length, 1, 'exactly one view toggle control');
  assert.equal((html.match(/data-pg-view=/g) || []).length, 0, 'no two-button view toggle left');
  assert.match(html, /data-ats-view-icon="list"[\s\S]*data-ats-view-icon="cards"/, 'toggle carries both target icons');
  assert.match(html, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(html, /data-pg-tray\b/, 'collapsed tray');
  assert.match(html, /data-pg-reset/, 'tray reset control');
  assert.doesNotMatch(html, /data-pg-footer/, 'keine Fußzeile, die nur den Reiterzähler wiederholt (Textdiät Regel 5)');
});

test('credentials: counted band has >= 2 real views derived from status', () => {
  const bands = html.match(/data-pg-band="[^"]+"/g) || [];
  assert.ok(bands.length >= 2, `expected >= 2 view band tabs, got ${bands.length}`);
  const counts = html.match(/data-pg-count="[^"]+"/g) || [];
  assert.ok(counts.length >= 2, 'each band tab exposes a count target');
});

test('credentials: header carries the standing Neu / Import / Export icon actions', () => {
  assert.match(html, /data-action="new"/, 'primary create action');
  assert.match(html, /data-action="import"/, 'import action');
  assert.match(html, /data-action="export"/, 'export action');
  // Icon buttons must be labelled.
  assert.match(html, /data-action="import"[^>]*aria-label=/, 'import icon has aria-label');
  assert.match(html, /data-action="export"[^>]*aria-label=/, 'export icon has aria-label');
});

test('credentials: import/export/new/collapse handlers are wired in index.js', () => {
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
  // The single view toggle: it swaps SVG icons, and SVGElement has no `hidden`
  // IDL property — assigning `.hidden` sets a JS expando the stylesheet never
  // sees and the icon silently stops swapping. Attributes only.
  assert.match(indexJs, /toggleAttribute\('hidden'/, 'view icons swap via the hidden attribute');
  assert.ok(!/\bicon\.hidden\s*=/.test(indexJs), 'view icons never use the .hidden property');
  assert.match(indexJs, /removeAttribute\('aria-pressed'\)/, 'the view toggle is an action, not a state');
});

test('credentials: existing command flows are untouched', () => {
  // The two server-authoritative commands keep their exact types and payloads.
  assert.match(indexJs, /ats\.deployment\.check/, 'deployment gate command');
  assert.match(indexJs, /ats\.leistungsnachweis\.signoff/, 'sign-off command');
  // Capture stays a plain collection write (no command), now through api.put.
  assert.match(indexJs, /api\.put\(PRIMARY, record\)/, 'credential capture is a plain write');
  assert.match(indexJs, /api\.command\(type, payload\)/, 'commands go through api.command without waiting');
});

test('credentials: auto-reveal follows hasSelection && !userCollapsed', async () => {
  const mod = await import('../index.js');
  assert.equal(mod.shouldRevealRecord(true, false), true, 'selected + not collapsed → shown');
  assert.equal(mod.shouldRevealRecord(false, false), false, 'no selection → hidden');
  assert.equal(mod.shouldRevealRecord(true, true), false, 'user collapsed → hidden');
});

test('credentials: record list renders selector rows from a stub doc array', async () => {
  const mod = await import('../index.js');
  const now = 1_781_990_000_000;
  const rows = [
    { id: 'c1', subject_id: 'cand-1', subject_name: 'Clara Eins', credential_type: 'staplerschein', verified: true, valid_until_ms: now + 365 * 24 * 3600 * 1000 },
    { id: 'c2', subject_id: 'cand-2', subject_name: 'Dirk Zwei', credential_type: 'g25', verified: false },
  ];
  const out = mod.renderRecordList(rows, { view: 'cards', selectedId: 'c1', nowMs: now });
  assert.match(out, /data-context-record-id="c1"/, 'row carries the record id');
  assert.match(out, /data-context-record-type="nachweis"/, 'row carries the record type');
  assert.match(out, /data-context-label="/, 'row carries the record label');
  assert.match(out, /is-selected/, 'the selected row is marked');
  // No inline expansion, no per-row buttons inside the selection list.
  assert.ok(!/<details/.test(out), 'shards do not expand inline');
  assert.ok(!/<button/.test(out), 'shards carry no per-row buttons');

  assert.equal(mod.renderRecordList([], { view: 'cards', nowMs: now }), '', 'an empty list renders nothing (App-API 4.0)');
});

test('credentials: card and list views are different shapes, not two paddings', async () => {
  const mod = await import('../index.js');
  const now = 1_781_990_000_000;
  const rows = [{
    id: 'c1', subject_id: 'cand-1', subject_name: 'Clara Eins', credential_type: 'staplerschein', issuer: 'TÜV Nord',
    verified: true, valid_until_ms: now + 365 * 24 * 3600 * 1000,
  }];
  const cards = mod.renderRecordList(rows, { view: 'cards', nowMs: now });
  const list = mod.renderRecordList(rows, { view: 'list', nowMs: now });

  // Cards: 2 meta lines under the title = a 3-line shard, with the fields that
  // decide a credential (subject, issuer, expiry date, remaining days).
  assert.equal((cards.match(/nachweise-row-meta/g) || []).length, 2, 'shard carries two meta lines');
  assert.match(cards, /Clara Eins/, 'shard names the subject');
  assert.match(cards, /TÜV Nord/, 'shard names the issuer');
  assert.match(cards, /nachweise-row-head/, 'shard separates title row from meta');

  // List: exactly one line — no meta rows, no head wrapper, identity in the
  // title and one short meta (the status badge) right.
  assert.equal((list.match(/nachweise-row-meta/g) || []).length, 0, 'list row has no meta lines');
  assert.ok(!/nachweise-row-head/.test(list), 'list row is a single flat line');
  assert.equal((list.match(/ctox-badge/g) || []).length, 1, 'list row carries exactly one short meta');
  assert.match(list, /nachweise-row-title">[^<]*·[^<]*Clara Eins/, 'list title carries type and subject');
  assert.ok(list.length < cards.length, 'the list row is the denser of the two');
});

test('credentials: no readiness / syncing state in the content (App-API 4.0, R4/R11)', async () => {
  const mod = await import('../index.js');
  const now = 1_781_990_000_000;
  assert.equal(mod.renderRecordList([], { view: 'cards', nowMs: now, readiness: { ready: false } }), '');
  const rows = [{ id: 'c1', subject_id: 'cand-1', subject_name: 'Clara Eins', credential_type: 'g25', verified: false }];
  assert.match(mod.renderRecordList(rows, { view: 'cards', nowMs: now }), /data-context-record-id="c1"/);
  assert.doesNotMatch(indexJs, /ctox-syncing|renderListOrState|collectionReadiness|commandBus|ctx\.db/, 'no v3 data/sync paths left');
});

test('credentials: band + counts derive from the derived credential status', async () => {
  const mod = await import('../index.js');
  const now = 1_781_990_000_000;
  const day = 24 * 3600 * 1000;
  const rows = [
    { id: 'valid', subject_id: 's', credential_type: 'g37', verified: true, valid_until_ms: now + 400 * day }, // valid
    { id: 'expiring', subject_id: 's', credential_type: 'g37', verified: true, valid_until_ms: now + 10 * day }, // expiring
    { id: 'expired', subject_id: 's', credential_type: 'g37', verified: true, valid_until_ms: now - 5 * day }, // expired → critical
    { id: 'unverified', subject_id: 's', credential_type: 'g37', verified: false }, // unverified → critical
  ];
  assert.equal(mod.statusOf(rows[0], now), 'valid');
  assert.equal(mod.statusOf(rows[1], now), 'expiring');
  assert.equal(mod.statusOf(rows[2], now), 'expired');
  assert.equal(mod.statusOf(rows[3], now), 'unverified');
  assert.equal(mod.credentialBand('expired'), 'critical');
  assert.equal(mod.credentialBand('unverified'), 'critical');
  assert.equal(mod.credentialBand('valid'), 'valid');
  assert.deepEqual(mod.countsFor(rows, now), { all: 4, valid: 1, expiring: 1, critical: 2 });
  assert.equal(mod.filterRows(rows, { band: 'critical' }, now).length, 2);
  assert.equal(mod.filterRows(rows, { band: 'valid' }, now).length, 1);
  assert.equal(mod.filterRows(rows, { status: 'expired' }, now).length, 1);
  assert.equal(mod.filterRows(rows, { search: 'g37' }, now).length, 4);
});
