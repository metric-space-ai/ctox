// OS-A3: per-collection checkpoint staleness in the sync diagnostics.
//
// Contract pinned here:
//   1. `checkpointDiagnosticFields` records `pullCheckpointLwt` /
//      `pushCheckpointLwt` from the replication state's per-peer checkpoint
//      maps (max across peers); collections without checkpoints record
//      nothing. The fields ride transport-status activity — there is NO
//      timer keeping them fresh.
//   2. `snapshotDiagnostics` derives `pullCheckpointAgeMs` /
//      `pushCheckpointAgeMs` AT SNAPSHOT TIME, so consumers read staleness
//      without the runtime doing any idle work.
//
// Pure logic drive through the exported test hooks — no network, no timers.

import { __ctoxSyncTestHooks } from '../../shared/sync.js';

const { checkpointDiagnosticFields, maxCheckpointLwt, snapshotDiagnostics } = __ctoxSyncTestHooks;

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

// --- 1. recording helper semantics -------------------------------------------
{
  const state = {
    pullCheckpointsByPeer: new Map([
      ['peer-a', { id: 'doc-3', lwt: 1_000 }],
      ['peer-b', { id: 'doc-9', lwt: 5_000 }],
    ]),
    pushCheckpointsByPeer: new Map([['peer-a', { id: 'doc-2', lwt: 4_000 }]]),
  };
  const fields = checkpointDiagnosticFields(state);
  assert(fields.pullCheckpointLwt === 5_000, 'pull lwt is the max across peers');
  assert(fields.pushCheckpointLwt === 4_000, 'push lwt recorded');

  assert(maxCheckpointLwt(null) === 0, 'missing map -> 0');
  assert(maxCheckpointLwt(new Map([['p', { lwt: 'bogus' }]])) === 0, 'non-numeric lwt ignored');
  const empty = checkpointDiagnosticFields({ pullCheckpointsByPeer: new Map() });
  assert(!('pullCheckpointLwt' in empty), 'no checkpoints -> no fields recorded');
  assert(!('pushCheckpointLwt' in empty), 'no push checkpoints -> no push field');
}

// --- 2. snapshot derives ages lazily ------------------------------------------
{
  const now = Date.now();
  const diagnostics = {
    mode: 'webrtc',
    phase: 'ready',
    collections: {
      business_notes: {
        collection: 'business_notes',
        status: 'connected',
        pullCheckpointLwt: now - 60_000,
        pushCheckpointLwt: now - 5_000,
      },
      business_commands: {
        collection: 'business_commands',
        status: 'connected',
      },
    },
  };

  const snapshot = snapshotDiagnostics(diagnostics);
  const notes = snapshot.collections.business_notes;
  assert(Number(notes.pullCheckpointAgeMs) >= 60_000, `pull age derived at snapshot time (got ${notes.pullCheckpointAgeMs})`);
  assert(Number(notes.pullCheckpointAgeMs) < 90_000, 'pull age is an age, not a timestamp');
  assert(Number(notes.pushCheckpointAgeMs) >= 5_000, 'push age derived at snapshot time');

  const commands = snapshot.collections.business_commands;
  assert(!('pullCheckpointAgeMs' in commands), 'no recorded checkpoint -> no age field');

  // The snapshot must not mutate the recorded entries (ages are derived per
  // snapshot, never persisted back into the live diagnostics object).
  assert(!('pullCheckpointAgeMs' in diagnostics.collections.business_notes),
    'age fields never leak into the live diagnostics state');
}

console.log('ctox-rxdb checkpoint-age diagnostics smoke OK');
process.exit(0);
