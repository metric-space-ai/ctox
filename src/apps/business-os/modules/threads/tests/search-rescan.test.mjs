import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import { collectUniquePages } from '../paging.js';

// Exercise the module's real scan, observation and selection lifecycle without
// a browser. Only rendering and detail transport are replaced at the boundary.
const source = await readFile(new URL('../index.js', import.meta.url), 'utf8');
const observers = new Map();
const timers = new Map();
let timerId = 0;
let rows = [];
let pendingPage;
let pendingRecord;
let onRecordLookup;
let attentionStates = [];
const listeners = new Map();
const context = vm.createContext({
  collectUniquePages, console, isVisible: true,
  window: {
    setTimeout(fn) { timers.set(++timerId, fn); return timerId; },
    clearTimeout(id) { timers.delete(id); },
    setInterval() { return 0; }, clearInterval() {},
  },
  document: { addEventListener(name, fn) { listeners.set(name, fn); }, removeEventListener(name) { listeners.delete(name); } },
  database: { collection(name) { return {
    observe(fn) { observers.set(name, fn); return () => observers.delete(name); },
    find({ skip = 0, limit = 100 }) { return { exec: () => name === 'user_threads'
      ? pendingPage || Promise.resolve(rows.slice(skip, skip + limit))
      : Promise.resolve(name === 'user_thread_states' ? attentionStates : []) }; },
    findOne(id) { return { exec: () => {
      onRecordLookup?.();
      return pendingRecord || Promise.resolve(rows.find((row) => row.id === id));
    } }; },
  }; } },
});
vm.runInContext(source.replace(/^import[\s\S]*?;\n/gm, '').replace(/^export /gm, '').replaceAll('import.meta.url', '"file:///threads/index.js"') + `
  moduleIsVisible = () => globalThis.isVisible;
  render = () => {};
  updateConnectivity = () => {};
  notifyActionRequired = () => {};
  currentUserId = () => 'alice';
  clearLoadError = () => {};
  showError = (error) => { throw error; };
  hydrateSelectedThread = async () => {};
  visibleThreads = () => state.data.threads;
  Object.assign(state, {
    ctx: { db: database }, search: 'target', searchCorpus: [],
    searchCorpusComplete: false, searchScanGeneration: 0,
    threadsReadiness: { ready: true }, searchScanInFlight: null,
  });
  globalThis.api = { state, scheduleSearchScan, wireRealtime, refreshOnce };
`, context);
const { state, scheduleSearchScan, wireRealtime, refreshOnce } = context.api;
const stop = wireRealtime();
async function scan() {
  scheduleSearchScan();
  const callback = [...timers.values()].at(-1);
  assert.ok(callback, 'scan scheduled');
  timers.clear();
  callback();
  await state.searchScanInFlight;
}

rows = [{ id: 'removed', title: 'target', updated_at_ms: 1 }];
await scan();
assert.equal(state.searchCorpusComplete, true);
assert.equal(state.selectedId, 'removed');
state.detailCompleteThreadId = 'removed';
state.data.messages = [{ id: 'secret-message', thread_id: 'removed' }];
state.data.commands = [{ id: 'old-command' }];
// Absence in a complete scan must evict the old record even without an event.
rows = [];
state.searchCorpusComplete = false;
await scan();
assert.equal(state.searchCorpus.length, 0);
assert.equal(state.data.threads.length, 0);
assert.equal(state.selectedId, '');
assert.equal(state.detailCompleteThreadId, '');
assert.equal(state.data.messages.length, 0);
assert.equal(state.data.commands.length, 0);

rows = [{ id: 'removed', updated_at_ms: 2 }];
state.searchCorpusComplete = false;
await scan();
let resolvePage;
pendingPage = new Promise((resolve) => { resolvePage = resolve; });
state.searchCorpusComplete = false;
const inFlight = scan();
observers.get('user_threads')({ success: { removed: { id: 'removed', _deleted: true } } });
assert.equal(state.data.threads.length, 0, 'tombstone removes selected record immediately');
assert.equal(state.selectedId, '');
resolvePage(rows);
await inFlight;
assert.equal(state.searchCorpusComplete, false, 'invalidated in-flight scan is incomplete');
assert.equal(state.data.threads.length, 0, 'late page cannot resurrect deleted thread');
pendingPage = undefined;
rows = [];
await scan();
assert.equal(state.searchCorpusComplete, true);
assert.equal(state.searchCorpus.length, 0);
// A refresh that already fetched old rows must not undo a later tombstone.
rows = [{ id: 'removed', updated_at_ms: 3 }];
state.searchCorpusComplete = false;
await scan();
pendingPage = new Promise((resolve) => { resolvePage = resolve; });
const oldRefresh = refreshOnce();
observers.get('user_threads')({ success: { removed: { id: 'removed', _deleted: true } } });
resolvePage(rows);
await oldRefresh;
assert.equal(state.data.threads.length, 0, 'late recent-window response cannot resurrect a tombstone');
assert.equal(state.recentThreadsComplete, false);
pendingPage = undefined;

// Exercise the later await boundary too: recent rows arrived, personal lookup
// is pending when the deletion lands.
attentionStates = [{ id: 'state-removed', user_id: 'alice', thread_id: 'removed', attention_score: 50 }];
let resolveRecord;
pendingRecord = new Promise((resolve) => { resolveRecord = resolve; });
const recordRequested = new Promise((resolve) => { onRecordLookup = resolve; });
const oldPersonalRefresh = refreshOnce();
await recordRequested;
observers.get('user_threads')({ success: { removed: { id: 'removed', is_deleted: true } } });
resolveRecord(rows[0]);
await oldPersonalRefresh;
assert.equal(state.data.threads.length, 0, 'late personal response cannot resurrect a tombstone');
assert.equal(state.personalComplete, false);
pendingRecord = undefined;
onRecordLookup = undefined;
attentionStates = [];

state.searchCorpusComplete = false;
await scan();
assert.equal(state.searchCorpusComplete, true);
context.isVisible = false;
observers.get('user_threads')({ success: { removed: { id: 'removed', _deleted: true } } });
assert.equal(state.searchCorpusComplete, false, 'hidden deletion invalidates completed search');
assert.equal(state.data.threads.length, 0);
rows = [];
context.isVisible = true;
listeners.get('visibilitychange')();
await state.refreshInFlight;
assert.equal(state.data.threads.length, 0, 'visible refresh cannot merge deleted cached row back');
assert.equal(state.searchCorpusComplete, false, 'visible refresh does not substitute for a complete rescan');
await scan();
assert.equal(state.searchCorpusComplete, true);
assert.equal(state.selectedId, '');
stop();

let active = true;
const cancelled = await collectUniquePages(async () => { active = false; return [{ id: 'late' }]; }, {
  shouldContinue: () => active,
});
assert.equal(cancelled.complete, false, 'cancellation during final page is not completion');
assert.equal(cancelled.records.length, 0);
console.log('Threads authoritative rescan/deletion/cancellation regression passed');
