import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readStoredLeadRows } from '../../customer-modules/outbound-lead-generation/lead-list-loader.mjs';

const collection = (docs) => ({
  storageCollection: {
    calls: [],
    async findDocumentsById(ids) {
      this.calls.push(ids);
      return Object.fromEntries(ids.filter((id) => docs[id]).map((id) => [id, docs[id]]));
    },
  },
});

test('stored rows are read from the local store without a CTOX query', async () => {
  const col = collection({
    a: { id: 'a', _rev: '3-x', name: 'A' },
    b: { id: 'b', _rev: '1-y', _deleted: true },
    c: { id: 'c' },
    d: { id: 'other' },
  });
  const rows = await readStoredLeadRows(col, ['a', 'b', 'c', 'd', 'missing', 'a']);
  assert.deepEqual([...rows.keys()], ['a', 'c']);
  assert.deepEqual(col.storageCollection.calls, [['a', 'b', 'c', 'd', 'missing']]);
});

test('a collection without a local store yields no rows', async () => {
  assert.equal((await readStoredLeadRows({}, ['a'])).size, 0);
  assert.equal((await readStoredLeadRows(null, ['a'])).size, 0);
});
