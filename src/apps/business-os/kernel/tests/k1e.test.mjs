// K-1e: §16.45 Collection-Liveness in kernel.metrics + kernel.reconnect(), §16.43 identity.device_id.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStubKernel } from '../kernel.mjs';

let n = 0;
async function mk(t, o = {}) {
  const k = createStubKernel({ locks: testLocks, instanceId: `k1e${process.pid}-${++n}`, userId: 'u1', latencyMs: 2, ...o });
  t.after(() => k.close());
  await k.ready;
  return k;
}
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(pred, ms = 4000, what = 'Bedingung') { const t0 = Date.now(); while (!(await pred())) { if (Date.now() - t0 > ms) throw new Error(`until: ${what}`); await tick(5); } }
const cols = (k) => k.metrics.get().collections;
const states = (k) => Object.fromEntries(Object.entries(cols(k)).map(([c, v]) => [c, v.state]));
const allLive = (k) => Object.values(cols(k)).length > 0 && Object.values(cols(k)).every((c) => c.state === 'live');
const page = (k, collection, backlog, complete = true) => k.receivePage({ collection, epoch: '1', scope_hash: 'local', gen: 1, cursor_to: '0', head_seq: '0', complete, backlog, docs: [] });

test('§16.45: metrics.collections – live nur bei Verbindung oben und Backlog 0 nach vollständiger Seite, sonst catching_up/offline', async (t) => {
  const k = await mk(t);
  await until(() => allLive(k), 4000, 'alle live');
  const c0 = cols(k).files;
  assert.deepEqual([c0.state, c0.backlog, c0.gen, typeof c0.since_ms], ['live', 0, 1, 'number']);
  // Backlog > 0 → catching_up (nur diese Collection), nicht mehr alle live
  await page(k, 'files', 3);
  assert.deepEqual([cols(k).files.state, cols(k).files.backlog], ['catching_up', 3]);
  assert.equal(cols(k).business_apps.state, 'live');
  assert.equal(allLive(k), false, 'live nur, wenn ALLE Bereiche Backlog 0 haben');
  const since = cols(k).files.since_ms;
  await tick(3);
  await page(k, 'files', 0, false); // Backlog 0, aber Seite unvollständig → noch nicht live
  assert.equal(cols(k).files.state, 'catching_up');
  assert.equal(cols(k).files.since_ms, since, 'since_ms bleibt ohne Zustandswechsel');
  await page(k, 'files', 0, true);
  assert.equal(cols(k).files.state, 'live');
  assert.ok(cols(k).files.since_ms > since);
  assert.equal(allLive(k), true);
  // Verbindung weg → offline (lokale Daten bleiben lesbar), wieder da → live
  await k.sim.setLink({ state: 'offline', rtt_ms: 0, relay: false });
  assert.deepEqual([...new Set(Object.values(states(k)))], ['offline']);
  assert.ok(Array.isArray(k.appApi('crm', '1').query('files', {}).get()));
  await k.sim.setLink({ state: 'online', rtt_ms: 5, relay: false });
  await until(() => allLive(k), 4000, 'wieder live');
});

test('§16.45: Leser-Tab sieht dieselben collections; kernel.reconnect() baut den Transport voll neu auf (kein Reload)', async (t) => {
  const instanceId = `k1e-tabs-${process.pid}-${++n}`;
  const A = await mk(t, { instanceId });
  const B = await mk(t, { instanceId });
  await until(() => allLive(A), 4000, 'A live');
  await until(() => JSON.stringify(states(B)) === JSON.stringify(states(A)), 4000, 'B spiegelt A');
  await page(A, 'files', 2);
  await until(() => cols(B).files?.state === 'catching_up', 4000, 'B sieht catching_up');
  const before = A.sim.transportConnects;
  const r = await B.reconnect(); // im Leser angestoßen, im Leiter ausgeführt
  assert.equal(A.sim.transportConnects, before + 1, 'Transport neu verbunden');
  assert.equal(r.link.state, 'online');
  await until(() => allLive(A) && allLive(B), 4000, 'nach reconnect alles live');
  assert.equal((await A.appApi('crm', '1').put('notes', { id: 'n1' }).result).status, 'accepted', 'Schreiben geht nach dem Neuaufbau');
});

test('§16.43: identity.device_id = eigene device_pairing_id; Fake-Core deterministisch je Instanz+Nutzer+Gerät', async (t) => {
  const instanceId = `k1e-dev-${process.pid}`;
  const a = await mk(t, { instanceId, userId: 'u1', dbName: `dev-a-${process.pid}` });
  const a2 = await mk(t, { instanceId, userId: 'u1', dbName: `dev-a2-${process.pid}` });
  const b = await mk(t, { instanceId, userId: 'u2', dbName: `dev-b-${process.pid}` });
  const m = await mk(t, { instanceId, userId: 'u1', deviceLabel: 'mobile', dbName: `dev-m-${process.pid}` });
  const fixed = await mk(t, { instanceId, userId: 'u1', deviceId: 'pair-123', dbName: `dev-f-${process.pid}` });
  assert.match(a.identity.device_id, /^dev-[0-9a-f]+$/);
  assert.equal(a.identity.device_id, a2.identity.device_id, 'gleiches Gerät, gleiche id');
  assert.notEqual(a.identity.device_id, b.identity.device_id, 'anderer Nutzer');
  assert.notEqual(a.identity.device_id, m.identity.device_id, 'anderes Gerät');
  assert.equal(fixed.identity.device_id, 'pair-123', 'Gerätebindung des Bootstraps geht vor');
  assert.equal(a.shell.identity.device_id, a.identity.device_id);
});

test('§16.45: abonnierte, noch leere Collection erscheint sofort (catching_up bis zur ersten vollständigen Seite, dann live mit 0 Dokumenten) – auch nach reconnect', async (t) => {
  const k = await mk(t);
  const seen = [];
  const off = k.metrics.subscribe((m) => { if (m.collections.leads) seen.push(m.collections.leads.state); });
  t.after(off);
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': 'export function mount(){ return { unmount(){} }; }' }, collections: { leads: { schema: { primaryKey: 'id', type: 'object', properties: { id: { type: 'string' } } } } } });
  await until(() => cols(k).leads?.state === 'live', 4000, 'leads live');
  assert.equal(seen[0], 'catching_up', 'sofort sichtbar, zuerst catching_up');
  assert.deepEqual([cols(k).leads.backlog, k.appApi('crm', '1').query('leads', {}).get().length], [0, 0], 'live auch mit 0 Dokumenten');
  await k.reconnect();
  await until(() => cols(k).leads?.state === 'live', 4000, 'leads nach reconnect live');
});
