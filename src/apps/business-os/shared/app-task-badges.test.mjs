import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { crewAppTasksFromTasks, crewLiveKeys, __businessChatTestInternals } from './business-chat.js';

const task = (id, fields = {}) => ({
  id, title: 'Task ' + id, status: 'running', module: 'ctox', source_module: 'documents',
  updated_at_ms: 1, ...fields,
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

test('the actual presence query reads active statuses before applying its bounded window', async () => {
  const source = readFileSync(new URL('./business-chat.js', import.meta.url), 'utf8');
  const body = source.match(/^async function loadCrewAppTasks\(db\) \{[^]*?^\}/m)?.[0];
  assert.ok(body);
  let query;
  const load = runInNewContext(body + '\nloadCrewAppTasks', {
    CREW_APP_PRESENCE_STATUSES: new Set(['running', 'leased', 'review', 'drafting']),
    CREW_APP_PRESENCE_TASK_LIMIT: 200, console,
  });
  await load({ raw: { ctox_queue_tasks: { find: options => {
    query = options; return { exec: async () => [] };
  } } } });
  assert.deepEqual([...query.selector.status.$in], ['running', 'leased', 'review', 'drafting']);
  assert.equal(query.limit, 200);
});

test('a stopped native harness retires workload even while the task query is cancelled', async () => {
  const previousWindow = globalThis.window;
  const previousDocument = globalThis.document;
  const callbacks = new Map();
  let cancelled = false;
  let harness = { id: 'harness', service_running: true, active_task_ids: ['a'] };
  const state = { crewMembers: [{ id: 'luma', name: 'Luma' }] };
  globalThis.window = { setTimeout: callback => callback, clearTimeout() {} };
  globalThis.document = { querySelector: () => null, querySelectorAll: () => [] };
  let dispose;
  try {
    dispose = __businessChatTestInternals.wireCrewAppPresence({
      state,
      db: { raw: {
        ctox_queue_tasks: { find: () => ({ exec: async () => {
          if (cancelled) throw Object.assign(new Error('retired query'), { code: 'QUERY_CANCELLED' });
          return [task('a', { crew_member_id: 'luma' })];
        } }) },
        ctox_harness_status: { find: () => ({ exec: async () => [harness] }) },
      } },
      syncFacade: { subscribeCollectionReadiness: (name, callback) => {
        callbacks.set(name, callback); return () => callbacks.delete(name);
      } },
    });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(state.crewWorkload.get('luma'), 1);
    cancelled = true;
    harness = { ...harness, service_running: false };
    await callbacks.get('ctox_queue_tasks')();
    assert.equal(state.crewWorkload.size, 0);
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
          { id: 'harness', service_running: true, active_task_ids: [] },
        ] }) },
      } },
      syncFacade: { subscribeCollectionReadiness: (name, callback) => {
        callbacks.set(name, callback); return () => callbacks.delete(name);
      } },
    });
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
