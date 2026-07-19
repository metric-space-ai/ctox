// REGRESSION (SYNC-42): a single un-mergeable structured conflict must be
// QUARANTINED, not freeze the whole collection's pull.
//
// Before: a field-merge pull that hit a value it could not safely merge
// (concurrent edits to arrays / ordered lists / structured values) threw
// `structured_conflict_requires_resolution` out of the batch apply. That throw
// aborted the whole pulled batch's IndexedDB transaction AND propagated out of
// bulkWrite, so the pull checkpoint never advanced — ONE bad document froze
// every other (fine) document in the collection until a human resolved it.
//
// After: the one conflicting document is journaled to the conflict store
// (base + local + master, everything db.conflicts.resolve() needs) and SKIPPED,
// while every other document in the batch commits in the same transaction and
// the checkpoint advances past them. The collection keeps syncing; only the
// quarantined doc waits for manual resolution.
//
// Contract pinned here (driven against REAL IndexedDB in headless Chrome, so
// the per-doc-skip-within-one-transaction semantics are exercised for real):
//   1. a batch of N docs where ONE is an unmergeable structured conflict: the
//      other N-1 apply and the conflicting doc is recorded in db.conflicts
//      with resolvable state (local + master + base present);
//   2. idempotency: replaying the SAME batch does not double-journal the
//      conflict and does not lose the good docs;
//   3. resolve(keep_master) applies the master state and clears the conflict;
//   4. the collection's pull is NOT frozen — a later pull of a new doc succeeds.

import http from 'node:http';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const playwrightModule = process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href
  : '../../node_modules/playwright/index.mjs';
const { chromium } = await import(playwrightModule);

const testDir = dirname(fileURLToPath(import.meta.url));
const bundle = readFileSync(resolve(testDir, '../dist/ctox-rxdb-js.mjs'));
const server = http.createServer((request, response) => {
  if (request.url === '/bundle.mjs') {
    response.writeHead(200, { 'content-type': 'text/javascript' });
    response.end(bundle);
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html' });
  response.end('<!doctype html><title>structured conflict quarantine smoke</title>');
});
await new Promise((resolveReady) => server.listen(0, '127.0.0.1', resolveReady));
const { port } = server.address();
const systemChrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const browser = await chromium.launch({
  headless: true,
  ...(existsSync(systemChrome) ? { executablePath: systemChrome } : {}),
});

try {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${port}/`);
  const summary = await page.evaluate(async () => {
    const assert = (condition, message) => {
      if (!condition) throw new Error(message);
    };
    const { openCtoxIndexedDbStorage } = await import('/bundle.mjs');
    const databaseName = `quarantine-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const storage = await openCtoxIndexedDbStorage({ databaseName });
    const schema = {
      version: 0,
      primaryKey: 'id',
      type: 'object',
      properties: { id: { type: 'string', maxLength: 64 } },
    };
    const records = storage.collection('records', { schema, conflictStrategy: 'field-merge' });
    const journal = storage.recoveryJournal;
    const origin = { role: 'ctox_instance', peerId: 'peer-native' };

    // --- seed A: an unsynced LOCAL edit over a master-confirmed base ----------
    // base (master row) -> local edit records that base -> A is a pushable
    // local write with a merge base to diverge from.
    await records.bulkWrite([{ id: 'A', name: 'A', tags: ['base'], updated_at_ms: 1000 }], {
      replicationOrigin: origin,
    });
    await records.bulkWrite([{ id: 'A', name: 'A', tags: ['local'], updated_at_ms: 2000 }]);
    const seededA = await records.findOne('A');
    assert(JSON.stringify(seededA.tags) === JSON.stringify(['local']), 'A holds the unsynced local edit');
    const seededRecord = await records.getStoredRecord('A');
    assert(JSON.stringify(seededRecord.base?.tags) === JSON.stringify(['base']), 'A carries the merge base');
    assert(seededRecord.pushable === 1, 'A is an unsynced (pushable) local write');

    // --- the pulled batch: A conflicts (concurrent array edit), B/C/D fine ----
    const batch = [
      { id: 'A', name: 'A', tags: ['master'], updated_at_ms: 3000 }, // unmergeable vs local
      { id: 'B', name: 'B-master', updated_at_ms: 3000 },
      { id: 'C', name: 'C-master', updated_at_ms: 3000 },
      { id: 'D', name: 'D-master', updated_at_ms: 3000 },
    ];

    // --- 1. quarantine: the batch applies, one doc is skipped + journaled -----
    const firstPull = await records.bulkWrite(batch, { replicationOrigin: origin });
    // bulkWrite RESOLVED (did not throw) -> the batch was not aborted.
    assert(firstPull.success.B && firstPull.success.C && firstPull.success.D,
      'the three mergeable master docs applied in the same batch');
    assert(!firstPull.success.A, 'the conflicting doc A was skipped, not applied');
    assert((await records.findOne('B')).name === 'B-master', 'B committed to the primary store');
    assert((await records.findOne('C')).name === 'C-master', 'C committed to the primary store');
    assert((await records.findOne('D')).name === 'D-master', 'D committed to the primary store');
    // A is left in its local state (neither silently overwritten nor lost).
    assert(JSON.stringify((await records.findOne('A')).tags) === JSON.stringify(['local']),
      'A is left in its unsynced local state, not overwritten by master');

    let conflicts = await journal.listConflicts();
    assert(conflicts.length === 1, `exactly one conflict journaled (got ${conflicts.length})`);
    const conflict = conflicts[0];
    assert(conflict.collection === 'records', 'conflict tagged with its collection');
    assert(conflict.conflictType === 'structured_field_conflict', 'conflict typed as a structured field conflict');
    assert(Array.isArray(conflict.fields) && conflict.fields.includes('tags'), 'the conflicting field is reported');
    assert(JSON.stringify(conflict.local?.tags) === JSON.stringify(['local']), 'local state captured for resolution');
    assert(JSON.stringify(conflict.master?.tags) === JSON.stringify(['master']), 'master state captured for resolution');
    assert(JSON.stringify(conflict.base?.tags) === JSON.stringify(['base']), 'merge base captured for resolution');

    // --- 2. idempotency: replaying the SAME batch does not double-journal -----
    const replay = await records.bulkWrite(batch, { replicationOrigin: origin });
    assert(replay.success.B && replay.success.C && replay.success.D, 'good docs survive an idempotent replay');
    conflicts = await journal.listConflicts();
    assert(conflicts.length === 1, `replay must not double-journal the conflict (got ${conflicts.length})`);
    assert(conflicts[0].conflictId === conflict.conflictId, 'the deterministic conflict id was reused');
    assert(JSON.stringify((await records.findOne('A')).tags) === JSON.stringify(['local']),
      'A is still the local edit after the idempotent replay');

    // --- 3. resolve(keep_master): applies master + clears the conflict --------
    const resolved = await journal.resolveConflict(conflict.conflictId, 'keep_master');
    assert(resolved === true, 'keep_master resolution reported success');
    const resolvedA = await records.findOne('A');
    assert(JSON.stringify(resolvedA.tags) === JSON.stringify(['master']), 'keep_master applied the master state');
    const resolvedRecord = await records.getStoredRecord('A');
    assert(resolvedRecord.pushable === 0, 'resolved A is origin-stamped (non-pushable)');
    assert(resolvedRecord.base === undefined, 'resolved A cleared its merge base');
    assert((await journal.listConflicts()).length === 0, 'the conflict cleared from db.conflicts.list()');

    // --- 4. the collection is NOT frozen: a later pull of a new doc lands -----
    const laterPull = await records.bulkWrite([{ id: 'E', name: 'E-master', updated_at_ms: 4000 }], {
      replicationOrigin: origin,
    });
    assert(laterPull.success.E, 'a subsequent pull applies new documents (collection not frozen)');
    assert((await records.findOne('E')).name === 'E-master', 'E committed after the quarantine');

    storage.close();
    return {
      appliedInFirstBatch: Object.keys(firstPull.success).sort().join(','),
      resolvedTags: resolvedA.tags.join(','),
      finalDocE: (await (async () => 'E-master')()),
    };
  });

  if (summary.appliedInFirstBatch !== 'B,C,D') {
    throw new Error(`expected B,C,D applied, got ${summary.appliedInFirstBatch}`);
  }
  if (summary.resolvedTags !== 'master') {
    throw new Error(`expected keep_master to apply master tags, got ${summary.resolvedTags}`);
  }
  console.log('ctox-rxdb structured conflict quarantine smoke OK');
} finally {
  await browser.close();
  server.close();
}

process.exit(0);
