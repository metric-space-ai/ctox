import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadLeadRevisionChanges } from '../../customer-modules/outbound-lead-generation/lead-revision-loader.mjs';
function world(rows) {
  const stored = new Map(rows.map((row) => [row.id, structuredClone(row)]));
  const requests = [];
  let afterManifest = null;
  const collection = { find: (query) => ({ exec: async () => {
    requests.push(structuredClone(query));
    assert.ok(query.requireRevision, 'every fetch is current-generation strict');
    let found = [...stored.values()].sort((a, b) => a.id.localeCompare(b.id));
    if (query.selector?.id?.$gt) found = found.filter((row) => row.id > query.selector.id.$gt);
    if (query.selector?.id?.$in) found = found.filter((row) => query.selector.id.$in.includes(row.id));
    found = found.slice(0, query.limit);
    const result = found.map((row) => {
      const dto = query.projection ? Object.freeze({ id: row.id, _rev: row._rev, _deleted: row._deleted === true }) : structuredClone(row);
      return { toJSON: () => dto };
    });
    if (query.projection && afterManifest) { const fn = afterManifest; afterManifest = null; fn(); }
    return result;
  } }) };
  return { stored, requests, collection, afterManifest: (fn) => { afterManifest = fn; } };
}
const lead = (id, rev = '1-' + id) => ({ id, _rev: rev, campaign: 'K', updated_at_ms: 100,
  data: { firma_name: 'Firma ' + id }, contacts: [{ id: 'person-' + id, email: id + '@firma.test' }],
  evidence: [{ quote: 'A'.repeat(60000) }], field_status: { firma_name: { status: 'verified' } },
});
test('first load keeps full records; manifest pages include every one of 351 leads', async () => {
  const w = world(Array.from({ length: 351 }, (_, i) => lead('l' + String(i).padStart(4, '0'))));
  const result = await loadLeadRevisionChanges(w.collection, []);
  assert.equal(result.rows.length, 351); assert.equal(result.changedIds.size, 351);
  assert.equal(w.requests.filter((q) => q.projection).length, 3);
  for (const q of w.requests.filter((q) => q.projection)) assert.deepEqual(q.projection, ['id']);
  for (const q of w.requests.filter((q) => !q.projection)) assert.ok(q.limit <= 8);
  assert.equal(result.rows[0].evidence[0].quote.length, 60000);
  assert.equal(result.rows[0].contacts[0].email, 'l0000@firma.test');
});
test('unchanged revisions require only metadata queries, no full record fetch', async () => {
  const w = world([lead('a'), lead('b')]);
  const first = await loadLeadRevisionChanges(w.collection, []); w.requests.length = 0;
  const second = await loadLeadRevisionChanges(w.collection, first.rows);
  assert.equal(second.changedIds.size, 0); assert.equal(second.removedIds.size, 0);
  assert.ok(w.requests.every((q) => q.projection));
  assert.equal(second.rows[0], first.rows[0], 'reuse full data; never replace with the metadata DTO');
});
test('insert, delete, campaign move and equal/future timestamps are revision-driven', async () => {
  const w = world([lead('a'), lead('b'), lead('c'), lead('d')]);
  const first = await loadLeadRevisionChanges(w.collection, []); w.requests.length = 0;
  w.stored.set('a', { ...lead('a', '2-a'), campaign: 'New', updated_at_ms: 100 });
  w.stored.set('b', { ...lead('b', '2-b'), updated_at_ms: 9e15 });
  w.stored.delete('c'); w.stored.set('e', lead('e'));
  const second = await loadLeadRevisionChanges(w.collection, first.rows);
  assert.deepEqual([...second.changedIds], ['a', 'b', 'e']); assert.deepEqual([...second.removedIds], ['c']);
  assert.deepEqual(w.requests.filter((q) => !q.projection).flatMap((q) => q.selector.id.$in), ['a', 'b', 'e']);
  assert.equal(second.rows.find((r) => r.id === 'a').campaign, 'New');
  assert.equal(second.rows.find((r) => r.id === 'd'), first.rows.find((r) => r.id === 'd'));
});
test('a concurrent change is rechecked once against a new manifest', async () => {
  const w = world([lead('a')]);
  w.afterManifest(() => w.stored.set('a', lead('a', '2-a')));
  const result = await loadLeadRevisionChanges(w.collection, []);
  assert.equal(result.rows[0]._rev, '2-a');
  assert.equal(w.requests.filter((q) => !q.projection).length, 2);
});
test('repeated revision mismatch fails without publishing any partial result', async () => {
  let fullReads = 0;
  const collection = { find: (q) => ({ exec: async () => {
    if (q.selector.id?.$gt) return [];
    if (!q.projection) fullReads++;
    return [{ toJSON: () => lead('a', q.projection ? '1-a' : '2-a') }];
  } }) };
  const existing = [lead('old')]; const before = structuredClone(existing);
  await assert.rejects(loadLeadRevisionChanges(collection, existing), { code: 'LEAD_HYDRATION_RACE', retryable: true });
  assert.equal(fullReads, 2); assert.deepEqual(existing, before);
});
test('bridge replacement aborts its cycle instead of retrying an old manifest as a document race', async () => {
  const failure = Object.assign(new Error('bridge replaced'), { code: 'QUERY_GENERATION_REPLACED' });
  const requests = [];
  const collection = { find: (q) => ({ exec: async () => {
    requests.push(q);
    if (!q.projection) throw failure;
    return q.selector.id?.$gt ? [] : [{ toJSON: () => ({ id: 'a', _rev: '1-a' }) }];
  } }) };
  await assert.rejects(loadLeadRevisionChanges(collection, []), (error) => error === failure);
  assert.equal(requests.filter((q) => !q.projection).length, 1);
});
test('each hydration cycle uses fresh strict tokens, even for the same document revisions', async () => {
  const w = world([lead('a')]);
  await loadLeadRevisionChanges(w.collection, []);
  const first = w.requests.filter((q) => !q.projection)[0].requireRevision;
  w.requests.length = 0;
  await loadLeadRevisionChanges(w.collection, []);
  const second = w.requests.filter((q) => !q.projection)[0].requireRevision;
  assert.notEqual(first, second);
});
test('unsupported projection never retries as a full query', async () => {
  const requests = [];
  const collection = { find: (q) => ({ exec: async () => {
    requests.push(q); throw Object.assign(new Error('Old native'), { code: 'QUERY_PROJECTION_NOT_SUPPORTED' });
  } }) };
  await assert.rejects(loadLeadRevisionChanges(collection, []), { code: 'QUERY_PROJECTION_NOT_SUPPORTED' });
  assert.equal(requests.length, 1); assert.deepEqual(requests[0].projection, ['id']);
});
test('missing revisions and broken pagination fail without full hydration', async () => {
  let reads = 0;
  await assert.rejects(loadLeadRevisionChanges({ find: (q) => ({ exec: async () => {
    reads++; assert.ok(q.projection); return [{ toJSON: () => ({ id: 'a' }) }];
  } }) }, []), { code: 'LEAD_REVISION_MISSING' });
  assert.equal(reads, 1);
  await assert.rejects(loadLeadRevisionChanges({ find: () => ({ exec: async () => [{ toJSON: () => ({ id: 'a', _rev: '1-a' }) }] }) }, []), { code: 'LEAD_REVISION_PAGINATION' });
});
