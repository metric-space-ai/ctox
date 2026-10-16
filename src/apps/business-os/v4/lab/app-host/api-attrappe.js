// Shell v4 – Labor App-Host: Kernel-/App-API-Attrappe (nur Browser, kein Netz, keine Abhängigkeiten).
//
// Bildet die Schnittstelle nach ctox-sync-v4-architektur.md §10.2, §10.2a, §16.7 und §16.10 so weit nach, wie der
// App-Host-Prototyp sie braucht. Alles, was „Core“ heißt, ist hier simuliert (Senden, Annahme, Befehlsausführung,
// Veröffentlichung einer App-Version). Die echte Implementierung kommt vom Strang „ctox sync v4“ (Stub/Kernel).
//
// Aufbau:
//   - IndexedDB `ctox-lab-app-host` mit den Stores docs / meta / outbox / window_state (Namen wie §16.7).
//   - Jeder Tab hält alle Dokumente im Speicher (Map je Collection) und liest beim Start aus IDB.
//   - Leiter per navigator.locks (`leader_gen` in meta, monoton). Nur der Leiter „sendet“ die Outbox, führt
//     simulierte Befehle aus, setzt Aktivierungszeiger und berechnet Messwerte.
//   - Datenänderungen, Schreibergebnisse, Aktivierungszeiger und Messwerte gehen per BroadcastChannel an alle Tabs;
//     zusätzlich gleichen Leser `meta.local_seq` bei Fokus/Sichtbarkeit und alle 2 s ab (verlorene Ereignisse).
//
// Schnittstelle für die Shell (§10.2a Nr. 8, §16.15):
//   kernel.appApi(appId, ver)            → Datenteil der App-API (query/get/put/patch/remove/command/setVisible/
//                                          metrics/actor); status und shell setzt die Shell darüber.
//   kernel.apps                          → LiveQuery der Apps mit active_ver und Aktivierungszustand
//   kernel.activation.onChange(cb)       → cb({appId, ver, state, t0, error}) in jedem Tab
//   kernel.activation.current(appId)     → {ver, state}
//   kernel.artifacts.manifest(appId,ver) → {entry, styles, title, icon} | null
//   kernel.artifacts.url(appId,ver,path) → versionierte URL /~a/<app>@<ver>/<pfad>; bind(appId,ver) → {url(path)} je Mount
//   kernel.windowState.put/get/list/del  → IDB-Store window_state
//   kernel.metrics                       → LiveDoc
//   kernel.leader                        → LiveDoc {isLeader, gen}
//   kernel.lab.publish(appId, ver, opts) → simulierte Veröffentlichung (Core) + Aktivierung (Leiter)
//   kernel.lab.setLink(state), kernel.lab.setBacklog(n)

export const APP_API_VERSION = '4.0';
const DB_NAME = 'ctox-lab-app-host';
const DB_VERSION = 1;
const CHANNEL = 'ctox-lab-app-host';
const LOCK = 'ctox-lab-app-host-leader';

// Labor-Katalog der App-Versionen (im Produkt: business_app_versions + Manifest aus artifacts).
const CATALOG = {
  formular: {
    title: 'Formular', icon: '▤',
    versions: { 1: { entry: 'index.js', styles: ['index.css'] }, 2: { entry: 'index.js', styles: ['index.css'] }, 3: { entry: 'index.js', styles: ['index.css'] }, 4: { entry: 'index.js', styles: ['index.css'] } },
    initial: '1',
  },
  notizen: {
    title: 'Notizen', icon: '✎',
    versions: { 1: { entry: 'index.js', styles: ['index.css'] }, 2: { entry: 'index.js', styles: ['index.css'] } },
    initial: '1',
  },
};

export class CtoxError extends Error {
  constructor(code, message, extra = {}) {
    super(message || code);
    this.name = 'CtoxError';
    this.code = code;
    Object.assign(this, extra);
  }
}

// ---------------------------------------------------------------------------------------------------------------
// Hilfen
// ---------------------------------------------------------------------------------------------------------------
const now = () => performance.timeOrigin + performance.now(); // tabübergreifend vergleichbare Uhr (ms, Epoche)
const clone = (v) => (v === undefined ? v : structuredClone(v));
const uid = () => (crypto.randomUUID ? crypto.randomUUID() : String(Math.random()).slice(2));

function idbOpen() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      db.createObjectStore('docs', { keyPath: 'k' });
      db.createObjectStore('meta', { keyPath: 'key' });
      db.createObjectStore('outbox', { keyPath: 'op_id' });
      db.createObjectStore('window_state', { keyPath: 'key' });
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}
function txDone(tx) {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onabort = tx.onerror = () => reject(tx.error || new Error('tx aborted'));
  });
}
function reqP(req) {
  return new Promise((resolve, reject) => { req.onsuccess = () => resolve(req.result); req.onerror = () => reject(req.error); });
}

// LiveDoc: get() + subscribe(cb) → Abmeldung. subscribe ruft cb sofort mit dem aktuellen Wert.
function createLiveDoc(initial) {
  let value = initial;
  const subs = new Set();
  return {
    get: () => value,
    subscribe(cb) { subs.add(cb); try { cb(value); } catch (e) { console.error(e); } return () => subs.delete(cb); },
    _set(v) { value = v; for (const cb of [...subs]) { try { cb(v); } catch (e) { console.error(e); } } },
  };
}

// ---------------------------------------------------------------------------------------------------------------
// Mango-Teilmenge (§10.2a Nr. 5, §16.10): fehlendes Feld ≠ null; $exists:false trifft fehlende Felder.
// ---------------------------------------------------------------------------------------------------------------
const MISSING = Symbol('missing');
function getPath(doc, path) {
  let cur = doc;
  for (const part of path.split('.')) {
    if (cur === null || typeof cur !== 'object' || !(part in cur)) return MISSING;
    cur = cur[part];
  }
  return cur;
}
function cmp(a, b) {
  if (a === b) return 0;
  if (a === MISSING || a === undefined) return -1;
  if (b === MISSING || b === undefined) return 1;
  if (a === null) return -1;
  if (b === null) return 1;
  if (typeof a === typeof b) return a < b ? -1 : a > b ? 1 : 0;
  return String(typeof a) < String(typeof b) ? -1 : 1;
}
const eq = (a, b) => (a === MISSING ? false : JSON.stringify(a) === JSON.stringify(b));
function matchOps(value, cond) {
  if (cond === null || typeof cond !== 'object' || Array.isArray(cond) || !Object.keys(cond).some((k) => k.startsWith('$'))) {
    return eq(value, cond) || (Array.isArray(value) && value.some((v) => eq(v, cond)));
  }
  for (const [op, arg] of Object.entries(cond)) {
    const present = value !== MISSING;
    switch (op) {
      case '$eq': if (!eq(value, arg)) return false; break;
      case '$ne': if (eq(value, arg)) return false; break;
      case '$gt': if (!present || value === null || cmp(value, arg) <= 0) return false; break;
      case '$gte': if (!present || value === null || cmp(value, arg) < 0) return false; break;
      case '$lt': if (!present || value === null || cmp(value, arg) >= 0) return false; break;
      case '$lte': if (!present || value === null || cmp(value, arg) > 0) return false; break;
      case '$in': if (!arg.some((a) => eq(value, a) || (Array.isArray(value) && value.some((v) => eq(v, a))))) return false; break;
      case '$nin': if (arg.some((a) => eq(value, a))) return false; break;
      case '$exists': if (present !== Boolean(arg)) return false; break;
      case '$regex': {
        const [src, flags] = arg instanceof RegExp ? [arg.source, arg.flags] : [String(arg), cond.$options || ''];
        if (typeof value !== 'string' || !new RegExp(src, flags.includes('i') ? 'i' : '').test(value)) return false;
        break;
      }
      case '$options': break;
      case '$not': if (matchOps(value, arg)) return false; break;
      case '$elemMatch': if (!Array.isArray(value) || !value.some((v) => (v && typeof v === 'object' ? matchSelector(v, arg) : matchOps(v, arg)))) return false; break;
      default: throw new CtoxError('invalid', `Operator ${op} nicht unterstützt`);
    }
  }
  return true;
}
export function matchSelector(doc, selector = {}) {
  for (const [key, cond] of Object.entries(selector)) {
    if (key === '$and') { if (!cond.every((s) => matchSelector(doc, s))) return false; continue; }
    if (key === '$or') { if (!cond.some((s) => matchSelector(doc, s))) return false; continue; }
    if (key === '$not') { if (matchSelector(doc, cond)) return false; continue; }
    if (!matchOps(getPath(doc, key), cond)) return false;
  }
  return true;
}
export function runQuery(docs, { selector = {}, sort = [], limit, skip = 0 } = {}) {
  const rows = docs.filter((d) => matchSelector(d, selector));
  const keys = sort.map((s) => Object.entries(s)[0]);
  rows.sort((a, b) => {
    for (const [field, dir] of keys) {
      const c = cmp(getPath(a, field), getPath(b, field));
      if (c) return dir === 'desc' ? -c : c;
    }
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0; // Gleichstände nach id (§16.10)
  });
  const from = Math.max(0, skip | 0);
  return rows.slice(from, limit == null ? undefined : from + limit);
}

// ---------------------------------------------------------------------------------------------------------------
// Kernel
// ---------------------------------------------------------------------------------------------------------------
export async function createKernel({ tabId = uid(), actor = { id: 'u-lab', display_name: 'Labor', role: 'admin', permissions: ['apps.source.view'] } } = {}) {
  const db = await idbOpen();
  const bc = new BroadcastChannel(CHANNEL);
  const mem = new Map(); // collection → Map(id → doc)
  const collSubs = new Map(); // collection → Set(fn)
  const pendingWrites = new Map(); // op_id → {resolveResult}
  const activationSubs = new Set();
  let activation = {}; // appId → {ver, state, t0, at, error}
  let localSeq = 0;
  const leader = createLiveDoc({ isLeader: false, gen: 0 });
  const metrics = createLiveDoc({ backlog: 0, outbox: { count: 0, oldest_ms: 0 }, link: { state: 'up', rtt_ms: 20, relay: false } });
  const labState = { link: 'up', backlog: 0 };
  const stats = { broadcastsIn: 0, broadcastsOut: 0, seqResyncs: 0 };

  const coll = (c) => { if (!mem.has(c)) mem.set(c, new Map()); return mem.get(c); };
  const notify = (c) => { for (const fn of [...(collSubs.get(c) || [])]) { try { fn(); } catch (e) { console.error(e); } } };
  const post = (msg) => { stats.broadcastsOut++; bc.postMessage(msg); };

  async function loadAll() {
    const tx = db.transaction(['docs', 'meta'], 'readonly');
    const [rows, metas] = await Promise.all([reqP(tx.objectStore('docs').getAll()), reqP(tx.objectStore('meta').getAll())]);
    mem.clear();
    for (const r of rows) coll(r.c).set(r.id, r.doc);
    const m = Object.fromEntries(metas.map((x) => [x.key, x.value]));
    localSeq = m.local_seq || 0;
    return m;
  }
  const initialMeta = await loadAll();
  activation = initialMeta.activation || {};

  // --- Schreibweg: Speicher sofort, IDB-Transaktion (docs + outbox + local_seq), dann Broadcast. ---------------
  function applyLocal(c, id, doc) {
    if (doc === null) coll(c).delete(id); else coll(c).set(id, doc);
    notify(c);
  }
  function write(c, id, nextDoc, kind) {
    const opId = uid();
    const prev = coll(c).has(id) ? coll(c).get(id) : null;
    let cancelled = false;
    let committed = false;
    let resolveResult;
    const result = new Promise((r) => { resolveResult = r; });
    applyLocal(c, id, nextDoc);
    const local = (async () => {
      await Promise.resolve(); // cancel() im selben Takt ist noch möglich
      if (cancelled) { applyLocal(c, id, prev); resolveResult({ status: 'rejected', reason: 'cancelled' }); return; }
      const tx = db.transaction(['docs', 'outbox', 'meta'], 'readwrite');
      const k = `${c}\u0000${id}`;
      if (nextDoc === null) tx.objectStore('docs').delete(k); else tx.objectStore('docs').put({ k, c, id, doc: nextDoc });
      tx.objectStore('outbox').put({ op_id: opId, c, id, kind, created_at: now(), tab: tabId });
      const metaStore = tx.objectStore('meta');
      const seqReq = metaStore.get('local_seq');
      seqReq.onsuccess = () => { localSeq = (seqReq.result?.value || 0) + 1; metaStore.put({ key: 'local_seq', value: localSeq }); };
      await txDone(tx);
      committed = true;
      pendingWrites.set(opId, { resolveResult });
      post({ type: 'doc', c, id, doc: nextDoc, seq: localSeq, at: now() });
      if (leader.get().isLeader) scheduleSend();
    })();
    return {
      local,
      result,
      cancel() { if (committed) return false; cancelled = true; return true; },
    };
  }

  // --- Leiter: Outbox „senden“ (simulierte RTT), Ergebnis an den schreibenden Tab, Befehle ausführen. ----------
  let sendTimer = null;
  function scheduleSend() { if (!sendTimer) sendTimer = setTimeout(sendOutbox, 20); }
  async function sendOutbox() {
    sendTimer = null;
    if (!leader.get().isLeader || labState.link !== 'up') return;
    const entries = await reqP(db.transaction('outbox', 'readonly').objectStore('outbox').getAll());
    if (!entries.length) return;
    await new Promise((r) => setTimeout(r, metrics.get().link.rtt_ms));
    const tx = db.transaction('outbox', 'readwrite');
    for (const e of entries) tx.objectStore('outbox').delete(e.op_id);
    await txDone(tx);
    for (const e of entries) {
      const res = { status: 'accepted', rev: `r-${Math.round(now())}` };
      deliverResult(e.op_id, res);
      post({ type: 'write-result', opId: e.op_id, result: res });
      if (e.c === 'business_commands') runCommand(e.id);
    }
  }
  function deliverResult(opId, res) {
    const p = pendingWrites.get(opId);
    if (p) { pendingWrites.delete(opId); p.resolveResult(res); }
  }
  const runningCommands = new Set();
  function runCommand(id) {
    if (runningCommands.has(id)) return;
    const doc = coll('business_commands').get(id);
    if (!doc || doc.status !== 'queued') return;
    runningCommands.add(id);
    const step = (ms, patch) => setTimeout(() => {
      const cur = coll('business_commands').get(id);
      if (cur) write('business_commands', id, { ...cur, ...patch }, 'patch');
    }, ms);
    step(80, { status: 'accepted' });
    step(200, { status: 'running', progress: 0.5 });
    if (doc.type === 'lab.fail') step(400, { status: 'failed', error: 'simulierter Fehler' });
    else step(400, { status: 'done', progress: 1, result: { echo: doc.payload } });
  }

  // --- Messwerte (Leiter berechnet, alle Tabs erhalten sie) -------------------------------------------------
  async function computeMetrics() {
    if (!leader.get().isLeader) return;
    const entries = await reqP(db.transaction('outbox', 'readonly').objectStore('outbox').getAll());
    const oldest = entries.reduce((m, e) => Math.min(m, e.created_at), Infinity);
    const m = {
      backlog: labState.backlog,
      outbox: { count: entries.length, oldest_ms: entries.length ? Math.round(now() - oldest) : 0 },
      link: { state: labState.link, rtt_ms: 20, relay: false },
    };
    metrics._set(m);
    post({ type: 'metrics', metrics: m });
    if (entries.length && labState.link === 'up') scheduleSend();
  }

  // --- Aktivierung (Leiter setzt Zeiger in meta; alle Tabs folgen) ------------------------------------------
  const activatedHistory = {}; // appId → [ver, …] in Aktivierungsreihenfolge (für reportFailed)
  const failedReports = [];
  function emitActivation(appId, rec) {
    if (rec.state === 'activated' && !rec.reverted) (activatedHistory[appId] ||= []).push(String(rec.ver));
    activation = { ...activation, [appId]: rec };
    for (const cb of [...activationSubs]) { try { cb({ appId, ...rec }); } catch (e) { console.error(e); } }
    appsDoc._set(appsList());
  }
  async function setActivation(appId, rec) {
    const tx = db.transaction('meta', 'readwrite');
    const store = tx.objectStore('meta');
    const cur = (await reqP(store.get('activation')))?.value || {};
    store.put({ key: 'activation', value: { ...cur, [appId]: rec } });
    await txDone(tx);
    emitActivation(appId, rec);
    post({ type: 'activation', appId, rec });
  }
  async function leaderPublish(appId, ver, { t0 = now(), delays = {} } = {}) {
    // Simuliert Core-Veröffentlichung + Client-Aktivierungsfolge received → fetched → data-ready → activated.
    if (!CATALOG[appId]?.versions[ver]) {
      await setActivation(appId, { ver, state: 'failed', t0, at: now(), error: `Version ${appId}@${ver} unbekannt` });
      return;
    }
    for (const state of ['received', 'fetched', 'data-ready']) {
      await setActivation(appId, { ver, state, t0, at: now() });
      const d = delays[state] || 0;
      if (d) await new Promise((r) => setTimeout(r, d));
    }
    await setActivation(appId, { ver, state: 'activated', t0, at: now() });
  }

  // Shell meldet Rückfall (§9.3 app_activation{failed}): Zeiger auf die vorige aktivierte Version zurück, alle Tabs folgen.
  async function leaderReportFailed(appId, ver, reason) {
    const cur = activation[appId];
    if (!cur || String(cur.ver) !== String(ver) || cur.state !== 'activated') return;
    const prev = [...(activatedHistory[appId] || [])].reverse().find((v) => v !== String(ver));
    if (!prev) return;
    await setActivation(appId, { ver: prev, state: 'activated', t0: cur.t0, at: now(), reverted: true, error: `${ver}: ${reason}` });
  }

  // --- Eingehende Broadcasts ---------------------------------------------------------------------------------
  bc.onmessage = (ev) => {
    stats.broadcastsIn++;
    const m = ev.data;
    switch (m.type) {
      case 'doc':
        localSeq = Math.max(localSeq, m.seq);
        applyLocal(m.c, m.id, m.doc);
        if (leader.get().isLeader) scheduleSend();
        break;
      case 'write-result': deliverResult(m.opId, m.result); break;
      case 'metrics': metrics._set(m.metrics); break;
      case 'activation': emitActivation(m.appId, m.rec); break;
      case 'publish-request': if (leader.get().isLeader) leaderPublish(m.appId, m.ver, m.opts); break;
      case 'report-failed': if (leader.get().isLeader) leaderReportFailed(m.appId, m.ver, m.reason); break;
      case 'lab-state': Object.assign(labState, m.state); if (leader.get().isLeader) computeMetrics(); break;
      default: break;
    }
  };

  // Leser-Abgleich (§16.7): local_seq bei Fokus, Sichtbarkeit und alle 2 s – rein lokal.
  async function resync() {
    const metaTx = db.transaction('meta', 'readonly');
    const seq = (await reqP(metaTx.objectStore('meta').get('local_seq')))?.value || 0;
    if (seq === localSeq) return;
    stats.seqResyncs++;
    const m = await loadAll();
    for (const c of collSubs.keys()) notify(c);
    const act = m.activation || {};
    for (const [appId, rec] of Object.entries(act)) {
      const cur = activation[appId];
      if (!cur || cur.ver !== rec.ver || cur.state !== rec.state) emitActivation(appId, rec);
    }
  }
  const resyncTimer = setInterval(resync, 2000);
  addEventListener('focus', resync);
  document.addEventListener('visibilitychange', resync);

  // Leiterwahl
  let metricsTimer = null;
  navigator.locks.request(LOCK, { mode: 'exclusive' }, async () => {
    const tx = db.transaction('meta', 'readwrite');
    const store = tx.objectStore('meta');
    const gen = ((await reqP(store.get('leader_gen')))?.value || 0) + 1;
    store.put({ key: 'leader_gen', value: gen });
    await txDone(tx);
    leader._set({ isLeader: true, gen });
    // Erstbefüllung (nur wenn leer) und Startaktivierung.
    if (!coll('eintraege').size) await seed();
    for (const [appId, def] of Object.entries(CATALOG)) {
      if (!activation[appId]) await setActivation(appId, { ver: def.initial, state: 'activated', t0: now(), at: now() });
    }
    metricsTimer = setInterval(computeMetrics, 500);
    scheduleSend();
    return new Promise(() => {}); // Sperre bis Tab-Ende halten
  });

  async function seed() {
    const tx = db.transaction(['docs'], 'readwrite');
    for (let i = 0; i < 40; i++) {
      const doc = { id: `e${String(i).padStart(3, '0')}`, titel: `Eintrag ${i}`, prio: i % 5, kategorie: ['a', 'b', 'c'][i % 3] };
      tx.objectStore('docs').put({ k: `eintraege\u0000${doc.id}`, c: 'eintraege', id: doc.id, doc });
      coll('eintraege').set(doc.id, doc);
    }
    await txDone(tx);
    post({ type: 'doc', c: 'eintraege', id: '__seed', doc: null, seq: localSeq });
    notify('eintraege');
  }

  // --- App-API (Datenteil) ------------------------------------------------------------------------------------
  function query(c, q = {}) {
    const run = () => runQuery([...coll(c).values()], q);
    return {
      get: run,
      subscribe(cb) {
        const fn = () => cb(run());
        if (!collSubs.has(c)) collSubs.set(c, new Set());
        collSubs.get(c).add(fn);
        fn();
        return () => collSubs.get(c)?.delete(fn);
      },
    };
  }
  function getDoc(c, id) {
    const doc = createLiveDoc(coll(c).get(id) ?? null);
    const fn = () => doc._set(coll(c).get(id) ?? null);
    return {
      get: () => coll(c).get(id) ?? null,
      subscribe(cb) {
        if (!collSubs.has(c)) collSubs.set(c, new Set());
        collSubs.get(c).add(fn);
        const off = doc.subscribe(cb);
        return () => { off(); collSubs.get(c)?.delete(fn); };
      },
    };
  }
  function rejectedHandle(err) {
    const local = Promise.reject(err);
    local.catch(() => {});
    return { local, result: new Promise(() => {}), cancel: () => false };
  }
  const actorPerms = createLiveDoc(actor.permissions);
  const actorApi = { id: actor.id, display_name: actor.display_name, role: actor.role, permissions: { get: actorPerms.get, subscribe: actorPerms.subscribe } };

  function appApi(appId, ver) {
    return {
      version: APP_API_VERSION,
      app: { id: appId, ver: String(ver) },
      query,
      get: getDoc,
      put(c, doc) {
        if (!doc || typeof doc.id !== 'string' || !doc.id) return rejectedHandle(new CtoxError('invalid', 'id fehlt', { path: 'id' }));
        try { return write(c, doc.id, clone(doc), 'put'); } catch (e) { return rejectedHandle(new CtoxError('invalid', String(e))); }
      },
      patch(c, id, p) {
        const cur = coll(c).get(id);
        if (!cur) return rejectedHandle(new CtoxError('not_found', `${c}/${id}`));
        return write(c, id, { ...cur, ...clone(p), id }, 'patch');
      },
      remove(c, id) { return write(c, id, null, 'remove'); },
      command(type, payload = {}) {
        const id = `cmd-${uid()}`;
        write('business_commands', id, { id, type, payload: clone(payload), status: 'queued', app_id: appId, created_at: now() }, 'put');
        const live = getDoc('business_commands', id);
        return { id, status: { get: () => live.get(), subscribe: live.subscribe } };
      },
      setVisible() {},
      metrics: { get: metrics.get, subscribe: metrics.subscribe },
      actor: actorApi,
    };
  }

  function appsList() {
    return Object.entries(CATALOG).map(([id, def]) => ({
      id, title: def.title, icon: def.icon,
      active_ver: activation[id]?.ver ?? def.initial, activation_state: activation[id]?.state ?? 'activated',
    }));
  }
  const appsDoc = createLiveDoc(appsList());

  // --- window_state ------------------------------------------------------------------------------------------
  const windowState = {
    async put(key, value) { const tx = db.transaction('window_state', 'readwrite'); tx.objectStore('window_state').put({ key, value, at: now() }); await txDone(tx); },
    async get(key) { return (await reqP(db.transaction('window_state', 'readonly').objectStore('window_state').get(key)))?.value ?? null; },
    async list(prefix = '') { const all = await reqP(db.transaction('window_state', 'readonly').objectStore('window_state').getAll()); return all.filter((r) => r.key.startsWith(prefix)); },
    async del(key) { const tx = db.transaction('window_state', 'readwrite'); tx.objectStore('window_state').delete(key); await txDone(tx); },
  };

  return {
    tabId,
    appApi,
    apps: { get: appsDoc.get, subscribe: appsDoc.subscribe },
    activation: {
      onChange(cb) { activationSubs.add(cb); return () => activationSubs.delete(cb); },
      current: (appId) => activation[appId] ?? { ver: CATALOG[appId]?.initial, state: 'activated' },
      reportFailed(appId, ver, reason) {
        failedReports.push({ appId, ver: String(ver), reason, tab: tabId, at: now() });
        if (leader.get().isLeader) leaderReportFailed(appId, String(ver), reason);
        else post({ type: 'report-failed', appId, ver: String(ver), reason });
      },
      reports: failedReports,
    },
    artifacts: {
      manifest(appId, ver) {
        const def = CATALOG[appId];
        const v = def?.versions[ver];
        return v ? { app_id: appId, app_ver: String(ver), title: def.title, icon: def.icon, ...v } : null;
      },
      url: (appId, ver, path) => `/~a/${appId}@${ver}/${path}`,
      // Bindung je Mount wie kernel.artifacts.bind (K-1d); die Attrappe kennt keine Shell-/Lib-Bindung in der URL.
      bind: (appId, ver) => Object.freeze({ appId, ver: String(ver), shellVer: null, libs: Object.freeze({}), root: `/~a/${appId}@${ver}/`, url: (path) => `/~a/${appId}@${ver}/${path}` }),
    },
    windowState,
    metrics: { get: metrics.get, subscribe: metrics.subscribe },
    leader: { get: leader.get, subscribe: leader.subscribe },
    stats,
    lab: {
      publish(appId, ver, opts = {}) {
        const o = { t0: now(), ...opts };
        if (leader.get().isLeader) leaderPublish(appId, String(ver), o);
        else post({ type: 'publish-request', appId, ver: String(ver), opts: o });
        return o.t0;
      },
      setLink(state) { labState.link = state; post({ type: 'lab-state', state: { link: state } }); computeMetrics(); },
      setBacklog(n) { labState.backlog = n; post({ type: 'lab-state', state: { backlog: n } }); computeMetrics(); },
      now,
    },
    close() { clearInterval(resyncTimer); clearInterval(metricsTimer); bc.close(); db.close(); },
  };
}
