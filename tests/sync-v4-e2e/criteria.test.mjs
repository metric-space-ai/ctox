import test from 'node:test';
import assert from 'node:assert/strict';
import { completeReload, completeFirstWindow, checkpointDelivered } from './criteria.mjs';
test('fast but incomplete data and an absent positive control cannot become acceptance', () => {
  const good = { visibleMs: 100, rows: 2, preQueryLocalCount: 2, missing: [], duplicates: 0, emptyRender: false };
  assert.equal(completeReload(good), true);
  assert.equal(completeReload({ ...good, missing: ['native-row'] }), false);
  assert.equal(completeReload({ ...good, preQueryLocalCount: 1 }), false);
  assert.equal(completeReload({ ...good, visibleMs: NaN }), false);
  assert.equal(completeFirstWindow({ ...good, missing: ['native-row'] }), false);
  const row = { id: 'e2e-row', lwt: 42 };
  assert.equal(checkpointDelivered({found:true}, {found:false}, row, row), false);
  assert.equal(checkpointDelivered({found:true}, {found:true}, row, row), true);
});
