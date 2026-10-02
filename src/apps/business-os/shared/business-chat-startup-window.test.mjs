import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { __businessChatTestInternals } from './business-chat.js';

// This isolated Node process has no real browser storage or tenant state.
globalThis.localStorage = { setItem() {}, getItem() { return null; }, removeItem() {} };
const createdAt = Date.now();
const rows = Array.from({ length: 871 }, (_, index) => ({ id: `chat-${index}`,
  owner_user_id: 'user-1', createdAt, updated_at_ms: createdAt, messages: [], open: false,
}));
let query;
const state = { chats: [], ownerUserId: 'user-1', activeChatId: '', dockCollapsed: true,
  selectedDate: '2026-10-02', remoteHydrationComplete: false };
await __businessChatTestInternals.hydrateChatsFromRxDb({ state, session: { user: { id: 'user-1' } },
  db: { raw: { business_chats: { find(value) {
    query = value;
    return { exec: async () => rows.slice(0, value?.limit ?? rows.length).map(row => ({ toJSON: () => row })) };
  } } } },
});
assert.equal(query?.limit, 200, 'warm/offline hydration must explicitly bound the query before a loader attaches');
assert.equal(state.chats.length, 200);

// A query observable has already paid for this snapshot. Hydration must merge
// it without starting the same storage/demand read again, and retain drafts.
const completedDocuments = rows.map(row => ({ toJSON: () => row }));
const snapshotState = { ...state, chats: [{ ...rows[0], id: 'local-draft', draft: 'keep me' }],
  remoteHydrationComplete: false };
await __businessChatTestInternals.hydrateChatsFromRxDb({ state: snapshotState,
  session: { user: { id: 'user-1' } }, documents: completedDocuments,
  db: { raw: { business_chats: { find() { throw new Error('completed query snapshot must not be read twice'); } } } },
});
assert.equal(snapshotState.chats.length, 201, 'snapshot stays bounded while existing local history survives');
assert.equal(snapshotState.chats.find(chat => chat.id === 'local-draft')?.draft, 'keep me');
assert.equal(completedDocuments.length, 871, 'caller snapshot is not mutated');

const source = await readFile(new URL('./business-chat.js', import.meta.url), 'utf8');
const syncBody = source.match(/  const syncChats = [\s\S]*?\n  };/)?.[0];
assert.ok(syncBody);
const pending = [];
let reads = 0;
let active = 0;
let peak = 0;
let renders = 0;
const snapshots = [];
const createRun = new Function('hydrateChatsFromRxDb', 'renderChatRoot', 'shouldDeferRemoteChatHydration',
  'currentChatOpenOwnership', 'ownsChatOpenOwnership', 'captureDrafts', `
  const root = {}, state = {}, db = {}, session = {}, commandBus = {}, getActiveModule = () => '';
  let chatHydrationDisposed = false, chatHydrationInFlight = false, chatHydrationRequested = false;
  let chatHydrationSnapshot;
  const CHAT_QUERY_WINDOW_LIMIT = 200;
  const scheduleChatHydrationRetry = () => {};
  ${syncBody}
  return { syncChats, dispose() { chatHydrationDisposed = true; chatHydrationRequested = false; chatHydrationSnapshot = undefined; } };
`);
const fakeHydrate = ({ documents }) => {
  snapshots.push(documents);
  reads += 1; active += 1; peak = Math.max(peak, active);
  return new Promise(resolve => pending.push(() => { active -= 1; resolve(true); }));
};
const makeRun = () => createRun(fakeHydrate, () => { renders += 1; }, () => false, () => 1, () => true, () => {});
const run = makeRun();
const pause = () => new Promise(resolve => setTimeout(resolve, 0));
for (let tick = 0; tick < 20; tick += 1) run.syncChats();
assert.equal(peak, 1, 'chat change notifications must not overlap hydration reads');
assert.equal(reads, 1);
pending[0]();
await pause();
assert.equal(reads, 2, 'many invalidations coalesce into one follow-up');
assert.equal(renders, 1, 'first available hydrated state still renders');
run.dispose();
pending[1]();
await pause();
assert.equal(renders, 1, 'disposed chat companion cannot render a late result');
assert.equal(active, 0);

const live = makeRun();
live.syncChats(completedDocuments);
for (let tick = 0; tick < 20; tick += 1) live.syncChats([completedDocuments[tick]]);
assert.equal(reads, 3, 'live snapshots still have only one active merge');
assert.equal(snapshots[2].length, 200, 'pending snapshot cannot retain an unbounded collection');
pending[2]();
await pause();
assert.equal(reads, 4);
assert.deepEqual(snapshots[3], [completedDocuments[19]], 'follow-up consumes only the newest supplied snapshot');
live.dispose();
pending[3]();
await pause();
assert.equal(peak, 1);
assert.equal(active, 0);
console.log('Business chat warm/offline bounded window, single-flight and snapshot reuse PASS');
