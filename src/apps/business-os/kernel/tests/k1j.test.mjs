// K-1j: §16.50 – Lib-Miss aus jedem Client (auch Worker) bedient, Antwort mit Datei (sha256-geprüft) oder Grund;
// binding.workerUrl für gleich-ursprüngliche Worker unter der gebundenen Lib-Wurzel.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createStubKernel } from '../kernel.mjs';

let n = 0;
async function mk(t, o = {}) {
  const k = createStubKernel({ locks: testLocks, instanceId: `k1j${process.pid}-${++n}`, userId: 'u1', latencyMs: 2, ...o });
  t.after(() => k.close());
  await k.ready;
  return k;
}
const sha = (x) => createHash('sha256').update(x).digest('hex');
const WORKER = 'self.onmessage = () => {};\n';
const miss = async (k, extra) => {
  const replies = [];
  await k.libs.onSwMessage({ type: 'ctox:lib-miss', lib: 'monaco', major: '0', path: 'vs/editor.worker.js', ver: '0.3', reason: 'artifact', ...extra }, { postMessage: (m) => replies.push(m) });
  return replies[0];
};

test('§16.50: Lib-Miss eines Workers wird bedient – Antwort an den SW mit der Datei; falsche sha → Fehler statt Auslieferung', async (t) => {
  const instanceId = `k1j-tabs-${process.pid}-${++n}`;
  const A = await mk(t, { instanceId });
  const B = await mk(t, { instanceId }); // Leser-Fenster: bedient über den Leiter
  await A.sim.publishAppVersion('lib:monaco', '0.3', { files: { 'vs/editor.worker.js': WORKER } });
  const bad = await miss(B, { sha: sha('etwas anderes'), client_id: 'worker-1', client_type: 'worker' });
  assert.deepEqual([bad.type, bad.reason, bad.body], ['ctox:lib-abort', 'hash_mismatch', undefined]);
  assert.equal((await A.sim.artifactRows()).some((r) => r.app_id === 'lib:monaco'), false, 'nichts abgelegt');
  const ok = await miss(B, { sha: sha(WORKER), client_id: 'worker-1', client_type: 'worker' });
  assert.deepEqual([ok.type, ok.ver, ok.path, ok.sha, ok.mime, ok.size], ['ctox:lib-ready', '0.3', 'vs/editor.worker.js', sha(WORKER), 'text/javascript', WORKER.length]);
  assert.equal(new TextDecoder().decode(ok.body), WORKER, 'Datei in der Antwort');
  const viaLeader = await miss(A, { sha: null, client_type: 'worker' });
  assert.equal(viaLeader.type, 'ctox:lib-ready');
});

test('§16.50: binding.workerUrl → gleich-ursprüngliche URL unter der gebundenen Lib-Wurzel, nie data:/blob:', async (t) => {
  const k = await mk(t);
  await k.sim.publishAppVersion('lib:monaco', '0.3', { files: { 'vs/editor.worker.js': WORKER } });
  await k.sim.publishAppVersion('lib:ctox-office', '1.4', { files: { 'o.js': 'o' } });
  await k.sim.publishAppVersion('shell', '5', { files: { 'index.js': 'x' }, manifestExtra: { libs: { monaco: '0', 'ctox-office': '1' } } });
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': 'x' } });
  const b = k.artifacts.bind('crm', '1');
  assert.equal(b.workerUrl('monaco', 'vs/editor.worker.js'), '/~a/lib:monaco@0.3~lib:ctox-office@1.4/vs/editor.worker.js');
  assert.equal(k.artifacts.bind('shell', '5').workerUrl('ctox-office', 'w.js'), '/~a/lib:ctox-office@1.4~lib:monaco@0.3/w.js');
  assert.throws(() => b.workerUrl('fehlt', 'x.js'), (e) => e.code === 'not_found');
  assert.throws(() => b.workerUrl('monaco', '../x.js'), (e) => e.code === 'invalid');
});
