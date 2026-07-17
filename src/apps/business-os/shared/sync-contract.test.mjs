import assert from 'node:assert/strict';
import test from 'node:test';

import { batchSizeFor } from './sync-contract.js';

test('knowledge table replication pulls one byte-bounded document at a time', () => {
  assert.equal(batchSizeFor('knowledge_tables'), 1);
  assert.equal(batchSizeFor('desktop_file_chunks'), 6);
  assert.equal(batchSizeFor('research_runs'), 20);
});
