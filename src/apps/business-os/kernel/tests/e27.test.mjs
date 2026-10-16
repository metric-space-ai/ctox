// E27.1/E27.2/E27.8: Ergebnis als Wert, unknown_command beim Schreiben, Fake-Core-Latenz. Commit-Zustellung parallel je Teilnehmer/Abo (Reihenfolge je Abo bleibt), und zwischen
// accepted und running liegt kein Timer. Beide Prüfungen zählen statt Wandzeit zu messen, damit sie unter Last stabil bleiben.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStubKernel } from '../kernel.mjs';
import { MemoryCoreTransport } from '../fake-core.mjs';

let n = 0;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Jede Seitenübergabe kostet einen Timer-Schritt (wie eine IndexedDB-Transaktion im Browser); gezählt wird, wie viele
// Übergaben gleichzeitig laufen – gesamt und je Abo.
function instrumentPages(t) {
  const stats = { inFlight: 0, maxInFlight: 0, perCollection: new Map(), maxPerCollection: 0, cursors: new Map(), regressions: [] };
  const connect = MemoryCoreTransport.prototype.connect;
  MemoryCoreTransport.prototype.connect = function (identity, events) {
    const page = events.page;
    const wrapped = { ...events, page: async (p) => {
      const c = p.collection, now = (stats.perCollection.get(c) ?? 0) + 1;
      stats.perCollection.set(c, now); stats.maxPerCollection = Math.max(stats.maxPerCollection, now);
      stats.inFlight++; stats.maxInFlight = Math.max(stats.maxInFlight, stats.inFlight);
      const last = stats.cursors.get(c);
      if (last !== undefined && BigInt(p.cursor_to) < BigInt(last)) stats.regressions.push(`${c}: ${p.cursor_to} < ${last}`);
      stats.cursors.set(c, p.cursor_to);
      try { await sleep(3); return await page(p); } finally { stats.inFlight--; stats.perCollection.set(c, stats.perCollection.get(c) - 1); }
    } };
    stats.core = this.core;
    return connect.call(this, identity, wrapped);
  };
  t.after(() => { MemoryCoreTransport.prototype.connect = connect; });
  return stats;
}

async function twoTabsThreeSubs(t) {
  const instanceId = `e27-${process.pid}-${++n}`;
  const tabs = [];
  for (let i = 0; i < 2; i++) {
    const k = createStubKernel({ locks: testLocks, instanceId, userId: 'u1', latencyMs: 2 });
    t.after(() => k.close());
    await k.ready;
    tabs.push(k);
  }
  for (const k of tabs) { const api = k.appApi('notes', '1'); for (const c of ['e27_a', 'e27_b', 'e27_c']) { const off = api.query(c, {}).subscribe(() => {}); t.after(off); } }
  await tabs[0].sim.settle();
  return tabs;
}

test('E27.8: Commit-Zustellung parallel über Abos, je Abo nie überholend', async (t) => {
  const stats = instrumentPages(t);
  const [k1] = await twoTabsThreeSubs(t);
  assert.equal(k1.isLeader, true);
  stats.maxInFlight = 0; stats.maxPerCollection = 0;
  for (let i = 0; i < 5; i++) await k1.sim.push('e27_a', [{ id: `x${i}`, v: i }]);
  await Promise.all([k1.sim.push('e27_b', [{ id: 'y' }]), k1.sim.push('e27_c', [{ id: 'z' }]), k1.sim.push('e27_b', [{ id: 'y2' }])]);
  await k1.sim.settle();
  assert.ok(stats.maxInFlight >= 3, `mindestens 3 Abos gleichzeitig beliefert (seriell wäre 1), gemessen ${stats.maxInFlight}`);
  assert.equal(stats.maxPerCollection, 1, 'je Abo höchstens eine Seite gleichzeitig');
  assert.deepEqual(stats.regressions, [], 'cursor_to je Abo monoton');
  assert.deepEqual(k1.appApi('notes', '1').query('e27_b', {}).get().map((d) => d.id).sort(), ['y', 'y2']);
});

test('E27.8: zwischen accepted und running kein Timer-Verzug, Handler startet sofort', async (t) => {
  const stats = instrumentPages(t);
  const [k1] = await twoTabsThreeSubs(t);
  k1.sim.onCommand('e27.run', async () => ({ ok: true }));
  const api = k1.appApi('notes', '1');
  const fc = stats.core;
  assert.ok(fc, 'Fake-Core über die Transport-Instanz erreichbar');
  const commit = fc.commit;
  const order = [];
  fc.commit = function (collection, docs, depth) {
    const doc = Array.isArray(docs) ? docs[0] : docs;
    const tag = collection === 'business_commands' ? doc?.status : null;
    if (tag) order.push(`commit:${tag}`);
    const p = commit.call(this, collection, docs, depth);
    if (tag === 'accepted') p.then(() => setTimeout(() => order.push('macrotask-after-accepted'), 0));
    return p;
  };
  t.after(() => { fc.commit = commit; });
  const h = api.command('e27.run', {});
  for (const end = Date.now() + 4000; !['done', 'failed'].includes(h.status.get().status);) { if (Date.now() > end) throw new Error('Befehl endet nicht'); await sleep(5); }
  await sleep(5);
  assert.equal(h.status.get().status, 'done');
  const iAcc = order.indexOf('commit:accepted'), iRun = order.indexOf('commit:running'), iMark = order.indexOf('macrotask-after-accepted');
  assert.ok(iAcc >= 0 && iRun > iAcc && iMark >= 0, order.join(' → '));
  assert.ok(iRun < iMark, `running-Commit vor dem ersten Makrotask nach accepted (kein Timer dazwischen): ${order.join(' → ')}`);
});

test('K-WJ-B2 (E27.2): unbekannter Befehlstyp → write_result rejected/unknown_command beim Schreiben, nie accepted', async (t) => {
  const k = createStubKernel({ locks: testLocks, instanceId: `e27b-${process.pid}-${++n}`, userId: 'u1', latencyMs: 2 });
  t.after(() => k.close());
  await k.ready;
  const api = k.appApi('notes', '1');
  const h = api.command('wj.nope', { x: 1 });
  const seen = [];
  h.status.subscribe((s) => seen.push(s.status));
  const r = await h.result;
  assert.deepEqual([r.status, r.code], ['rejected', 'unknown_command'], JSON.stringify(r));
  for (const end = Date.now() + 2000; h.status.get().status !== 'failed';) { if (Date.now() > end) throw new Error(JSON.stringify(h.status.get())); await sleep(5); }
  assert.match(h.status.get().error, /^unknown_command/);
  assert.ok(!seen.includes('accepted') && !seen.includes('running'), seen.join(','));
  assert.ok(!(await k.sim.coreRows('business_commands')).some((d) => d.id === h.id), 'kein Befehlsdokument im Core');
  // Routen: sim.onCommand / sim.failCommand des Transports, Plattformbefehle, handlers der aktiven Version je (app_id, type)
  k.sim.onCommand('wj.yes', async () => ({ ok: 1 }));
  assert.equal((await api.command('wj.yes', {}).result).status, 'accepted');
  await k.sim.failCommand('wj.fails', 'kaputt');
  assert.equal((await api.command('wj.fails', {}).result).status, 'accepted');
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': 'export function mount(){ return { unmount(){} }; }', 'h.js': 'export default async function handle() { return { ok: true }; }' }, handlers: { 'crm.export': 'h.js' } });
  const own = k.appApi('crm', '1').command('crm.export', {});
  assert.equal((await own.result).status, 'accepted', 'Manifest-Route der eigenen App');
  const foreign = await api.command('crm.export', {}).result;
  assert.deepEqual([foreign.status, foreign.code], ['rejected', 'unknown_command'], 'Route gilt je (app_id, type)');
  await k.sim.settle();
});

test('K-WJ-6 (E27.1): command.result ist ein Wert – verschachtelte null-Felder bleiben', async (t) => {
  const k = createStubKernel({ locks: testLocks, instanceId: `e27v-${process.pid}-${++n}`, userId: 'u1', latencyMs: 2 });
  t.after(() => k.close());
  await k.ready;
  k.sim.onCommand('wj.value', async () => ({ a: null, b: { c: null, d: 1 } }));
  const h = k.appApi('notes', '1').command('wj.value', {});
  for (const end = Date.now() + 4000; h.status.get().status !== 'done';) { if (Date.now() > end) throw new Error(JSON.stringify(h.status.get())); await sleep(5); }
  assert.deepEqual(h.status.get().result, { a: null, b: { c: null, d: 1 } });
  assert.deepEqual((await k.sim.coreRows('business_commands')).find((d) => d.id === h.id).result, { a: null, b: { c: null, d: 1 } });
});
