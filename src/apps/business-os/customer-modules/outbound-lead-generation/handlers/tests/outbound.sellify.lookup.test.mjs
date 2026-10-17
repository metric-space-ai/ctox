import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.sellify.lookup.js';
import { likeContains, sqlCastInteger, projectBatchRecord } from '../_lib/sellify-lookup.js';

const data = () => ({
  sellify_companies: {
    c1: { id: 'c1', name: 'Müller GmbH', contact_id: 101, website_url: 'https://mueller.de', updated_at_ms: 10 },
    c2: { id: 'c2', name: 'MÜLLER AG', contact_id: '102', website_url: 'https://mueller.ch', updated_at_ms: 30 },
    c3: { id: 'c3', name: 'Acme Müller', contact_id: 103, updated_at_ms: 30, email: 'info@acme.de' },
    c4: { id: 'c4', name: 'Zeta', contact_id: 104, updated_at_ms: '20x' },
  },
  sellify_people: {
    p1: { id: 'p1', person_id: 'P-1', contact_id: 101, email: 'a@mueller.de', last_name: 'Alt', updated_at_ms: 5, meta: { role: 'CEO', extra: 1 } },
    p2: { id: 'p2', person_id: 'P-2', contact_id: 101, email: 'b@mueller.de', last_name: 'Bern' },
  },
  sellify_campaigns: {
    m1: { id: 'm1', name: 'Herbst 2025 ', contact_id: 101 },
    m2: { id: 'm2', name: 'Herbst 2025', contact_id: 102 },
    m3: { id: 'm3', name: 'Herbst 2025 ', contact_id: 103 },
    m4: { id: 'm4', name: 'Frühjahr', contact_id: 104 },
    m5: { id: 'm5', name: 'Herbst 2025', contact_id: 105, is_deleted: true },
  },
});

const run = (t, payload) => handle(cmd('outbound.sellify.lookup', payload), t.ctx);

test('LIKE and CAST semantics', () => {
  assert.equal(likeContains('MÜLLER AG', 'ller'), true);
  assert.equal(likeContains('MÜLLER AG', 'müller'), false); // SQLite folds ASCII only
  assert.equal(likeContains('Müller GmbH', 'MÜLLER'.toLowerCase().replace('ü', 'ü')), true);
  assert.equal(sqlCastInteger('20x'), 20);
  assert.equal(sqlCastInteger(undefined), 0);
  assert.equal(sqlCastInteger(12.9), 12);
});

test('exact selector matches string or i64, ordered by updated_at_ms desc, id desc (ref :1493)', async () => {
  const t = createTestCtx({ data: data() });
  const r = await run(t, { entity: 'company', selectors: [{ field: 'contact_id', value: '101' }, { field: 'contact_id', value: '102' }] });
  assert.deepEqual(r.records.map((x) => x.id), ['c1', 'c2']);
  assert.equal(r.entity, 'company');
  assert.equal(r.ok, true);
});

test('fuzzy selector: ASCII-case-insensitive containment, dedupe, limit', async () => {
  const t = createTestCtx({ data: data() });
  const r = await run(t, { entity: 'company', fuzzy_selectors: [{ field: 'name', value: 'LLER' }, { field: 'name', value: 'Acme' }], limit: 2 });
  // updated 30: c3 > c2 by id desc; then limit reached
  assert.deepEqual(r.records.map((x) => x.id), ['c3', 'c2']);
  const short = await run(t, { entity: 'company', fuzzy_selectors: [{ field: 'name', value: 'M' }] });
  assert.deepEqual(short.records, []);
  const umlaut = await run(t, { entity: 'company', fuzzy_selectors: [{ field: 'name', value: 'ü' }] }); // 2 UTF-8 bytes → runs
  assert.deepEqual(umlaut.records.map((x) => x.id), ['c3', 'c1']);
});

test('ids, field projection and unsupported field', async () => {
  const t = createTestCtx({ data: data() });
  const r = await run(t, { entity: 'company', ids: [' c4 ', 'c4', 'missing', 7], fields: ['name'] });
  assert.deepEqual(r.records, [{ id: 'c4', name: 'Zeta' }]);
  const e = await rejects(run(t, { entity: 'company', selectors: [{ field: 'secret', value: 'x' }] }));
  assert.equal(e.message, 'unsupported company lookup field `secret`');
  assert.equal((await rejects(run(t, { entity: 'deal' }))).message, 'entity must be company, person or campaign');
  assert.equal((await rejects(run(t, {}))).message, 'entity is required');
});

test('campaign grouping by name: trimmed, max per trimmed name, excludes is_deleted (ref :1547)', async () => {
  const t = createTestCtx({ data: data() });
  const r = await run(t, { entity: 'campaign', group_by: 'name', selectors: [{ field: 'name', value: 'herbst' }] });
  assert.deepEqual(r.groups, [{ name: 'Herbst 2025', count: 2 }]);
  assert.equal(r.scanned, 3);
  assert.equal(r.truncated, false);
  assert.deepEqual(r.records, []);
});

test('batch: projection, complete flag via limit+1 (ref :1889)', async () => {
  const t = createTestCtx({ data: data() });
  const r = await run(t, { batch: [
    { key: 'k1', entity: 'person', selectors: [{ field: 'contact_id', value: '101' }], fields: ['email', 'meta.role'], limit: 1 },
    { key: 'k2', entity: 'company', fuzzy_selectors: [{ field: 'name', value: 'zeta' }], fields: ['name'] },
  ] });
  assert.equal(r.schema, 'ctox.outbound.sellify_lookup_batch.v1');
  assert.equal(r.results[0].complete, false);
  assert.equal(r.results[0].records.length, 1);
  assert.deepEqual(Object.keys(r.results[0].records[0]).sort(), ['email', 'id', 'meta']);
  assert.deepEqual(r.results[0].records[0].meta, { role: 'CEO' });
  assert.deepEqual(r.results[1], { key: 'k2', entity: 'company', records: [{ id: 'c4', name: 'Zeta' }], complete: true });
});

test('batch validation messages', async () => {
  const t = createTestCtx({ data: data() });
  const bad = async (batch) => (await rejects(run(t, { batch }))).message;
  assert.equal(await bad(null), 'Sellify batch must be an array');
  assert.equal(await bad([]), 'Sellify batch must contain 1 to 50 requests');
  const ok = { key: 'a', entity: 'company', selectors: [{ field: 'name', value: 'x' }], fields: ['name'] };
  assert.equal(await bad([ok, ok]), 'invalid or duplicate Sellify batch key');
  assert.equal(await bad([{ ...ok, entity: 'campaign' }]), 'Sellify batch entity must be company or person');
  assert.equal(await bad([{ ...ok, ids: null }]), 'nested batches, groups and internal record IDs are not supported in a Sellify batch');
  assert.equal(await bad([{ ...ok, selectors: null }]), 'Sellify selectors must be an array');
  assert.equal(await bad([{ ...ok, selectors: [], fuzzy_selectors: [{ field: 'name', value: 'x' }] }]), 'Sellify fuzzy batch selector is too short to execute');
  assert.equal(await bad([{ ...ok, selectors: [] }]), 'Sellify batch request requires a nonempty selector');
  assert.equal(await bad([{ ...ok, fields: [] }]), 'Sellify batch needs 1 to 32 output fields');
  assert.equal(await bad([{ ...ok, fields: ['a..b'] }]), 'invalid Sellify output field path');
  assert.equal(await bad([{ ...ok, limit: 51 }]), 'Sellify batch limit must be 1 to 50');
  const nine = Array.from({ length: 9 }, (_, i) => ({ key: `f${i}`, entity: 'company', fuzzy_selectors: [{ field: 'name', value: 'xx' }], fields: ['name'] }));
  assert.equal(await bad(nine), 'Sellify batch exceeds the 200-selector/8-fuzzy-probe budget; split the batch');
  assert.equal(t.calls.writes.length, 0);
});

test('projection paths; missing collection is an error, not empty', async () => {
  // a missing path is skipped; a later parent path overwrites the nested projection (Rust insert)
  assert.deepEqual(projectBatchRecord({ id: 'x', a: { b: 1, c: 2 } }, ['a.b', 'z.q']), { id: 'x', a: { b: 1 } });
  assert.deepEqual(projectBatchRecord({ id: 'x', a: { b: 1, c: 2 } }, ['a.b', 'a']), { id: 'x', a: { b: 1, c: 2 } });
  const t = createTestCtx({ data: {} });
  const ctx = { ...t.ctx, read: async (c) => { throw Object.assign(new Error(`collection ${c} not readable`), { code: 'forbidden' }); } };
  const e = await rejects(handle(cmd('outbound.sellify.lookup', { entity: 'person' }), ctx));
  assert.equal(e.code, 'forbidden');
});
