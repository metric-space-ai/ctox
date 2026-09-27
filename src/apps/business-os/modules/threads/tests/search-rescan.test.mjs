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
const context = vm.createContext({
  collectUniquePages, console,
  window: {
    setTimeout(fn) { timers.set(++timerId, fn); return timerId; },
    clearTimeout(id) { timers.delete(id); },
    setInterval() { return 0; }, clearInterval() {},
  },
  document: { addEventListener() {}, removeEventListener() {} },
  database: { collection(name) { return {
    observe(fn) { observers.set(name, fn); return () => observers.delete(name); },
    find({ skip = 0, limit = 100 }) { return { exec: () => pendingPage || Promise.resolve(rows.slice(skip, skip + limit)) }; },
  }; } },
});
vm.runInContext(source.replace(/^import[\s\S]*?;\n/gm, '').replace(/^export /gm, '').replaceAll('import.meta.url', '"file:///threads/index.js"') + `
  moduleIsVisible = () => true;
  render = () => {};
  clearLoadError = () => {};
  showError = (error) => { throw error; };
  hydrateSelectedThread = async () => {};
  visibleThreads = () => state.data.threads;
  Object.assign(state, {
    ctx: { db: database }, search: 'target', searchCorpus: [],
    searchCorpusComplete: false, searchScanGeneration: 0,
    threadsReadiness: { ready: true }, searchScanInFlight: null,
  });
  globalThis.api = { state, scheduleSearchScan, wireRealtime };
`, context);
const { state, scheduleSearchScan, wireRealtime } = context.api;
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
stop();

let active = true;
const cancelled = await collectUniquePages(async () => { active = false; return [{ id: 'late' }]; }, {
  shouldContinue: () => active,
});
assert.equal(cancelled.complete, false, 'cancellation during final page is not completion');
assert.equal(cancelled.records.length, 0);
console.log('Threads authoritative rescan/deletion/cancellation regression passed');
