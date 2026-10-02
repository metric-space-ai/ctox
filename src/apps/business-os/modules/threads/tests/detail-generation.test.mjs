import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../index.js', import.meta.url), 'utf8');
const pending = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};
const context = vm.createContext({ console });
vm.runInContext(source.replace(/^import[\s\S]*?;\n/gm, '').replace(/^export /gm, '')
  .replaceAll('import.meta.url', '"file:///threads/index.js"') + `
  currentUserId = () => 'alice';
  render = () => {};
  linkedCommandIds = () => ['command'];
  linkedTaskIds = () => ['task'];
  globalThis.api = {
    state, hydrateSelectedThread,
    pages(fn) { loadPersonalPages = fn; },
    records(fn) { loadRecordsByIds = fn; },
  };
`, context);
const api = context.api;
const { state, hydrateSelectedThread } = api;
const reset = () => {
  Object.assign(state, {
    ctx: {}, selectedId: 'thread', searchScanGeneration: 0,
    detailCompleteThreadId: '', data: {
      threads: [{ id: 'thread' }], messages: [], links: [], approvals: [],
      notifications: [], commands: [], queue: [],
    },
  });
  api.pages(async (name) => name === 'user_thread_messages' ? [{ id: 'new-message' }] : []);
  api.records(async (name) => [{ id: name === 'business_commands' ? 'new-command' : 'new-task' }]);
};

// Two refreshes can hydrate the same ID; the older response must not win.
reset();
const oldPages = pending();
api.pages(() => oldPages.promise);
const oldDetail = hydrateSelectedThread('thread');
api.pages(async (name) => name === 'user_thread_messages' ? [{ id: 'new-message' }] : []);
await hydrateSelectedThread('thread');
oldPages.resolve([{ id: 'old-message' }]);
await oldDetail;
assert.equal(state.data.messages[0].id, 'new-message');
assert.equal(state.data.commands[0].id, 'new-command');

// Removal/reselection of the same ID invalidates the former collection generation.
reset();
const revokedPages = pending();
api.pages(() => revokedPages.promise);
const revokedDetail = hydrateSelectedThread('thread');
state.searchScanGeneration += 1;
revokedPages.resolve([{ id: 'revoked-message' }]);
await revokedDetail;
assert.equal(state.data.messages.length, 0);
assert.equal(state.detailCompleteThreadId, '');

// A stale command response must not start another task lookup or overwrite newer detail.
reset();
const oldCommands = pending();
const commandStarted = pending();
let oldQueueLookups = 0;
api.records((name) => {
  if (name === 'business_commands') { commandStarted.resolve(); return oldCommands.promise; }
  oldQueueLookups += 1;
  return Promise.resolve([{ id: 'old-task' }]);
});
const commandDetail = hydrateSelectedThread('thread');
await commandStarted.promise;
api.records(async (name) => [{ id: name === 'business_commands' ? 'new-command' : 'new-task' }]);
await hydrateSelectedThread('thread');
oldCommands.resolve([{ id: 'old-command' }]);
await commandDetail;
assert.equal(oldQueueLookups, 0);
assert.equal(state.data.commands[0].id, 'new-command');
assert.equal(state.data.queue[0].id, 'new-task');

// Invalidation during the final task lookup cannot mark stale detail complete.
reset();
const oldQueue = pending();
const queueStarted = pending();
api.records((name) => {
  if (name === 'business_commands') return Promise.resolve([{ id: 'old-command' }]);
  queueStarted.resolve();
  return oldQueue.promise;
});
const queueDetail = hydrateSelectedThread('thread');
await queueStarted.promise;
state.searchScanGeneration += 1;
oldQueue.resolve([{ id: 'revoked-task' }]);
await queueDetail;
assert.equal(state.data.commands.length, 0);
assert.equal(state.data.queue.length, 0);
assert.equal(state.detailCompleteThreadId, '');
console.log('threads detail generation regression passed');
