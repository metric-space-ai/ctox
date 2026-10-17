import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { registerHooks } from 'node:module';

// App-API 4.0: 'ctox:shared/x.js' löst in der Shell der Service Worker auf (§16.15); hier wie dort auf shared/.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('ctox:shared/')) {
      return nextResolve(new URL(`../../../shared/${specifier.slice('ctox:shared/'.length)}`, import.meta.url).href, context);
    }
    return nextResolve(specifier, context);
  },
});

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

test('consent: schema declares its owned collection (v4: collections.schema.json ist die einzige Quelle)', () => {
  const schema = JSON.parse(read('collections.schema.json'));
  assert.ok((schema.collections || schema).business_consents, 'business_consents declared');
  assert.ok(!existsSync(resolve(moduleRoot, 'schema.js')), 'schema.js entfällt (PORTING.md R17)');
});

test('consent: denied collection access renders a locked state instead of an empty state', () => {
  const source = read('index.js');
  // v4: der Core antwortet forbidden; die Sperre erscheint in der Zustandszone, nicht als Kasten im Inhalt.
  assert.match(source, /error\?\.code !== 'forbidden'/);
  assert.match(source, /renderLockedCollection\(\)/);
  assert.match(source, /code: MODULE_ID \+ '\.data\.locked'/);
  assert.match(source, /dataLockedHint/);
});

test('consent: left column carries the canonical grammar markup pins', () => {
  // Search + shard/list toggle + collapsed tray with reset + footer target.
  assert.match(html, /data-pg-search/, 'grammar search input');
  assert.match(html, /data-pg-view="cards"/, 'view toggle starts in the card view');
  assert.match(html, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(html, /data-pg-tray\b/, 'collapsed tray');
  assert.match(html, /data-pg-reset/, 'tray reset control');
  assert.doesNotMatch(html, /data-pg-footer/, 'keine Fußzeile, die nur den Reiterzähler wiederholt (Textdiät Regel 5)');
  // Status select in the tray drives the filter.
  assert.match(html, /data-pg-filter[^>]*data-pg-name="status"/, 'status filter in tray');
});

// Betreiber-Direktive 31.08.2026: der Karten/Listen-Umschalter ist EIN Knopf,
// der die Ansicht wechselt — kein Paar aus zwei Zustandsknoepfen. Der Wächter
// prüft deshalb die Anzahl der Bedienelemente, nicht mehr nur die Existenz
// zweier Marker, und dazu die Aktionssemantik (Ziel-Beschriftung, kein
// aria-pressed) sowie die Verdrahtung beider Ansichten in index.js.
test('consent: the card/list switch is ONE action button, not a state pair', () => {
  const viewControls = html.match(/data-pg-view="[^"]*"/g) || [];
  assert.equal(viewControls.length, 1, `expected exactly one view control, got ${viewControls.length}`);
  // Der eine Knopf benennt beide Ansichten: aktuelle und Gegenansicht.
  assert.match(html, /data-pg-view="cards"[^>]*data-pg-view-alt="list"/, 'toggle declares current + alternate view');
  // Eine Aktion hat keinen Gedrueckt-Zustand.
  const toggleTag = html.match(/<button[^>]*consent-view-toggle[^>]*>/)?.[0] || '';
  assert.ok(toggleTag, 'toggle button is present');
  assert.ok(!/aria-pressed/.test(toggleTag), 'action button carries no aria-pressed');
  assert.match(toggleTag, /aria-label="Als Liste anzeigen"/, 'label names the target view, not the current one');
  // index.js tauscht beide Werte, Icon und Beschriftung und raeumt das von der
  // generischen Shell-Verdrahtung gesetzte aria-pressed wieder ab.
  assert.match(indexJs, /viewToList/, 'target-view copy for the list direction');
  assert.match(indexJs, /viewToCards/, 'target-view copy for the card direction');
  assert.match(indexJs, /removeAttribute\('aria-pressed'\)/, 'shell-set aria-pressed is stripped');
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
  assert.match(indexJs, /api\.put\(PRIMARY, prepareImport\(/, 'import writes records via api.put');
  assert.match(indexJs, /prepareImport\(/, 'import normalizes via the record helper');
});

test('consent: existing command flows are untouched', () => {
  // The three server-authoritative commands keep their exact types and payloads.
  assert.match(indexJs, /ats\.consent\.check/, 'consent-check command');
  assert.match(indexJs, /ats\.subject\.export/, 'Art. 15 export command');
  assert.match(indexJs, /ats\.subject\.erase/, 'Art. 17 erase command');
  // The check payload keeps its exact shape { subject_id, purpose }.
  assert.match(indexJs, /purpose: purposeRaw \|\| null/, 'consent-check payload unchanged');
  // v4: Befehle über api.command (Typ + Payload), Status gepusht – kein commandBus, kein Warten.
  assert.match(indexJs, /runCommand\(commandType, \{ subject_id \}/, 'subject dispatch payload unchanged');
  assert.match(indexJs, /api\.command\(type, /);
  assert.doesNotMatch(indexJs, /commandBus/);
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
  assert.equal(empty, '', 'v4 (R11): an empty list renders nothing');
});

// Betreiber-Direktive 31.08.2026: die beiden Ansichten muessen sich in der
// DICHTE unterscheiden, nicht nur marginal — Karte mehrzeilig mit den
// wichtigsten Detailfeldern, Liste genau eine kompakte Zeile.
test('consent: card and list view differ in density, not just in detail', async () => {
  const mod = await import('../index.js');
  const now = 1_781_990_000_000;
  const rows = [{
    id: 'c1', subject_id: 'cand-10422', subject_name: 'Ella Vier', purpose: 'Bewerbung', legal_basis: 'consent',
    granted_at_ms: now - 86_400_000, expires_at_ms: now + 86_400_000,
  }];

  const cards = mod.renderRecordList(rows, { view: 'cards', nowMs: now });
  const cardMeta = cards.match(/class="consent-row-meta/g) || [];
  assert.ok(cardMeta.length >= 2, `card view carries >= 2 meta lines, got ${cardMeta.length}`);
  assert.match(cards, /consent-row--cards/, 'card rows carry the card modifier');
  // Die Detailfelder der App, keine neuen Datenpfade.
  assert.match(cards, /Ella Vier/, 'card meta shows the subject');
  assert.match(cards, /Rechtsgrundlage/, 'card meta shows the legal basis');
  assert.match(cards, /erteilt/, 'card meta shows the grant date');
  assert.match(cards, /gültig bis/, 'card meta shows the expiry');

  const list = mod.renderRecordList(rows, { view: 'list', nowMs: now });
  assert.match(list, /consent-row--list/, 'list rows carry the list modifier');
  assert.equal((list.match(/class="consent-row-meta/g) || []).length, 0,
    'the compact list row carries no meta lines');
  // Nur der sichtbare Rumpf zaehlt — die Kontext-Attribute des Zeilen-Wrappers
  // (data-context-label) tragen den Subjektnamen weiterhin fuer das Kontextmenue.
  const listBody = list.replace(/^<div[^>]*>/, '').replace(/<\/div>$/, '');
  assert.ok(!/cand-10422/.test(listBody), 'the compact row shows no assignment detail');
  assert.ok(!/consent-row-head/.test(listBody), 'the compact row needs no head wrapper');
  // Genau ein Kurz-Meta rechts: der Status.
  assert.equal((list.match(/ctox-badge/g) || []).length, 1, 'exactly one short meta in the list row');
});

test('consent: v4 – no readiness, no syncing shell; empty renders nothing (App-API 4.0)', async () => {
  const mod = await import('../index.js');
  const now = 1_781_990_000_000;
  assert.equal(mod.renderRecordList([], { view: 'cards', nowMs: now, readiness: { ready: false } }), '');
  const rows = [{ id: 'c1', subject_id: 's', purpose: 'p', granted_at_ms: now - 1000 }];
  assert.match(mod.renderRecordList(rows, { view: 'cards', nowMs: now }), /data-context-record-id="c1"/);
  assert.doesNotMatch(indexJs, /subscribeCollectionReadiness|collectionReadiness|ctox-syncing/);
  assert.match(indexJs, /api\.query\(PRIMARY, \{\}\)\.subscribe\(/, 'live query');
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
  assert.equal(Object.hasOwn(rec, '_deleted'), false, '_deleted ist reserviert (Kern-Tombstone, E47) und wird nie geschrieben');
  // A round-tripped export keeps its id.
  assert.equal(mod.prepareImport({ id: 'keep', subject_id: 'x', purpose: 'p' }, now).id, 'keep');
});
