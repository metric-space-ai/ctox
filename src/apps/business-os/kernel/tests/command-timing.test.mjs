import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStubKernel } from '../kernel.mjs';

async function until(pred) {
  const deadline = Date.now() + 5000;
  while (!pred()) { if (Date.now() > deadline) assert.fail('timing delivery deadline'); await new Promise(r => setTimeout(r, 2)); }
}
let serial = 0;
async function setup(t, opts = {}) {
  const k = createStubKernel({instanceId:`wj5-${process.pid}-${++serial}`, userId:'u', multiTab:false, latencyMs:1, ...opts});
  t.after(() => k.close()); await k.ready; return k;
}
test('timings distinguish durable queue, dispatch, receipt and terminal completion', async t => {
  let finish; const gate = new Promise(r => {finish = r;});
  t.after(() => finish());
  const events = [];
  const k = await setup(t, {diagnostics:{onCommandTiming(value){events.push(value); if (value.sent === null) return Promise.reject(Error('broken async observer')); throw Error('broken observer');}}});
  await k.sim.setLink({state:'offline'});
  k.sim.onCommand('notes.timing', async () => {await gate; return {ok:true};});
  const api = k.appApi('notes','1');
  const before = Date.now(), command = api.command('notes.timing', {secret:'must not enter metrics'});
  await command.local;
  const queued = k.metrics.commands.find(row => row.command_id === command.id);
  assert.ok(queued, 'committed command absent from timing metrics');
  assert.ok(queued.queued >= before && queued.queued <= Date.now());
  assert.deepEqual([queued.sent,queued.acked,queued.completed],[null,null,null]);
  assert.deepEqual(k.metrics.get().commands, k.metrics.commands);
  await k.sim.setLink({state:'online'});
  assert.equal((await command.result).status,'accepted');
  const acked = k.metrics.commands.find(row => row.command_id === command.id);
  assert.ok(acked.sent >= acked.queued);
  assert.ok(acked.acked >= acked.sent);
  assert.equal(acked.completed,null,'Core receipt is not command completion');
  finish();
  await until(() => k.metrics.commands.find(row => row.command_id === command.id)?.completed !== null);
  const done = k.metrics.commands.find(row => row.command_id === command.id);
  assert.equal(command.status.get().status,'done');
  assert.ok(done.completed >= done.acked);
  assert.ok(Object.isFrozen(k.metrics.commands) && Object.isFrozen(done));
  assert.deepEqual(Object.keys(done).sort(), ['command_id','queued','sent','acked','completed'].sort());
  assert.ok(events.some(row => row.sent !== null) && events.some(row => row.acked !== null) && events.some(row => row.completed !== null));
  assert.ok(!JSON.stringify(events).includes('must not enter metrics'));
  await until(() => Date.now() > done.completed);
  await k.sim.push('business_commands', [api.get('business_commands', command.id).get()]);
  await k.sim.settle();
  assert.deepEqual(k.metrics.commands.find(row => row.command_id === command.id),done,'replay must not retime a completed command');
});
test('timing ring retains only the latest 500 queued commands', async t => {
  const k = await setup(t); await k.sim.setLink({state:'offline'});
  const api = k.appApi('notes','1'), ids = [];
  for (let i=0;i<503;i++) { const h = api.command('notes.wait',{i}); ids.push(h.id); await h.local; }
  assert.equal(k.metrics.commands.length,500);
  assert.deepEqual(k.metrics.commands.map(row=>row.command_id),ids.slice(-500));
});
