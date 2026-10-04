import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const names = [
  'waitForSyncBridgeReady', 'waitForProjectedWorkjetComputer',
  'waitForProjectedWorkjetProject', 'waitForProjectedWorkjetWorkingCopy',
  'waitForProjectedWorkjetSession',
];
const definitions = names.map(name => {
  const body = source.match(new RegExp('^(?:async )?function ' + name + '\\([^]*?^\\}', 'm'))?.[0];
  assert.ok(body, 'exercise the actual ' + name);
  return body;
}).join('\n');

const cases = [
  { kind: 'computer', name: names[1], args: bridge => ['computer-a', 'owner-a', 'assigned', bridge, 5000],
    row: { id: 'computer-a', owner_user_id: 'owner-a', status: 'assigned' } },
  { kind: 'project', name: names[2], args: bridge => ['project-a', 'Project A', 'owner-a', bridge, 5000],
    row: { id: 'project-a', owner_user_id: 'owner-a', name: 'Project A', status: 'active' } },
  { kind: 'working copy', name: names[3], args: bridge => [
      'project-a', { computerId: 'computer-a', path: '/workspace' }, 'owner-a', bridge, 5000,
    ], row: { id: 'copy-a', owner_user_id: 'owner-a', path: '/workspace' } },
  { kind: 'session', name: names[4], args: bridge => ['session-a', 'owner-a', bridge, 5000],
    row: { id: 'session-a', owner_user_id: 'owner-a' } },
];

function fixture(item, clock = {}) {
  let reads = 0;
  const read = async () => { reads++; return item.row; };
  const collection = {
    findOne: () => ({ exec: read }),
    find: () => ({ exec: async () => [await read()] }),
  };
  const context = {
    state: { db: { collection: () => collection } },
    boundedWorkjetComputerResult: row => row,
    boundedWorkjetProjectResult: row => row,
    boundedWorkjetWorkingCopyResult: row => ({ ...row, ownerUserId: row.owner_user_id }),
    publicWorkjetWorkingCopyResult: row => row,
    boundedWorkjetSessionResult: row => row,
    setTimeout, clearTimeout, window: { setTimeout }, ...clock,
  };
  const wait = runInNewContext(definitions + '\n' + item.name, context);
  return { wait, reads: () => reads };
}

for (const item of cases) {
  for (const method of ['awaitInSync', 'awaitInitialReplication']) {
    test(item.kind + ' waits for bridge.state.' + method + ' before reading the projection',
      { timeout: 1000 }, async () => {
        let release;
        let calls = 0;
        const pending = new Promise(resolve => { release = resolve; });
        const bridge = {
          awaitInSync() { throw new Error('outer bridge is not the replication state'); },
          state: { [method]() { calls++; return pending; } },
        };
        const f = fixture(item);
        const result = f.wait(...item.args(bridge));
        await new Promise(resolve => setImmediate(resolve));
        assert.equal(calls, 1);
        assert.equal(f.reads(), 0, 'the nested replication state is still pending');
        release();
        assert.equal((await result).id, item.row.id);
        assert.equal(f.reads(), 1);
      });
  }
}

test('a pending projection bridge uses the remaining deadline and releases its timer', async () => {
  let now = 0;
  const budgets = [];
  const cleared = [];
  const f = fixture({ ...cases[0], row: null }, {
    Date: { now: () => now },
    setTimeout(fn, ms) {
      budgets.push(ms);
      queueMicrotask(() => { now += ms; fn(); });
      return budgets.length;
    },
    clearTimeout: id => cleared.push(id),
    window: { setTimeout(fn, ms) { now += ms; fn(); } },
  });
  const bridge = { state: { awaitInSync: () => new Promise(() => {}) } };
  await assert.rejects(f.wait('computer-a', 'owner-a', 'assigned', bridge, 7),
    error => error.code === 'workjet_computer_projection_timeout');
  assert.deepEqual(budgets, [7]);
  assert.deepEqual(cleared, [1]);
});
