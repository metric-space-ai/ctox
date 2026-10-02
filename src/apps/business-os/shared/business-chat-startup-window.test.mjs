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

const source = await readFile(new URL('./business-chat.js', import.meta.url), 'utf8');
const syncBody = source.match(/  const syncChats = [\s\S]*?\n  };/)?.[0];
assert.ok(syncBody);
const pending = [];
let reads = 0;
let active = 0;
let peak = 0;
let renders = 0;
const run = new Function('hydrateChatsFromRxDb', 'renderChatRoot', 'shouldDeferRemoteChatHydration',
  'currentChatOpenOwnership', 'ownsChatOpenOwnership', 'captureDrafts', `
  const root = {}, state = {}, db = {}, session = {}, commandBus = {}, getActiveModule = () => '';
  let chatHydrationDisposed = false, chatHydrationInFlight = false, chatHydrationRequested = false;
  const scheduleChatHydrationRetry = () => {};
  ${syncBody}
  return { syncChats, dispose() { chatHydrationDisposed = true; chatHydrationRequested = false; } };
`)(() => {
  reads += 1; active += 1; peak = Math.max(peak, active);
  return new Promise(resolve => pending.push(() => { active -= 1; resolve(true); }));
}, () => { renders += 1; }, () => false, () => 1, () => true, () => {});
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
console.log('Business chat warm/offline 200-row window and single-flight hydration PASS');
