import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');
const html = read('index.html');
const indexJs = read('index.js');

test('consent: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'consent');
  assert.ok(manifest.collections.includes('business_consents'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('consent: schema declares its owned collection', () => {
  const schemaSrc = read('schema.js');
  assert.match(schemaSrc, /business_consents/);
  assert.match(schemaSrc, /export const collections/);
});

test('consent: denied collection access renders a locked state instead of an empty state', () => {
  const source = read('index.js');
  assert.match(source, /permissionCheck\(PRIMARY\)/);
  assert.match(source, /CTOX_BUSINESS_OS_PERMISSION_DENIED/);
  assert.match(source, /renderLockedCollection\(\)/);
  assert.match(source, /dataLockedHint/);
});

test('consent: left column carries the canonical grammar markup pins', () => {
  // Search + shard/list toggle + collapsed tray with reset + footer target.
  assert.match(html, /data-pg-search/, 'grammar search input');
  assert.match(html, /data-pg-view="cards"/, 'shard view toggle');
  assert.match(html, /data-pg-view="list"/, 'list view toggle');
  assert.match(html, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(html, /data-pg-tray\b/, 'collapsed tray');
  assert.match(html, /data-pg-reset/, 'tray reset control');
  assert.match(html, /data-pg-footer/, 'one-line footer target');
  // Status select in the tray drives the filter.
  assert.match(html, /data-pg-filter[^>]*data-pg-name="status"/, 'status filter in tray');
});

test('consent: counted band has >= 2 real views derived from status', () => {
  const bands = html.match(/data-pg-band="[^"]+"/g) || [];
  assert.ok(bands.length >= 2, `expected >= 2 view band tabs, got ${bands.length}`);
  const counts = html.match(/data-pg-count="[^"]+"/g) || [];
  assert.ok(counts.length >= 2, 'each band tab exposes a count target');
});

test('consent: header carries the standing Neu / Import / Export icon actions', () => {
  assert.match(html, /data-action="new"/, 'primary create action');
  assert.match(html, /data-action="import"/, 'import action');
  assert.match(html, /data-action="export"/, 'export action');
  // Icon buttons must be labelled.
  assert.match(html, /data-action="import"[^>]*aria-label=/, 'import icon has aria-label');
  assert.match(html, /data-action="export"[^>]*aria-label=/, 'export icon has aria-label');
});

test('consent: import/export/new/collapse handlers are wired in index.js', () => {
  assert.match(indexJs, /=== 'import'/, 'import action handled');
  assert.match(indexJs, /=== 'export'/, 'export action handled');
  assert.match(indexJs, /=== 'new'/, 'new action handled');
  assert.match(indexJs, /=== 'collapse-detail'/, 'detail collapse handled');
  // Export = JSON download via Blob URL (no HTTP).
  assert.match(indexJs, /new Blob\(/, 'export builds a Blob');
  assert.match(indexJs, /URL\.createObjectURL/, 'export uses an object URL');
  // Import = file input reading JSON, upserting via the record helper.
  assert.match(indexJs, /type = 'file'/, 'import creates a file input');
  assert.match(indexJs, /\.upsert\(/, 'import upserts records');
  assert.match(indexJs, /prepareImport\(/, 'import normalizes via the record helper');
});

test('consent: existing command flows are untouched', () => {
  // The three server-authoritative commands keep their exact types and payloads.
  assert.match(indexJs, /ats\.consent\.check/, 'consent-check command');
  assert.match(indexJs, /ats\.subject\.export/, 'Art. 15 export command');
  assert.match(indexJs, /ats\.subject\.erase/, 'Art. 17 erase command');
  // The check payload keeps its exact shape { subject_id, purpose }.
  assert.match(indexJs, /purpose: purposeRaw \|\| null/, 'consent-check payload unchanged');
  // Subject rights dispatch keeps the legacy `type` + `command_type` pair.
  assert.match(indexJs, /type: commandType, command_type: commandType/, 'subject dispatch shape unchanged');
});

test('consent: auto-reveal follows hasSelection && !userCollapsed', async () => {
  const mod = await import('../index.js');
  assert.equal(mod.shouldRevealRecord(true, false), true, 'selected + not collapsed → shown');
  assert.equal(mod.shouldRevealRecord(false, false), false, 'no selection → hidden');
  assert.equal(mod.shouldRevealRecord(true, true), false, 'user collapsed → hidden');
});

test('consent: record list renders selector rows from a stub doc array', async () => {
  const mod = await import('../index.js');
  const now = 1_781_990_000_000;
  const rows = [
    { id: 'c1', subject_id: 'cand-1', purpose: 'Bewerbung', legal_basis: 'consent', granted_at_ms: now - 1000 },
    { id: 'c2', subject_id: 'cand-2', purpose: 'Talentpool', legal_basis: 'consent' },
  ];
  const out = mod.renderRecordList(rows, { view: 'cards', selectedId: 'c1', nowMs: now });
  assert.match(out, /data-context-record-id="c1"/, 'row carries the record id');
  assert.match(out, /data-context-record-type="consent"/, 'row carries the record type');
  assert.match(out, /data-context-label="/, 'row carries the record label');
  assert.match(out, /is-selected/, 'the selected row is marked');
  // No inline expansion, no per-row buttons inside the selection list.
  assert.ok(!/<details/.test(out), 'shards do not expand inline');
  assert.ok(!/<button/.test(out), 'shards carry no per-row buttons');

  const empty = mod.renderRecordList([], { view: 'cards', nowMs: now });
  assert.match(empty, /ctox-empty/, 'empty state renders the kit empty class');
});

test('consent: band + counts derive from the derived consent status', async () => {
  const mod = await import('../index.js');
  const now = 1_781_990_000_000;
  const day = 24 * 3600 * 1000;
  const rows = [
    { id: 'active', subject_id: 's', purpose: 'p', legal_basis: 'consent', granted_at_ms: now - day }, // active → valid
    { id: 'pending', subject_id: 's', purpose: 'p', legal_basis: 'consent' }, // no grant → pending → open
    { id: 'withdrawn', subject_id: 's', purpose: 'p', legal_basis: 'consent', granted_at_ms: now - 5 * day, withdrawn_at_ms: now - day }, // withdrawn → ended
    { id: 'expired', subject_id: 's', purpose: 'p', legal_basis: 'consent', granted_at_ms: now - 10 * day, expires_at_ms: now - day }, // expired → ended
  ];
  assert.equal(mod.statusOf(rows[0], now), 'active');
  assert.equal(mod.statusOf(rows[1], now), 'pending');
  assert.equal(mod.statusOf(rows[2], now), 'withdrawn');
  assert.equal(mod.statusOf(rows[3], now), 'expired');
  assert.equal(mod.consentBand('active'), 'valid');
  assert.equal(mod.consentBand('pending'), 'open');
  assert.equal(mod.consentBand('withdrawn'), 'ended');
  assert.equal(mod.consentBand('expired'), 'ended');
  assert.deepEqual(mod.countsFor(rows, now), { all: 4, valid: 1, open: 1, ended: 2 });
  assert.equal(mod.filterRows(rows, { band: 'ended' }, now).length, 2);
  assert.equal(mod.filterRows(rows, { band: 'valid' }, now).length, 1);
  assert.equal(mod.filterRows(rows, { status: 'withdrawn' }, now).length, 1);
  assert.equal(mod.filterRows(rows, { search: 'talent' }, now).length, 0);
  assert.equal(mod.filterRows(rows, { search: 's' }, now).length, 4);
});

test('consent: prepareImport fills the schema-required fields for upsert', async () => {
  const mod = await import('../index.js');
  const now = 1_781_990_000_000;
  const rec = mod.prepareImport({ subject_id: 'x', purpose: 'p' }, now, '0');
  assert.equal(rec.subject_id, 'x');
  assert.equal(rec.purpose, 'p');
  assert.equal(rec.updated_at_ms, now, 'updated_at_ms stamped');
  assert.ok(rec.id, 'id is generated when missing');
  assert.equal(rec._deleted, false);
  // A round-tripped export keeps its id.
  assert.equal(mod.prepareImport({ id: 'keep', subject_id: 'x', purpose: 'p' }, now).id, 'keep');
});
