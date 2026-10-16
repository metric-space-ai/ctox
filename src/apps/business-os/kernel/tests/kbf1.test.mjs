// K-BF1: Der Leiter meldet den Rückstand an die Folger erst NACH dem Checkpoint-Vorlauf. Erreicht der Rückstand 0,
// geht immer eine frische Meldung hinaus – ohne weiteren Write bleibt kein Folger auf backlog 1 / catching_up stehen.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createKernel, createStubKernel } from '../kernel.mjs';
import { KernelTransport } from '../transport.mjs';
import { MemoryCoreTransport } from '../fake-core.mjs';

let n = 0;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const until = async (fn, ms = 3000) => { const end = Date.now() + ms; while (!fn()) { if (Date.now() > end) throw new Error('until: Zeitüberschreitung'); await sleep(5); } };
async function mk(t, instanceId) {
  const k = createStubKernel({ locks: testLocks, instanceId, userId: 'u1', latencyMs: 2 });
  t.after(() => k.close());
  await k.ready;
  return k;
}
const notLive = (m) => Object.entries(m.collections ?? {}).filter(([, c]) => c.state !== 'live').map(([name, c]) => `${name}:${c.state}`);

test('K-BF1: letzter offener Eintrag – Folger endet ohne weiteren Write auf backlog 0 und live', async (t) => {
  const instanceId = `kbf1-${process.pid}-${++n}`;
  const A = await mk(t, instanceId);
  const B = await mk(t, instanceId);
  assert.deepEqual([A.isLeader, B.isLeader], [true, false]);
  const h = A.appApi('notes', '1').put('notes', { id: 'last', v: 1 });
  await h.local; await h.result;
  // Der Leiter verarbeitet die zugehörige Seite: der Checkpoint erreicht den Rückstand 0.
  await until(() => A.appApi('notes', '1').get('notes', 'last').get()?.v === 1 && A.metrics.get().backlog === 0 && !notLive(A.metrics.get()).length);
  // Danach kein weiterer Write: der Folger muss allein durch die Meldung des Leiters auf 0 kommen.
  await until(() => B.metrics.get().backlog === 0 && !notLive(B.metrics.get()).length, 1000).catch(() => {});
  await sleep(50);
  const m = B.metrics.get();
  assert.equal(m.backlog, 0, 'Folger trägt den Rückstand des Leiters (0), nicht den Stand vor dem Checkpoint-Vorlauf');
  assert.deepEqual(notLive(m), [], 'keine Collection des Folgers bleibt catching_up');
  assert.deepEqual(m.collections, A.metrics.get().collections, 'Folger sieht dieselben Collection-Zustände wie der Leiter');
});

test('K-BF1: Core-Commit ohne Kernel-Write (anderer Teilnehmer) lässt den Folger nicht auf altem Rückstand stehen', async (t) => {
  const instanceId = `kbf1-ext-${process.pid}-${++n}`;
  const A = await mk(t, instanceId);
  const B = await mk(t, instanceId);
  await until(() => A.metrics.get().collections.files?.state === 'live');
  // Derselbe Core (Namespace des Stub-Kernels), aber ein fremder Schreiber: kein netChanged() des Leiters nach dem Commit.
  const other = new MemoryCoreTransport({ namespace: `ctox-${instanceId}-u1`, latencyMs: 2 });
  // Commit in die zuletzt abonnierte Collection: danach folgt keine weitere (leere) Seite, die den Fehler verdecken würde.
  const leaderCore = [...other.core.listeners][0];
  const last = [...leaderCore.subscriptions.keys()].at(-1);
  for (let i = 0; i < 3; i++) await other.push(last, [{ id: `ext-${i}`, v: i }]);
  await until(() => leaderCore.subscriptions.get(last).checkpoint_seq === leaderCore.head && leaderCore.backlog === 0 && A.metrics.get().backlog === 0);
  await until(() => B.metrics.get().backlog === 0, 1000).catch(() => {});
  await sleep(50);
  assert.equal(B.metrics.get().backlog, 0);
  assert.deepEqual(notLive(B.metrics.get()), []);
});

// Echter Leiterpfad (kein Fake-Core): receivePage meldete über trackScope() schon die neuen Collection-Zustände, aber mit
// dem Rückstand VOR dem Seitenvorlauf (extraBacklog alt). Der Folger sah kurz „alle live, backlog N“ – das ist genau der
// nicht zugeordnete Rückstand, den die Kopfleiste gelb macht. Jede Meldung muss in sich stimmig sein.
class CapturingTransport extends KernelTransport {
  async connect(identity, events) { this.identity = identity; this.events = events; events.link({ state: 'online', rtt_ms: 1, relay: false }); }
  async subscribe(sub) { (this.subs ??= []).push(sub); }
  async write() { return undefined; }
}
test('K-BF1: echter Leiterpfad – jede Meldung an den Folger trägt den Rückstand nach dem Seitenvorlauf', async (t) => {
  const transport = new CapturingTransport(), opts = { instanceId: `kbf1-real-${process.pid}-${++n}`, userId: 'u1', storage: 'memory', locks: testLocks };
  const A = createKernel({ ...opts, transport }); t.after(() => A.close()); await A.ready;
  const B = createKernel({ ...opts, transport: new CapturingTransport() }); t.after(() => B.close()); await B.ready;
  assert.deepEqual([A.isLeader, B.isLeader], [true, false]);
  const seen = [];
  const off = B.shell.metrics.subscribe((m) => seen.push({ backlog: m.backlog, sum: Object.values(m.collections ?? {}).reduce((a, c) => a + (c.backlog ?? 0), 0) }));
  t.after(off);
  await transport.events.page({ epoch: '1', scope_hash: 'local', gen: 1, collection: 'notes', cursor_to: '1', complete: false, backlog: 5, docs: [] });
  await until(() => B.shell.metrics.get().backlog === 5);
  await transport.events.page({ epoch: '1', scope_hash: 'local', gen: 1, collection: 'notes', cursor_to: '2', complete: true, backlog: 0, docs: [] });
  await until(() => B.shell.metrics.get().backlog === 0, 1000).catch(() => {});
  await sleep(50);
  assert.equal(B.shell.metrics.get().backlog, 0);
  assert.equal(B.shell.metrics.get().collections.notes?.state, 'live');
  assert.deepEqual(seen.filter((s) => s.backlog > s.sum), [], 'kein Folgerstand mit Rückstand, den keine Collection trägt');
});
