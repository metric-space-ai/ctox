import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import update from '../ctox.task.update.js';
import del from '../ctox.task.delete.js';

const HANDLER_DIR = join(dirname(fileURLToPath(import.meta.url)), '..');

// Core-Wahrheit (route_status) liegt in der Attrappe in der Projektion selbst;
// task.get liest sie von dort, damit die Statuswirkung von task.update sichtbar wird.
function taskWorld(routeStatus = 'pending') {
  const { ctx, state } = makeCtx({
    collections: {
      business_users: [{ id: 'user-chef', display_name: 'Chefin', role: 'chef' }],
      ctox_queue_tasks: [{ id: 'task-1', command_id: 'cmd-9', title: 'Alt', route_status: routeStatus }],
      business_commands: [
        { id: 'cmd-9', status: 'queued' },
        { id: 'cmd-x', status: 'queued' },
      ],
    },
    caps: {
      agent: {
        queue: {
          task: {
            get: ({ task_id }) => {
              const doc = state.collections.get('ctox_queue_tasks').get(task_id);
              return doc ? { message_key: task_id, route_status: doc.route_status } : null;
            },
            for_command: ({ command_id }) => (command_id === 'cmd-9' ? { task_id: 'task-1' } : null),
          },
        },
      },
    },
  });
  const calls = (name) => state.capCalls.filter((c) => c.path === `agent.queue.task.${name}`).map((c) => c.args[0]);
  return { ctx, state, calls };
}

function assertNoStoreWrites(state) {
  assert.deepEqual(state.forbiddenWrites, []);
  assert.deepEqual(state.writes, [], 'task handlers must not write the store directly');
}

test('attrappe: ctox_queue_tasks is read-only for put/patch/remove, also inside transaction', async () => {
  const { ctx, state } = taskWorld();
  await rejectsWithCode(ctx.put('ctox_queue_tasks', { id: 'task-2' }), 'forbidden');
  await rejectsWithCode(ctx.patch('ctox_queue_tasks', 'task-1', { title: 'x' }), 'forbidden');
  await rejectsWithCode(ctx.remove('ctox_queue_tasks', 'task-1'), 'forbidden');
  await rejectsWithCode(ctx.transaction((tx) => tx.remove('ctox_queue_tasks', 'task-1')), 'forbidden');
  assert.equal(state.forbiddenWrites.length, 4);
  assert.equal(state.collections.get('ctox_queue_tasks').get('task-1').title, 'Alt');
  // Lesen bleibt erlaubt.
  assert.equal((await ctx.get('ctox_queue_tasks', 'task-1')).command_id, 'cmd-9');
  assert.equal((await ctx.read('ctox_queue_tasks', { selector: { command_id: 'cmd-9' } })).length, 1);
});

test('ctox.task.update trims, resolves queue-/command ids, labels editor – via task.update only', async () => {
  const { ctx, state, calls } = taskWorld();
  const out = await update(makeCommand('ctox.task.update', { task_id: 'queue-task-1', title: '  Neu ' }), ctx);
  assert.equal(out.ok, true);
  assert.equal(out.task.id, 'task-1');
  assert.equal(out.task.title, 'Neu');
  assert.equal(out.task.status_note, 'business-os: edited by Chefin (chef)');
  assert.deepEqual(calls('update')[0], {
    task_id: 'task-1',
    patch: { title: 'Neu', prompt: null, priority: null, status_note: 'business-os: edited by Chefin (chef)' },
  });
  // Core hat die Projektion neu geschrieben.
  assert.equal(state.collections.get('ctox_queue_tasks').get('task-1').title, 'Neu');
  await update(makeCommand('ctox.task.update', { task_id: 'cmd-9', priority: 'low' }), ctx);
  assert.equal(calls('update')[1].task_id, 'task-1');
  assert.equal(state.collections.get('ctox_queue_tasks').get('task-1').priority, 'low');
  assertNoStoreWrites(state);
});

test('ctox.task.update rules still reject before the capability is called', async () => {
  const { ctx, calls } = taskWorld();
  await rejectsWithCode(update(makeCommand('ctox.task.update', { task_id: 'task-1', title: '  ' }), ctx), 'invalid');
  await rejectsWithCode(update(makeCommand('ctox.task.update', { task_id: ' ', title: 'x' }), ctx), 'invalid');
  await rejectsWithCode(update(makeCommand('ctox.task.update', { title: 'x' }), ctx), 'invalid');
  await rejectsWithCode(update(makeCommand('ctox.task.update', { task_id: 'task-1', title: 7 }), ctx), 'invalid');
  assert.equal(calls('update').length, 0);
});

test('ctox.task.update surfaces core not_found', async () => {
  const { ctx } = taskWorld();
  await rejectsWithCode(update(makeCommand('ctox.task.update', { task_id: 'nope', title: 'x' }), ctx), 'not_found');
});

test('ctox.task.delete cancels live task via task.update(route_status), then one task.delete (projection + command)', async () => {
  const { ctx, state, calls } = taskWorld('pending');
  const out = await del(makeCommand('ctox.task.delete', { task_id: 'task-1' }), ctx);
  assert.deepEqual(out, { ok: true, task_id: 'task-1', command_id: 'cmd-9', status: 'cancelled' });
  assert.deepEqual(calls('update'), [
    { task_id: 'task-1', patch: { route_status: 'cancelled', status_note: 'business-os: deleted by Chefin (chef)' } },
  ]);
  assert.deepEqual(calls('delete'), [{ task_id: 'task-1', command_id: 'cmd-9' }]);
  const order = state.capCalls.map((c) => c.path).filter((p) => /task\.(update|delete)$/.test(p));
  assert.deepEqual(order, ['agent.queue.task.update', 'agent.queue.task.delete']);
  assert.equal(state.collections.get('ctox_queue_tasks').has('task-1'), false);
  assert.equal(state.collections.get('business_commands').has('cmd-9'), false);
  assert.ok(state.tombstones.some((t) => t.collection === 'business_commands' && t.id === 'cmd-9' && t.status === 'cancelled'));
  assertNoStoreWrites(state);
});

test('ctox.task.delete on terminal task does not change status; explicit command_id wins', async () => {
  const { ctx, state, calls } = taskWorld('completed');
  const out = await del(makeCommand('ctox.task.delete', { task_id: 'task-1', command_id: ' cmd-x ' }), ctx);
  assert.equal(calls('update').length, 0);
  assert.deepEqual(calls('delete'), [{ task_id: 'task-1', command_id: 'cmd-x' }]);
  assert.equal(out.command_id, 'cmd-x');
  assert.equal(state.collections.get('business_commands').has('cmd-x'), false);
  assert.equal(state.collections.get('business_commands').has('cmd-9'), true);
  assertNoStoreWrites(state);
});

test('ctox.task.delete of unknown task skips cancel and lets the core tombstone idempotently', async () => {
  const { ctx, state, calls } = taskWorld();
  const out = await del(makeCommand('ctox.task.delete', { task_id: 'ghost' }), ctx);
  assert.equal(calls('update').length, 0);
  assert.deepEqual(calls('delete'), [{ task_id: 'ghost' }]);
  assert.deepEqual(out, { ok: true, task_id: 'ghost', command_id: null, status: 'cancelled' });
  assertNoStoreWrites(state);
});

test('ctox.task.delete payload rules reject before any capability write', async () => {
  const { ctx, calls } = taskWorld();
  await rejectsWithCode(del(makeCommand('ctox.task.delete', {}), ctx), 'invalid');
  await rejectsWithCode(del(makeCommand('ctox.task.delete', { task_id: 'task-1', command_id: 5 }), ctx), 'invalid');
  await rejectsWithCode(del(makeCommand('ctox.task.delete', { task_id: '  ' }), ctx), 'invalid');
  assert.equal(calls('update').length + calls('delete').length, 0);
});

test('ctox.task.delete propagates core failure of the cancel update and does not delete', async () => {
  const { ctx, state, calls } = taskWorld('leased');
  // Core meldet einen Konflikt (z. B. Lease wechselte zwischen get und task.update).
  const failing = makeCtx({
    collections: { ctox_queue_tasks: [{ id: 'task-1', command_id: 'cmd-9', route_status: 'leased' }] },
    caps: {
      agent: {
        queue: {
          task: {
            get: () => ({ message_key: 'task-1', route_status: 'leased' }),
            update: () => Promise.reject({ code: 'conflict', message: 'lease changed' }),
          },
        },
      },
    },
  });
  await rejectsWithCode(del(makeCommand('ctox.task.delete', { task_id: 'task-1' }), failing.ctx), 'conflict');
  assert.equal(failing.state.capCalls.some((c) => c.path === 'agent.queue.task.delete'), false);
  assert.equal(failing.state.collections.get('ctox_queue_tasks').has('task-1'), true);
  // Gegenprobe: leased darf storniert und gelöscht werden.
  await del(makeCommand('ctox.task.delete', { task_id: 'task-1' }), ctx);
  assert.equal(calls('update')[0].patch.route_status, 'cancelled');
  assert.equal(state.collections.get('ctox_queue_tasks').has('task-1'), false);
});

test('core task.update status contract: invalid status, leased guard, command mirrored', async () => {
  const { ctx, state } = taskWorld('leased');
  await rejectsWithCode(ctx.cap.agent.queue.task.update({ task_id: 'task-1', patch: { route_status: 'leased' } }), 'invalid');
  await rejectsWithCode(ctx.cap.agent.queue.task.update({ task_id: 'task-1', patch: { route_status: 'pending' } }), 'conflict');
  await rejectsWithCode(ctx.cap.agent.queue.task.update({ task_id: 'nope', patch: { route_status: 'cancelled' } }), 'not_found');
  const doc = await ctx.cap.agent.queue.task.update({ task_id: 'task-1', patch: { route_status: 'cancelled', status_note: ' weg ' } });
  assert.equal(doc.route_status, 'cancelled');
  assert.equal(doc.status_note, 'weg');
  assert.equal(state.collections.get('business_commands').get('cmd-9').status, 'cancelled');
});

test('no handler source writes ctox_queue_tasks directly', () => {
  const files = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === 'tests') continue;
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.js')) files.push(path);
    }
  };
  walk(HANDLER_DIR);
  assert.ok(files.length >= 15);
  const write = /\b(put|patch|remove|upsert|bulkInsert|bulkRemove|insert)\s*\(\s*['"`]ctox_queue_tasks/;
  for (const file of files) {
    assert.equal(write.test(readFileSync(file, 'utf8')), false, `${file} writes ctox_queue_tasks`);
  }
});
