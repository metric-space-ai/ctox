// K-1h: §16.44-Ergänzung – draft.info().rev nach Neuanlage, draft.write → {local, result{rev}}, Agent auf dem vollen Baum,
// Konflikt-Patch mit 3 Kontextzeilen.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStubKernel } from '../testkit.mjs';

let n = 0;
async function mk(t, o = {}) {
  const k = createStubKernel({ locks: testLocks, instanceId: `k1h${process.pid}-${++n}`, userId: 'u1', latencyMs: 2, actor: { permissions: ['apps.source.view', 'apps.source.edit'] }, ...o });
  t.after(() => k.close());
  await k.ready;
  return k;
}
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(pred, ms = 4000, what = '') { const t0 = Date.now(); while (!(await pred())) { if (Date.now() - t0 > ms) throw new Error(`until ${what}`); await tick(5); } }
const BASE = 'l1\nl2\nl3\nl4\nl5\n';
const MOUNT = 'export function mount(){ return { unmount(){} }; }';
const turnOf = async (k, id) => (await k.sim.coreRows('coding_turns')).find((x) => x.command_id === id);
const end = async (h) => { await until(() => ['done', 'failed', 'cancelled'].includes(h.status.get().status), 4000, 'Ende'); return h.status.get(); };

test('(2)+(3) draft.write → {local, result{rev}}; info().rev nach Neuanlage nie die Revision der gelöschten Zeile', async (t) => {
  const k = await mk(t);
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': MOUNT } });
  const src = k.source('crm');
  const w1 = src.draft.write('a.txt', 'A');
  assert.deepEqual(Object.keys(w1).sort(), ['local', 'result']);
  assert.ok(w1 instanceof Promise, 'weiterhin ein Promise (await/.catch)');
  await w1; // weiterhin awaitbar (lokaler Commit)
  const r1 = await w1.result;
  assert.equal(r1.status, 'accepted');
  assert.equal((await src.draft.info()).rev, r1.rev, 'result.rev = bestätigte Revision');
  await src.draft.discard();
  await k.sim.settle();
  const w2 = src.draft.write('b.txt', 'B'); // Neuanlage
  await w2.local;
  const early = (await src.draft.info()).rev;
  assert.equal(early, null, 'vor der Bestätigung keine Revision – nie die der gelöschten Zeile');
  const r2 = await w2.result;
  assert.equal(r2.status, 'accepted');
  assert.notEqual(r2.rev, r1.rev);
  await k.sim.settle();
  assert.equal((await src.draft.info()).rev, r2.rev);
  k.sim.setPermissions(['apps.source.view']);
  const denied = src.draft.write('c.txt', 'C');
  await assert.rejects(denied.local, (e) => e.code === 'forbidden');
  assert.equal((await denied.result).code, 'forbidden');
});

test('(1) Agent arbeitet auf dem vollen Baum: frischer Entwurf, Prompt „index.js“ ändert index.js der Basis', async (t) => {
  const k = await mk(t, { codingDelayMs: 20 });
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': BASE, 'z.md': 'z' } });
  const src = k.source('crm');
  const w = src.draft.write('README.md', '# neu'); // frischer Entwurf ohne index.js
  const { rev } = await w.result;
  const h = k.appApi('crm', '1').command('ctox.coding.turn', { app_id: 'crm', draft_id: 'u1/crm', base_draft_rev: rev, prompt: 'index.js', turn_id: 'turn-f' });
  assert.equal((await end(h)).status, 'done');
  await k.sim.settle();
  const info = await src.draft.info();
  assert.equal(info.files['index.js'], `${BASE}// coding-agent turn-f: index.js\n`);
  assert.equal(info.files['NOTES.md'], undefined, 'kein NOTES.md');
  assert.deepEqual((await turnOf(k, h.id)).changed_files, ['index.js']);
});

test('(4) Konflikt-Patch = unified diff mit 3 Kontextzeilen je Hunk (Basis = base_draft_rev)', async (t) => {
  const k = await mk(t, { codingDelayMs: 120 });
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': BASE } });
  const src = k.source('crm');
  const { rev } = await src.draft.write('README.md', '# x').result;
  const h = k.appApi('crm', '1').command('ctox.coding.turn', { app_id: 'crm', draft_id: 'u1/crm', base_draft_rev: rev, prompt: 'index.js', turn_id: 'turn-p' });
  await until(async () => (await turnOf(k, h.id))?.state === 'running', 4000, 'running');
  await src.draft.write('index.js', 'nutzer\n').result;
  assert.equal((await end(h)).status, 'failed');
  const turn = await turnOf(k, h.id);
  const patch = await (await k.appApi('crm', '1').files.get(turn.patch.ref)).text();
  assert.equal(patch, '--- a/index.js\n+++ b/index.js\n@@ -3,3 +3,4 @@\n l3\n l4\n l5\n+// coding-agent turn-p: index.js\n');
});
