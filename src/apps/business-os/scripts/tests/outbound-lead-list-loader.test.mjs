import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadLeadList, loadFullLeadRows, leadListRow, LEAD_LIST_PROJECTION } from '../../customer-modules/outbound-lead-generation/lead-list-loader.mjs';

const lead = (id, rev = `1-${id}`) => ({ id, _rev: rev, name: 'Firma ' + id, campaign: 'Chemie', country: 'DE', city: 'Köln',
  research_status: 'needs_review', updated_at_ms: 1,
  payload: { weitere_kampagnen: ['Test'], imported_row: { sellify_contact_id: 'crm-' + id, blob: 'unused'.repeat(10000) } },
  contacts: [{ id: 'person-' + id, person_email: id + '@firma.test' }],
  data: { firma_name: id }, evidence: [{ quote: 'Beleg'.repeat(12000) }], field_status: { firma_name: { status: 'verified' } },
});
function world(rows) {
  const stored = new Map(rows.map(row => [row.id, structuredClone(row)]));
  const requests = [];
  let bytes = 0;
  const collection = { find(query) { return { exec: async () => {
    requests.push(query); assert.ok(query.requireRevision);
    const found = [...stored.values()].sort((a, b) => a.id.localeCompare(b.id))
      .filter(row => !query.selector.id || (query.selector.id.$in ? query.selector.id.$in.includes(row.id) : row.id > query.selector.id.$gt))
      .slice(0, query.limit).map(row => query.projection ? leadListRow(row) : structuredClone(row));
    bytes += Buffer.byteLength(JSON.stringify(found));
    return found.map(row => ({ toJSON: () => row }));
  } }; } };
  return { stored, collection, requests, bytes: () => bytes };
}
test('850-lead cold list transfers only compact summaries, not contacts, evidence or raw import data', async () => {
  const w = world(Array.from({ length: 850 }, (_, i) => lead(String(i).padStart(4, '0'))));
  const list = await loadLeadList(w.collection);
  assert.equal(list.rows.length, 850);
  assert.ok(w.requests.every(query => query.projection));
  assert.ok(w.bytes() < 1_000_000, 'fixture response bytes, not production network measurement');
  for (const row of list.rows) {
    for (const key of ['data', 'contacts', 'evidence', 'field_status']) assert.equal(row[key], undefined);
    assert.deepEqual(row.payload.imported_row, { sellify_contact_id: 'crm-' + row.id });
    assert.deepEqual(row.payload.weitere_kampagnen, ['Test']);
  }
});
test('changed/new/deleted rows are revision driven even with equal or future timestamps', async () => {
  const w = world([lead('a'), lead('b'), lead('c')]);
  const first = await loadLeadList(w.collection);
  w.stored.set('a', { ...lead('a', '2-a'), updated_at_ms: 1, campaign: 'Moved' });
  w.stored.delete('b'); w.stored.set('d', { ...lead('d'), updated_at_ms: 9e15 });
  const second = await loadLeadList(w.collection, first.rows);
  assert.deepEqual([...second.changedIds], ['a', 'd']); assert.deepEqual([...second.removedIds], ['b']);
  assert.equal(second.rows.find(row => row.id === 'c'), first.rows.find(row => row.id === 'c'));
  assert.equal(second.rows.find(row => row.id === 'a').campaign, 'Moved');
  assert.ok(w.requests.every(query => query.projection));
});
test('full hydration reads only explicitly selected IDs and retains all evidence and contacts', async () => {
  const w = world([lead('a'), lead('b'), lead('c')]);
  await loadLeadList(w.collection); w.requests.length = 0;
  const rows = await loadFullLeadRows(w.collection, ['b', 'b']);
  assert.deepEqual(rows.map(row => row.id), ['b']); assert.equal(rows[0].contacts[0].person_email, 'b@firma.test');
  assert.equal(rows[0].evidence[0].quote.length, 60000); assert.equal(rows[0].field_status.firma_name.status, 'verified');
  assert.deepEqual(w.requests.map(query => query.selector.id.$in), [['b']]);
  assert.ok(w.requests.every(query => !query.projection));
});
test('missing full rows reject the whole action input; partial results never become successful exports', async () => {
  const w = world([lead('a')]);
  await assert.rejects(loadFullLeadRows(w.collection, ['a', 'missing'], { batchSize: 1 }), { code: 'LEAD_DETAIL_MISSING' });
});
test('unsupported or ignored projection fails without a full-query fallback', async () => {
  const calls = [];
  await assert.rejects(loadLeadList({ find: query => ({ exec: async () => { calls.push(query); return [{ toJSON: () => lead('a') }]; } }) }), { code: 'LEAD_LIST_PROJECTION_NOT_APPLIED' });
  assert.equal(calls.length, 1); assert.deepEqual(calls[0].projection, [...LEAD_LIST_PROJECTION]);
  await assert.rejects(loadLeadList({ find: () => ({ exec: async () => [{ id: 'a', _rev: '1-a', payload: { imported_row: { large: 'unused' } } }] }) }), { code: 'LEAD_LIST_PROJECTION_NOT_APPLIED' });
  await assert.rejects(loadLeadList({ find: () => ({ exec: async () => [{ id: 'a', _rev: '1-a', payload: { sellify_precheck: { known: true, entire_company: 'unused' } } }] }) }), { code: 'LEAD_LIST_PROJECTION_NOT_APPLIED' });
});
test('invalid revisions, non-advancing pages and duplicate detail rows fail closed', async () => {
  await assert.rejects(loadLeadList({ find: () => ({ exec: async () => [{ id: 'a' }] }) }), { code: 'LEAD_LIST_REVISION_MISSING' });
  await assert.rejects(loadLeadList({ find: () => ({ exec: async () => [leadListRow(lead('a'))] }) }), { code: 'LEAD_LIST_PAGINATION' });
  await assert.rejects(loadFullLeadRows({ find: () => ({ exec: async () => [lead('a'), lead('a')] }) }, ['a']), { code: 'LEAD_DETAIL_INVALID' });
});
