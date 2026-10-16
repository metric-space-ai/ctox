// node --test src/apps/business-os/v4/tools/isolation/sw-lib-miss.test.mjs
// Service Worker (v4/sw.js), Standardbibliotheken nach Kern §16.37/§16.39: /~a/lib:<name>@<major>.<minor>/<pfad> aus IDB
// (sha256-geprueft, immutable), bei Miss Anforderung an den Kernel (postMessage + MessagePort), Warten auf seine Antwort,
// dann Auslieferung; Treffer gebuendelt per ctox:lib-hit (<= 1/s).
// Der echte sw.js-Fetch-Handler laeuft in einem vm-Kontext mit IDB-Attrappe und simuliertem Kernel-Client.
// IDB-Layout des Kernels (Entscheidung ctox sync v4): artifacts {hash, content: string | Uint8Array},
// meta {key: 'files:<app>@<ver>', value: {pfad: sha256}}; kein Store manifests, keine active:-Schluessel.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SW_FILE_SRC = readFileSync(path.resolve(HERE, '..', '..', 'sw.js'), 'utf8');
// Mutationsbeleg (§16.50): SW_MUTATE=<name> laedt eine gezielt verdorbene sw.js; die zugehoerigen Tests muessen rot werden.
//   worker-own   Anforderung aus einem Worker geht an den Worker selbst (alter Stand vor §16.50)
//   no-timeout   keine Frist: der SW wartet unbegrenzt auf das Fenster bzw. die Kernel-Antwort
//   no-reply-timeout  keine Frist auf die Kernel-Antwort eines gefundenen Fensters
//   no-leader    Leiter-Meldung wird ignoriert (erstes Fenster gewinnt)
//   shared-bare  ctox:shared/ ohne ~lib:-Segmente (alter Stand vor §16.50 Nr. 2)
const SW_MUTATIONS = {
  'worker-own': ["if (own && (own.type === 'window' || own.type === undefined)) return own;", 'if (own) return own;'],
  'no-timeout': ['if (client || Date.now() >= deadline) return client;', 'if (client) return client;'],
  'no-reply-timeout': ["const timer = setTimeout(() => { channel.port1.close(); resolve(timeout('keine Kernel-Antwort')); }, Math.max(0, deadline - Date.now()));", 'const timer = 0;'],
  'no-leader': ['return known.find((c) => kernelWindows.get(c.id).leader) || known[0]', 'return known[0]'],
  'shared-bare': ['const root = `/~a/shell@${shellVer}${bindingSuffix(binding)}/shared/`;', 'const root = `/~a/shell@${shellVer}/shared/`;'],
};
const MUT = process.env.SW_MUTATE ? SW_MUTATIONS[process.env.SW_MUTATE] : null;
if (process.env.SW_MUTATE && (!MUT || !SW_FILE_SRC.includes(MUT[0]))) throw new Error(`SW_MUTATE ${process.env.SW_MUTATE}: Stelle nicht gefunden`);
const SW_SRC = MUT ? SW_FILE_SRC.replace(MUT[0], MUT[1]) : SW_FILE_SRC;
const TIMEOUT_DECL = 'const LIB_MISS_TIMEOUT_MS = 15000;';
const sha = (content) => createHash('sha256').update(typeof content === 'string' ? Buffer.from(content, 'utf8') : Buffer.from(content)).digest('hex');
// Artefakt-Zeile wie der Kernel sie schreibt; content = Text oder Uint8Array.
const art = (content, hash = sha(content)) => ({ hash, content });
const files = (appId, ver, map) => ({ key: `files:${appId}@${ver}`, value: map });
const tick = (ms) => new Promise((r) => setTimeout(r, ms));

// IDB-Attrappe: nur das, was sw.js benutzt (open -> db.transaction(store).objectStore(store).get(key)).
function fakeIndexedDb(stores) {
  return {
    open() {
      const req = {};
      queueMicrotask(() => {
        req.result = {
          transaction(store) {
            return { objectStore() { return { get(key) { const r = {}; queueMicrotask(() => { r.result = stores[store].get(key); r.onsuccess(); }); return r; } }; } };
          },
        };
        req.onsuccess();
      });
      return req;
    },
  };
}

// kernel(msg, stores) -> Antwort (oder Promise); null = Client existiert nicht.
// windows: [{id, kernel}] (kernel null = Fenster ohne Kernel, antwortet nie); workers: [id] (Worker-Clients, ohne Kernel).
// timeoutMs: verkuerzte Frist LIB_MISS_TIMEOUT_MS (die Datei selbst sagt 15000).
function loadSw({ kernel = null, clientId = 'tab-1', windows = null, workers = [], timeoutMs = null } = {}) {
  const stores = { meta: new Map(), artifacts: new Map() };
  const listeners = {};
  const messages = [];
  const targets = []; // Client-ID je lib-miss-Anforderung
  const hits = []; // {at, to, msg} je ctox:lib-hit-Meldung
  const mk = (id, type, k) => ({
    id, type, focused: false,
    postMessage(msg, ports = []) {
      const copy = JSON.parse(JSON.stringify(msg)); // vm-Realm -> Test-Realm
      if (copy.type === 'ctox:lib-hit') { hits.push({ at: Date.now(), to: id, msg: copy }); return; }
      messages.push(copy);
      targets.push(id);
      if (!k) return; // Worker / Fenster ohne Kernel: antwortet nie
      const [port] = ports;
      Promise.resolve(k(msg, stores)).then((reply) => { port.postMessage(reply); port.close(); });
    },
  });
  const all = [
    ...(windows ? windows.map((w) => mk(w.id, 'window', w.kernel)) : (kernel ? [mk(clientId, 'window', kernel)] : [])),
    ...workers.map((id) => mk(id, 'worker', null)),
  ];
  const self = {
    addEventListener(type, fn) { listeners[type] = fn; },
    location: { origin: 'https://ctox.test', href: 'https://ctox.test/sw.js?db=ctox-inst-u1' },
    clients: {
      get: async (id) => all.find((c) => c.id === id),
      matchAll: async (o = {}) => all.filter((c) => !o.type || o.type === 'all' || c.type === o.type),
      claim: async () => {},
    },
  };
  let src = SW_SRC;
  if (timeoutMs != null) {
    assert.ok(src.includes(TIMEOUT_DECL), 'Frist steht in sw.js');
    src = src.replace(TIMEOUT_DECL, `const LIB_MISS_TIMEOUT_MS = ${timeoutMs};`);
  }
  const ctx = { self, indexedDB: fakeIndexedDb(stores), crypto: globalThis.crypto, URL, TextDecoder, TextEncoder, Response, MessageChannel, queueMicrotask, setTimeout, clearTimeout };
  vm.createContext(ctx);
  vm.runInContext(src, ctx);
  async function fetchPath(pathname, from = clientId) {
    let pending;
    listeners.fetch({ request: { url: `https://ctox.test${pathname}` }, clientId: from, respondWith(p) { pending = p; } });
    assert.ok(pending, `SW beantwortet ${pathname}`);
    const res = await pending;
    return { status: res.status, type: res.headers.get('Content-Type'), cache: res.headers.get('Cache-Control'), text: await res.text() };
  }
  // Kernel-Fenster meldet sich beim SW (postMessage({type:'ctox:kernel', leader})).
  const announce = (id, leader) => listeners.message({ data: { type: 'ctox:kernel', leader }, source: all.find((c) => c.id === id), waitUntil() {} });
  const addWindow = (id, k) => { all.unshift(mk(id, 'window', k)); };
  return { stores, messages, targets, hits, fetchPath, announce, addWindow };
}

// Kernel legt Dateiliste (meta files:lib:<name>@<M.m>) und Dateien ab (Spur 4 simuliert).
function putLib(stores, name, ver, map, { bytes = {} } = {}) {
  stores.meta.set(`files:lib:${name}@${ver}`, files(`lib:${name}`, ver, Object.fromEntries(Object.entries(map).map(([p, t]) => [p, sha(t)]))));
  for (const [p, t] of Object.entries(map)) stores.artifacts.set(sha(t), art(bytes[p] ?? t, sha(t)));
}
// App-/Shell-Version wie bei der Aktivierung durch den Kernel.
function putApp(stores, appId, ver, map) {
  stores.meta.set(`files:${appId}@${ver}`, files(appId, ver, Object.fromEntries(Object.entries(map).map(([p, c]) => [p, sha(c)]))));
  for (const c of Object.values(map)) stores.artifacts.set(sha(c), art(c));
}

const DOC = "export const doc = 'office'; export const base = 'ctox:lib/monaco@0/vs/';";

test('Treffer: Datei aus IDB, sha256 geprueft, immutable, ctox:lib in der Bibliothek mit gebundener Minor, keine Anforderung', async () => {
  const sw = loadSw({ kernel: () => ({ type: 'ctox:lib-abort', reason: 'darf nicht gefragt werden' }) });
  putLib(sw.stores, 'ctox-office', '1.4', { 'ctox-office-document.mjs': DOC });
  const r = await sw.fetchPath('/~a/lib:ctox-office@1.4~lib:monaco@0.32/ctox-office-document.mjs');
  assert.equal(r.status, 200);
  assert.match(r.type, /^text\/javascript/);
  assert.equal(r.cache, 'public, max-age=31536000, immutable', 'Minor in der URL -> dieselbe URL liefert immer dieselben Bytes');
  assert.equal(r.text, "export const doc = 'office'; export const base = '/~a/lib:monaco@0.32~lib:ctox-office@1.4/vs/';");
  assert.equal(sw.messages.length, 0);
  // nur die URL bestimmt die Antwort: eine neuere veroeffentlichte Minor aendert sie nicht
  putLib(sw.stores, 'ctox-office', '1.5', { 'ctox-office-document.mjs': 'export const doc = 1.5;' });
  assert.equal((await sw.fetchPath('/~a/lib:ctox-office@1.4~lib:monaco@0.32/ctox-office-document.mjs')).text, r.text);
  assert.equal((await sw.fetchPath('/~a/lib:ctox-office@1.4~lib:monaco@0.32/ctox-office-document.mjs')).status, 200);
});

test('Miss: Kernel-Anforderung mit Pfad/Grund, Warten ohne eigenen Timeout, dann Auslieferung', async () => {
  const sw = loadSw({
    kernel: async (msg, stores) => { await tick(300); putLib(stores, 'ctox-office', '1.4', { 'ctox-office-document.mjs': DOC }); return { type: 'ctox:lib-ready' }; },
  });
  const t0 = Date.now();
  const r = await sw.fetchPath('/~a/lib:ctox-office@1.4~lib:monaco@0.32/ctox-office-document.mjs');
  assert.equal(r.status, 200);
  assert.ok(Date.now() - t0 >= 290, 'SW wartet auf die Kernel-Antwort');
  assert.deepEqual(sw.messages, [{ type: 'ctox:lib-miss', lib: 'ctox-office', major: '1', path: 'ctox-office-document.mjs', ver: '1.4', sha: null, reason: 'manifest' }]);
});

test('Miss: Datei fehlt im artifacts-Store -> Grund artifact mit erwarteter sha256 und Minor', async () => {
  const sw = loadSw({ kernel: (msg, stores) => { stores.artifacts.set(msg.sha, art('x{}')); return { type: 'ctox:lib-ready' }; } });
  putLib(sw.stores, 'monaco', '0.32', { 'vs/editor.css': 'x{}' });
  sw.stores.artifacts.clear();
  const r = await sw.fetchPath('/~a/lib:monaco@0.32/vs/editor.css');
  assert.equal(r.status, 200);
  assert.match(r.type, /^text\/css/);
  assert.deepEqual(sw.messages, [{ type: 'ctox:lib-miss', lib: 'monaco', major: '0', path: 'vs/editor.css', ver: '0.32', sha: sha('x{}'), reason: 'artifact' }]);
});

test('sha256-Fehler: Kernel holt neu (Grund hash) -> 200; bleibt falsch -> 404 nach genau einer Anforderung', async () => {
  const fixed = loadSw({ kernel: (msg, stores) => { stores.artifacts.set(msg.sha, art(DOC)); return { type: 'ctox:lib-ready' }; } });
  putLib(fixed.stores, 'ctox-office', '1.4', { 'ctox-office-document.mjs': DOC }, { bytes: { 'ctox-office-document.mjs': 'manipuliert' } });
  const ok = await fixed.fetchPath('/~a/lib:ctox-office@1.4~lib:monaco@0.32/ctox-office-document.mjs');
  assert.equal(ok.status, 200);
  assert.deepEqual(fixed.messages.map((m) => m.reason), ['hash']);

  const broken = loadSw({ kernel: (msg, stores) => { stores.artifacts.set(msg.sha, art('wieder manipuliert', msg.sha)); return { type: 'ctox:lib-ready' }; } });
  putLib(broken.stores, 'ctox-office', '1.4', { 'ctox-office-document.mjs': DOC });
  broken.stores.artifacts.clear();
  const bad = await broken.fetchPath('/~a/lib:ctox-office@1.4~lib:monaco@0.32/ctox-office-document.mjs');
  assert.equal(bad.status, 404, 'manipulierte Datei wird nie ausgeliefert');
  assert.equal(broken.messages.length, 1, 'kein Endlos-Nachladen');
});

test('Kernel-Abbruch -> 404; kein Fenster binnen Frist -> 504 mit Grund', async () => {
  const sw = loadSw({ kernel: () => ({ type: 'ctox:lib-abort', reason: 'Spur 4 getrennt' }) });
  assert.equal((await sw.fetchPath('/~a/lib:ctox-office@1.4/a.mjs')).status, 404);
  assert.equal(sw.messages.length, 1);
  // Abbruch gilt, auch wenn bereits Bytes in IDB liegen: der SW folgt dem Kernel-Signal, nicht eigener Vermutung
  const late = loadSw({ kernel: (msg, stores) => { putLib(stores, 'ctox-office', '1.4', { 'a.mjs': 'x' }); return { type: 'ctox:lib-abort', reason: 'Budget' }; } });
  assert.equal((await late.fetchPath('/~a/lib:ctox-office@1.4/a.mjs')).status, 404);
  const none = loadSw({ kernel: null, timeoutMs: 200 });
  const r = await none.fetchPath('/~a/lib:ctox-office@1.4/a.mjs');
  assert.equal(r.status, 504);
  assert.match(r.text, /kein Fenster mit Kernel binnen 200 ms \(lib:ctox-office@1\.4\/a\.mjs\)/);
  assert.equal(r.cache, 'no-store');
});

test('Gleichzeitige Misses derselben Datei teilen sich eine Kernel-Anforderung; fremder Client -> erstes Fenster', async () => {
  const sw = loadSw({ kernel: async (msg, stores) => { await tick(50); putLib(stores, 'monaco', '0.32', { 'vs/loader.js': 'var a;' }); return { type: 'ctox:lib-ready' }; } });
  const all = await Promise.all([1, 2, 3].map(() => sw.fetchPath('/~a/lib:monaco@0.32/vs/loader.js', 'worker-x')));
  assert.deepEqual(all.map((r) => r.status), [200, 200, 200]);
  assert.equal(sw.messages.length, 1);
});

test('Nicht freigegeben, falsche Major, Pfad nicht im Manifest, Pfadflucht -> 404 ohne Kernel-Anforderung', async () => {
  const sw = loadSw({ kernel: () => ({ type: 'ctox:lib-ready' }) });
  putLib(sw.stores, 'ctox-office', '1.4', { 'ctox-office-document.mjs': DOC });
  for (const p of [
    '/~a/lib:lexical@0.1/lexical.mjs',
    '/~a/lib:ctox-office@2.0/ctox-office-document.mjs',
    '/~a/lib:ctox-office@1.4/gibt-es-nicht.mjs',
    '/~a/lib:ctox-office@1.4/a/%2e%2e/b.mjs', // (unkodiertes ../ normalisiert schon der URL-Parser des Browsers)
    '/~a/lib:ctox-office@1.4/%E0%A4%A',
    '/~a/lib:ctox-office@1/ctox-office-document.mjs', // Altform ohne Minor (§16.37) gibt es nicht mehr
    '/~a/lib:ctox-office@1.4~lib:ctox-office@1.5/ctox-office-document.mjs', // mehrdeutige Bindung
    '/~a/lib:ctox-office@1.4~lib:monaco@0.32~lib:monaco@0.33/ctox-office-document.mjs',
  ]) assert.equal((await sw.fetchPath(p)).status, 404, p);
  assert.equal(sw.messages.length, 0);
});

test('App-Datei: ctox:lib nur mit Bindung der App-URL (Minor fix je Instanz), ctox:shared ohne Lib-Bindung', async () => {
  const sw = loadSw({ kernel: () => ({ type: 'ctox:lib-abort' }) });
  const src = "import { doc } from 'ctox:lib/ctox-office@1/ctox-office-document.mjs';";
  putApp(sw.stores, 'documents', '3', { 'index.js': src });
  const bound = await sw.fetchPath('/~a/documents@3~shell@4.2~lib:ctox-office@1.4~lib:monaco@0.32/index.js');
  assert.equal(bound.status, 200);
  assert.equal(bound.text, "import { doc } from '/~a/lib:ctox-office@1.4~lib:monaco@0.32/ctox-office-document.mjs';");
  assert.equal(bound.cache, 'public, max-age=31536000, immutable');
  // eine zweite Instanz mit neuerer Minor bekommt eine andere URL; die erste bleibt unveraendert
  const newer = await sw.fetchPath('/~a/documents@3~shell@4.2~lib:ctox-office@1.5~lib:monaco@0.32/index.js');
  assert.equal(newer.text, "import { doc } from '/~a/lib:ctox-office@1.5~lib:monaco@0.32/ctox-office-document.mjs';");
  // ungebunden (ohne Lib-Bindung): kein Rueckgriff auf eine aktuelle Minor, Verweis bleibt und laedt nicht
  assert.equal((await sw.fetchPath('/~a/documents@3/index.js')).text, src);
  assert.equal((await sw.fetchPath('/~a/documents@3~lib:ctox-office@1.4~lib:ctox-office@1.5/index.js')).status, 404, 'mehrdeutig');

  const shared = "import { t } from 'ctox:shared/i18n.js'; import 'ctox:lib/monaco@0/vs/loader.js';";
  putApp(sw.stores, 'notes', '1', { 'index.js': shared });
  // ctox:shared ohne ~shell@ in der URL: kein Rueckgriff auf eine aktive Shell (auch nicht ueber Kernel-meta) -> 404
  sw.stores.meta.set('activation:shell', { key: 'activation:shell', app_id: 'shell', activated_ver: '4.2' });
  assert.equal((await sw.fetchPath('/~a/notes@1/index.js')).status, 404, 'ctox:shared ohne Shell-Bindung bleibt 404');
  const pinned = await sw.fetchPath('/~a/notes@1~shell@4.1~lib:monaco@0.32/index.js');
  // §16.50 Nr. 2: ctox:shared/ zeigt auf die exakt gebundene Shell-Wurzel samt ~lib:-Segmenten
  assert.equal(pinned.text, "import { t } from '/~a/shell@4.1~lib:monaco@0.32/shared/i18n.js'; import '/~a/lib:monaco@0.32/vs/loader.js';");
  assert.equal(pinned.cache, 'public, max-age=31536000, immutable');
  // Shell-Dateien gehoeren zu ihrer eigenen Version (app_ver der Shell-URL); ungebunden -> Wurzel ohne ~lib:
  putApp(sw.stores, 'shell', '4.1', { 'shared/a.js': shared });
  assert.equal((await sw.fetchPath('/~a/shell@4.1/shared/a.js')).text, "import { t } from '/~a/shell@4.1/shared/i18n.js'; import 'ctox:lib/monaco@0/vs/loader.js';");
  assert.equal((await sw.fetchPath('/~a/shell@4.1~lib:monaco@0.32/shared/a.js')).text, "import { t } from '/~a/shell@4.1~lib:monaco@0.32/shared/i18n.js'; import '/~a/lib:monaco@0.32/vs/loader.js';");
});

// Laborzaehlung: Modulgraph ab Rahmen (Shell-Instanz) und App-Instanz ueber den echten Fetch-Handler auflösen wie der
// Browser (relative Spezifizierer gegen die Datei-URL) und je shared-Datei die verschiedenen Modul-URLs zaehlen.
async function moduleUrls(sw, entries) {
  const seen = new Map(); // URL-Pfad -> Status
  const queue = [...entries];
  const SPEC = /(?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s*)(['"])([^'"\r\n]+)\1/g;
  while (queue.length) {
    const p = queue.shift();
    if (seen.has(p)) continue;
    const r = await sw.fetchPath(p);
    seen.set(p, r.status);
    if (r.status !== 200) continue;
    for (const m of r.text.matchAll(SPEC)) queue.push(new URL(m[2], `https://ctox.test${p}`).pathname);
  }
  return seen;
}

test('shared genau einmal geladen: Rahmen (relativ) und App (ctox:shared/) teilen eine Modul-URL je shared-Datei (§16.50 Nr. 2)', async () => {
  const sw = loadSw({ kernel: () => ({ type: 'ctox:lib-abort' }) });
  putApp(sw.stores, 'shell', '4.2', {
    'v4/frame/index.js': "import { wm } from '../../shared/window-manager.js'; import { t } from '../source/editor.js'; export const f = wm + t;",
    'v4/source/editor.js': "import { cc } from 'ctox:shared/context-chat.js'; export const t = cc;",
    'shared/window-manager.js': "import { cc } from './context-chat.js'; export const wm = cc;",
    'shared/context-chat.js': 'export const cc = 1;',
  });
  putApp(sw.stores, 'notes', '1', { 'index.js': "import { cc } from 'ctox:shared/context-chat.js'; import { wm } from 'ctox:shared/window-manager.js'; export default cc + wm;" });
  const LIBS = '~lib:ctox-office@1.4~lib:monaco@0.32';
  const seen = await moduleUrls(sw, [`/~a/shell@4.2${LIBS}/v4/frame/index.js`, `/~a/notes@1~shell@4.2${LIBS}/index.js`]);
  const per = (file) => [...seen.keys()].filter((u) => u.endsWith(`/shared/${file}`));
  assert.deepEqual([...seen.values()].filter((s) => s !== 200), [], JSON.stringify([...seen]));
  assert.deepEqual(per('context-chat.js'), [`/~a/shell@4.2${LIBS}/shared/context-chat.js`], 'context-chat.js genau einmal');
  assert.deepEqual(per('window-manager.js'), [`/~a/shell@4.2${LIBS}/shared/window-manager.js`], 'window-manager.js genau einmal');
});

test('Worker-Miss mit Fenster-Client: Anforderung geht an das Fenster mit Kernel, nie an den Worker; Treffer ebenso (§16.50 Nr. 1)', async () => {
  const sw = loadSw({
    windows: [{ id: 'tab-1', kernel: (msg, stores) => { putLib(stores, 'monaco', '0.32', { 'vs/base/worker/workerMain.js': 'self.x=1;' }); return { type: 'ctox:lib-ready' }; } }],
    workers: ['worker-1'], timeoutMs: 400,
  });
  const r = await sw.fetchPath('/~a/lib:monaco@0.32/vs/base/worker/workerMain.js', 'worker-1');
  assert.equal(r.status, 200, r.text);
  assert.deepEqual(sw.targets, ['tab-1']);
  await tick(1100);
  assert.deepEqual(sw.hits.map((h) => h.to), ['tab-1'], 'Treffermeldung aus dem Worker geht ans Kernel-Fenster');
});

test('Worker-Miss: Leiter-Fenster wird bevorzugt, sonst gemeldetes Kernel-Fenster vor erstem Fenster', async () => {
  const ready = (msg, stores) => { putLib(stores, 'monaco', msg.ver, { [msg.path]: 'var a;' }); return { type: 'ctox:lib-ready' }; }; // je Minor eigene Liste
  const sw = loadSw({ windows: [{ id: 'tab-1', kernel: () => ({ type: 'ctox:lib-abort', reason: 'kein Leiter' }) }, { id: 'tab-2', kernel: ready }, { id: 'tab-3', kernel: ready }], workers: ['worker-1'], timeoutMs: 400 });
  sw.announce('tab-1', false);
  sw.announce('tab-3', true);
  assert.equal((await sw.fetchPath('/~a/lib:monaco@0.32/vs/a.js', 'worker-1')).status, 200);
  assert.deepEqual(sw.targets, ['tab-3'], 'Leiter');
  sw.announce('tab-3', false);
  sw.announce('tab-2', true);
  assert.equal((await sw.fetchPath('/~a/lib:monaco@0.33/vs/b.js', 'worker-1')).status, 200);
  assert.deepEqual(sw.targets, ['tab-3', 'tab-2'], 'Leiterwechsel');
  // Anforderung aus einem Fenster bleibt bei diesem Fenster (sein eigener Kernel)
  assert.equal((await sw.fetchPath('/~a/lib:monaco@0.34/vs/c.js', 'tab-3')).status, 200);
  assert.deepEqual(sw.targets, ['tab-3', 'tab-2', 'tab-3']);
});

test('Worker-Miss ohne Fenster-Client: kein Senden an den Worker, 504 mit Grund nach der Frist; spaetes Fenster wird noch genommen', async () => {
  const sw = loadSw({ workers: ['worker-1'], timeoutMs: 300 });
  const t0 = Date.now();
  const r = await sw.fetchPath('/~a/lib:monaco@0.32/vs/base/worker/workerMain.js', 'worker-1');
  const ms = Date.now() - t0;
  assert.equal(r.status, 504);
  assert.match(r.text, /kein Fenster mit Kernel binnen 300 ms/);
  assert.ok(ms >= 280 && ms < 2000, `Frist eingehalten (${ms} ms)`);
  assert.deepEqual(sw.targets, [], 'nichts an den Worker');
  // Fenster erscheint waehrend der Frist -> Anforderung geht dorthin
  const late = loadSw({ workers: ['worker-1'], timeoutMs: 1500 });
  setTimeout(() => late.addWindow('tab-9', (msg, stores) => { putLib(stores, 'monaco', '0.32', { 'vs/x.js': 'var x;' }); return { type: 'ctox:lib-ready' }; }), 200);
  assert.equal((await late.fetchPath('/~a/lib:monaco@0.32/vs/x.js', 'worker-1')).status, 200);
  assert.deepEqual(late.targets, ['tab-9']);
});

test('Fenster antwortet nicht: 504 nach der Frist (Grund im Body); Standardfrist in sw.js = 15 s', async () => {
  assert.ok(SW_FILE_SRC.includes(TIMEOUT_DECL));
  const sw = loadSw({ windows: [{ id: 'tab-1', kernel: () => new Promise(() => {}) }], workers: ['worker-1'], timeoutMs: 300 });
  const t0 = Date.now();
  const r = await sw.fetchPath('/~a/lib:monaco@0.32/vs/a.js', 'worker-1');
  assert.equal(r.status, 504);
  assert.match(r.text, /keine Kernel-Antwort binnen 300 ms \(lib:monaco@0\.32\/vs\/a\.js\)/);
  assert.ok(Date.now() - t0 < 2000);
});

test('ctox:lib-hit: Treffer gebuendelt je Client, hoechstens eine Meldung je Sekunde, ohne Port', async () => {
  const sw = loadSw({ kernel: (msg, stores) => { stores.artifacts.set(msg.sha, art('var b;')); return { type: 'ctox:lib-ready' }; } });
  putLib(sw.stores, 'monaco', '0.32', { 'vs/a.js': 'var a;', 'vs/b.js': 'var b;' });
  sw.stores.artifacts.delete(sha('var b;'));
  for (let i = 0; i < 5; i += 1) assert.equal((await sw.fetchPath('/~a/lib:monaco@0.32/vs/a.js')).status, 200);
  assert.equal((await sw.fetchPath('/~a/lib:monaco@0.32/vs/b.js')).status, 200); // nach Miss geliefert -> zaehlt auch
  assert.equal((await sw.fetchPath('/~a/lib:monaco@0.32/vs/fehlt.js')).status, 404); // kein Treffer
  assert.equal(sw.hits.length, 0, 'noch gebuendelt');
  await tick(1100);
  assert.equal(sw.hits.length, 1, 'eine Meldung fuer alle Treffer');
  const byPath = Object.fromEntries(sw.hits[0].msg.hits.map((h) => [h.path, h]));
  assert.deepEqual(Object.keys(byPath).sort(), ['vs/a.js', 'vs/b.js']);
  assert.equal(byPath['vs/a.js'].n, 5);
  assert.deepEqual([byPath['vs/a.js'].lib, byPath['vs/a.js'].major, byPath['vs/a.js'].ver, byPath['vs/a.js'].sha], ['monaco', '0', '0.32', sha('var a;')]);
  assert.equal(typeof byPath['vs/a.js'].last_ms, 'number');
  assert.equal(sw.messages.length, 1, 'nur die eine lib-miss-Anforderung, Treffer laufen getrennt');
  // sofort weitere Treffer: naechste Meldung fruehestens 1 s nach der vorigen
  await sw.fetchPath('/~a/lib:monaco@0.32/vs/a.js');
  await tick(300);
  await sw.fetchPath('/~a/lib:monaco@0.32/vs/a.js');
  await tick(900);
  assert.equal(sw.hits.length, 2);
  assert.ok(sw.hits[1].at - sw.hits[0].at >= 1000, `Abstand ${sw.hits[1].at - sw.hits[0].at} ms`);
  assert.equal(sw.hits[1].msg.hits[0].n, 2);
});

test('Kernel-IDB-Layout: Binaerartefakt (Uint8Array) und Text, sha256 ueber die Bytes; ohne ?db= keine Quelle; fremde meta-Formen -> 404', async () => {
  const sw = loadSw({ kernel: () => ({ type: 'ctox:lib-abort' }) });
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 255]);
  putApp(sw.stores, 'notes', '2', { 'icon.png': png, 'index.js': 'export const a = 1;' });
  const bin = await sw.fetchPath('/~a/notes@2~shell@4.2/icon.png');
  assert.equal(bin.status, 200);
  assert.equal(bin.type, 'image/png');
  const text = await sw.fetchPath('/~a/notes@2~shell@4.2/index.js');
  assert.equal(text.text, 'export const a = 1;');
  assert.equal(text.cache, 'public, max-age=31536000, immutable');
  // Byte-Manipulation hinter dem Hash (Binaer) -> 404
  sw.stores.artifacts.set(sha(png), art(new Uint8Array([1, 2, 3]), sha(png)));
  assert.equal((await sw.fetchPath('/~a/notes@2~shell@4.2/icon.png')).status, 404);
  // altes Labor-Layout (Store manifests, artifacts {bytes}) wird nicht gelesen
  sw.stores.artifacts.set(sha('x'), art('x'));
  sw.stores.meta.set('files:old@1', { key: 'files:old@1', files: { 'index.js': sha('x') } }); // Liste nicht unter value
  assert.equal((await sw.fetchPath('/~a/old@1~shell@4.2/index.js')).status, 404, 'Dateiliste nur unter value');
  sw.stores.meta.set('files:old@1', files('old', '1', { 'index.js': sha('x') }));
  assert.equal((await sw.fetchPath('/~a/old@1~shell@4.2/index.js')).status, 200, 'Gegenprobe: Kernel-Form wird gelesen');
  sw.stores.artifacts.set(sha('x'), { hash: sha('x'), bytes: new TextEncoder().encode('x').buffer });
  assert.equal((await sw.fetchPath('/~a/old@1~shell@4.2/index.js')).status, 404, 'artifacts ohne content (altes {bytes})');
});

test('Ohne Datenbankname in der Registrierung (/sw.js ohne ?db=) -> 404 statt einer geratenen Datenbank', async () => {
  const sw = loadSw({ kernel: () => ({ type: 'ctox:lib-abort' }) });
  putApp(sw.stores, 'notes', '2', { 'index.js': 'export const a = 1;' });
  const SRC = readFileSync(path.resolve(HERE, '..', '..', 'sw.js'), 'utf8');
  const listeners = {};
  const ctx = { self: { addEventListener: (t, f) => { listeners[t] = f; }, location: { origin: 'https://ctox.test', href: 'https://ctox.test/sw.js' }, clients: { matchAll: async () => [] } }, indexedDB: fakeIndexedDb(sw.stores), crypto: globalThis.crypto, URL, TextDecoder, TextEncoder, Response, MessageChannel, queueMicrotask, setTimeout };
  vm.createContext(ctx);
  vm.runInContext(SRC, ctx);
  let pending;
  listeners.fetch({ request: { url: 'https://ctox.test/~a/notes@2~shell@4.2/index.js' }, clientId: '', respondWith(p) { pending = p; } });
  assert.equal((await pending).status, 404);
});
