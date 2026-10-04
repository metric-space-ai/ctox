import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { crewAppTasksFromTasks, crewLiveKeys, crewAppLiveKeys, __businessChatTestInternals } from './business-chat.js';

const task = (id, fields = {}) => ({
  id, title: 'Task ' + id, status: 'running', module: 'ctox', source_module: 'documents',
  updated_at_ms: 1, attempt: 1, lease_worker_id: 'own-boot:worker-' + id, ...fields,
});
const liveHarness = ids => ({ id: 'harness', service_running: true, boot_id: 'own-boot',
  active_task_ids: ids, current_queue_workers: ids.map(task_id => ({ task_id, boot_id: 'own-boot',
    lease_worker_id: 'own-boot:worker-' + task_id, attempt: 1,
    leased_at: new Date(Date.now() - 1000).toISOString(), lease_expires_at: new Date(Date.now() + 60000).toISOString(),
  })),
});

test('an open app or a queue row without native worker truth creates no task count', () => {
  assert.equal(crewAppTasksFromTasks([task('a')], null).size, 0);
  assert.equal(crewAppTasksFromTasks([], new Set(['a'])).size, 0);
  assert.equal(crewAppTasksFromTasks([task('a')], crewLiveKeys({ service_running: false, active_task_ids: ['a'] })).size, 0);
});

test('the originating app owns all its executing tasks, including unassigned tasks', () => {
  const rows = [task('a'), task('b', { crew_member_id: 'luma' }), task('c', { source_module: 'mail' })];
  const result = crewAppTasksFromTasks(rows, new Set(['a', 'b', 'c']));
  assert.deepEqual(result.get('documents').map(row => row.id), ['a', 'b']);
  assert.deepEqual(result.get('mail').map(row => row.id), ['c']);
  assert.equal(result.has('ctox'), false, 'the coordinator module is not the originating app');
});

test('stale leases, terminal rows and tombstones cannot count as executing work', () => {
  const rows = [task('live'), task('old'), task('done', { status: 'completed' }),
    task('deleted', { _deleted: true }), task('hidden', { is_deleted: true })];
  assert.deepEqual(crewAppTasksFromTasks(rows, new Set(['live', 'done', 'deleted', 'hidden']))
    .get('documents').map(row => row.id), ['live']);
});

test('projection identities are deduplicated and use the real task navigation key', () => {
  const rows = [task('row-a', { task_id: 'queue-a', command_id: 'cmd-a' }),
    task('row-b', { task_id: 'queue-a', title: 'Duplicate' })];
  const items = crewAppTasksFromTasks(rows, new Set(['queue-a'])).get('documents');
  assert.equal(items.length, 1);
  assert.equal(items[0].id, 'queue-a');
  assert.equal(items[0].commandId, 'cmd-a');
});

test('details retain bounded public title/status and no task payload', () => {
  const details = crewAppTasksFromTasks([task('a', { title: 'x'.repeat(1000), prompt: 'private payload' })],
    new Set(['a'])).get('documents')[0];
  assert.equal(details.title.length, 256);
  assert.deepEqual(Object.keys(details).sort(), ['commandId', 'id', 'status', 'title']);
});

test('legacy module attribution remains available when native source_module is absent', () => {
  const rows = [task('a', { source_module: '', module: 'mail' }), task('', { source_module: 'mail' })];
  assert.deepEqual(crewAppTasksFromTasks(rows, new Set(['a'])).get('mail').map(row => row.id), ['a']);
});

test('the actual presence query selects native identities before applying its bounded batch', async () => {
  const source = readFileSync(new URL('./business-chat.js', import.meta.url), 'utf8');
  const body = source.match(/^async function loadCrewAppTasks\([^]*?^\}/m)?.[0];
  assert.ok(body);
  let query;
  const load = runInNewContext(body + '\nloadCrewAppTasks', {
    CREW_APP_PRESENCE_STATUSES: new Set(['running', 'leased', 'review', 'drafting']),
    CREW_APP_PRESENCE_TASK_LIMIT: 200, console,
  });
  await load({ raw: { ctox_queue_tasks: { find: options => {
    query = options; return { exec: async () => [] };
  } } } }, new Set(['a']));
  assert.deepEqual([...query.selector.status.$in], ['running', 'leased', 'review', 'drafting']);
  assert.deepEqual([...query.selector.id.$in], ['a']);
  assert.equal(query.limit, 200);
});

test('a stopped native harness retires workload without reading its stale queue', async () => {
  const previousWindow = globalThis.window;
  const previousDocument = globalThis.document;
  const callbacks = new Map();
  let cancelled = false;
  let queueReads = 0;
  let harness = liveHarness(['a']);
  const state = { crewMembers: [{ id: 'luma', name: 'Luma' }] };
  globalThis.window = { setTimeout: callback => callback, clearTimeout() {} };
  globalThis.document = { querySelector: () => null, querySelectorAll: () => [] };
  let dispose;
  try {
    dispose = __businessChatTestInternals.wireCrewAppPresence({
      state,
      db: { raw: {
        ctox_queue_tasks: { find: () => ({ exec: async () => {
          queueReads++;
          if (cancelled) throw Object.assign(new Error('retired query'), { code: 'QUERY_CANCELLED' });
          return [task('a', { crew_member_id: 'luma' })];
        } }) },
        ctox_harness_status: { find: () => ({ exec: async () => [harness] }) },
      } },
      syncFacade: { collectionFreshness: () => ({ ready: true, state: 'live' }), subscribeCollectionReadiness: (name, callback) => {
        callbacks.set(name, callback); return () => callbacks.delete(name);
      } },
    });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(state.crewWorkload.get('luma'), 1);
    cancelled = true;
    harness = { ...harness, service_running: false };
    await callbacks.get('ctox_queue_tasks')();
    assert.equal(state.crewWorkload.size, 0);
    assert.equal(queueReads, 1, 'stopped native truth requires no further queue read');
  } finally {
    dispose?.();
    globalThis.window = previousWindow;
    globalThis.document = previousDocument;
  }
});

test('presence reloads keep one storage read and coalesce repeated readiness notifications', async () => {
  const previousWindow = globalThis.window;
  const previousDocument = globalThis.document;
  const callbacks = new Map();
  const pending = [];
  let reads = 0;
  let active = 0;
  let maximum = 0;
  const state = { crewMembers: [] };
  globalThis.window = { setTimeout: callback => callback, clearTimeout() {} };
  globalThis.document = { querySelector: () => null, querySelectorAll: () => [] };
  let dispose;
  try {
    dispose = __businessChatTestInternals.wireCrewAppPresence({
      state,
      db: { raw: {
        ctox_queue_tasks: { find: () => ({ exec: () => {
          reads++; active++; maximum = Math.max(maximum, active);
          return new Promise(resolve => pending.push(() => { active--; resolve([]); }));
        } }) },
        ctox_harness_status: { find: () => ({ exec: async () => [
          liveHarness(['a']),
        ] }) },
      } },
      syncFacade: { collectionFreshness: () => ({ ready: true, state: 'live' }), subscribeCollectionReadiness: (name, callback) => {
        callbacks.set(name, callback); return () => callbacks.delete(name);
      } },
    });
    await new Promise(resolve => setImmediate(resolve));
    const notifications = Array.from({ length: 12 }, () => callbacks.get('ctox_queue_tasks')());
    assert.equal(reads, 1);
    assert.equal(active, 1);
    pending.shift()();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(reads, 2, 'the storm produces one coalesced follow-up');
    assert.equal(active, 1);
    pending.shift()();
    await Promise.all(notifications);
    assert.equal(maximum, 1);
    assert.equal(reads, 2);
  } finally {
    dispose?.();
    globalThis.window = previousWindow;
    globalThis.document = previousDocument;
  }
});

test('unknown native worker truth removes the app execution avatar as well as its count', async () => {
  const previousWindow = globalThis.window;
  const previousDocument = globalThis.document;
  let removed = 0;
  let recreated = 0;
  const badge = { dataset: {}, remove() { removed++; } };
  const glyph = {
    closest: () => ({ dataset: { target: 'documents' } }),
    querySelector: () => badge,
    classList: { add() {}, remove() {} },
    insertAdjacentHTML() { recreated++; },
  };
  globalThis.window = { setTimeout: callback => callback, clearTimeout() {} };
  globalThis.document = { querySelector: () => null,
    querySelectorAll: selector => selector.includes('.desktop-icon[data-target]') ? [glyph] : [] };
  let dispose;
  try {
    dispose = __businessChatTestInternals.wireCrewAppPresence({
      state: { crewMembers: [{ id: 'luma', name: 'Luma' }] },
      db: { raw: { ctox_queue_tasks: { find: () => ({ exec: async () => [
        task('a', { crew_member_id: 'luma' }),
      ] }) } } },
      syncFacade: { collectionFreshness: () => ({ ready: true, state: 'live' }), subscribeCollectionReadiness: () => () => {} },
    });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(removed, 1, 'cached execution indicator is retired');
    assert.equal(recreated, 0, 'a running queue status alone cannot recreate the app badge');
  } finally {
    dispose?.();
    globalThis.window = previousWindow;
    globalThis.document = previousDocument;
  }
});

test('native executing tasks survive a newer stale queue window without retaining task payloads', async () => {
  const source = readFileSync(new URL('./business-chat.js', import.meta.url), 'utf8');
  const body = source.match(/^async function loadCrewAppTasks\([^]*?^\}/m)?.[0];
  assert.ok(body, 'exercise the actual task loader');
  const load = runInNewContext(body + '\nloadCrewAppTasks', {
    CREW_APP_PRESENCE_STATUSES: new Set(['running', 'leased', 'review', 'drafting']),
    CREW_APP_PRESENCE_TASK_LIMIT: 200, console,
  });
  const live = Array.from({ length: 301 }, (_, index) => task('live-' + index, {
    crew_member_id: 'luma', updated_at_ms: 1, prompt: 'private payload', title: 'x'.repeat(1000),
  }));
  const stale = Array.from({ length: 500 }, (_, index) => task('stale-' + index, { updated_at_ms: 1000 }));
  const rows = [...live, ...stale];
  const keys = new Set(live.map(row => row.id));
  const queries = [];
  const loaded = await load({ raw: { ctox_queue_tasks: { find: query => {
    queries.push(query);
    const ids = query.selector.id?.$in;
    const matching = rows.filter(row => (!ids || ids.includes(row.id))
      && query.selector.status.$in.includes(row.status) && row.updated_at_ms > 0);
    matching.sort((a, b) => b.updated_at_ms - a.updated_at_ms);
    return { exec: async () => matching.slice(0, query.limit) };
  } } } }, keys);
  assert.equal(crewAppTasksFromTasks(loaded, keys).get('documents')?.length || 0, 301,
    'newer stale rows cannot hide native executing tasks');
  assert.equal(queries.length, 2, 'native identities are loaded in bounded batches');
  assert.ok(queries.every(query => query.limit <= 200 && query.selector.id.$in.length <= 200));
  assert.ok(loaded.every(row => !('prompt' in row) && row.title.length <= 256),
    'presence state retains only bounded public fields');
});

test('app execution requires the current native boot lease projection rather than an active-id snapshot', () => {
  assert.equal(crewAppLiveKeys({ service_running: true, active_task_ids: ['old'] }), null);
  const harness = liveHarness(['live', 'wrong-boot', 'wrong-worker', 'no-attempt']);
  harness.current_queue_workers[1].boot_id = 'previous-boot';
  harness.current_queue_workers[2].lease_worker_id = 'previous-boot:worker';
  harness.current_queue_workers[3].attempt = 0;
  assert.deepEqual([...crewAppLiveKeys(harness)], ['live']);
  assert.equal(crewAppLiveKeys({ ...harness, active_task_ids: 'corrupt' }), null);
  assert.equal(crewAppLiveKeys({ ...harness, service_running: false }).size, 0);
});

test('unconfirmed native freshness retires work immediately and never reads the cached task queue', async () => {
  const previousWindow = globalThis.window;
  const previousDocument = globalThis.document;
  let fresh = true;
  let changed;
  let reads = 0;
  const state = { crewMembers: [{ id: 'luma', name: 'Luma' }] };
  globalThis.window = { setTimeout: callback => callback, clearTimeout() {} };
  globalThis.document = { querySelector: () => null, querySelectorAll: () => [] };
  let dispose;
  try {
    dispose = __businessChatTestInternals.wireCrewAppPresence({ state,
      db: { raw: {
        ctox_queue_tasks: { find: () => ({ exec: async () => { reads++; return [task('a', { crew_member_id: 'luma' })]; } }) },
        ctox_harness_status: { find: () => ({ exec: async () => [liveHarness(['a'])] }) },
      } },
      syncFacade: {
        collectionFreshness: () => ({ ready: fresh }),
        subscribeCollectionFreshness: (_name, callback) => { changed = callback; return () => {}; },
      },
    });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(state.crewWorkload.get('luma'), 1);
    fresh = false;
    const pending = changed({ ready: false });
    assert.equal(state.crewWorkload.size, 0, 'offline transition does not await another storage read');
    await pending;
    assert.equal(reads, 1, 'cached task rows cannot manufacture offline execution');
    fresh = true;
    await changed({ ready: true });
    assert.equal(state.crewWorkload.get('luma'), 1);
    assert.equal(reads, 2, 'confirmed native freshness can restore the current executing task');
  } finally {
    dispose?.();
    globalThis.window = previousWindow;
    globalThis.document = previousDocument;
  }
});

test('native stop retires workload while a task read remains pending and its late result cannot revive work', async () => {
  const previousWindow = globalThis.window;
  const previousDocument = globalThis.document;
  const state = { crewMembers: [{ id: 'luma', name: 'Luma' }] };
  let harness = liveHarness(['a']);
  let hold = false;
  let finish;
  let nativeChanged;
  let reload;
  globalThis.window = { setTimeout: callback => callback, clearTimeout() {} };
  globalThis.document = { querySelector: () => null, querySelectorAll: () => [] };
  let dispose;
  try {
    dispose = __businessChatTestInternals.wireCrewAppPresence({ state,
      db: { raw: {
        ctox_queue_tasks: { find: () => ({ exec: async () => hold
          ? new Promise(resolve => { finish = resolve; }) : [task('a', { crew_member_id: 'luma' })] }) },
        ctox_harness_status: { find: () => ({ exec: async () => [harness] }),
          $: { subscribe: callback => { nativeChanged = callback; return { unsubscribe() {} }; } } },
      } },
      syncFacade: { collectionFreshness: () => ({ ready: true }),
        subscribeCollectionReadiness: (name, callback) => { if (name === 'ctox_queue_tasks') reload = callback; return () => {}; } },
    });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(state.crewWorkload.get('luma'), 1);
    hold = true;
    const pending = reload();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(typeof finish, 'function', 'task storage read is actually pending');
    harness = { ...harness, service_running: false };
    await nativeChanged();
    assert.equal(state.crewWorkload.size, 0, 'current native truth is not serialized behind task storage');
    finish([task('a', { crew_member_id: 'luma' })]);
    await pending;
    assert.equal(state.crewWorkload.size, 0, 'the late older task result cannot restore execution');
  } finally {
    dispose?.();
    finish?.([]);
    globalThis.window = previousWindow;
    globalThis.document = previousDocument;
  }
});

test('the actual Shell freshness subscription follows current pull confirmation and releases a closed host', () => {
  const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
  const definition = appSource.match(/^function createLiveSyncFacade\([^]*?^\}/m)?.[0];
  assert.ok(definition, 'exercise the actual module facade');
  const listeners = new Set();
  const host = { isConnected: true };
  let snapshot = { state: 'catching-up', ready: false };
  const create = runInNewContext(definition + '\ncreateLiveSyncFacade', {
    state: { sync: {} },
    currentCollectionFreshness: () => snapshot,
    window: { addEventListener: (_name, listener) => listeners.add(listener),
      removeEventListener: (_name, listener) => listeners.delete(listener) },
  });
  const facade = create({ host });
  const states = [];
  facade.subscribeCollectionFreshness('ctox_harness_status', value => states.push(value.state));
  assert.deepEqual(states, ['catching-up']);
  snapshot = { state: 'live', ready: true };
  [...listeners].forEach(listener => listener());
  [...listeners].forEach(listener => listener());
  assert.deepEqual(states, ['catching-up', 'live'], 'unchanged diagnostics do not requery the task queue');
  snapshot = { state: 'offline-pending', ready: false };
  [...listeners].forEach(listener => listener());
  assert.deepEqual(states, ['catching-up', 'live', 'offline-pending']);
  host.isConnected = false;
  [...listeners].forEach(listener => listener());
  assert.equal(listeners.size, 0, 'owner teardown releases the actual diagnostic listener');
});

test('app presence joins the current native attempt and worker before attributing a Luma member', async () => {
  const previousWindow = globalThis.window;
  const previousDocument = globalThis.document;
  const state = { crewMembers: [{ id: 'luma', name: 'Luma' }, { id: 'old', name: 'Previous member' }] };
  let harness = liveHarness(['a', 'b']);
  let rows = [task('a', { crew_member_id: 'luma' }),
    task('b', { crew_member_id: 'old', lease_worker_id: 'own-boot:previous-worker' })];
  let nativeChanged;
  let reload;
  globalThis.window = { setTimeout: callback => callback, clearTimeout() {} };
  globalThis.document = { querySelector: () => null, querySelectorAll: () => [] };
  let dispose;
  try {
    dispose = __businessChatTestInternals.wireCrewAppPresence({ state,
      db: { raw: {
        ctox_queue_tasks: { find: () => ({ exec: async () => rows }) },
        ctox_harness_status: { find: () => ({ exec: async () => [harness] }),
          $: { subscribe: callback => { nativeChanged = callback; return { unsubscribe() {} }; } } },
      } },
      syncFacade: { collectionFreshness: () => ({ ready: true }),
        subscribeCollectionReadiness: (name, callback) => { if (name === 'ctox_queue_tasks') reload = callback; return () => {}; } },
    });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(state.crewWorkload.get('luma'), 1);
    assert.equal(state.crewWorkload.has('old'), false, 'previous worker assignment is not current execution');
    harness.current_queue_workers[0] = { ...harness.current_queue_workers[0],
      attempt: 2, lease_worker_id: 'own-boot:worker-a-2' };
    await nativeChanged();
    assert.equal(state.crewWorkload.size, 0, 'a new native attempt immediately retires the earlier queue projection');
    rows = [task('a', { crew_member_id: 'luma', attempt: 2, lease_worker_id: 'own-boot:worker-a-2' })];
    await reload();
    assert.equal(state.crewWorkload.get('luma'), 1, 'matching current queue projection restores execution');
    assert.equal(state.crewWorkload.has('old'), false);
    harness = { ...harness, current_queue_workers: 'invalid' };
    await nativeChanged();
    assert.equal(state.crewWorkload.size, 0, 'an invalid native worker projection retires cached attribution');
  } finally {
    dispose?.();
    globalThis.window = previousWindow;
    globalThis.document = previousDocument;
  }
});
