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
const {
  renderList,
  filterRecords,
  bandCounts,
  lifecycleBand,
  computeDetailVisible,
  normalizeSignatureRequest,
} = await import('../index.js');

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');

test('esign: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'esign');
  assert.ok(manifest.collections.includes('signature_requests'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('esign: schema declares its owned collection (v4: collections.schema.json ist die einzige Quelle)', () => {
  const schema = JSON.parse(read('collections.schema.json'));
  assert.ok((schema.collections || schema).signature_requests, 'signature_requests declared');
  assert.ok(!existsSync(resolve(moduleRoot, 'schema.js')), 'schema.js entfällt (PORTING.md R17)');
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
  // ONE view control, not a pressed/unpressed pair (Betreiber-Direktive
  // 31.08.2026): the single button toggles, `data-pg-view` carries the CURRENT
  // view for the shell grammar, and the label names the TARGET view. A button
  // that is an action must not advertise `aria-pressed`.
  const viewControls = (html.match(/data-pg-view=/g) || []).length;
  assert.equal(viewControls, 1, `expected exactly one view control, found ${viewControls}`);
  assert.match(html, /data-pg-view="cards"/);
  assert.match(html, /data-esign-view-toggle/);
  const toggleTag = html.match(/<button[^>]*data-esign-view-toggle[^>]*>/)[0];
  assert.ok(!/aria-pressed/.test(toggleTag), 'the single view toggle must not carry aria-pressed');
  assert.match(toggleTag, /aria-label="Als Liste anzeigen"/);
  assert.match(toggleTag, /title="Als Liste anzeigen"/);
  // Both icons live inside that one button; CSS shows the target view's icon.
  assert.match(html, /data-esign-view-icon="list"/);
  assert.match(html, /data-esign-view-icon="cards"/);
  // Footer target.
  assert.doesNotMatch(html, /data-pg-footer/, 'keine Fußzeile, die nur den Reiterzähler wiederholt (Textdiät Regel 5)');
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
  // Export via Blob URL download, import via file input + JSON, write through
  // api.put (App-API 4.0) — no HTTP.
  assert.match(js, /new Blob\(/);
  assert.match(js, /URL\.createObjectURL/);
  assert.match(js, /input\.type = 'file'/);
  assert.match(js, /JSON\.parse/);
  assert.match(js, /api\.put\(PRIMARY, record\)/);
  assert.match(js, /\[data-action="import"\]/);
  assert.match(js, /\[data-action="export"\]/);
});

test('esign: renders a shard list from a stub doc array with the context trio', () => {
  const docs = [
    { id: 'r1', document_id: 'DOC-1', document_title: 'Vertrag Eins', subject_kind: 'arbeitsvertrag', status: 'created', signers: [{ id: 's1', state: 'pending' }] },
    { id: 'r2', document_id: 'DOC-2', document_title: 'Vertrag Zwei', subject_kind: 'vermittlungsvertrag', status: 'completed', signers: [{ id: 's2', state: 'signed' }] },
  ];
  const cardsHtml = renderList(docs, { view: 'cards', selectedId: 'r2' });
  assert.match(cardsHtml, /Vertrag Eins/);
  assert.match(cardsHtml, /Vertrag Zwei/);
  assert.match(cardsHtml, /data-context-record-id="r1"/);
  assert.match(cardsHtml, /data-context-record-type="signature_request"/);
  assert.match(cardsHtml, /data-esign-row="r2"/);
  // Selected row is highlighted via the kit class.
  assert.match(cardsHtml, /data-esign-row="r2"[^>]*is-selected|is-selected[^>]*data-esign-row="r2"/);
  // Compact variant is a distinct rendering behind the toggle.
  const listHtml = renderList(docs, { view: 'list', selectedId: '' });
  assert.match(listHtml, /esign-row-compact/);
  // v4 (R11): an empty list renders nothing.
  assert.equal(renderList([], { view: 'cards' }), '');
});

test('esign: cards and list are genuinely different renderings', () => {
  const docs = [{
    id: 'r1', document_id: 'DOC-1', document_title: 'Vertrag Eins', subject_kind: 'arbeitsvertrag', status: 'sent',
    signers: [{ id: 's1', state: 'signed' }, { id: 's2', state: 'pending' }],
    created_at_ms: 1756000000000, updated_at_ms: 1756600000000,
  }];
  const cardsHtml = renderList(docs, { view: 'cards' });
  const listHtml = renderList(docs, { view: 'list' });
  // Rows carry their variant so the stylesheet can give each its own density.
  assert.match(cardsHtml, /esign-row--card/);
  assert.match(listHtml, /esign-row--list/);
  assert.ok(!/esign-row--list/.test(cardsHtml));
  assert.ok(!/esign-row--card/.test(listHtml));
  // Card = title + meta lines carrying the record's own detail fields.
  const cardMetaLines = (cardsHtml.match(/esign-shard-meta/g) || []).length;
  assert.ok(cardMetaLines >= 2, `expected >= 2 card meta lines, found ${cardMetaLines}`);
  assert.match(cardsHtml, /Arbeitsvertrag/);
  assert.match(cardsHtml, /1\/2 signiert/);
  assert.match(cardsHtml, /Aktualisiert/);
  // List = exactly one dense line: title + one short meta, no meta stack.
  assert.ok(!/esign-shard-meta/.test(listHtml));
  assert.ok(!/Aktualisiert/.test(listHtml));
  assert.ok(listHtml.length < cardsHtml.length, 'the list row must be leaner than the card');
});

test('esign: v4 – no readiness, no syncing shell; empty renders nothing (App-API 4.0)', () => {
  assert.equal(renderList([], { view: 'cards', readiness: { ready: false } }), '');
  assert.match(renderList([{ id: 'r1', document_id: 'DOC-1', document_title: 'Vertrag Eins', status: 'created', signers: [] }], { view: 'cards' }), /Vertrag Eins/);
  const js = read('index.js');
  assert.doesNotMatch(js, /ctox-syncing|collectionReadiness|subscribeCollectionReadiness|commandBus/);
  assert.match(js, /api\.query\(PRIMARY, \{\}\)\.subscribe\(onRows\)/);
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
