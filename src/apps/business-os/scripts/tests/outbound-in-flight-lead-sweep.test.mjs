import assert from 'node:assert/strict';
import { test } from 'node:test';
import { inFlightLeadsOutsideWindow } from '../../customer-modules/outbound-lead-generation/in-flight-lead-sweep.mjs';

// Like the Business OS store: selector + sort by id, never more than 200 rows.
function fakeFind(rows) {
  return async ({ selector, limit }) => {
    const statuses = selector.research_status.$in;
    const after = selector.id?.$gt || '';
    return rows
      .filter((row) => statuses.includes(row.research_status) && row.id > after)
      .sort((a, b) => (a.id < b.id ? -1 : 1))
      .slice(0, Math.min(limit, 200));
  };
}

test('finds every in-flight lead beyond the 200-row query cap', async () => {
  const rows = [];
  for (let i = 0; i < 494; i += 1) {
    const id = `lead_${String(i).padStart(4, '0')}`;
    rows.push({ id, research_status: i % 3 === 0 ? 'running' : i % 3 === 1 ? 'needs_review' : 'queued' });
  }
  const found = await inFlightLeadsOutsideWindow(fakeFind(rows));
  const expected = rows.filter((row) => ['queued', 'running'].includes(row.research_status)).length;
  assert.equal(found.length, expected);
  assert.ok(expected > 200);
  assert.equal(new Set(found.map((row) => row.id)).size, found.length);
});

test('skips leads the window already holds', async () => {
  const rows = [
    { id: 'lead_a', research_status: 'running' },
    { id: 'lead_b', research_status: 'running' },
    { id: 'lead_c', research_status: 'completed' },
  ];
  const found = await inFlightLeadsOutsideWindow(fakeFind(rows), new Set(['lead_a']));
  assert.deepEqual(found.map((row) => row.id), ['lead_b']);
});
