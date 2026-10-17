// REGRESSION: a local lead whose copy of native-owned fields went stale was
// refused forever, together with the user's ordinary change on it.
//
// The native masterWrite guard (outbound_field_review.rs
// `peer_lead_write_allowed`) refuses the WHOLE batch when one row's stamped
// field statuses (non-null `revision`/`review`) differ from the stored master.
// The browser pushed whole documents, so a pure campaign move on a lead whose
// local field_status predates a later native writeback was rejected with
// "peer cannot change native-owned document fields", rolled back and kept only
// in the recovery journal — on every retry, for good.
//
// This smoke drives the real push code (`pushToPeer`) against a fake native
// peer that implements the guard, the optimistic-lock conflict and LWW writes:
//   1. stale native-owned field + changed peer-writable field -> accepted, the
//      change lands, the native-owned value equals the master;
//   2. a stale row no longer takes its slice neighbour down with it;
//   3. a row refused for another reason keeps reconcile-and-journal;
//   4. journaled refusals from before the fix are re-applied on the master and
//      accepted without user action; an update_vs_update entry stays pending;
//   5. when the master wins whole-document LWW, the master row is written with
//      a replication origin, which is the write storage-indexeddb journals as
//      update_vs_update (lwwOverwriteConflict, replication-lww-origin-smoke).
//
// Set CTOX_NATIVE_OWNED_SMOKE_EXPECT_RED=1 together with a disabled rebase to
// see the mutation fail with the native authz rejection.

import { replicateWebRTC } from '../src/replication-webrtc.mjs';
import { formatHybridLogicalClock } from '../src/hybrid-logical-clock.mjs';

const COLLECTION = 'outbound_lead_generation_leads';
const failures = [];
const check = (condition, message) => {
  if (!condition) failures.push(message);
};
const clone = (value) => JSON.parse(JSON.stringify(value));
const hlc = (physicalMs, nodeId) => formatHybridLogicalClock({ physicalMs, logical: 0, nodeId });
const NOW = Date.now();

const NATIVE_OWNED_REFUSAL = {
  type: 'ctoxError', scope: 'replication', rxdb: true,
  code: 'RC_PUSH', phase: 'replication-push', direction: 'push', rowCount: 1,
  errors: [{
    rxdb: true,
    code: 'RC_WEBRTC_PEER',
    message: '\n\n        RxDB Error-Code: RC_WEBRTC_PEER.\n',
    parameters: { collection: COLLECTION, message: 'peer cannot change native-owned document fields' },
  }],
};

// ---- fake native peer -----------------------------------------------------
// ref: outbound_field_review.rs field_status_snapshot / peer_lead_write_allowed
const stamped = (status) => ['revision', 'review'].some((key) => status?.[key] !== undefined && status?.[key] !== null);
function snapshot(lead) {
  const out = {};
  for (const [field, status] of Object.entries(lead?.field_status || {})) {
    if (stamped(status)) out[`lead||${field}`] = status;
  }
  for (const [person, statuses] of Object.entries(lead?.person_field_status || {})) {
    for (const [field, status] of Object.entries(statuses || {})) {
      if (stamped(status)) out[`person|${person}|${field}`] = status;
    }
  }
  for (const contact of lead?.contacts || []) {
    for (const [field, status] of Object.entries(contact?.field_status || {})) {
      if (stamped(status)) out[`contact|${contact.person_key}|${field}`] = status;
    }
  }
  return JSON.stringify(Object.keys(out).sort().map((key) => [key, out[key]]));
}
function nativeAllows(incoming, master) {
  if (incoming.refuse_for_other_reason) return false;
  if (!master) return snapshot(incoming) === '[]';
  if (snapshot(incoming) !== snapshot(master)) return false;
  const researched = ['needs_review', 'completed'].includes(master.research_status);
  return !(researched && incoming.research_status === 'failed');
}
const stripMeta = (doc) => {
  const copy = clone(doc);
  delete copy._meta;
  return copy;
};

function createNativePeer(masterRows) {
  const masters = new Map(masterRows.map((doc) => [doc.id, clone(doc)]));
  const log = { writes: 0, refusals: 0, fetches: 0, accepted: [] };
  const peer = {
    async request(_peerId, method, params) {
      if (method !== 'masterWrite') throw new Error(`unexpected native method ${method}`);
      log.writes += 1;
      const rows = params[0];
      for (const row of rows) {
        if (!nativeAllows(row.newDocumentState, masters.get(row.newDocumentState.id) || null)) {
          // index_mod.rs master_write: the validator fails the whole batch.
          log.refusals += 1;
          return clone(NATIVE_OWNED_REFUSAL);
        }
      }
      const conflicts = [];
      for (const row of rows) {
        const id = row.newDocumentState.id;
        const master = masters.get(id) || null;
        if (!master || (row.assumedMasterState && JSON.stringify(stripMeta(row.assumedMasterState)) === JSON.stringify(stripMeta(master)))) {
          masters.set(id, clone(row.newDocumentState));
          log.accepted.push(id);
        } else {
          conflicts.push(clone(master));
        }
      }
      return conflicts;
    },
  };
  const demandTransport = {
    async requestQueryFetch(envelope) {
      log.fetches += 1;
      const ids = envelope?.query?.selector?.id?.$in || [];
      return { documents: ids.filter((id) => masters.has(id)).map((id) => clone(masters.get(id))) };
    },
  };
  return { masters, log, peer, demandTransport };
}

// ---- real replication state over a scripted storage ---------------------------
async function createState({ pending, journalEntries = [] }) {
  const collection = {
    name: COLLECTION,
    schema: { version: 0, primaryPath: 'id', hash: async () => 'hash-leads' },
    observe() { return { unsubscribe() {} }; },
    storageCollection: {
      conflictStrategy: 'lww',
      replicationCheckpointStatus: async () => ({ epoch: 'e1', state: 'ready' }),
      getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
      bulkWrite: async () => ({}),
    },
  };
  const state = await replicateWebRTC({
    collection,
    topic: 'room-native-owned-rebase-abcdef',
    connectionHandlerCreator: {
      kind: 'ctox-native-webrtc',
      signalingServerUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
      config: {},
    },
    pull: { batchSize: 100 },
    push: { batchSize: 100 },
    retryTime: 60,
  });
  state.initialReplication?.catch?.(() => {});
  const storage = state.collection.storageCollection;
  const local = { pending: pending.map(clone), localWrites: [], originWrites: [], reconciled: [] };
  const conflicts = journalEntries.map(clone);
  storage.getChangedDocumentsSince = async (checkpoint) => {
    const docs = local.pending.filter((_, index) => !checkpoint || index >= checkpoint.index);
    return { documents: docs.map(clone), checkpoint: { index: local.pending.length }, scanned: docs.length, scanLimitReached: false };
  };
  storage.getStoredRecord = async (id) => {
    const doc = local.pending.find((row) => row.id === id);
    return doc ? { doc: clone(doc), base: null, replicationOriginRole: '' } : null;
  };
  storage.bulkWrite = async (rows, options = {}) => {
    const docs = rows.map((row) => clone(row?.document || row));
    if (options?.replicationOrigin) {
      local.originWrites.push(...docs);
    } else {
      // Like storage-indexeddb prepareJournalRows: a local write gets a fresh HLC.
      for (const doc of docs) doc._meta = { ...(doc._meta || {}), ctoxHlc: hlc(NOW + 1_000, 'tab-a') };
      local.localWrites.push(...docs);
      local.pending.push(...docs);
    }
    return { success: {} };
  };
  storage.reconcileRejectedLocalWrites = async (documents, options) => {
    local.reconciled.push({ ids: documents.map((doc) => doc.id), message: options?.message || '' });
    return documents.map((doc) => doc.id);
  };
  storage.recoveryJournal = {
    async listConflicts() { return conflicts.filter((entry) => entry.state === 'pending').map(clone); },
    async markConflictReplayed(conflictId) {
      const entry = conflicts.find((row) => row.conflictId === conflictId);
      if (!entry || entry.state !== 'pending') return false;
      entry.state = 'resolved';
      entry.resolution = 'replayed_on_master';
      return true;
    },
    async recordConflict() {},
  };
  state.remoteProtocolForPeer = () => ({ peerSession: { sessionId: 'native-1', role: 'ctox_instance' } });
  state.pullFromRemotePeers = async () => {};
  state.invalidateDemandCacheForRemoteWrite = async () => {};
  return { state, local, conflicts };
}

const OLD_RECEIPT = { revision: 'rev-1', value_hash: 'h1' };
const NEW_RECEIPT = { revision: 'rev-2', value_hash: 'h2' };
const REVIEW = { review: { by: 'native', at: 1 } };

function masterLead(id, overrides = {}) {
  return {
    id,
    name: `Lead ${id}`,
    campaign: 'campaign-a',
    research_status: 'needs_review',
    updated_at_ms: NOW - 10_000,
    field_status: { website: NEW_RECEIPT, phone: { note: 'unstamped' } },
    person_field_status: { person_1: { email: REVIEW } },
    contacts: [{ person_key: 'person_1', name: 'Contact', field_status: { email: NEW_RECEIPT } }],
    _meta: { ctoxHlc: hlc(NOW - 10_000, 'native') },
    ...overrides,
  };
}
// The local copy predates the native writeback: old receipts, no review.
function staleLocalMove(id, overrides = {}) {
  return {
    ...masterLead(id),
    campaign: 'campaign-b',
    updated_at_ms: NOW,
    field_status: { website: OLD_RECEIPT, phone: { note: 'edited locally' } },
    person_field_status: {},
    contacts: [{ person_key: 'person_1', name: 'Contact renamed', field_status: { email: OLD_RECEIPT } }],
    _meta: { ctoxHlc: hlc(NOW, 'tab-a') },
    ...overrides,
  };
}

// ---- 1 + 2 + 3: live push ---------------------------------------------------
{
  const native = createNativePeer([masterLead('lead_a'), masterLead('lead_b'), masterLead('lead_c')]);
  const { state, local } = await createState({
    pending: [
      staleLocalMove('lead_a'),
      { ...masterLead('lead_b'), campaign: 'campaign-b', updated_at_ms: NOW, _meta: { ctoxHlc: hlc(NOW, 'tab-a') } },
      { ...masterLead('lead_c'), campaign: 'campaign-b', refuse_for_other_reason: true, _meta: { ctoxHlc: hlc(NOW, 'tab-a') } },
    ],
  });
  state.shared.peer = native.peer;
  state.shared.demandTransport = native.demandTransport;
  let pushError = null;
  try {
    await state.pushToPeer('p1');
  } catch (error) {
    pushError = error;
  }
  check(!pushError, `push must not throw (${pushError?.message})`);
  const a = native.masters.get('lead_a');
  check(a.campaign === 'campaign-b', `1: the peer-writable campaign move lands (master has ${a.campaign})`);
  check(a.contacts?.[0]?.name === 'Contact renamed', '1: other peer-writable edits on the stale row land');
  check(a.field_status?.phone?.note === 'edited locally', '1: unstamped field statuses stay peer-writable');
  check(JSON.stringify(a.field_status?.website) === JSON.stringify(NEW_RECEIPT), '1: the native-owned receipt equals the master');
  check(JSON.stringify(a.person_field_status?.person_1?.email) === JSON.stringify(REVIEW), '1: the native-owned review equals the master');
  check(JSON.stringify(a.contacts?.[0]?.field_status?.email) === JSON.stringify(NEW_RECEIPT), '1: the contact receipt equals the master');
  check(native.masters.get('lead_b').campaign === 'campaign-b', '2: the innocent slice neighbour lands');
  check(native.masters.get('lead_c').campaign === 'campaign-a', '3: a row refused for another reason is not written');
  const reconciledIds = local.reconciled.flatMap((entry) => entry.ids);
  check(reconciledIds.length === 1 && reconciledIds[0] === 'lead_c', `3: only the otherwise-refused row is reconciled (got ${JSON.stringify(reconciledIds)})`);
  check(!reconciledIds.includes('lead_a'), '1: the stale row is no longer rolled back into the journal');
  check(native.log.fetches >= 1, 'the master state was fetched after the refusal');
  check(state.pushCheckpointsByPeer.get('p1')?.index === 3, 'the push checkpoint advances');
  await state.cancel();
}

// ---- 4: journaled refusals from before the fix ------------------------------
{
  const ids = Array.from({ length: 15 }, (_, index) => `lead_j${index + 1}`);
  const native = createNativePeer(ids.map((id) => masterLead(id)));
  const journalEntries = ids.slice(0, 14).map((id, index) => ({
    conflictId: `c-${id}`,
    state: 'pending',
    createdAtMs: NOW - 5_000 + index,
    code: 'structured_conflict_requires_resolution',
    conflictType: 'authz_rejected',
    collection: COLLECTION,
    base: null,
    master: null,
    local: staleLocalMove(id),
    message: 'peer cannot change native-owned document fields',
    rejectionCode: 'RC_PUSH',
  }));
  // The 15th lost move: the master won whole-document LWW. That is a genuine
  // concurrent update and stays pending for the user.
  journalEntries.push({
    conflictId: 'c-lww',
    state: 'pending',
    createdAtMs: NOW - 4_000,
    code: 'structured_conflict_requires_resolution',
    conflictType: 'update_vs_update',
    collection: COLLECTION,
    base: null,
    local: staleLocalMove(ids[14]),
    master: masterLead(ids[14]),
    message: 'The master row won the whole-document LWW gate; the concurrent local update remains recoverable here.',
  });
  // An older refusal far behind the master trips the native stale-regression
  // guard and must not be replayed.
  journalEntries.push({
    conflictId: 'c-stale',
    state: 'pending',
    createdAtMs: NOW - 9_000,
    conflictType: 'authz_rejected',
    collection: COLLECTION,
    base: null,
    master: null,
    local: staleLocalMove(ids[0], { campaign: 'campaign-old', updated_at_ms: NOW - 3 * 60 * 60 * 1000 }),
    message: 'peer cannot change native-owned document fields',
  });
  const { state, local, conflicts } = await createState({ pending: [], journalEntries });
  state.shared.peer = native.peer;
  state.shared.demandTransport = native.demandTransport;
  let pushError = null;
  try {
    await state.pushToPeer('p1');
  } catch (error) {
    pushError = error;
  }
  check(!pushError, `replay push must not throw (${pushError?.message})`);
  const landed = ids.slice(0, 14).filter((id) => native.masters.get(id).campaign === 'campaign-b');
  check(landed.length === 14, `4: all 14 refused moves land without user action (got ${landed.length})`);
  const consistent = ids.slice(0, 14).every((id) => JSON.stringify(native.masters.get(id).field_status.website) === JSON.stringify(NEW_RECEIPT));
  check(consistent, '4: replayed moves keep the master native-owned values');
  const closed = conflicts.filter((entry) => entry.resolution === 'replayed_on_master').map((entry) => entry.conflictId);
  check(closed.length === 14, `4: exactly the 14 native-owned entries are closed (got ${closed.length})`);
  check(conflicts.find((entry) => entry.conflictId === 'c-lww').state === 'pending', '4: the update_vs_update entry stays pending');
  check(conflicts.find((entry) => entry.conflictId === 'c-stale').state === 'pending', '4: a stale-regression entry stays pending');
  check(native.masters.get(ids[14]).campaign === 'campaign-a', '4: the update_vs_update lead is not overwritten automatically');
  check(local.reconciled.length === 0, '4: no replayed move is refused again');
  await state.cancel();
}

// ---- 5: master wins whole-document LWW -> origin write journals the loss ---
{
  const native = createNativePeer([masterLead('lead_w', { _meta: { ctoxHlc: hlc(NOW + 60_000, 'native') } })]);
  const { state, local } = await createState({
    pending: [staleLocalMove('lead_w', { _meta: { ctoxHlc: hlc(NOW, 'tab-a') } })],
  });
  state.shared.peer = native.peer;
  state.shared.demandTransport = native.demandTransport;
  await state.pushToPeer('p1').catch((error) => check(false, `5: push threw ${error?.message}`));
  check(native.masters.get('lead_w').campaign === 'campaign-a', '5: the newer master keeps its campaign');
  check(local.originWrites.some((doc) => doc.id === 'lead_w' && doc.campaign === 'campaign-a'),
    '5: the winning master row is applied with a replication origin (storage journals update_vs_update)');
  check(local.reconciled.length === 0, '5: an LWW loss is not an authz refusal');
  await state.cancel();
}

const expectRed = process.env.CTOX_NATIVE_OWNED_SMOKE_EXPECT_RED === '1';
if (failures.length) {
  console.error(`native-owned-push-rebase-smoke: ${failures.length} failed\n - ${failures.join('\n - ')}`);
  process.exit(expectRed ? 0 : 1);
}
if (expectRed) {
  console.error('native-owned-push-rebase-smoke: expected red under mutation, got green');
  process.exit(1);
}
console.log('native-owned-push-rebase-smoke ok');
process.exit(0);
