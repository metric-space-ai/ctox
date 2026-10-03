// A complete eager replica answers locally, but in the loader's window: at most
// DEFAULT_WINDOW_LIMIT rows per query. Unbounded local reads handed the chat
// dock every Business chat (871 on the customer tenant, 01.10.2026); it merged and
// re-persisted all of them on each pass and froze the page.
const { createRxDatabase, replicationWebRtcTestInternals } = await import(
  process.argv.includes('--source') ? '../src/index.mjs' : '../dist/ctox-rxdb-js.mjs'
);

const seen = [];
const all = Array.from({ length: 871 }, (_, index) => ({ id: `chat-${String(index).padStart(4, '0')}`, n: index }));
const database = await createRxDatabase({
  name: 'eager-replica-read-window',
  storage: {
    nativeStorage: {
      collection() {
        return {
          observe() { return () => {}; },
          async queryDocuments(query) {
            seen.push({ limit: query.limit, skip: query.skip });
            const skip = Number(query.skip) || 0;
            const limit = Number.isFinite(Number(query.limit)) ? Number(query.limit) : all.length;
            return all.slice(skip, skip + limit);
          },
          async countDocuments() { return all.length; },
        };
      },
      close() {},
    },
  },
});
await database.addCollections({
  business_chats: { schema: { version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string' }, n: { type: 'number' } } } },
});
let failures = 0;
function check(name, condition) {
  if (condition) console.log(`ok   ${name}`);
  else { failures += 1; console.log(`FAIL ${name}`); }
}
const chats = database.collection('business_chats');
chats.setDemandLoader({ currentReadPermissionDigest: () => 'epoch', async resolveQuery() { throw new Error('loader must not be used'); } });
const State = replicationWebRtcTestInternals.getReplicationStateClass();
const eager = Object.create(State.prototype);
Object.assign(eager, { collection: chats, pull: { batchSize: 10 }, firstPullCompletedAtMs: 0, publishTransportStatus() {} });
eager.markFirstPullCompleted();

const unbounded = await chats.find().exec();
check('unbounded find on a complete replica returns one 200-row window', unbounded.length === 200 && seen.at(-1).limit === 200);
const small = await chats.find({ selector: {}, limit: 25, skip: 50 }).exec();
check('explicit smaller limit and skip are kept', small.length === 25 && small[0].toJSON().id === 'chat-0050' && seen.at(-1).limit === 25);
const big = await chats.find({ selector: {}, limit: 5000 }).exec();
check('limits above the window are capped like the loader', big.length === 200 && seen.at(-1).limit === 200);
const one = await chats.findOne('chat-0007').exec();
check('findOne reads a single row', seen.at(-1).limit === 1 && one !== null);

if (failures) {
  console.error(`eager-replica-read-window smoke: ${failures} failure(s)`);
  process.exit(1);
}
console.log('eager-replica-read-window smoke OK');
