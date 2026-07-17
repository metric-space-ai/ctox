// SYNC-41 (opt-in per collection): `deleteStrategy: 'final'` — a tombstone
// ALWAYS wins over a concurrent non-tombstone update regardless of HLC/lwt, on
// EVERY path that decides a delete-vs-update, so once deleted a concurrent
// update can never resurrect the row. Declared as an INDEPENDENT sibling of
// `conflictStrategy` (a collection may combine field-merge updates with final
// deletes), OUTSIDE the schema object, so schema hashes are unaffected.
//
// Contract pinned here (pure in-memory — drives the REAL gate/wiring/journal
// helpers, no IndexedDB transactions, so it is fully deterministic):
//
//   1. Pull gate (`shouldAcceptDocumentWrite`):
//      - finalDelete: a master tombstone is accepted over a HIGHER-HLC local
//        update (no resurrection); a local (unsynced) tombstone REJECTS a
//        higher-HLC master update (local delete is authoritative);
//      - two tombstones or two updates fall through to normal LWW/HLC ordering;
//      - DEFAULT collections are UNCHANGED: a higher-HLC update wins over — and
//        resurrects — a tombstone in BOTH directions (today's whole-doc LWW).
//   2. Case (b) journaling (`finalDeleteRejectedUpdateConflict`): the master
//      update a local tombstone beat is journaled as `delete_vs_update` so it
//      stays recoverable, never silently dropped — and only for finalDelete
//      collections, never for default/native-authoritative ones.
//   3. Collection wiring: `deleteStrategy` normalizes ('final' | 'default') and
//      composes independently with `conflictStrategy`.

import {
  CtoxIndexedDbCollection,
  ctoxIndexedDbStorageTestInternals,
  formatHybridLogicalClock,
} from '../dist/ctox-rxdb-js.mjs';

const { shouldAcceptDocumentWrite, finalDeleteRejectedUpdateConflict, lwwOverwriteConflict } =
  ctoxIndexedDbStorageTestInternals;

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const origin = { role: 'ctox_instance', peerId: 'peer-native' };
const hlc = (physicalMs, nodeId) => formatHybridLogicalClock({ physicalMs, logical: 0, nodeId });

// An UNSYNCED local row carries no ctoxReplicationOrigin marker.
const localUpdate = (lwt, hlcValue) => ({
  lwt,
  replicationOriginRole: '',
  doc: { id: 'd', purpose: 'edit', updated_at_ms: lwt, _meta: { lwt, ctoxHlc: hlcValue } },
});
const localTombstone = (lwt, hlcValue) => ({
  lwt,
  replicationOriginRole: '',
  doc: { id: 'd', _deleted: true, _meta: { lwt, ctoxHlc: hlcValue } },
});
// Master rows arrive WITHOUT `_meta.lwt` (keep_meta=false), carrying the HLC.
const masterUpdate = (hlcValue) => ({ id: 'd', purpose: 'remote-edit', _meta: { ctoxHlc: hlcValue } });
const masterTombstone = (hlcValue) => ({ id: 'd', _deleted: true, _meta: { ctoxHlc: hlcValue } });

// The delete racer always carries the LOWER HLC on purpose: whole-doc LWW would
// let the higher-HLC update win; finalDelete must override that and let the
// delete win regardless.
const HIGH = hlc(9000, 'tab-a');
const LOW = hlc(1000, 'native');

// ---------------------------------------------------------------------------
// 1a. Pull gate — master TOMBSTONE over a higher-HLC local UPDATE.
// ---------------------------------------------------------------------------
{
  // DEFAULT (lww): the local update's higher HLC vetoes the tombstone —
  // the doc is resurrected. This is today's behavior and must NOT change.
  assert(
    shouldAcceptDocumentWrite(
      localUpdate(2000, HIGH), 1000, origin, masterTombstone(LOW), 'records', 'lww', 'default',
    ) === false,
    'default: a higher-HLC local update vetoes a master tombstone (today\'s LWW, resurrection possible)',
  );
  // finalDelete: the tombstone wins regardless of HLC — no resurrection.
  assert(
    shouldAcceptDocumentWrite(
      localUpdate(2000, HIGH), 1000, origin, masterTombstone(LOW), 'records', 'lww', 'final',
    ) === true,
    'finalDelete: a master tombstone is accepted over a higher-HLC local update',
  );
  // finalDelete composes with field-merge updates.
  assert(
    shouldAcceptDocumentWrite(
      localUpdate(2000, HIGH), 1000, origin, masterTombstone(LOW), 'records', 'field-merge', 'final',
    ) === true,
    'finalDelete + field-merge: a master tombstone still wins whole-doc',
  );
}

// ---------------------------------------------------------------------------
// 1b. Pull gate — master UPDATE over an existing local TOMBSTONE.
// ---------------------------------------------------------------------------
{
  // DEFAULT (lww): the master update's higher HLC wins — resurrection. Today's
  // behavior, unchanged.
  assert(
    shouldAcceptDocumentWrite(
      localTombstone(2000, LOW), 3000, origin, masterUpdate(HIGH), 'records', 'lww', 'default',
    ) === true,
    'default: a higher-HLC master update wins over a local tombstone (today\'s LWW, resurrection)',
  );
  // finalDelete: the local delete is authoritative — the master update is
  // rejected regardless of HLC.
  assert(
    shouldAcceptDocumentWrite(
      localTombstone(2000, LOW), 3000, origin, masterUpdate(HIGH), 'records', 'lww', 'final',
    ) === false,
    'finalDelete: a local tombstone rejects a higher-HLC master update',
  );
  assert(
    shouldAcceptDocumentWrite(
      localTombstone(2000, LOW), 3000, origin, masterUpdate(HIGH), 'records', 'field-merge', 'final',
    ) === false,
    'finalDelete + field-merge: a local tombstone still rejects a master update',
  );
}

// ---------------------------------------------------------------------------
// 1c. finalDelete falls through to normal LWW when NOT a delete-vs-update race.
// ---------------------------------------------------------------------------
{
  // Two tombstones → normal HLC ordering (master higher HLC → accept).
  assert(
    shouldAcceptDocumentWrite(
      localTombstone(2000, LOW), 1000, origin, masterTombstone(HIGH), 'records', 'lww', 'final',
    ) === true,
    'finalDelete: two tombstones fall through to normal LWW (higher-HLC master accepted)',
  );
  // Two updates → normal HLC ordering (local higher HLC → local wins/veto).
  assert(
    shouldAcceptDocumentWrite(
      localUpdate(2000, HIGH), 1000, origin, masterUpdate(LOW), 'records', 'lww', 'final',
    ) === false,
    'finalDelete: two updates fall through to normal LWW (higher-HLC local vetoes master)',
  );
  assert(
    shouldAcceptDocumentWrite(
      localUpdate(2000, LOW), 1000, origin, masterUpdate(HIGH), 'records', 'lww', 'final',
    ) === true,
    'finalDelete: two updates, higher-HLC master wins (normal LWW)',
  );
}

// ---------------------------------------------------------------------------
// 2. Case (b) journaling — a local tombstone beat a master update.
// ---------------------------------------------------------------------------
{
  const previous = localTombstone(2000, LOW);
  const incoming = masterUpdate(HIGH);
  const conflict = finalDeleteRejectedUpdateConflict({
    previous, incomingDocument: incoming, collectionName: 'records', deleteStrategy: 'final', replicationOrigin: origin,
  });
  assert(conflict && conflict.conflictType === 'delete_vs_update',
    'finalDelete case (b): the losing master update is journaled as delete_vs_update');
  assert(conflict.master === incoming, 'the recoverable losing side is the master update');
  assert(conflict.local === previous.doc, 'the winning side recorded is the local tombstone');

  // No journaling for default collections, native-authoritative collections,
  // incoming tombstones, non-tombstone local rows, or synced previous rows.
  assert(
    finalDeleteRejectedUpdateConflict({
      previous, incomingDocument: incoming, collectionName: 'records', deleteStrategy: 'default', replicationOrigin: origin,
    }) === null,
    'default deleteStrategy never mints a finalDelete rejection conflict',
  );
  assert(
    finalDeleteRejectedUpdateConflict({
      previous, incomingDocument: incoming, collectionName: 'business_commands', deleteStrategy: 'final', replicationOrigin: origin,
    }) === null,
    'native-authoritative collections are exempt',
  );
  assert(
    finalDeleteRejectedUpdateConflict({
      previous, incomingDocument: masterTombstone(HIGH), collectionName: 'records', deleteStrategy: 'final', replicationOrigin: origin,
    }) === null,
    'an incoming tombstone is not a rejected update',
  );
  assert(
    finalDeleteRejectedUpdateConflict({
      previous: localUpdate(2000, LOW), incomingDocument: incoming, collectionName: 'records', deleteStrategy: 'final', replicationOrigin: origin,
    }) === null,
    'a non-tombstone local row is not a finalDelete delete-vs-update rejection',
  );
  assert(
    finalDeleteRejectedUpdateConflict({
      previous: { ...previous, replicationOriginRole: 'ctox_instance' },
      incomingDocument: incoming, collectionName: 'records', deleteStrategy: 'final', replicationOrigin: origin,
    }) === null,
    'an already-synced previous row is not a local-tombstone-wins case',
  );
}

// ---------------------------------------------------------------------------
// 3. Collection wiring: deleteStrategy normalization + independent composition.
// ---------------------------------------------------------------------------
{
  const schema = { version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string', maxLength: 64 } } };
  const finalOnly = new CtoxIndexedDbCollection(null, 'records', { schema, deleteStrategy: 'final' });
  const both = new CtoxIndexedDbCollection(null, 'records', { schema, conflictStrategy: 'field-merge', deleteStrategy: 'final' });
  const plain = new CtoxIndexedDbCollection(null, 'records', { schema });

  assert(finalOnly.deleteStrategy === 'final' && finalOnly.conflictStrategy === 'lww',
    'deleteStrategy composes independently of conflictStrategy (final + lww)');
  assert(both.deleteStrategy === 'final' && both.conflictStrategy === 'field-merge',
    'a collection can combine field-merge updates with final deletes');
  assert(plain.deleteStrategy === 'default' && plain.conflictStrategy === 'lww',
    'default (unset) collections keep whole-doc LWW deletes');
  assert(
    new CtoxIndexedDbCollection(null, 'records', { schema, deleteStrategy: 'weird' }).deleteStrategy === 'default',
    'an unknown deleteStrategy normalizes to default',
  );

  // Sanity: the whole-doc overwrite journaling helper still ignores tombstone
  // races (those go to the delete_vs_update paths), so no double-journal.
  assert(
    lwwOverwriteConflict({
      previous: localTombstone(2000, LOW), incomingDocument: masterUpdate(HIGH),
      collectionName: 'records', conflictStrategy: 'lww', replicationOrigin: origin,
    }) === null,
    'lwwOverwriteConflict never fires when the previous row is a tombstone',
  );
}

console.log('ctox-rxdb final-delete conflict smoke OK');
process.exit(0);
