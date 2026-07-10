// Field-merge conflict strategy (opt-in per collection) smoke.
//
// Contract pinned here:
//   1. threeWayMergeDocuments: concurrent edits to DIFFERENT fields both
//      survive; a true same-field conflict keeps the LOCAL value (it pushes);
//      a master tombstone wins whole-doc; an unsynced local tombstone
//      survives; a local field DELETION survives.
//   2. resolveIncomingWrite (storage layer):
//      - a local write over a master-origin row records that row as the
//        merge base, and consecutive local writes keep the ORIGINAL base;
//      - a replication write over an unsynced local row on a field-merge
//        collection stores the MERGED doc as a LOCAL (pushable) write with
//        the incoming master doc as the new base;
//      - once no local-only change survives (own push round-tripped), the
//        master row is stored normally (origin-stamped, base cleared);
//      - 'lww' collections (default) pass through COMPLETELY unchanged —
//        the §8.1 LWW/origin invariants stay intact.
//
// Pure in-memory: drives the real class methods, no IndexedDB transactions.

import {
  CtoxIndexedDbCollection,
  threeWayMergeDocuments,
} from '../dist/ctox-rxdb-js.mjs';

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const schema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: { id: { type: 'string', maxLength: 64 } },
};
const origin = { role: 'ctox_instance', peerId: 'peer-native' };

// ---------------------------------------------------------------------------
// 1. Merge semantics.
// ---------------------------------------------------------------------------
{
  const base = { id: 'r1', name: 'Acme', phone: '111', notes: 'old', tags: ['a'] };
  const local = { id: 'r1', name: 'Acme GmbH', phone: '111', notes: 'old', tags: ['a'] };
  const master = { id: 'r1', name: 'Acme', phone: '222', notes: 'newer', tags: ['a', 'b'] };
  const { merged, identicalToMaster } = threeWayMergeDocuments(base, local, master, { primaryPath: 'id' });
  assert(merged.name === 'Acme GmbH', 'local-only field change survives');
  assert(merged.phone === '222', 'master-only field change survives');
  assert(merged.notes === 'newer', 'second master-only field change survives');
  assert(JSON.stringify(merged.tags) === JSON.stringify(['a', 'b']), 'master array change survives');
  assert(identicalToMaster === false, 'surviving local change flags the doc as NOT master-identical');

  // True same-field conflict: local wins (it will push).
  const conflicted = threeWayMergeDocuments(
    { id: 'r1', name: 'Acme' },
    { id: 'r1', name: 'Local Name' },
    { id: 'r1', name: 'Master Name' },
    { primaryPath: 'id' },
  );
  assert(conflicted.merged.name === 'Local Name', 'same-field conflict keeps the local value');

  const structuredConflict = threeWayMergeDocuments(
    { id: 'r1', tags: ['base'] },
    { id: 'r1', tags: ['local'] },
    { id: 'r1', tags: ['master'] },
    { primaryPath: 'id' },
  );
  assert(structuredConflict.requiresManualResolution === true,
    'concurrent list edits require explicit/manual resolution');
  assert(structuredConflict.conflictFields.includes('tags'),
    'structured conflict reports the exact field');

  // Local field deletion survives; untouched fields follow the master.
  const deletion = threeWayMergeDocuments(
    { id: 'r1', name: 'Acme', fax: '333' },
    { id: 'r1', name: 'Acme' },
    { id: 'r1', name: 'Acme', fax: '333', phone: '222' },
    { primaryPath: 'id' },
  );
  assert(!('fax' in deletion.merged), 'local field deletion survives the merge');
  assert(deletion.merged.phone === '222', 'master-added field survives alongside');

  // Tombstones stay whole-doc.
  const masterDelete = threeWayMergeDocuments(
    { id: 'r1' },
    { id: 'r1', name: 'Local' },
    { id: 'r1', _deleted: true },
    { primaryPath: 'id' },
  );
  assert(masterDelete.identicalToMaster === true && masterDelete.merged._deleted === true,
    'master tombstone wins whole-doc');
  const localDelete = threeWayMergeDocuments(
    { id: 'r1' },
    { id: 'r1', _deleted: true },
    { id: 'r1', name: 'Master' },
    { primaryPath: 'id' },
  );
  assert(localDelete.identicalToMaster === false && localDelete.merged._deleted === true,
    'unsynced local tombstone survives until it pushes');

  // Round-tripped state: local equals master → master-identical.
  const settled = threeWayMergeDocuments(
    { id: 'r1', name: 'Old' },
    { id: 'r1', name: 'Same' },
    { id: 'r1', name: 'Same' },
    { primaryPath: 'id' },
  );
  assert(settled.identicalToMaster === true, 'identical local+master state settles to master');
}

// ---------------------------------------------------------------------------
// 2. Storage integration (resolveIncomingWrite).
// ---------------------------------------------------------------------------
{
  const mergeCollection = new CtoxIndexedDbCollection(null, 'records', {
    schema,
    conflictStrategy: 'field-merge',
  });
  const lwwCollection = new CtoxIndexedDbCollection(null, 'records', { schema });
  assert(mergeCollection.conflictStrategy === 'field-merge', 'strategy accepted');
  assert(lwwCollection.conflictStrategy === 'lww', 'default stays lww');

  const masterRow = {
    lwt: 1000,
    replicationOriginRole: 'ctox_instance',
    doc: { id: 'r1', name: 'Acme', phone: '111', _meta: { lwt: 1000, ctoxReplicationOrigin: { role: 'ctox_instance' } } },
  };

  // Local write over a master-origin row: the master doc becomes the base.
  const localEdit = { id: 'r1', name: 'Acme GmbH', phone: '111' };
  const localResolved = mergeCollection.resolveIncomingWrite({
    previous: masterRow,
    doc: localEdit,
    lwt: 2000,
    replicationOrigin: null,
  });
  assert(localResolved.base === masterRow.doc, 'local write records the master doc as merge base');
  assert(localResolved.doc === localEdit && !localResolved.replicationOrigin, 'local write otherwise untouched');

  // Consecutive local write keeps the ORIGINAL base.
  const localRow = {
    lwt: 2000,
    replicationOriginRole: '',
    base: masterRow.doc,
    doc: { id: 'r1', name: 'Acme GmbH', phone: '111', _meta: { lwt: 2000 } },
  };
  const secondLocal = mergeCollection.resolveIncomingWrite({
    previous: localRow,
    doc: { id: 'r1', name: 'Acme GmbH & Co', phone: '111' },
    lwt: 2100,
    replicationOrigin: null,
  });
  assert(secondLocal.base === masterRow.doc, 'consecutive local writes keep the original base');

  // Replication write over the unsynced local row: field merge, stored LOCAL.
  const incomingMaster = { id: 'r1', name: 'Acme', phone: '222' };
  const mergedResolved = mergeCollection.resolveIncomingWrite({
    previous: localRow,
    doc: incomingMaster,
    lwt: 1500,
    replicationOrigin: origin,
  });
  assert(mergedResolved.replicationOrigin === null, 'merged doc is stored as a LOCAL (pushable) write');
  assert(mergedResolved.doc.name === 'Acme GmbH', 'local field change survived the pull');
  assert(mergedResolved.doc.phone === '222', 'master field change survived the pull');
  assert(mergedResolved.base === incomingMaster, 'incoming master doc becomes the new base');
  assert(mergedResolved.lwt > localRow.lwt, 'merged lwt stays monotonic');

  const structuredLocalRow = {
    ...localRow,
    base: { id: 'r1', tags: ['base'] },
    doc: { id: 'r1', tags: ['local'], _meta: { lwt: 2000 } },
  };
  let structuredError = null;
  try {
    mergeCollection.resolveIncomingWrite({
      previous: structuredLocalRow,
      doc: { id: 'r1', tags: ['master'] },
      lwt: 2200,
      replicationOrigin: origin,
    });
  } catch (error) {
    structuredError = error;
  }
  assert(structuredError?.code === 'structured_conflict_requires_resolution',
    'storage preserves a structured conflict as a stable typed failure');

  // Once the local change round-tripped, the master row lands normally.
  const roundTripped = mergeCollection.resolveIncomingWrite({
    previous: localRow,
    doc: { id: 'r1', name: 'Acme GmbH', phone: '111' },
    lwt: 2500,
    replicationOrigin: origin,
  });
  assert(roundTripped.replicationOrigin === origin, 'master-identical state is stored origin-stamped');
  assert(roundTripped.base === undefined, 'and clears the merge base');

  // LWW collections: complete pass-through in every direction.
  const lwwLocal = lwwCollection.resolveIncomingWrite({
    previous: masterRow, doc: localEdit, lwt: 2000, replicationOrigin: null,
  });
  assert(lwwLocal.base === undefined && lwwLocal.doc === localEdit, 'lww local write untouched (no base)');
  const lwwPull = lwwCollection.resolveIncomingWrite({
    previous: localRow, doc: incomingMaster, lwt: 1500, replicationOrigin: origin,
  });
  assert(lwwPull.doc === incomingMaster && lwwPull.replicationOrigin === origin,
    'lww replication write untouched (whole-doc semantics preserved)');

  // OS-C4: the push-conflict repair passes the absorbed master row as the
  // EXPLICIT new base — the stale stored base must not survive, or absorbed
  // master fields would be re-won as "local changes" on the next round.
  const absorbedMaster = { id: 'r1', name: 'Acme GmbH', phone: '222' };
  const pushRepair = mergeCollection.resolveIncomingWrite({
    previous: localRow,
    doc: absorbedMaster,
    lwt: 2600,
    replicationOrigin: null,
    explicitBase: incomingMaster,
  });
  assert(pushRepair.base === incomingMaster, 'explicit base overrides the stale stored base');
  const lwwExplicit = lwwCollection.resolveIncomingWrite({
    previous: localRow, doc: absorbedMaster, lwt: 2600, replicationOrigin: null,
    explicitBase: incomingMaster,
  });
  assert(lwwExplicit.base === undefined, 'lww collections never store a base, explicit or not');

  // OS-C4: merge observability — the pull merge above incremented the
  // per-collection counter exactly once.
  assert(mergeCollection.mergeStats.pullFieldMerges === 1,
    `pull merge counter records field merges (got ${mergeCollection.mergeStats.pullFieldMerges})`);
  assert(mergeCollection.mergeStats.pushConflictMerges === 0, 'push counter untouched by pulls');
  assert(lwwCollection.mergeStats.pullFieldMerges === 0, 'lww collections never merge');
}

console.log('ctox-rxdb field-merge conflict smoke OK');
process.exit(0);
