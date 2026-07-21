import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import { batchSizeFor } from './sync-contract.js';

test('knowledge table replication pulls one byte-bounded document at a time', () => {
  assert.equal(batchSizeFor('knowledge_tables'), 1);
  assert.equal(batchSizeFor('desktop_file_chunks'), 6);
  assert.equal(batchSizeFor('research_runs'), 20);
});

test('sync runtime version-binds its nested sync contract import', async () => {
  const source = await readFile(new URL('./sync.js', import.meta.url), 'utf8');
  assert.match(
    source,
    /from '\.\/sync-contract\.js\?v=[^']+'/,
    'mutable nested sync contract imports must not use an unversioned CDN URL',
  );
});

test('slow collection startup remains pending instead of dereferencing an empty timeout result', async () => {
  const source = await readFile(new URL('./sync.js', import.meta.url), 'utf8');
  assert.match(source, /if \(!bridge\) \{/);
  assert.match(source, /reason: 'startup-in-progress'/);
  assert.match(source, /return pendingBridge;/);
});
