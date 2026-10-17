import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import pause from '../ctox.queue.pause.js';
import capacity from '../ctox.queue.capacity.js';
import abortTurn from '../ctox.queue.abort_turn.js';
import release from '../ctox.queue.release.js';
import block from '../ctox.queue.block.js';
import retry from '../ctox.queue.retry.js';

function queueWorld() {
  const tasks = {
    t_pending: { message_key: 't_pending', route_status: 'pending' },
    t_failed: { message_key: 't_failed', route_status: 'failed' },
    t_leased: { message_key: 't_leased', route_status: 'leased' },
  };
  const controls = [];
  const { ctx, state } = makeCtx({
    caps: {
      agent: {
        flow: { record: () => ({}) },
        queue: {
          task: { get: ({ task_id }) => tasks[task_id] ?? null },
          control: (req) => {
            controls.push(req);
            if (req.action === 'capacity') return { workers: req.value };
            return req.task_id ? { ...tasks[req.task_id], route_status: req.value.route_status } : {};
          },
        },
      },
    },
  });
  return { ctx, state, controls };
}

test('ctox.queue.pause stores {paused, reason} and drops reason when unpaused', async () => {
  const { ctx, controls, state } = queueWorld();
  assert.deepEqual(await pause(makeCommand('ctox.queue.pause', { paused: true, reason: ' Wartung ' }), ctx), {
    ok: true,
    pause: { paused: true, reason: 'Wartung' },
  });
  assert.deepEqual(await pause(makeCommand('ctox.queue.pause', { paused: false, reason: 'x' }), ctx), {
    ok: true,
    pause: { paused: false, reason: null },
  });
  assert.equal(controls.length, 2);
  await rejectsWithCode(pause(makeCommand('ctox.queue.pause', { paused: 'yes' }), ctx), 'invalid');
  assert.equal(state.capCalls.some((c) => c.path === 'agent.queue.projection.refresh'), false, 'v4: Projektion ist live');
});

test('ctox.queue.capacity accepts 1..8', async () => {
  const { ctx } = queueWorld();
  assert.deepEqual(await capacity(makeCommand('ctox.queue.capacity', { workers: 3 }), ctx), { workers: 3, ok: true });
  await rejectsWithCode(capacity(makeCommand('ctox.queue.capacity', { workers: 9 }), ctx), 'invalid');
  await rejectsWithCode(capacity(makeCommand('ctox.queue.capacity', { workers: 2.5 }), ctx), 'invalid');
});

test('ctox.queue.abort_turn is unsupported and needs task_id', async () => {
  const { ctx } = queueWorld();
  const out = await abortTurn(makeCommand('ctox.queue.abort_turn', { task_id: 't_leased' }), ctx);
  assert.equal(out.code, 'unsupported');
  await rejectsWithCode(abortTurn(makeCommand('ctox.queue.abort_turn', {}), ctx), 'invalid');
});

test('ctox.queue.release/block/retry transitions', async () => {
  const { ctx, controls, state } = queueWorld();
  const r = await release(makeCommand('ctox.queue.release', { task_id: 't_pending', priority: 'high', note: 'los' }), ctx);
  assert.equal(r.ok, true);
  assert.deepEqual(controls[0].value, { route_status: 'pending', priority: 'high', clear_note: false, status_note: 'los', reset_attempts: false });
  await block(makeCommand('ctox.queue.block', { task_id: 't_pending', reason: 'warten' }), ctx);
  assert.equal(controls[1].value.route_status, 'blocked');
  assert.equal(controls[1].value.priority, null);
  await rejectsWithCode(block(makeCommand('ctox.queue.block', { task_id: 't_pending' }), ctx), 'invalid');
  await retry(makeCommand('ctox.queue.retry', { task_id: 't_failed' }), ctx);
  assert.equal(controls.at(-1).value.clear_note, true);
  await rejectsWithCode(retry(makeCommand('ctox.queue.retry', { task_id: 't_pending' }), ctx), 'conflict');
  await rejectsWithCode(release(makeCommand('ctox.queue.release', { task_id: 't_leased' }), ctx), 'conflict');
  await rejectsWithCode(release(makeCommand('ctox.queue.release', { task_id: 'missing' }), ctx), 'not_found');
  await rejectsWithCode(release(makeCommand('ctox.queue.release', { task_id: 't_pending', note: 7 }), ctx), 'invalid');
  const results = state.capCalls.filter((c) => c.path === 'agent.flow.record' && c.args[0].metadata.stage === 'result');
  assert.equal(results.at(-1).args[0].metadata.result.ok, false);
});
