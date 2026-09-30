// An eagerly pulled collection whose pull drained holds the whole collection
// locally; its queries must read that replica. On the customer tenant
// (30.09.2026) every Outbound reload still asked the native peer for 200 leads
// (~20 MB) in one demand window, hit QUERY_COLLECTOR_TIMEOUT, and the app
// showed "Noch keine Kampagne" over a complete local store.
const { createRxDatabase, createQueryDemandLoader, createSidecarWithMemoryBackend, replicationWebRtcTestInternals } = await import(
  process.argv.includes('--source') ? '../src/index.mjs' : '../dist/ctox-rxdb-js.mjs'
);

const rows = new Map([
  ['outbound_lead_generation_leads', { id: 'lead-1', state: 'local' }],
  ['business_commands', { id: 'command-1', state: 'local' }],
]);
let directReads = 0;
const database = await createRxDatabase({
  name: 'eager-replica-local-read',
  storage: {
    nativeStorage: {
      collection(name) {
        return {
          observe() { return () => {}; },
          async queryDocuments() { directReads += 1; return [rows.get(name)]; },
          async countDocuments() { directReads += 1; return 1; },
          async bulkWrite() {}, // Keep the local row stale to prove strict reads return authority.
          async findDocumentsById(ids) {
            const row = rows.get(name);
            return ids.includes(row.id) ? { [row.id]: row } : {};
          },
        };
      },
      close() {},
    },
  },
});
const schema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: { id: { type: 'string' }, state: { type: 'string' } },
};
await database.addCollections(Object.fromEntries([...rows.keys()].map((name) => [name, { schema }])));

let failures = 0;
function check(name, condition) {
  if (condition) console.log(`ok   ${name}`);
  else { failures += 1; console.log(`FAIL ${name}`); }
}
const timeoutLoader = {
  calls: 0,
  currentReadPermissionDigest: () => 'epoch',
  async resolveQuery() {
    this.calls += 1;
    throw new Error('QUERY_COLLECTOR_TIMEOUT: terminal frame missing');
  },
};

const leads = database.collection('outbound_lead_generation_leads');
leads.setDemandLoader(timeoutLoader);
let rejected = false;
try { await leads.find().exec(); } catch { rejected = true; }
check('before the first pull drains, queries go to the demand loader', rejected && timeoutLoader.calls === 1);

// Drive the real replication state's coverage publication.
const State = replicationWebRtcTestInternals.getReplicationStateClass();
const eager = Object.create(State.prototype);
Object.assign(eager, { collection: leads, pull: { batchSize: 10 }, firstPullCompletedAtMs: 0, publishTransportStatus() {} });
eager.markFirstPullCompleted();
check('drained eager pull marks the local replica complete', leads.localReplicaComplete === true);
const docs = await leads.find({ selector: {}, sort: [{ id: 'asc' }], limit: 200 }).exec();
check('complete eager replica answers locally', docs.length === 1 && docs[0].toJSON().id === 'lead-1' && timeoutLoader.calls === 1);

// A complete replica is sufficient for ordinary reads, but cannot satisfy an
// explicit authority token. Exercise the real loader across bridge generations.
let generation = 'bridge-1';
let authoritativeFetches = 0;
const strictLoader = createQueryDemandLoader({
  storageCollection: leads.storageCollection,
  sidecar: createSidecarWithMemoryBackend({ databaseName: 'eager-strict-revision' }),
  collectionName: leads.name,
  schemaVersion: 0,
  queryGeneration: () => generation,
  requestQueryFetch: async () => {
    authoritativeFetches += 1;
    return {
      documents: [{ id: 'lead-1', state: generation }],
      authoritativeRevision: `native-${authoritativeFetches}`,
    };
  },
});
leads.setDemandLoader(strictLoader);
const strictRead = token => leads.find({ selector: {}, requireRevision: token }).exec();
let strictDocs = await strictRead('revision-1');
check('complete eager replica forwards strict revision to authority',
  authoritativeFetches === 1 && strictDocs[0].toJSON().state === 'bridge-1');
await strictRead('revision-1');
check('same strict token and bridge may reuse the authoritative window', authoritativeFetches === 1);
await strictRead('revision-2');
check('new strict token requires another authoritative fetch', authoritativeFetches === 2);
generation = 'bridge-2';
strictDocs = await strictRead('revision-2');
check('changed bridge invalidates strict authority despite complete replica',
  authoritativeFetches === 3 && strictDocs[0].toJSON().state === 'bridge-2');
const ordinaryDocs = await leads.find().exec();
check('strict read checks preserve the ordinary local fast path',
  authoritativeFetches === 3 && ordinaryDocs[0].toJSON().state === 'local');

eager.firstPullCompletedAtMs = 0;
eager.publishLocalReplicaCoverage();
check('invalidated checkpoint returns queries to the demand loader', leads.localReplicaComplete === false);

const demandOnly = Object.create(State.prototype);
Object.assign(demandOnly, { collection: leads, pull: null, firstPullCompletedAtMs: 0, publishTransportStatus() {} });
demandOnly.markFirstPullCompleted();
check('demand-only collection never claims a complete replica', leads.localReplicaComplete === false);

const commands = database.collection('business_commands');
commands.setLocalReplicaComplete(true);
check('control-plane ledger stays behind its authorized window', commands.localReplicaComplete === false);

if (failures) {
  console.error(`eager-replica-local-read smoke: ${failures} failure(s)`);
  process.exit(1);
}
console.log('eager-replica-local-read smoke OK', { directReads });
