// ZONE45 – Unit-Tests für sync-state.js (node --test). Jeder Fall läuft zweimal: gegen das echte Modul (muss grün
// sein) und gegen eine Mutante, die genau die geprüfte Regel bricht (muss rot sein = Mutationsbeleg).
// Zusätzlich gegen den Kern-Stub (Pin 74137b1, K-1e mit metrics.collections und reconnect), falls vorhanden.
// Lauf: node --test src/apps/business-os/v4/shell/sync-state.test.mjs
//       Stub-Pfad abweichend per CTOX_APP_API_STUB (Verzeichnis mit ctox-app-api-stub.mjs).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import * as real from './sync-state.js';
import { adaptKernel } from './kernel-adapter.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const SOURCE = fs.readFileSync(path.join(here, 'sync-state.js'), 'utf8');
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'zone45-mut-'));
let mutantSeq = 0;
async function mutant(pairs) {
  let src = SOURCE;
  for (const [from, to] of pairs) {
    assert.ok(src.includes(from), `Mutationsstelle fehlt im Modul: ${from}`);
    src = src.replace(from, to);
  }
  const file = path.join(tmp, `sync-state-m${++mutantSeq}.mjs`);
  fs.writeFileSync(file, src);
  return import(pathToFileURL(file).href);
}

// ---- Kernel-Attrappen -----------------------------------------------------------------------------------------
function liveDoc(value) {
  const subs = new Set();
  return {
    get: () => value,
    set(next) { value = next; for (const cb of [...subs]) cb(value); },
    subscribe(cb) { subs.add(cb); cb(value); return () => subs.delete(cb); },
    count: () => subs.size,
  };
}
const NAMES = Array.from({ length: 17 }, (_, i) => `c${String(i + 1).padStart(2, '0')}`);
function metricsWith(over = {}, link = 'online') {
  const collections = {};
  for (const n of NAMES) collections[n] = { state: 'live', backlog: 0, gen: 1, since_ms: 1 };
  Object.assign(collections, over);
  return { backlog: 0, outbox: { count: 0, oldest_ms: 0 }, link: { state: link, rtt_ms: 5, relay: false }, collections };
}
function fakeKernel(m = metricsWith()) {
  const k = { metrics: liveDoc(m), reconnects: 0, async reconnect() { k.reconnects++; return { link: { state: 'online' } }; } };
  return k;
}
const NONE = null; // diagnostics: keine V3-Quelle

// ---- Fälle (werfen bei Verletzung) -----------------------------------------------------------------------------
const CASES = {
  async '17 Collections, eine catching_up → warn'(mod) {
    const k = fakeKernel(metricsWith({ c07: { state: 'catching_up', backlog: 4, gen: 1, since_ms: 2 } }));
    const s = mod.createSyncState({ kernel: k, diagnostics: NONE });
    const o = s.getOverallState();
    assert.equal(Object.keys(o.collections).length, 17);
    assert.equal(o.level, 'warn');
    assert.deepEqual(o.pending, ['c07']);
    assert.equal(s.getAppState('x', ['c07']).level, 'warn');
    assert.equal(s.getAppState('x', ['c01', 'c02']).level, 'ok', 'App ohne die hängende Collection bleibt ok');
    s.destroy();
  },
  async 'error → error'(mod) {
    const k = fakeKernel(metricsWith({ c03: { state: 'error', backlog: 0, gen: 1, since_ms: 2, error: { code: 'forbidden', message: 'Recht entzogen' } }, c09: { state: 'catching_up', backlog: 1, gen: 1, since_ms: 2 } }));
    const s = mod.createSyncState({ kernel: k, diagnostics: NONE });
    const o = s.getOverallState();
    assert.equal(o.level, 'error');
    assert.deepEqual(o.errors, [{ name: 'c03', code: 'forbidden', message: 'Recht entzogen' }]);
    assert.equal(s.getAppState('x', ['c03']).level, 'error');
    s.destroy();
  },
  async 'alles live → ok'(mod) {
    const k = fakeKernel();
    const s = mod.createSyncState({ kernel: k, diagnostics: NONE });
    const o = s.getOverallState();
    assert.equal(o.level, 'ok');
    assert.deepEqual([o.pending, o.errors, o.unknown], [[], [], []]);
    assert.equal(s.getAppState('x', ['c01', 'c17']).level, 'ok');
    s.destroy();
  },
  async 'V3-Objekt mit peer_connect_timeout → warn'(mod) {
    const diag = { collections: { notes: { connectionStatus: 'connected', queryReady: true, lastLifecycleEvent: { code: 'peer_connect_timeout' } }, tasks: { connectionStatus: 'connected', queryReady: true } } };
    const s = mod.createSyncState({ kernel: null, diagnostics: diag });
    assert.equal(s.getOverallState().source, 'v3');
    assert.equal(s.getOverallState().level, 'warn');
    assert.equal(s.getAppState('notes', ['notes']).level, 'warn');
    assert.equal(s.getAppState('tasks', ['tasks']).level, 'ok');
    s.destroy();
  },
  async 'V3-Funktion mit peer_connect_timeout → warn'(mod) {
    const snap = { lastLifecycleEvent: { code: 'peer_connect_timeout' }, collections: { notes: { connectionStatus: 'connected', queryReady: true } } };
    // Standardquelle: window.ctoxBusinessOsSyncDiagnostics als Funktion (so liefert V3 sie aus).
    const prev = globalThis.window;
    globalThis.window = { ctoxBusinessOsSyncDiagnostics: () => snap, addEventListener() {}, removeEventListener() {} };
    try {
      const s = mod.createSyncState({ kernel: null });
      assert.equal(s.getOverallState().source, 'v3');
      assert.equal(s.getOverallState().level, 'warn');
      s.destroy();
    } finally { if (prev === undefined) delete globalThis.window; else globalThis.window = prev; }
  },
  async 'keine Quelle → unknown, nie ok'(mod) {
    const s = mod.createSyncState({ kernel: null, diagnostics: NONE });
    assert.equal(s.getOverallState().level, 'unknown');
    assert.equal(s.getAppState('x', []).level, 'unknown');
    assert.equal(s.getAppState('x', ['notes']).level, 'unknown');
    const s2 = mod.createSyncState({ kernel: { metrics: liveDoc(null) }, diagnostics: NONE }); // Messwerte ohne collections
    assert.equal(s2.getOverallState().level, 'unknown');
    s.destroy(); s2.destroy();
  },
  async 'reconnect ruft kernel.reconnect'(mod) {
    const k = fakeKernel();
    const s = mod.createSyncState({ kernel: k, diagnostics: NONE });
    assert.equal(s.canReconnect, true);
    assert.equal(await s.reconnect(), true);
    assert.equal(k.reconnects, 1);
    // Über adaptKernel: der Adapter hat kein reconnect, der Original-Kernel (adapter.source) schon.
    const raw = { apps: liveDoc([]), activation: { onChange: () => () => {} }, metrics: k.metrics, reconnect: k.reconnect };
    const s2 = mod.createSyncState({ kernel: adaptKernel(raw), diagnostics: NONE });
    assert.equal(await s2.reconnect(), true);
    assert.equal(k.reconnects, 2);
    s.destroy(); s2.destroy();
  },
};

const MUTANTS = {
  '17 Collections, eine catching_up → warn': [["c.state === 'catching_up' || c.state === 'offline'", "c.state === 'offline'"]],
  'error → error': [["if (c.state === 'error') errors.push", "if (c.state === 'errorX') errors.push"]],
  'alles live → ok': [["  else level = 'ok';", "  else level = 'unknown';"]],
  'V3-Objekt mit peer_connect_timeout → warn': [["x.lastLifecycleEvent?.code === 'peer_connect_timeout'", 'false']],
  'V3-Funktion mit peer_connect_timeout → warn': [["if (typeof d === 'function') d = d();", '']],
  'keine Quelle → unknown, nie ok': [
    ["if (src.source === 'none') level = 'unknown';", "if (false) level = 'unknown';"],
    ["get: () => ({ state: 'unknown' }), empty: () => 'unknown' };", "get: () => ({ state: 'live' }), empty: () => 'ok' };"],
  ],
  'reconnect ruft kernel.reconnect': [['try { await owner.reconnect(); }', 'try { /* mutiert */ }']],
};

for (const [name, fn] of Object.entries(CASES)) {
  test(`Z45 ${name}`, async () => { await fn(real); });
  test(`Z45 Mutationsbeleg: ${name}`, async () => {
    const mod = await mutant(MUTANTS[name]);
    await assert.rejects(() => fn(mod), `Mutante bleibt grün – der Fall „${name}“ prüft die Regel nicht`);
  });
}

// ---- Weitere Regeln ------------------------------------------------------------------------------------------
test('Z45 nicht gemeldete Collection: online → unknown (nie ok), offline → warn', () => {
  const k = fakeKernel();
  const s = real.createSyncState({ kernel: k, diagnostics: NONE });
  assert.equal(s.getAppState('x', ['fehlt']).level, 'unknown');
  k.metrics.set(metricsWith({}, 'offline'));
  assert.equal(s.getAppState('x', ['fehlt']).level, 'warn');
  s.destroy();
});
test('Z45 nur metrics.link (Kernel ohne collections): offline → warn (Quelle link), online → unknown (nie ok)', () => {
  const m = liveDoc({ backlog: 0, outbox: { count: 0, oldest_ms: 0 }, link: { state: 'online', rtt_ms: 1, relay: false } });
  const s = real.createSyncState({ kernel: { metrics: m }, diagnostics: NONE });
  assert.equal(s.getOverallState().level, 'unknown');
  assert.equal(s.getAppState('x', ['notes']).level, 'unknown');
  m.set({ ...m.get(), link: { state: 'offline', rtt_ms: 0, relay: false } });
  assert.deepEqual([s.getOverallState().level, s.getOverallState().source], ['warn', 'link']);
  assert.equal(s.getAppState('x', []).level, 'warn');
  assert.equal(s.getAppState('x', ['notes']).level, 'warn');
  s.destroy();
});
test('Z45 Rückstand ohne Collection-Zuordnung: Gesamtzustand warn (nie grün), App-Zustand unberührt', () => {
  const k = fakeKernel({ ...metricsWith(), backlog: 3 });
  const s = real.createSyncState({ kernel: k, diagnostics: NONE });
  assert.deepEqual([s.getOverallState().level, s.getOverallState().backlog], ['warn', 3]);
  assert.equal(s.getAppState('x', ['c01']).level, 'ok');
  k.metrics.set(metricsWith({ c02: { state: 'catching_up', backlog: 3, gen: 1, since_ms: 1 } }));
  assert.equal(s.getOverallState().backlog, undefined, 'zugeordneter Rückstand zählt nicht doppelt');
  s.destroy();
});
test('Z45 Generationen: angefordert mit Grundnamen, es zählt die höchste Generation', () => {
  const k = fakeKernel(metricsWith({ notes: { state: 'live', backlog: 0, gen: 1, since_ms: 1 }, 'notes@g2': { state: 'catching_up', backlog: 3, gen: 2, since_ms: 1 } }));
  const s = real.createSyncState({ kernel: k, diagnostics: NONE });
  assert.equal(s.getAppState('notes', ['notes']).level, 'warn');
  s.destroy();
});
test('Z45 angeforderte Shell-Collections begrenzen den Gesamtzustand; appCollections liefert die App-Liste', () => {
  const k = fakeKernel(metricsWith({ c05: { state: 'catching_up', backlog: 1, gen: 1, since_ms: 1 } }));
  const s = real.createSyncState({ kernel: k, diagnostics: NONE, requested: ['c01', 'c02'], appCollections: (id) => (id === 'a' ? ['c05'] : ['c01']) });
  assert.equal(s.getOverallState().level, 'ok');
  s.setRequested(null);
  assert.equal(s.getOverallState().level, 'warn');
  assert.equal(s.getAppState('a').level, 'warn');
  assert.equal(s.getAppState('b').level, 'ok');
  s.destroy();
});
test('Z45 ohne angeforderte App-Collections zählt die Verbindung', () => {
  const k = fakeKernel();
  const s = real.createSyncState({ kernel: k, diagnostics: NONE });
  assert.equal(s.getAppState('x', []).level, 'ok');
  k.metrics.set(metricsWith({}, 'offline'));
  assert.equal(s.getAppState('x', []).level, 'warn');
  s.destroy();
});
test('Z45 V3: Grün nur mit connected + queryReady; reconnecting/reconnectingSince → warn; error → error; sonst unknown', () => {
  const st = (c, top) => real.v3CollectionState(c, top)?.state;
  assert.equal(st({ connectionStatus: 'connected', queryReady: true }), 'live');
  assert.equal(st({ connectionStatus: 'connected', queryReady: false }), 'unknown');
  assert.equal(st({ connectionStatus: 'connecting' }), 'unknown');
  assert.equal(st({ connectionStatus: 'reconnecting' }), 'catching_up');
  assert.equal(st({ connectionStatus: 'connected', queryReady: true, reconnectingSince: '2026-10-15T19:00:00Z' }), 'catching_up');
  assert.equal(st({ connectionStatus: 'connected', queryReady: true }, { connectionStatus: 'reconnecting' }), 'catching_up');
  assert.equal(st({ connectionStatus: 'error', lastError: { message: 'kaputt' } }), 'error');
});
test('Z45 V3 ohne reconnect: canReconnect false, reconnect() → false (nur lesend)', async () => {
  const s = real.createSyncState({ kernel: null, diagnostics: { collections: {} } });
  assert.equal(s.canReconnect, false);
  assert.equal(await s.reconnect(), false);
  s.destroy();
});
test('Z45 subscribe: ereignisgetrieben über kernel.metrics, gleiche Zustände ohne Wiederholung, destroy meldet ab', () => {
  const k = fakeKernel();
  const s = real.createSyncState({ kernel: k, diagnostics: NONE });
  const seen = [];
  const off = s.subscribe((o) => seen.push(o.level));
  k.metrics.set(metricsWith());
  k.metrics.set(metricsWith({ c02: { state: 'catching_up', backlog: 1, gen: 1, since_ms: 1 } }));
  k.metrics.set(metricsWith());
  assert.deepEqual(seen, ['ok', 'warn', 'ok']);
  assert.equal(real.currentSyncState(), s);
  off();
  assert.equal(k.metrics.count(), 0, 'letzter Abonnent weg → kein Kernel-Abo');
  s.subscribe(() => {});
  s.destroy();
  assert.equal(k.metrics.count(), 0);
  assert.equal(real.currentSyncState(), null);
});
test('Z45 V3-Abfrage höchstens einmal je Sekunde (pollMs wird auf 1000 angehoben)', async () => {
  let reads = 0;
  const s = real.createSyncState({ kernel: null, diagnostics: () => { reads++; return { collections: { n: { connectionStatus: 'connected', queryReady: true } } }; }, pollMs: 10 });
  s.subscribe(() => {}, { app: true });
  const r0 = reads;
  await new Promise((r) => setTimeout(r, 450));
  assert.equal(reads, r0, `keine Abfrage vor 1 s (${reads - r0} Lesungen in 450 ms)`);
  s.destroy();
});

// ---- Gegen den Kern-Stub (Standard Pin 8265bd7: metrics.collections + reconnect + Abo-Liveness, K-1e) ----------
const PINS = path.join(os.homedir(), '.local/state/shell-v4/stub-pins');
const STUB_DIR = process.env.CTOX_APP_API_STUB || path.join(PINS, '8265bd7');
const stubFile = path.join(STUB_DIR, 'ctox-app-api-stub.mjs');
test('Z45 gegen Kern-Stub: live → ok, setBacklog → warn, failCollection → error, offline → warn, reconnect über den Adapter', { skip: !fs.existsSync(stubFile) && `Stub fehlt: ${stubFile}` }, async () => {
  const { createStubKernel } = await import(pathToFileURL(stubFile).href);
  const raw = createStubKernel({ instanceId: `z45-${process.pid}`, userId: 'u1', latencyMs: 2 });
  await raw.ready;
  try {
    const hasCollections = !!raw.metrics.get()?.collections;
    if (!hasCollections) {
      // Pin ohne §16.45 (z. B. 68b83a3): kein Beleg → unknown, nie ok.
      const s = real.createSyncState({ kernel: adaptKernel(raw), diagnostics: NONE });
      assert.equal(s.getOverallState().level, 'unknown');
      s.destroy();
      return;
    }
    const s = real.createSyncState({ kernel: adaptKernel(raw), diagnostics: NONE });
    const levels = [];
    s.subscribe((o) => levels.push(o.level));
    const until = async (pred, ms = 3000) => { const t0 = Date.now(); while (!pred()) { if (Date.now() - t0 > ms) throw new Error('Zeitüberschreitung'); await new Promise((r) => setTimeout(r, 10)); } };
    await until(() => s.getOverallState().level === 'ok');
    await raw.sim.setBacklog(2, 'files');
    await until(() => s.getAppState('files', ['files']).level === 'warn');
    assert.deepEqual(s.getOverallState().pending, ['files']);
    assert.equal(s.getAppState('apps', ['business_apps']).level, 'ok');
    await raw.sim.setBacklog(0, 'files');
    await raw.sim.failCollection('files', 'forbidden', 'Recht entzogen');
    await until(() => s.getOverallState().level === 'error');
    assert.equal(s.getOverallState().errors[0].code, 'forbidden');
    await raw.sim.failCollection('files', null);
    await raw.sim.setLink({ state: 'offline' });
    await until(() => s.getOverallState().level === 'warn');
    await raw.sim.setLink({ state: 'online' });
    await until(() => s.getOverallState().level === 'ok');
    const before = raw.sim.transportConnects;
    assert.equal(await s.reconnect(), true);
    assert.equal(raw.sim.transportConnects, before + 1);
    assert.ok(levels.includes('warn') && levels.includes('error') && levels.at(-1) === 'ok', levels.join(','));
    s.destroy();
  } finally {
    await raw.close?.();
  }
});

// App mit leerer, abonnierter Collection: zuerst gelb bzw. grau, nach Zustellung grün, nie dauerhaft grau; auch nach
// reconnect. Läuft gegen jeden vorhandenen Pin; Pins ohne Abo-Liveness (74137b1) bzw. ohne collections (68b83a3)
// bleiben grau (unknown), nie grün.
for (const pin of ['7b9560d', '74137b1', '68b83a3']) {
  const file = path.join(PINS, pin, 'ctox-app-api-stub.mjs');
  test(`Z45 leere abonnierte Collection gegen Pin ${pin}: zuerst gelb/grau, nach Zustellung grün, nie dauerhaft grau`, { skip: !fs.existsSync(file) && `Pin fehlt: ${file}` }, async () => {
    const { createStubKernel } = await import(pathToFileURL(file).href);
    const raw = createStubKernel({ instanceId: `z45e-${pin}-${process.pid}`, userId: 'u1', latencyMs: 2 });
    await raw.ready;
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    try {
      // Verlauf ab VOR der Veröffentlichung: der Kern abonniert die Collections aktivierter Manifeste selbst.
      const s = real.createSyncState({ kernel: adaptKernel(raw), diagnostics: NONE });
      const seen = [];
      s.subscribe(() => { const l = s.getAppState('leer', ['leer_items']).level; if (seen.at(-1) !== l) seen.push(l); }, { app: true });
      await raw.sim.publishAppVersion('leer', '1.0.0', { title: 'Leer', collections: { leer_items: {} } });
      const api = raw.appApi('leer', '1.0.0');
      const first = seen[0];
      const off = api.query('leer_items', {}).subscribe(() => {});
      const t0 = Date.now();
      while (s.getAppState('leer', ['leer_items']).level !== 'ok' && Date.now() - t0 < 2000) await sleep(10);
      const settled = s.getAppState('leer', ['leer_items']).level;
      const hasLiveness = !!raw.metrics.get()?.collections?.leer_items;
      if (pin === '7b9560d' || hasLiveness) {
        assert.ok(first === 'warn' || first === 'unknown', `zuerst gelb oder grau, war ${first}`);
        assert.equal(settled, 'ok', `nach Zustellung grün (Verlauf ${seen.join(' → ')})`);
        assert.ok(seen.includes('warn'), `Abo-Phase sichtbar gelb (catching_up), Verlauf ${seen.join(' → ')}`);
        assert.equal(raw.metrics.get().collections.leer_items.backlog, 0);
        assert.equal(await s.reconnect(), true);
        const t1 = Date.now();
        while (s.getAppState('leer', ['leer_items']).level !== 'ok' && Date.now() - t1 < 2000) await sleep(10);
        assert.equal(s.getAppState('leer', ['leer_items']).level, 'ok', 'nach reconnect wieder grün, nicht grau');
      } else {
        assert.equal(settled, 'unknown', `Pin ${pin} ohne Abo-Liveness: grau, nie grün (Verlauf ${seen.join(' → ')})`);
      }
      off(); s.destroy();
    } finally {
      await raw.close?.();
    }
  });
}
