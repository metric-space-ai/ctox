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

test('credentials: schema declares its owned collection', () => {
  const schemaSrc = read('schema.js');
  assert.match(schemaSrc, /business_credentials/);
  assert.match(schemaSrc, /export const collections/);
});

test('credentials: left column carries the canonical grammar markup pins', () => {
  // Search + shard/list toggle + collapsed tray with reset + footer target.
  assert.match(html, /data-pg-search/, 'grammar search input');
  assert.match(html, /data-pg-view="cards"/, 'shard view toggle');
  assert.match(html, /data-pg-view="list"/, 'list view toggle');
  assert.match(html, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(html, /data-pg-tray\b/, 'collapsed tray');
  assert.match(html, /data-pg-reset/, 'tray reset control');
  assert.match(html, /data-pg-footer/, 'one-line footer target');
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
  assert.match(indexJs, /\.upsert\(/, 'import upserts records');
  assert.match(indexJs, /prepareImport\(/, 'import normalizes via the record helper');
});

test('credentials: existing command flows are untouched', () => {
  // The two server-authoritative commands keep their exact types and payloads.
  assert.match(indexJs, /ats\.deployment\.check/, 'deployment gate command');
  assert.match(indexJs, /ats\.leistungsnachweis\.signoff/, 'sign-off command');
  // Capture stays a plain RxDB insert (no command).
  assert.match(indexJs, /\.insert\(record\)/, 'credential capture is a plain insert');
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
    { id: 'c1', subject_id: 'cand-1', credential_type: 'staplerschein', verified: true, valid_until_ms: now + 365 * 24 * 3600 * 1000 },
    { id: 'c2', subject_id: 'cand-2', credential_type: 'g25', verified: false },
  ];
  const out = mod.renderRecordList(rows, { view: 'cards', selectedId: 'c1', nowMs: now });
  assert.match(out, /data-context-record-id="c1"/, 'row carries the record id');
  assert.match(out, /data-context-record-type="nachweis"/, 'row carries the record type');
  assert.match(out, /data-context-label="/, 'row carries the record label');
  assert.match(out, /is-selected/, 'the selected row is marked');
  // No inline expansion, no per-row buttons inside the selection list.
  assert.ok(!/<details/.test(out), 'shards do not expand inline');
  assert.ok(!/<button/.test(out), 'shards carry no per-row buttons');

  const empty = mod.renderRecordList([], { view: 'cards', nowMs: now });
  assert.match(empty, /ctox-empty/, 'empty state renders the kit empty class');
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
