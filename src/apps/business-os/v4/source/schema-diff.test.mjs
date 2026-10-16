// node --test src/apps/business-os/v4/source/schema-diff.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { parseCollectionsFile, classifyCollections, publishBlockers, withMigration, toManifestCollections } from './schema-diff.js';

const base = { collections: { items: { type: 'object', properties: { id: { type: 'string' }, count: { type: 'number' } }, required: ['id'] } } };
const P = (o) => parseCollectionsFile(JSON.stringify(o));
const clone = (o) => structuredClone(o);
const kindOf = (a, b, opt) => classifyCollections(P(a), P(b), opt).find((d) => d.name === 'items');

test('unverändert', () => assert.equal(kindOf(base, base).kind, 'unchanged'));
test('optionales Feld mit Default = kompatibel, keine Migration', () => {
  const b = clone(base); b.collections.items.properties.note = { type: 'string', default: '' };
  const d = kindOf(base, b); assert.equal(d.kind, 'compatible'); assert.equal(d.needsMigration, false);
});
test('optionales Feld ohne Default = neue Generation', () => {
  const b = clone(base); b.collections.items.properties.note = { type: 'string' };
  assert.equal(kindOf(base, b).kind, 'generation');
});
test('neues Pflichtfeld mit Default = neue Generation', () => {
  const b = clone(base); b.collections.items.properties.note = { type: 'string', default: '' }; b.collections.items.required.push('note');
  assert.equal(kindOf(base, b).kind, 'generation');
});
test('Typänderung = neue Generation mit Begründung', () => {
  const b = clone(base); b.collections.items.properties.count = { type: 'string' };
  const d = kindOf(base, b); assert.equal(d.kind, 'generation'); assert.match(d.reasons[0], /number → string/);
});
test('Feld entfernt = neue Generation', () => {
  const b = clone(base); delete b.collections.items.properties.count;
  assert.equal(kindOf(base, b).kind, 'generation');
});
test('Index und Rechte = kompatibel (beide Dateiformen)', () => {
  const b = { collections: { items: { schema: base.collections.items, indexes: ['count'], rights: { read: ['user'] } } } };
  const d = kindOf(base, b); assert.equal(d.kind, 'compatible'); assert.deepEqual(d.reasons, ['Indizes geändert', 'Rechte geändert']);
});
test('neue / entfernte Collection', () => {
  const b = clone(base); b.collections.tags = { type: 'object', properties: { id: { type: 'string' } } };
  assert.equal(classifyCollections(P(base), P(b)).find((d) => d.name === 'tags').kind, 'new');
  assert.equal(classifyCollections(P(b), P(base)).find((d) => d.name === 'tags').kind, 'removed');
});
test('Pflicht-Migration: fehlt, Datei fehlt, veraltete from_ver, passt', () => {
  const b = clone(base); b.collections.items.properties.count = { type: 'string' };
  let d = classifyCollections(P(base), P(b), { baseVer: 's6' });
  assert.deepEqual(publishBlockers(d, []).map((x) => x.reason), ['migration_missing']);
  const withM = JSON.parse(withMigration(JSON.stringify(b), 'items', 's6'));
  d = classifyCollections(P(base), P(withM), { baseVer: 's6' });
  assert.deepEqual(publishBlockers(d, []).map((x) => x.reason), ['migration_file_missing']);
  assert.deepEqual(publishBlockers(d, ['migrations/items.js']), []);
  d = classifyCollections(P(base), P(withM), { baseVer: 's9' }); // Rest einer früheren Generation zählt nicht
  assert.deepEqual(publishBlockers(d, ['migrations/items.js']).map((x) => x.reason), ['migration_missing']);
});
test('Manifest-Form §9.1', () => {
  const m = toManifestCollections(P({ collections: { items: { schema: base.collections.items, indexes: ['count'], class: 'full' } } }));
  assert.deepEqual(m.items.schema.indexes, ['count']); assert.equal(m.items.class, 'full');
});
test('ungültiges JSON wirft', () => assert.throws(() => parseCollectionsFile('{ x')));
test('Migration mit deklarativen steps zählt (wie Kern-Stub)', () => {
  const b = clone(base); b.collections.items.properties.count = { type: 'string' };
  b.migrations = { items: { from_ver: 's6', steps: [{ op: 'cast', field: 'count', to: 'string' }] } };
  const d = classifyCollections(P(base), P(b), { baseVer: 's6' });
  assert.deepEqual(publishBlockers(d, []), []);
});

// Gleichlauf mit der Einstufung des Kern-Stubs (maßgeblich, §16.22 A6). Pin über CTOX_APP_API_STUB, Default 9fa9948.
test('Gleichlauf mit Kern-Stub classifyCollections', async () => {
  const { homedir } = await import('node:os');
  const { join } = await import('node:path');
  const { pathToFileURL } = await import('node:url');
  const pin = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
  const stub = await import(pathToFileURL(join(pin, 'ctox-app-api-stub.mjs')).href);
  const variants = [];
  const v = (f) => { const b = clone(base); f(b); variants.push(b); };
  v(() => {});
  v((b) => { b.collections.items.properties.note = { type: 'string', default: '' }; });
  v((b) => { b.collections.items.properties.note = { type: 'string' }; });
  v((b) => { b.collections.items.properties.count = { type: 'string' }; });
  v((b) => { b.collections.items.properties.count = { type: 'string' }; b.migrations = { items: { from_ver: 's6', migrate_fn: 'migrations/items.js' } }; });
  v((b) => { b.collections.items.properties.count = { type: 'string' }; b.migrations = { items: { from_ver: 's5', migrate_fn: 'migrations/items.js' } }; });
  v((b) => { b.collections.items.properties.count = { type: 'string' }; b.migrations = { items: { from_ver: 's6', steps: [{ op: 'cast', field: 'count', to: 'string' }] } }; });
  v((b) => { delete b.collections.items.properties.count; });
  v((b) => { b.collections.items.required.push('count'); });
  v((b) => { b.collections.tags = { type: 'object', properties: { id: { type: 'string' } } }; });
  v((b) => { delete b.collections.items; });
  v((b) => { b.collections.items = { schema: b.collections.items, indexes: ['count'], rights: { read: ['user'] }, class: 'full' }; });
  for (const b of variants) {
    const shell = classifyCollections(P(base), P(b), { baseVer: 's6' });
    const kern = stub.classifyCollections(stub.parseCollectionsFile(JSON.stringify(base)), stub.parseCollectionsFile(JSON.stringify(b)), { baseVer: 's6' });
    assert.deepEqual(shell, kern, JSON.stringify(b));
  }
});
