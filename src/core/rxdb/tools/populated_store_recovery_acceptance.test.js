'use strict';

// Unit checks for the host acceptance oracle, not native recovery acceptance.
// Execute the actual polling function; fake only the CLI/SQLite observations.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, 'populated_store_recovery_acceptance.js'), 'utf8');
const start = source.indexOf('async function waitForAcceptedWrites(');
const end = source.indexOf('\nfunction table(', start);
assert(start >= 0 && end > start, 'acceptance polling function must exist');
const commandId = 'cmd-post-cutover-001';
const accepted = { ok: true, status: 'accepted', command_id: commandId, task_id: 'queue::admitted' };

function harness(response, tasks = []) {
  let clock = 0;
  const calls = { dispatch: 0, reads: 0 };
  const wait = new vm.Script(`${source.slice(start, end)}\nwaitForAcceptedWrites;`).runInNewContext({
    POST_CUTOVER_COMMAND_ID: commandId,
    Date: { now: () => clock },
    delay: async (ms) => { clock += ms; },
    table: (collection, version) => `${collection}_v${version}`,
    runCtox: () => { calls.dispatch += 1; return response; },
    sqliteRows: (sql) => {
      calls.reads += 1;
      return sql.includes('FROM business_commands_')
        ? [{ id: commandId, command_id: commandId, deleted: 0, status: 'accepted' }]
        : tasks;
    },
  });
  return { wait, calls };
}

for (const [name, response] of [
  ['native rejection', { ...accepted, ok: false, status: 'failed' }],
  ['wrong command identity', { ...accepted, command_id: 'foreign-command' }],
  ['missing queue identity', { ...accepted, task_id: '' }],
  ['empty response', null],
]) {
  test(`admission oracle fails immediately on ${name}`, async () => {
    const { wait, calls } = harness(response);
    await assert.rejects(wait(1000), /native command admission failed/);
    assert.equal(calls.dispatch, 1);
    assert.equal(calls.reads, 0, 'rejected admission cannot be rescued by database rows');
  });
}

test('a different task for the command cannot satisfy acceptance', async () => {
  const { wait, calls } = harness(accepted, [
    { id: 'queue::foreign', command_id: commandId, deleted: 0 },
  ]);
  await assert.rejects(wait(1000), /serve did not persist accepted command\/queue identity/);
  assert.equal(calls.dispatch, 1, 'accepted commands must not be redispatched while awaiting projection');
});

test('acceptance selects the admitted live task and retains its native receipt', async () => {
  const { wait, calls } = harness(accepted, [
    { id: 'queue::foreign', command_id: commandId, deleted: 0 },
    { id: accepted.task_id, command_id: commandId, deleted: 1 },
    { id: accepted.task_id, command_id: commandId, deleted: 0 },
  ]);
  const result = await wait(1000);
  assert.equal(result.task.id, accepted.task_id);
  assert.equal(result.task.deleted, 0);
  assert.equal(result.admission, accepted);
  assert.equal(calls.dispatch, 1);
});
