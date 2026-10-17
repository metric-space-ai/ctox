// E27.1/E27.2/E27.8: Ergebnis als Wert, unknown_command beim Schreiben, Fake-Core-Latenz.
// Zustellung: nur Abos der geschriebenen Collection, keine leeren Seiten, Teilnehmer parallel, je Teilnehmer höchstens eine
// Seite gleichzeitig (eigene Writes warten in IndexedDB nie hinter einem Stapel Seiten-Transaktionen), Reihenfolge je Abo
// bleibt. Zwischen accepted und running liegt kein Timer. Alle Prüfungen zählen statt Wandzeit zu messen (lastfest).
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStubKernel } from '../testkit.mjs';
import { MemoryCoreTransport } from '../testkit.mjs';

let n = 0;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const subscribed = async (core, collections, peers = 1) => {
  for (const end = Date.now() + 4000; ;) {
    const ok = [...core.listeners].filter((l) => collections.every((c) => l.subscriptions.has(c) && l.sent.has(c))).length >= peers;
    if (ok) return; if (Date.now() > end) throw new Error(`Abos ${collections} nicht aufgebaut`); await sleep(5);
  }
};
const ADMIN = { role: 'admin', permissions: ['apps.source.view', 'apps.source.edit', 'apps.modify', 'apps.install'] };

// Zählt je Seitenübergabe: gleichzeitig laufende (gesamt, je Teilnehmer, je Abo), angewandte Seiten je Collection und
// cursor_to-Rückschritte. hold = Wartezeit NACH der Übernahme (die Spur des Teilnehmers bleibt belegt, die DB nicht).
function instrumentPages(t, { before = 3, hold = 0 } = {}) {
  const stats = { inFlight: 0, maxInFlight: 0, perPeer: new Map(), maxPerPeer: 0, perCollection: new Map(), maxPerCollection: 0,
    cursors: new Map(), regressions: [], pages: new Map(), applied: 0, order: [] };
  const bump = (m, k, d) => { const v = (m.get(k) ?? 0) + d; m.set(k, v); return v; };
  const connect = MemoryCoreTransport.prototype.connect;
  MemoryCoreTransport.prototype.connect = function (identity, events) {
    const page = events.page, peer = identity.userId;
    const wrapped = { ...events, page: async (p) => {
      const c = p.collection, key = `${peer}|${c}`;
      stats.maxPerCollection = Math.max(stats.maxPerCollection, bump(stats.perCollection, key, 1));
      stats.maxPerPeer = Math.max(stats.maxPerPeer, bump(stats.perPeer, peer, 1));
      stats.inFlight++; stats.maxInFlight = Math.max(stats.maxInFlight, stats.inFlight);
      bump(stats.pages, c, 1);
      const last = stats.cursors.get(key);
      if (last !== undefined && BigInt(p.cursor_to) < BigInt(last)) stats.regressions.push(`${key}: ${p.cursor_to} < ${last}`);
      stats.cursors.set(key, p.cursor_to);
      try {
        if (before) await sleep(before);
        const r = await page(p); stats.applied++; stats.order.push(c);
        if (hold) await sleep(hold);
        return r;
      } finally { stats.inFlight--; bump(stats.perPeer, peer, -1); bump(stats.perCollection, key, -1); }
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
// Zwei Nutzer an EINEM Core (wie Shell v4 bind: Core-Wirt + zweiter Nutzer): zwei Teilnehmer.
async function twoPeers(t, collections) {
  const instanceId = `e27p-${process.pid}-${++n}`;
  const peers = [];
  for (const userId of ['p1', 'p2']) {
    const k = createStubKernel({ locks: testLocks, instanceId, coreNamespace: instanceId, userId, actor: ADMIN, latencyMs: 2 });
    t.after(() => k.close());
    await k.ready;
    const api = k.appApi('notes', '1');
    for (const c of collections) { const off = api.query(c, {}).subscribe(() => {}); t.after(off); await k.sim.push(c, [{ id: `seed-${userId}` }]); }
    peers.push(k);
  }
  for (const k of peers) await k.sim.settle();
  return peers;
}

test('E27.8: Zustellung nur an Abos der geschriebenen Collection, Teilnehmer parallel, je Teilnehmer und Abo nie überholend', async (t) => {
  const stats = instrumentPages(t);
  const [k1, k2] = await twoPeers(t, ['e27_a', 'e27_b', 'e27_c']);
  await subscribed(stats.core, ['e27_a', 'e27_b', 'e27_c'], 2);
  stats.maxInFlight = 0; stats.maxPerPeer = 0; stats.maxPerCollection = 0; stats.pages.clear();
  for (let i = 0; i < 5; i++) await k1.sim.push('e27_a', [{ id: `x${i}`, v: i }]);
  assert.deepEqual([...stats.pages.keys()], ['e27_a'], `keine leeren Seiten anderer Abos: ${JSON.stringify([...stats.pages])}`);
  await Promise.all([k1.sim.push('e27_b', [{ id: 'y' }]), k1.sim.push('e27_c', [{ id: 'z' }]), k1.sim.push('e27_b', [{ id: 'y2' }])]);
  await k1.sim.settle(); await k2.sim.settle();
  assert.ok(stats.maxInFlight >= 2, `beide Teilnehmer gleichzeitig beliefert (seriell wäre 1), gemessen ${stats.maxInFlight}`);
  assert.equal(stats.maxPerPeer, 1, 'je Teilnehmer höchstens eine Seite gleichzeitig');
  assert.equal(stats.maxPerCollection, 1, 'je Abo höchstens eine Seite gleichzeitig');
  assert.deepEqual(stats.regressions, [], 'cursor_to je Abo monoton');
  for (const k of [k1, k2]) assert.deepEqual(k.appApi('notes', '1').query('e27_b', {}).get().map((d) => d.id).filter((id) => !id.startsWith('seed')).sort(), ['y', 'y2']);
});

test('E27.8: Write wird angenommen, während eine große Zustellung läuft – nie hinter dem Seitenstapel', async (t) => {
  const COLLS = Array.from({ length: 10 }, (_, i) => `e27_f${i}`);
  const stats = instrumentPages(t, { before: 0, hold: 4 });
  const [k1] = await twoTabsThreeSubs(t);
  const api = k1.appApi('notes', '1');
  for (const c of [...COLLS, 'e27_w']) { const off = api.query(c, {}).subscribe(() => {}); t.after(off); await k1.sim.push(c, [{ id: 'seed' }]); }
  const core = stats.core;
  await subscribed(core, [...COLLS, 'e27_w']);
  await k1.sim.settle();
  // Große Zustellung: zehn Commits auf zehn Abos zugleich (synchron bis zur Übergabe an die Spur).
  stats.applied = 0; stats.order.length = 0;
  const fan = Promise.all(COLLS.map((c, i) => core.commit(c, [{ id: `f${i}`, v: i }])));
  const h = api.put('e27_w', { id: 'w1', v: 1 });
  await h.local;
  const appliedAtLocal = stats.applied;
  const r = await h.result;
  const appliedAtResult = stats.applied;
  await fan; await k1.sim.settle();
  assert.equal(r.status, 'accepted');
  assert.ok(appliedAtLocal <= 1, `lokaler Commit wartet höchstens hinter einer Seite, nicht hinter ${appliedAtLocal}`);
  assert.ok(appliedAtResult < COLLS.length, `angenommen, während die Zustellung noch läuft (${appliedAtResult}/${COLLS.length} Seiten)`);
  const before = stats.order.indexOf('e27_w');
  assert.ok(before >= 0 && before <= 4, `Seite zum eigenen Write überholt den Rückstand (vor ihr ${before} von ${COLLS.length} Seiten): ${stats.order.join(' ')}`);
  assert.equal(stats.applied, COLLS.length + 1, 'alle Seiten zugestellt (zehn Abos + der Write)');
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
