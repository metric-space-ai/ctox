// K-1k: §16.50 Nr. 3 – jedes Kernel-Fenster meldet dem SW {type:'ctox:kernel', leader} beim Start, bei jedem
// Leiterwechsel (Erwerb und Verlust) und nach controllerchange; ohne navigator.serviceWorker ein No-op.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStubKernel } from '../testkit.mjs';

let n = 0;
const until = async (fn, ms = 3000) => { const end = Date.now() + ms; while (!fn()) { if (Date.now() > end) throw new Error('until: Zeitüberschreitung'); await new Promise((r) => setTimeout(r, 5)); } };

// Gefälschter ServiceWorkerContainer je Fenster; der Kernel bindet ihn beim Erzeugen.
function fakeSw() {
  const listeners = new Map();
  const controller = () => { const sent = []; return { sent, postMessage: (m) => sent.push(m) }; };
  const sw = {
    controller: controller(),
    addEventListener(type, fn) { if (!listeners.has(type)) listeners.set(type, new Set()); listeners.get(type).add(fn); },
    removeEventListener(type, fn) { listeners.get(type)?.delete(fn); },
  };
  const all = [sw.controller];
  return {
    sw,
    /** leader-Werte aller ctox:kernel-Meldungen an alle bisherigen controller, in Reihenfolge. */
    hints: () => all.flatMap((c) => c.sent).filter((m) => m.type === 'ctox:kernel').map((m) => m.leader),
    changeController() { sw.controller = controller(); all.push(sw.controller); for (const fn of listeners.get('controllerchange') ?? []) fn({}); return sw.controller; },
    listening: (type) => listeners.get(type)?.size ?? 0,
  };
}
// Node < 21 hat kein globalThis.navigator (Lane: Node 20) – für die Dauer der Kernel-Erzeugung ein leeres Objekt stellen.
async function mk(t, fake, o = {}) {
  const ownNav = fake && globalThis.navigator === undefined;
  if (ownNav) Object.defineProperty(globalThis, 'navigator', { configurable: true, writable: true, value: {} });
  const had = globalThis.navigator ? Object.getOwnPropertyDescriptor(globalThis.navigator, 'serviceWorker') : undefined;
  if (fake) Object.defineProperty(globalThis.navigator, 'serviceWorker', { configurable: true, value: fake.sw });
  let k;
  try { k = createStubKernel({ locks: testLocks, instanceId: `k1k${process.pid}-${++n}`, userId: 'u1', latencyMs: 2, ...o }); }
  finally {
    if (fake) { if (had) Object.defineProperty(globalThis.navigator, 'serviceWorker', had); else delete globalThis.navigator.serviceWorker; }
    if (ownNav) delete globalThis.navigator;
  }
  t.after(() => k.close());
  await k.ready;
  return k;
}

test('§16.50 Nr. 3: Start, Verlust und Wiedererwerb der Leitung werden gemeldet, unveränderter Stand nicht doppelt', async (t) => {
  const fake = fakeSw();
  const k = await mk(t, fake, { latencyMs: 20 });
  assert.equal(k.isLeader, true);
  assert.deepEqual(fake.hints(), [true], 'Startmeldung genau einmal (Erwerb vor dem Start nicht doppelt)');
  const h = k.appApi('notes', '1').put('notes', { id: 'g', v: 1 });
  await h.local;
  await k.sim.bumpLeaderGen(); // anderer Leiter erhöhte die Generation → Verlust, danach Neuerwerb
  await h.result;
  assert.equal(k.sim.staleDiscards, 1);
  await until(() => k.isLeader);
  assert.deepEqual(fake.hints(), [true, false, true], 'Verlust und Erwerb je einmal gemeldet');
  await new Promise((r) => setTimeout(r, 50));
  assert.deepEqual(fake.hints(), [true, false, true], 'keine weiteren Meldungen ohne Wechsel');
});

test('§16.50 Nr. 3: zwei Fenster – Leser meldet false; Schließen des Leiters meldet false, der neue Leiter true', async (t) => {
  const instanceId = `k1k-tabs-${process.pid}-${++n}`;
  const fa = fakeSw(), fb = fakeSw();
  const A = await mk(t, fa, { instanceId });
  const B = await mk(t, fb, { instanceId });
  assert.deepEqual([A.isLeader, B.isLeader], [true, false]);
  assert.deepEqual([fa.hints(), fb.hints()], [[true], [false]], 'Startmeldung auch vom Leser');
  await A.close();
  await until(() => B.isLeader);
  assert.deepEqual(fa.hints(), [true, false], 'Schließen ist ein Leiterverlust');
  assert.deepEqual(fb.hints(), [false, true], 'Erwerb im Leser-Fenster');
  assert.equal(fa.listening('controllerchange'), 0, 'controllerchange-Hörer beim Schließen entfernt');
});

test('§16.50 Nr. 3: nach controllerchange erhält der neue controller den aktuellen Stand (auch unverändert)', async (t) => {
  const instanceId = `k1k-cc-${process.pid}-${++n}`;
  const fa = fakeSw(), fb = fakeSw();
  await mk(t, fa, { instanceId });
  await mk(t, fb, { instanceId });
  const ca = fa.changeController(), cb = fb.changeController();
  assert.deepEqual(ca.sent, [{ type: 'ctox:kernel', leader: true }]);
  assert.deepEqual(cb.sent, [{ type: 'ctox:kernel', leader: false }]);
  assert.deepEqual([fa.hints(), fb.hints()], [[true, true], [false, false]]);
});

test('§16.50 Nr. 3: ohne navigator.serviceWorker (Node, Worker) kein Fehler; ohne controller keine Meldung', async (t) => {
  assert.equal(globalThis.navigator?.serviceWorker, undefined);
  const k = await mk(t, null);
  assert.equal(k.isLeader, true);
  const fake = fakeSw();
  fake.sw.controller = null; // Seite (noch) nicht vom SW kontrolliert
  const k2 = await mk(t, fake);
  assert.equal(k2.isLeader, true);
  assert.deepEqual(fake.hints(), [], 'nichts an einen alten controller');
  const c = fake.changeController();
  assert.deepEqual(c.sent, [{ type: 'ctox:kernel', leader: true }], 'Stand nachgeholt, sobald ein controller da ist');
});
