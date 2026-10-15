import { artifactUrl, utf8Base64, bytesBase64 } from './artifact-url.mjs';
import { takeoverV3Database } from './migrate-v3.mjs';
import { snapshotBytes } from './snapshot.mjs';
import { evictCollection, reclaimCache, CLIENT_CACHE_BUDGET } from './cache.mjs';
import { KernelTransport, decimalSeq, seqGreater, maxSeq, checkpointKey } from './transport.mjs';
import { MemoryCoreTransport } from './fake-core.mjs';
import { assertLeader, acquireGeneration } from './leader.mjs';
// Local App API 4.0 kernel. Derived from the reviewed executable contract; no legacy runtime imports.
import { CtoxError, StaleLeader, cloneValue, deepFreeze, deepEqual, jsonEq, liveEqual, mergePatch, fnv1a, uuidv7, sha256Hex, hasBlob, isBlob, isPlainObject } from './values.mjs';
import { compileQuery, runCompiledQuery, normalizeJson, validateSchema } from './app-api.mjs';
import { openBackend, deleteBackend } from './store-idb-v5.mjs';
import { emptyBase, byOpId, docKey, nsName, applyOp, recompute } from './outbox.mjs';
import { SCHEMA_FILE, parseCollectionsFile, classifyCollections, applyMigrationSteps, manifestCollectionsToParsed, parsedToManifestCollections } from './schema-diff.mjs';
export { CtoxError, uuidv7, sha256Hex, sha256Js } from './values.mjs';
export { compileQuery, runCompiledQuery } from './app-api.mjs';
export { SCHEMA_FILE, parseCollectionsFile, classifyCollections, applyMigrationSteps } from './schema-diff.mjs';
export { createShellKernel } from './shell-api.mjs';
export const APP_API_VERSION = '4.0';
const SNAPSHOT_MAX_BYTES = 1024 * 1024;
const SYSTEM_COLLECTIONS = new Set(['business_commands', 'files', 'business_apps', 'business_app_versions', 'shell_preferences']);
// §16.19 Nr. 6: Shell-Layoutfelder im Manifest; fehlende Felder sind null.
export const LAYOUT_KEYS = Object.freeze(['icon_asset', 'icon_svg', 'category', 'frame_palette', 'default_width', 'default_height', 'min_width', 'min_height', 'presentation']);
export function normalizeLayout(l) {
  const out = {};
  for (const k of LAYOUT_KEYS) out[k] = l?.[k] ?? null;
  return out;
}
// §16.19 Nr. 3 / §16.17: Plattform-Collections (Eigentum ctox-platform). Schema aus
// docs/v4-manifests/ctox-platform.manifest.json; eine Zeile je Nutzer, Rechte nur der eigene Nutzer.
const PLATFORM_SCHEMAS = Object.freeze({
  shell_preferences: deepFreeze({
    primaryKey: 'id', type: 'object', additionalProperties: false, required: ['id'], indexes: ['user_id'],
    properties: {
      id: { type: 'string', maxLength: 180 }, user_id: { type: 'string' },
      pins: { type: 'array', items: { type: 'string' } }, launcher_order: { type: 'array', items: { type: 'string' } },
      settings: { type: 'object', additionalProperties: true }, updated_at_ms: { type: 'number' },
    },
  }),
});
const OWN_ROW_COLLECTIONS = new Set(['shell_preferences']);
// Tab-Identität für window_state: überlebt Neuladen desselben Tabs (sessionStorage), sonst je Kernel neu.
function sessionTabKey() {
  try {
    const ss = globalThis.sessionStorage;
    if (ss) {
      let k = ss.getItem('ctox-tab-key');
      if (!k) { k = uuidv7(); ss.setItem('ctox-tab-key', k); }
      return k;
    }
  } catch { /* kein sessionStorage */ }
  return uuidv7();
}
// §16.19 Nr. 6 / Befund K-1b: Artefakte sind Bytes. Text bleibt string (UTF-8), Binärinhalt wird Uint8Array;
// der Hash läuft immer über die Bytes, nie über String(content) ("[object Blob]").
export async function artifactContent(content) {
  if (typeof content === 'string') return content;
  if (isBlob(content)) return new Uint8Array(await content.arrayBuffer());
  if (content instanceof ArrayBuffer) return new Uint8Array(content.slice(0));
  if (ArrayBuffer.isView(content)) return new Uint8Array(content.buffer.slice(content.byteOffset, content.byteOffset + content.byteLength));
  throw new CtoxError('invalid', 'Artefaktinhalt muss string, Blob, ArrayBuffer oder Uint8Array sein', { path: 'files' });
}
const MIME_BY_EXT = Object.freeze({
  js: 'text/javascript', mjs: 'text/javascript', css: 'text/css', json: 'application/json', html: 'text/html', md: 'text/markdown',
  txt: 'text/plain', svg: 'image/svg+xml', png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp',
  ico: 'image/x-icon', woff: 'font/woff', woff2: 'font/woff2', wasm: 'application/wasm',
});
export function artifactMime(path) {
  const ext = /\.([a-z0-9]+)$/i.exec(String(path ?? ''))?.[1]?.toLowerCase();
  return MIME_BY_EXT[ext] ?? 'application/octet-stream';
}
const MANIFEST_FILE = 'manifest.json';
// Migrationsfunktion (migrate_fn) aus Entwurfstext laden: Blob-URL im Browser, sonst data:-URL.
async function importModuleText(text) {
  let url, revoke = null;
  if (typeof window !== 'undefined' && typeof URL.createObjectURL === 'function' && hasBlob) { url = URL.createObjectURL(new Blob([text], { type: 'text/javascript' })); revoke = url; }
  else url = `data:text/javascript;base64,${utf8Base64(text)}`;
  try { return await import(/* @vite-ignore */ url); } finally { if (revoke) URL.revokeObjectURL(revoke); }
}
// Vom Kern berechnete bzw. geschützte Manifest-Felder: weder per setManifest änderbar noch vom Basis-Manifest geerbt.
const MANIFEST_FIELDS_PROTECTED = new Set(['app_id', 'app_ver', 'files', 'generations', 'retires', 'provenance', 'author', 'created_at']);
const MANIFEST_FIELDS_COMPUTED = new Set([...MANIFEST_FIELDS_PROTECTED, 'title', 'entry', 'handlers', 'capabilities_required', 'min_app_api', 'layout', 'styles', 'collections']);
export function createKernel(opts = {}) {
  const instanceId = opts.instanceId;
  const userId = opts.userId ?? opts.actor?.id;
  if(typeof instanceId !== 'string' || !instanceId || typeof userId !== 'string' || !userId)throw new CtoxError('invalid','Authenticated instanceId and userId required');
  if(opts.actor?.id && opts.actor.id !== userId)throw new CtoxError('forbidden','Actor identity mismatch');
  const dbName = opts.dbName ?? `ctox-${instanceId}-${userId}`; // §16.7: DB je (Instanz, Nutzer)
  const latencyMs = opts.latencyMs ?? 50;
  const transport = opts.transport ?? new KernelTransport();
  const fakeCore = transport instanceof MemoryCoreTransport ? transport : null;
  const subscriptions = new Set();
  const scopeCursors = new Map();
  const scopeBacklog = new Map();
  const evicted = new Set();
  const lastUsed = new Map();
  let transportConnected = false;
  const remountTimeoutMs = opts.remountTimeoutMs ?? 10000;
  const tabKey = opts.tabKey ?? sessionTabKey();
  const capabilities = new Set(opts.capabilities ?? []);
  const tabId = uuidv7();
  const channelName = `ctox-kernel:${dbName}`;

  let db = null;
  let closed = false, loggedOut = false, loaded = false;
  let isLeader = false, myGen = 0, releaseLock = null, lockAbort = null;
  let bc = null, dropBroadcasts = false;
  const locks = opts.locks ?? globalThis.navigator?.locks;
  const hasLocks = !!locks?.request && typeof BroadcastChannel !== 'undefined' && opts.multiTab !== false;
  if(typeof window !== 'undefined' && opts.multiTab !== false && !hasLocks)throw new CtoxError('unavailable','Web Locks and BroadcastChannel required for multi-tab kernel');

  const mirror = new Map(); // ns -> Map(id -> record)
  let checkpoint = '0', localSeqKnown = 0;
  const activations = new Map();
  let link = { state: 'offline', rtt_ms: 0, relay: false };
  let extraBacklog = 0;
  const undelivered = [];
  let outboxStat = { count: 0, oldest_at: null };
  const waiters = new Map(); // op_id -> resolve(WriteResult)
  const cmdRejected = new Map();
  const zone = [];
  const mounts = new Set();
  const moduleRegistry = new Map(), moduleCache = new Map();
  const routeListeners = new Map();
  const windowStates = new Map();
  let locale = opts.locale ?? 'de';
  const translations = {};
  let theme = { ...(opts.theme ?? { '--ctox-bg': '#ffffff', '--ctox-fg': '#1d1d1f', '--ctox-accent': '#2f6fed' }) };
  let actor = { id: userId, display_name: 'Lokaler Nutzer', role: '', permissions: [], ...(opts.actor ?? {}) };
  const timers = new Set(), tasks = new Set(), deadlineCancels = new Set();
  const inflight = new Map(); // docKey -> op_id
  const inflightOps = new Set();
  const skipBeforeCommit = new Set();
  const uploadProgress = new Map(), uploadsDone = new Set();
  const activationListeners = new Set();
  const remountChains = new Map(), activationChains = new Map();
  const publishing = new Set();
  const doubleWrite = new Map(); // ns_old -> {ns_new, migrate}
  const sim0 = { reject: [], migrate: [], failCmd: new Map(), cmdHandlers: new Map() };
  const simRecord = { lastVisible: null, activationReports: [], navigations: [], dialogs: [], openedFiles: [], staleDiscards: 0, refetches: 0 };
  let corruptNextFetch = 0;
  let readyResolve;
  const ready = new Promise((r) => { readyResolve = r; });
  const preReady = [];

  // ---- Zeitgeber und Aufgaben ----
  function later(fn, ms) {
    const t = setTimeout(() => { timers.delete(t); if (!closed) fn(); }, ms);
    timers.add(t);
    return t;
  }
  const sleep = (ms) => new Promise((r) => later(r, ms));
  function track(p) {
    tasks.add(p);
    p.catch((err) => { if (!closed) console.error('[ctox-kernel]', err); }).finally(() => tasks.delete(p));
    return p;
  }
  function guardAlive() {
    if (closed || loggedOut) throw new CtoxError('unavailable', 'Kernel geschlossen (Abmeldung oder close)');
  }

  // ---- Live-Werte ----
  const tagIndex = new Map();
  const allLives = new Set();
  const dirty = new Set();
  let flushScheduled = false;
  const waitList = [];
  function notify(tag) {
    dirty.add(tag);
    if (!flushScheduled) { flushScheduled = true; queueMicrotask(flush); }
  }
  function flush() {
    flushScheduled = false;
    const hit = new Set();
    for (const t of dirty) for (const l of tagIndex.get(t) ?? []) hit.add(l);
    dirty.clear();
    for (const l of hit) l.refresh();
    for (const w of [...waitList]) {
      let ok = false;
      try { ok = w.pred(); } catch { ok = false; }
      if (ok) { waitList.splice(waitList.indexOf(w), 1); w.resolve(); }
    }
  }
  function waitUntil(pred) {
    if (pred()) return Promise.resolve();
    return new Promise((resolve) => waitList.push({ pred, resolve }));
  }
  function makeLive(tags, compute) {
    const subs = new Set();
    let last, has = false;
    const impl = {
      refresh() {
        if (!subs.size) return;
        const v = deepFreeze(compute());
        if (has && liveEqual(last, v)) return;
        last = v; has = true;
        for (const cb of [...subs]) { try { cb(v); } catch (e) { console.error('[ctox-kernel] subscriber', e); } }
      },
      dispose() { subs.clear(); for (const t of tags) tagIndex.get(t)?.delete(impl); },
    };
    return {
      get: () => { guardAlive(); return deepFreeze(compute()); },
      subscribe(cb) {
        if (typeof cb !== 'function') throw new CtoxError('invalid', 'subscribe braucht eine Funktion');
        if (closed || loggedOut) return () => {};
        if (!subs.size) { for (const t of tags) { if (!tagIndex.has(t)) tagIndex.set(t, new Set()); tagIndex.get(t).add(impl); } allLives.add(impl); }
        subs.add(cb);
        last = deepFreeze(compute()); has = true;
        try { cb(last); } catch (e) { console.error('[ctox-kernel] subscriber', e); }
        return () => {
          if (!subs.delete(cb) || subs.size) return;
          for (const t of tags) tagIndex.get(t)?.delete(impl);
          allLives.delete(impl);
          has = false;
        };
      },
    };
  }

  // ---- Broadcast ----
  function post(msg) {
    if (!bc || closed) return;
    try { bc.postMessage({ ...msg, from: tabId }); } catch (e) { console.error('[ctox-kernel] broadcast', e); }
  }

  // ---- Spiegel ----
  function mirrorGet(ns, id) { return mirror.get(ns)?.get(id) ?? null; }
  function visible(rec) { return rec && rec.data != null && !rec.deleted ? rec.data : null; }
  function mirrorPut(rec) {
    if (!rec) return false;
    if(rec.cache_evicted)evicted.add(rec.collection);
    let m = mirror.get(rec.collection);
    if (!m) { m = new Map(); mirror.set(rec.collection, m); }
    const prev = m.get(rec.id);
    if (prev && prev.rev === rec.rev && prev.seq === rec.seq && prev.pending === rec.pending && prev.deleted === rec.deleted && jsonEq(prev.data, rec.data)) return false;
    const r = { ...rec, data: rec.data == null ? null : deepFreeze(cloneValue(rec.data)) };
    m.set(rec.id, r);
    // localSeqKnown is advanced only after a complete reconciliation snapshot.
    notify(`c:${rec.collection}`);
    if (isLeader && (rec.collection === 'business_apps' || rec.collection === 'business_app_versions')) {
      const appId = rec.collection === 'business_apps' ? rec.id : visible(r)?.app_id;
      if (appId) scheduleActivation(appId);
    }
    if (rec.collection === 'business_commands') notify('commands');
    return true;
  }
  function setCheckpoint(cp) {
    if (seqGreater(cp, checkpoint)) { checkpoint = decimalSeq(cp); notify('checkpoint'); }
  }
  async function putDoc(t, rec) {
    const ls = ((await t.get('meta', 'local_seq'))?.value ?? 0) + 1;
    await t.put('meta', { key: 'local_seq', value: ls });
    rec.lseq = ls;
    await t.put('docs', rec);
    scheduleCacheBudget();
  }
  async function bumpLocalSeq(t) {
    const ls = ((await t.get('meta', 'local_seq'))?.value ?? 0) + 1;
    await t.put('meta', { key: 'local_seq', value: ls });
    return ls;
  }
  const assertGen = (t) => assertLeader(t, isLeader && !closed, myGen);
  const getCore = async () => ({ head: fakeCore?.head ?? checkpoint });

  let cacheBudgetTimer=null;
  function applyReclaim(result){
    for(const row of result.records)mirrorPut(row);
    for(const collection of result.collections){evicted.add(collection);scopeCursors.delete(collection);subscriptions.delete(collection);}
    if(result.records.length)post({type:'docs',keys:result.records.map(row=>[row.collection,row.id])});
  }
  function scheduleCacheBudget(){
    if(!isLeader || closed || cacheBudgetTimer)return;
    cacheBudgetTimer=later(()=>{
      cacheBudgetTimer=null;
      track(reclaimCache(db,assertGen,lastUsed,SYSTEM_COLLECTIONS,CLIENT_CACHE_BUDGET).then(result=>{
        applyReclaim(result);
        if(!result.within_budget)zoneReport('kernel',{level:'warn',code:'storage.cache_budget',text:'Cache budget exceeded by protected input; nothing was discarded'},'kernel');
        else zoneClear('kernel','storage.cache_budget');
      }).catch(err=>{if(err instanceof StaleLeader)stepDown();else throw err;}));
    },500);
  }

  // ---- Metriken ----
  async function refreshOutboxStat() {
    if (!db || closed) return;
    const next = await db.outboxStat();
    if (next.count !== outboxStat.count || next.oldest_at !== outboxStat.oldest_at) { outboxStat = next; notify('metrics'); }
  }
  function netChanged() {
    notify('metrics');
    if (isLeader) {
      const state={link:{...link},backlog:(fakeCore?.backlog ?? 0)+extraBacklog};
      post({type:'net',...state});
      if(db && !closed)track(db.tx(async t=>{
        await assertGen(t);const old=await t.get('meta','network');
        if(jsonEq(old?.value,state))return;
        await t.put('meta',{key:'network',value:state});await bumpLocalSeq(t);
      }).catch(err=>{if(err instanceof StaleLeader)stepDown();else throw err;}));
    }
  }
  let followerNet = null;
  const metricsLive = makeLive(['metrics'], () => {
    const net = isLeader || !followerNet ? { link, backlog: (fakeCore?.backlog ?? 0) + extraBacklog } : followerNet;
    return {
      backlog: net.backlog,
      outbox: { count: outboxStat.count, oldest_ms: outboxStat.oldest_at ? Math.max(0, Date.now() - outboxStat.oldest_at) : 0 },
      link: { ...net.link },
    };
  });
  let knownGen = 0;
  const leaderLive = makeLive(['leader'], () => ({ isLeader, leader_gen: isLeader ? myGen : knownGen, tab_id: tabId }));

  // ---- Zustandszone (je Tab, live, je App filterbar) ----
  function zoneReport(appId, item, source) {
    if (!item || !['info', 'warn', 'error'].includes(item.level)) throw new CtoxError('invalid', 'level muss info|warn|error sein', { path: 'level' });
    if (typeof item.code !== 'string' || !item.code) throw new CtoxError('invalid', 'code fehlt', { path: 'code' });
    const entry = Object.freeze({ app_id: appId, level: item.level, code: item.code, text: String(item.text ?? ''), ref: item.ref ?? null, source, at: Date.now() });
    const i = zone.findIndex((e) => e.app_id === appId && e.code === item.code);
    if (i >= 0) zone[i] = entry; else zone.push(entry);
    notify('status');
  }
  function zoneClear(appId, code) {
    const i = zone.findIndex((e) => e.app_id === appId && e.code === code);
    if (i >= 0) { zone.splice(i, 1); notify('status'); }
  }
  const zoneLive = makeLive(['status'], () => zone.slice());
  const statusZone = {
    get: zoneLive.get,
    subscribe: zoneLive.subscribe,
    forApp: (appId) => makeLive(['status'], () => zone.filter((e) => e.app_id === appId)),
    /** Eigene Einträge der Shell (source 'shell', §16.19 Nr. 1). */
    report: (appId, item) => zoneReport(appId, item, 'shell'),
    clear: (appId, code) => zoneClear(appId, code),
  };

  // ---- Laden, Leiterwahl ----
  async function loadFromStorage() {
    const docs = await db.all('docs');
    for (const r of docs) mirrorPut(r);
    const metas = await db.all('meta');
    for (const m of metas) {
      if(m.key.startsWith('upload_done:'))uploadsDone.add(m.ref);
      if (m.key === 'checkpoint') setCheckpoint(m.value);
      if (m.key.startsWith('cursor:'))scopeCursors.set(m.collection,m.cursor_to);
      else if (m.key === 'local_seq') localSeqKnown = Math.max(localSeqKnown, m.value);
      else if (m.key === 'leader_gen') knownGen = m.value;
      else if (m.key.startsWith('activation:')) activations.set(m.app_id, m);
    }
    notify('activation');
    await refreshOutboxStat();
  }
  async function reconcile() {
    if (!db || closed) return 0;
    const observed=(await db.get('meta','local_seq'))?.value??0;
    if(observed<=localSeqKnown)return 0;
    const snapshot = await db.read(async t => ({ metas: await t.all('meta'), docs: await t.all('docs') }));
    const ls = snapshot.metas.find(m=>m.key==='local_seq')?.value ?? 0;
    if(ls <= localSeqKnown)return 0;
    const known=localSeqKnown;let n=0;
    for(const record of snapshot.docs) if((record.lseq??0)>known && mirrorPut(record))n++;
    for(const meta of snapshot.metas){
      if(meta.key.startsWith('upload_done:')){uploadsDone.add(meta.ref);notify('uploads');}
      if(meta.key==='checkpoint')setCheckpoint(meta.value);
      if(meta.key==='network' && !isLeader){followerNet=meta.value;notify('metrics');}
      if(meta.key.startsWith('activation:'))applyActivationLocal(meta,false);
      if(meta.key.startsWith('result:') && waiters.has(meta.info.op_id))handleWriteResult(meta.info,meta.result);
    }
    localSeqKnown=ls;await refreshOutboxStat();if(isLeader)schedulePump();return n;
  }
  async function becomeLeader() {
    if (closed) return;
    myGen = await acquireGeneration(db);
    isLeader = true;
    // Ein neuer Leiter hat alles gelesen, was der alte geschrieben hat (gemeinsame IDB).
    for (const r of await db.all('docs')) mirrorPut(r);
    for (const m of await db.all('meta')) if (m.key.startsWith('activation:')) applyActivationLocal(m, false);
    notify('leader');
    post({ type: 'leader', gen: myGen });
    if (!transportConnected) {
      transportConnected = true;
      await transport.connect({ instanceId, userId }, {
        page: (page) => receivePage(page),
        link: (next) => { link = { ...link, ...next }; netChanged(); if(link.state === 'online') schedulePump(); },
        write_result: (message) => receiveResult(message),
        actor: (next) => { if(next.id !== userId)throw new CtoxError('forbidden','Actor identity mismatch');actor={...actor,...next};notify('actor'); },
      });
    }
    for (const collection of new Set(['business_apps', 'business_app_versions', 'business_commands', 'files', 'shell_preferences', ...mirror.keys()])) await subscribeCollection(collection);
    netChanged();scheduleCacheBudget();
    track(deliver());
    schedulePump();
    for (const id of mirror.get('business_apps')?.keys() ?? []) scheduleActivation(id);
  }
  function stepDown() {
    if (!isLeader) return;
    simRecord.staleDiscards++;
    isLeader = false;
    for (const op of inflightOps) inflightOps.delete(op);
    inflight.clear();
    notify('leader');
    const rel = releaseLock; releaseLock = null;
    if (rel) rel();
    if (closed) return;
    // neu anstellen; erneuter Erwerb erhöht leader_gen. Ohne Web Locks (Ein-Tab-Betrieb) sofort neu wählen.
    if (hasLocks) waitForLock();
    else track(becomeLeader());
  }
  function waitForLock() {
    if (!hasLocks) return;
    const ac = new AbortController();
    lockAbort = ac;
    locks.request(`ctox-kernel-leader:${dbName}`, { signal: ac.signal }, async () => {
      if (closed) return;
      const hold = new Promise((r) => { releaseLock = r; });
      await becomeLeader();
      await hold;
    }).catch(() => {});
  }
  async function elect() {
    if (!hasLocks) { await becomeLeader(); return; }
    await new Promise((resolveInitial) => {
      locks.request(`ctox-kernel-leader:${dbName}`, { ifAvailable: true }, async (lock) => {
        if (!lock) { resolveInitial(); waitForLock(); return; }
        const hold = new Promise((r) => { releaseLock = r; });
        await becomeLeader();
        resolveInitial();
        await hold;
      }).catch(() => resolveInitial());
    });
  }

  // ---- Broadcast-Empfang ----
  const rpcWaiters = new Map();
  async function onMessage(msg) {
    if (closed || dropBroadcasts || !msg || msg.from === tabId) return;
    switch (msg.type) {
      case 'docs': {
        for (const [ns, id] of msg.keys) mirrorPut(await db.get('docs', [ns, id]));
        if (msg.checkpoint) setCheckpoint(msg.checkpoint);
        await reconcile(); // keys may cover only the newest of several missed broadcasts
        await refreshOutboxStat();
        break;
      }
      case 'outbox': await refreshOutboxStat(); if (isLeader) schedulePump(); break;
      case 'priority': if(isLeader)transport.priority?.({app_id:msg.app_id,collections:msg.collections});break;
      case 'subscribe': if(isLeader && typeof msg.collection==='string')await subscribeCollection(msg.collection,'P1');break;
      case 'upload_done': uploadsDone.add(msg.ref);notify('uploads');break;
      case 'write_result': handleWriteResult(msg.info, msg.result); await refreshOutboxStat(); break;
      case 'net': followerNet = { link: msg.link, backlog: msg.backlog }; notify('metrics'); break;
      case 'hello': if (isLeader) netChanged(); break;
      case 'leader': knownGen = Math.max(knownGen, msg.gen ?? 0); notify('leader'); break;
      case 'activation': applyActivationLocal(msg.value, false); break;
      case 'actor': actor = msg.actor; notify('actor'); break;
      case 'upload': setUploadProgress(msg.ref, msg.sent, msg.total); break;
      case 'rpc': {
        if (!isLeader) break;
        try {
          const value = msg.name==='fetchBlob' ? await fetchBlob(...msg.args) : await simImpl[msg.name](...msg.args);
          post({ type: 'rpc_done', reqId: msg.reqId, value: value === undefined ? null : value });
        } catch (e) {
          post({ type: 'rpc_done', reqId: msg.reqId, error: { code: e?.code, message: String(e?.message ?? e) } });
        }
        break;
      }
      case 'rpc_done': {
        const w = rpcWaiters.get(msg.reqId);
        if (w) { rpcWaiters.delete(msg.reqId); msg.error ? w.rej(new CtoxError(msg.error.code ?? 'unavailable', msg.error.message)) : w.res(msg.value); }
        break;
      }
      default: break;
    }
  }
  // Core-Simulation läuft im Leiter; Leser leiten Sim-Aufrufe weiter (nur klonbare Argumente).
  function callLeader(name, args) {
    if (isLeader || !bc) return simImpl[name](...args);
    const reqId = uuidv7();
    return new Promise((res, rej) => {
      rpcWaiters.set(reqId, { res, rej });
      post({ type: 'rpc', reqId, name, args });
    });
  }

  // ---- Schreiben (§7.5, §16.6, §16.10) ----
  function handleFor(op_id, local, result, cancelFn) {
    local.catch(() => {});
    return { op_id, local, result, cancel: cancelFn };
  }
  function localWrite(app, op, collection, id, body) {
    const op_id = uuidv7();
    let resolveResult;
    const result = new Promise((r) => { resolveResult = r; });
    let entry = null;
    // Synchroner Teil: prüfen und optimistisch in den Spiegel (lokal sofort sichtbar, Flag pending).
    const prepare = () => {
      guardAlive();
      if (typeof collection !== 'string' || !collection) throw new CtoxError('invalid', 'collection fehlt', { path: 'collection' });
      if (typeof id !== 'string' || !id) throw new CtoxError('invalid', 'id (string) fehlt', { path: 'id' });
      if (!ownRowOk(collection, id)) throw new CtoxError('forbidden', `${collection}: nur die eigene Zeile (id = actor.id)`, { path: 'id' });
      const ns = resolveNs(app, collection);
      let clean = null;
      if (op === 'put') {
        clean = normalizeJson(body, '');
        if (!isPlainObject(clean)) throw new CtoxError('invalid', 'Dokument muss ein Objekt sein', { path: '' });
      } else if (op === 'patch') clean = normalizeJsonPatch(body);
      const cur = mirrorGet(ns, id);
      const st = applyOp(cur ? { data: cur.data, deleted: cur.deleted } : { data: null, deleted: false }, { op, body: clean, id });
      if (st.data) validateSchema(st.data, schemaFor(app, collection), ''); // lokale Schemaprüfung am Ergebnisdokument
      entry = {
        op_id, prev_op_id: cur?.pending && cur.rev?.startsWith('op:') ? cur.rev.slice(3) : null,
        collection: ns, id, op, body: clean, base_rev: cur?.rev ?? null, base_seq: decimalSeq(cur?.base?.seq ?? '0'),
        gen: genOf(ns), schema_ver: 1, app_id: app.id, app_ver: app.ver, created_at: Date.now(), origin_tab: tabId,
      };
      waiters.set(op_id, resolveResult);
      const base = cur?.base ?? emptyBase();
      mirrorPut({ collection: ns, id, data: st.data, deleted: st.deleted, rev: `op:${op_id}`, seq: base.seq, pending: true, base });
    };
    const persist = async () => {
      if (skipBeforeCommit.has(op_id)) {
        skipBeforeCommit.delete(op_id);
        waiters.delete(op_id);
        // optimistische Spiegelzeile zurücknehmen
        mirrorPut((await db.get('docs', [entry.collection, id])) ?? { collection: entry.collection, id, data: null, deleted: false, rev: null, seq: 0, pending: false, base: emptyBase() });
        return 'cancelled';
      }
      guardAlive();
      const ns = entry.collection;
      const rec = await db.tx(async (t) => {
        if(isLeader)await assertGen(t);
        const stored = await t.get('docs', [ns, id]);
        await t.put('outbox', entry); // docs + outbox in EINER Transaktion
        const r = recompute(ns, id, stored?.base ?? emptyBase(), await t.outboxFor(ns, id));
        await putDoc(t, r);
        return r;
      });
      mirrorPut(rec);
      post({ type: 'docs', keys: [[ns, id]], local_seq: rec.lseq });
      post({ type: 'outbox' });
      await refreshOutboxStat();
      if (isLeader) schedulePump();
      return 'ok';
    };
    let committed;
    try {
      if (loaded) { prepare(); committed = persist(); }
      else committed = ready.then(() => { prepare(); return persist(); });
    } catch (err) { committed = Promise.reject(err); }
    committed.then(
      (s) => { if (s === 'cancelled') resolveResult({ status: 'rejected', reason: 'cancelled' }); },
      (err) => {
        if(entry && db && !closed)void db.get('docs',[entry.collection,id]).then(record=>mirrorPut(record??{...recompute(entry.collection,id,emptyBase(),[]),data:null}));
        waiters.delete(op_id);
        if(err?.code==='quota')zoneReport(app.id,{level:'error',code:'storage.quota',text:'Local storage is full; the change was not saved'},'kernel');
        resolveResult({ status: 'rejected', reason: err instanceof CtoxError ? err.code : 'local_error' });
      },
    );
    const localDone = committed.then(() => undefined);
    return handleFor(op_id, localDone, result, () => cancelWrite(op_id, () => entry));
  }
  function normalizeJsonPatch(p) {
    if(!isPlainObject(p))throw new CtoxError('invalid','patch must be an object',{path:''});
    if(p.id!==undefined)throw new CtoxError('invalid','id cannot be changed by patch',{path:'id'});
    return normalizeJson(p,'');
  }
  function cancelWrite(op_id, getEntry) {
    if (closed || loggedOut) return false;
    const e = getEntry();
    if (!e) { skipBeforeCommit.add(op_id); return true; } // noch nicht vorbereitet
    // nur vor dem Senden: Leiter weiß es exakt; ein Leser nur, wenn der Leiter offline ist.
    const canCancel = isLeader ? !inflightOps.has(op_id) : (followerNet?.link?.state ?? 'online') !== 'online';
    if (!canCancel) return false;
    track((async () => {
      const rec = await db.tx(async (t) => {
        const queued = await t.get('outbox', op_id);
        if (!queued || queued.sent_gen) return null;
        await t.del('outbox', op_id);
        const stored = await t.get('docs', [e.collection, e.id]);
        const r = recompute(e.collection, e.id, stored?.base ?? emptyBase(), await t.outboxFor(e.collection, e.id));
        await putDoc(t, r);
        return r;
      });
      if (!rec) return;
      mirrorPut(rec);
      post({ type: 'docs', keys: [[e.collection, e.id]], local_seq: rec.lseq });
      const res = waiters.get(op_id);
      waiters.delete(op_id);
      res?.({ status: 'rejected', reason: 'cancelled' });
      await refreshOutboxStat();
      post({ type: 'outbox' });
    })());
    return true;
  }
  function handleWriteResult(info, result) {
    const res = waiters.get(info.op_id);
    if (res) { waiters.delete(info.op_id); res(cloneValue(result)); }
    if (result.status === 'rejected' && result.reason !== 'cancelled') {
      zoneReport(info.app_id, { level: 'error', code: `write.rejected:${info.op_id}`, text: `Änderung abgelehnt: ${result.reason}`, ref: result.archived_ref }, 'kernel');
      if (info.collection === 'business_commands') { cmdRejected.set(info.id, result.reason); notify('commands'); }
    }
  }

  // ---- Leiter: Outbox senden, simulierter Core bestätigt ----
  let pumping = false, pumpAgain = false, pumpScheduled = false;
  function schedulePump() {
    if (pumpScheduled || closed) return;
    pumpScheduled = true;
    later(() => { pumpScheduled = false; track(pump()); }, 0);
  }
  async function pump() {
    if (!isLeader || closed || link.state !== 'online') return;
    if (pumping) { pumpAgain = true; return; }
    pumping = true;
    try {
      do {
        pumpAgain = false;
        const entries = (await db.all('outbox')).sort(byOpId);
        const first = new Map();
        for (const e of entries) { const k = docKey(e.collection, e.id); if (!first.has(k)) first.set(k, e); }
        for (const [k, e] of first) {
          if (inflight.has(k) || !isLeader || link.state !== 'online') continue;
          inflight.set(k, e.op_id);
          inflightOps.add(e.op_id);
          if (e.op === 'upload') setUploadProgress(e.id, Math.floor((uploadProgress.get(e.id)?.total ?? 0) / 2), uploadProgress.get(e.id)?.total ?? 0, true);
          later(() => track(confirm(k, e)), latencyMs);
        }
      } while (pumpAgain);
    } finally { pumping = false; }
  }
  async function subscribeCollection(collection,priority='P2') {
    if (!isLeader || closed || !transportConnected) return;
    evicted.delete(collection);
    const scope = { epoch: opts.epoch ?? '1', collection, scope_hash: opts.scopeHash ?? 'local', gen: genOf(collection) };
    const cp = await db.get('meta', checkpointKey(scope));
    await transport.subscribe({ ...scope, checkpoint_seq: cp?.cursor_to ?? '0', priority });
    subscriptions.add(collection);
  }
  async function receivePage(page) {
    if (!isLeader || closed) return;
    const scope = { ...page, epoch: page.epoch ?? opts.epoch ?? '1', scope_hash: page.scope_hash ?? opts.scopeHash ?? 'local', gen: page.gen ?? genOf(page.collection) };
    if(scope.scope_hash !== (opts.scopeHash ?? 'local') || String(scope.epoch) !== String(opts.epoch ?? '1')) throw new CtoxError('revoked', 'Unexpected scope or epoch');
    if(!Number.isSafeInteger(scope.gen) || scope.gen!==genOf(page.collection))throw new CtoxError('invalid','Page generation does not match collection namespace');
    const cursor = decimalSeq(page.cursor_to ?? page.head_seq ?? '0');
    const out = await db.tx(async (t) => {
      await assertGen(t);
      const key = checkpointKey(scope), previous = await t.get('meta', key);
      const recs = [];
      for (const change of page.docs ?? []) {
        const ns = change.collection ?? page.collection;
        if(ns !== page.collection) throw new CtoxError('invalid', 'Page mixes collections');
        const seq = decimalSeq(change.seq);
        if(seqGreater(seq,cursor)) throw new CtoxError('invalid', 'Document exceeds page cursor');
        const old = await t.get('docs', [ns, change.id]);
        if(!old || seqGreater(seq,old.base?.seq ?? '0')) {
          const deleted = change.deleted || change.op === 'delete' || change.op === 'unproject';
          const base = {data: deleted ? null : cloneValue(change.data ?? change.doc),deleted:!!deleted,seq,rev:change.rev};
          const rec = recompute(ns,change.id,base,await t.outboxFor(ns,change.id));
          await putDoc(t,rec); recs.push(rec);
        }
      }
      await t.put('meta',{key,...scope,cursor_to:maxSeq(previous?.cursor_to ?? '0',cursor),complete:seqGreater(previous?.cursor_to??'0',cursor)?previous.complete:!!page.complete});
      const cp=maxSeq((await t.get('meta','checkpoint'))?.value ?? '0',cursor);
      await t.put('meta',{key:'checkpoint',value:cp});
      return {recs,cp};
    }).catch(err => { if(err instanceof StaleLeader){stepDown();return null;} throw err; });
    if(!out)return;
    for(const rec of out.recs)mirrorPut(rec);
    scopeCursors.set(page.collection,maxSeq(scopeCursors.get(page.collection)??'0',cursor));notify('checkpoint');
    scopeBacklog.set(checkpointKey(scope),Math.max(0,Number(page.backlog??0)));
    setCheckpoint(out.cp); extraBacklog=[...scopeBacklog.values()].reduce((a,b)=>a+b,0); netChanged();
    post({type:'docs',keys:out.recs.map(r=>[r.collection,r.id]),checkpoint:out.cp});
  }
  async function receiveResult(message) {
    const entry=await db.get('outbox',message.op_id);
    if(entry){await applyResult(entry,message);const key=docKey(entry.collection,entry.id);if(inflight.get(key)===entry.op_id)inflight.delete(key);inflightOps.delete(entry.op_id);await refreshOutboxStat();schedulePump();}
  }
  async function confirm(k,e) {
    let awaitingResult=false;
    try {
      // Mark dispatch and validate the generation in one durable transaction.
      const sent = await db.tx(async t=>{await assertGen(t);const entry=await t.get('outbox',e.op_id);if(!entry)return false;await t.put('outbox',{...entry,sent_gen:myGen});return true;});
      if(!sent || closed || !isLeader)return;
      if(e.op==='upload') {
        const blob=await db.get('blobs',e.id); await transport.upload(e.id,blob.blob);
        await db.tx(async t=>{await assertGen(t);await t.del('outbox',e.op_id);await t.put('blobs',{...blob,pinned:false});await t.put('meta',{key:`upload_done:${e.id}`,ref:e.id});await bumpLocalSeq(t);});
        setUploadProgress(e.id,blob.size,blob.size,true); uploadsDone.add(e.id);notify('uploads');post({type:'upload_done',ref:e.id});
      } else {
        if(!subscriptions.has(e.collection))await subscribeCollection(e.collection);
        const result=await transport.write(cloneValue(e));
        if(result)await applyResult(e,result);
        else awaitingResult=true;
      }
    } catch(err) {
      if(err instanceof StaleLeader)stepDown();
      else if(err?.code==='unavailable'){link={...link,state:'offline'};netChanged();}
      else zoneReport(e.app_id,{level:'error',code:'transport.write',text:String(err.message)},'kernel');
    } finally {
      if(!awaitingResult){if(inflight.get(k)===e.op_id)inflight.delete(k);inflightOps.delete(e.op_id);}
      await refreshOutboxStat(); if(!awaitingResult && link.state==='online')schedulePump();
    }
  }
  async function applyResult(e,result) {
    const out=await db.tx(async t=>{
      await assertGen(t);
      if(!(await t.get('outbox',e.op_id)))return null;
      const stored=await t.get('docs',[e.collection,e.id]); let base=stored?.base??emptyBase();
      const res={...result};delete res.doc;delete res.deleted;
      if(!['accepted','rejected','migrated'].includes(res.status))throw new CtoxError('invalid','Invalid write result');
      if(res.status==='rejected'){
        res.archived_ref=`archive_local:${e.op_id}`;
        await t.put('archive_local',{...e,ref:res.archived_ref,reason:res.reason,rejected_at:Date.now()});
      } else if(!seqGreater(base.seq??'0',result.seq??'0')) {
        const state=result.doc!==undefined ? {data:result.doc,deleted:!!result.deleted} : applyOp(base,e);
        base={...state,rev:res.rev,seq:decimalSeq(res.seq)};
      }
      await t.del('outbox',e.op_id);
      const rec=recompute(e.collection,e.id,base,await t.outboxFor(e.collection,e.id));await putDoc(t,rec);
      await t.put('meta',{key:`result:${e.op_id}`,info:{op_id:e.op_id,app_id:e.app_id,collection:e.collection,id:e.id},result:res});
      return {rec,res};
    });
    if(!out)return;
    mirrorPut(out.rec);setCheckpoint(out.rec.base.seq);
    const info={op_id:e.op_id,app_id:e.app_id,collection:e.collection,id:e.id};handleWriteResult(info,out.res);
    post({type:'docs',keys:[[e.collection,e.id]]});post({type:'write_result',info,result:out.res});
    const dw=doubleWrite.get(e.collection);
    if(fakeCore && dw && out.res.status!=='rejected')await corePush(dw.ns_new,[out.rec.base.deleted ? {id:e.id,_deleted:true} : {...dw.migrate(cloneValue(out.rec.base.data)),id:e.id}]);
  }
  const corePush = async (collection, docs) => {
    if(!fakeCore)throw new CtoxError('unavailable','Test core not configured');
    if(isLeader && !subscriptions.has(collection))await subscribeCollection(collection);
    const seqs=await fakeCore.push(collection,docs);netChanged();return seqs;
  };
  const coreView = async (collection,id) => fakeCore?.core.docs.get(fakeCore.key(collection,id))?.data ?? null;
  const coreMerge = async (ns,id,patch) => corePush(ns,[mergePatch(await coreView(ns,id)??{id},patch)]);
  const deliver = async () => { if(fakeCore)for(const c of subscriptions)await fakeCore.deliver(c); };

  // ---- App-Versionen, Generationen, Aktivierung (§9.3, §16.4) ----
  function versionDoc(appId, ver) { return visible(mirrorGet('business_app_versions', `${appId}@${ver}`)); }
  function genOf(ns) { const m = /@g(\d+)$/.exec(ns); return m ? Number(m[1]) : 1; }
  function resolveNs(app, collection) {
    if (SYSTEM_COLLECTIONS.has(collection) || collection.includes('@g')) return collection;
    const g = versionDoc(app.id, app.ver)?.manifest?.generations?.[collection] ?? 1;
    return nsName(collection, g);
  }
  function withDefaults(app,collection,data) {
    if(!data)return data;
    const schema=schemaFor(app,collection);let result=data;
    for(const [field,rule] of Object.entries(schema?.properties??{}))if(!(field in data) && rule.default!==undefined){
      if(result===data)result={...data};Object.defineProperty(result,field,{value:cloneValue(rule.default),enumerable:true,writable:true,configurable:true});
    }
    return deepFreeze(result);
  }
  function schemaFor(app, collection) { return PLATFORM_SCHEMAS[collection] ?? versionDoc(app.id, app.ver)?.manifest?.collections?.[collection]?.schema ?? null; }
  // §16.19 Nr. 3: shell_preferences – eine Zeile je Nutzer (id = actor.id), lesen und schreiben nur die eigene.
  function ownRowOk(collection, id) { return !OWN_ROW_COLLECTIONS.has(collection) || id === actor.id; }

  async function publishInternal(appId, ver, o = {}) {
    if (publishing.has(appId)) throw new CtoxError('conflict', `Veröffentlichung von ${appId} läuft bereits (busy)`);
    publishing.add(appId);
    try {
      const appDoc = visible(mirrorGet('business_apps', appId));
      if (o.baseAppVer !== undefined && (appDoc?.active_ver ?? null) !== o.baseAppVer) throw new CtoxError('conflict', 'stale_base');
      if (versionDoc(appId, ver)) throw new CtoxError('conflict', `Version ${appId}@${ver} existiert bereits`);
      const prevManifest = appDoc ? versionDoc(appId, appDoc.active_ver)?.manifest : null;
      const files = o.files ?? {};
      const fileHashes = {};
      for (const [path, raw] of Object.entries(files)) {
        const content = await artifactContent(raw); // Binärdateien (PNG …) bleiben Bytes
        const hash = await sha256Hex(content);
        fileHashes[path] = hash;
        if(!fakeCore)throw new CtoxError('unavailable','Publishing requires core publication transport');
        fakeCore.putArtifact(hash,content);
      }
      const generations = { ...(prevManifest?.generations ?? {}) };
      for (const c of o.incompatible ?? []) {
        const oldGen = generations[c] ?? 1;
        const newGen = oldGen + 1;
        generations[c] = newGen;
        const oldNs = nsName(c, oldGen), newNs = nsName(c, newGen);
        const fn = o.migrate?.[c];
        const migrate = typeof fn === 'function' ? fn : (d) => d;
        doubleWrite.set(oldNs, { ns_new: newNs, migrate }); // parallele Writes auf g<k> werden doppelt geschrieben
        const src = (await db.all('docs')).filter((r) => r.collection === oldNs && r.base?.data && !r.base.deleted);
        const migrated = src.map((r) => ({ ...migrate(cloneValue(r.base.data)), id: r.id }));
        if (migrated.length) await corePush(newNs, migrated);
      }
      const head = fakeCore?.head ?? '0';
      const M = o.migratedAtSeq ?? head;
      const manifest = {
        app_id: appId, app_ver: ver, files: fileHashes, entry: o.entry ?? 'index.js',
        collections: o.collections ?? prevManifest?.collections ?? {}, generations,
        handlers: o.handlers ?? {}, min_app_api: o.minAppApi ?? '4.0',
        layout: normalizeLayout(o.layout ?? prevManifest?.layout), styles: [...(o.styles ?? prevManifest?.styles ?? [])],
        capabilities_required: o.capabilitiesRequired ?? [],
        provenance: { base_release: null, base_hash: null, locally_modified: !!o.locallyModified },
        ...(o.manifestExtra ?? {}),
        author: actor.id, created_at: new Date().toISOString(),
      };
      await corePush('business_app_versions', [{ id: `${appId}@${ver}`, app_id: appId, app_ver: ver, manifest, migrated_at_seq: M }]);
      await corePush('business_apps', [{ ...(appDoc ?? {}), id: appId, title: o.title ?? appDoc?.title ?? appId, active_ver: ver, migrated_at_seq: M }]);
      return { app_id: appId, app_ver: ver, migrated_at_seq: M };
    } finally { publishing.delete(appId); }
  }
  function scheduleActivation(appId) {
    if (!isLeader || closed) return;
    const prev = activationChains.get(appId) ?? Promise.resolve();
    const next = prev.then(() => runActivation(appId)).catch((err) => {
      if (err instanceof StaleLeader) stepDown();
      else if (!closed) console.error('[ctox-kernel] activation', err);
    });
    activationChains.set(appId, next);
    track(next);
  }
  function isTerminalFor(appId, ver) {
    const a = activations.get(appId);
    return a && a.target_ver === ver && (a.state === 'activated' || a.state === 'failed');
  }
  async function runActivation(appId) {
    const appDoc = visible(mirrorGet('business_apps', appId));
    if (!appDoc?.active_ver || !isLeader) return;
    const ver = appDoc.active_ver;
    if (isTerminalFor(appId, ver)) return;
    const vdoc = versionDoc(appId, ver);
    if (!vdoc) return; // Manifest noch nicht da – Ankunft löst erneut aus
    const manifest = vdoc.manifest;
    const superseded = () => visible(mirrorGet('business_apps', appId))?.active_ver !== ver;
    await setActivation(appId, { target_ver: ver, state: 'received', error: null });
    // fetched: alle Dateien per Hash geprüft (aus „Spur 4“ = transport CAS)
    for (const [path, hash] of Object.entries(manifest.files ?? {})) {
      const cached=await db.get('artifacts',hash);
      if(cached && await sha256Hex(cached.content)===hash)continue;
      await waitUntil(() => link.state === 'online' || closed);
      // §16.15a Nr. 3 (wie Stub): sha256-Fehlschlag → einmal neu über Spur 4 holen, erst wiederholter Fehlschlag → failed.
      // Jeder Zweig endet in setActivation; kein Warten ohne Ende (K-1c Punkt 0).
      let content = null;
      for (let attempt = 0; attempt < 2 && content === null; attempt++) {
        let got = await transport.artifact(hash).then(artifactContent).catch(() => null);
        if (got == null) { await setActivation(appId, { target_ver: ver, state: 'failed', error: `artifact_missing:${path}` }); return; }
        if (corruptNextFetch > 0) { corruptNextFetch--; got = typeof got === 'string' ? `${got}\n/* beschädigt */` : Uint8Array.from([...got, 0]); }
        if ((await sha256Hex(got)) === hash) content = got;
        else simRecord.refetches++;
      }
      if (content === null) { await setActivation(appId, { target_ver: ver, state: 'failed', error: `hash_mismatch:${path}` }); return; }
      await db.tx(async (t) => { await assertGen(t); await t.put('artifacts', { hash, content, app_id: appId, app_ver: ver, path, stored_at: Date.now() }); });
    }
    await setActivation(appId, { target_ver: ver, state: 'fetched' });
    const missing = (manifest.capabilities_required ?? []).filter((c) => !capabilities.has(c));
    if (missing.length || compareVer(manifest.min_app_api ?? '4.0', APP_API_VERSION) > 0) {
      await setActivation(appId, { target_ver: ver, state: 'failed', error: `needs_update:${missing.join(',') || `app_api ${manifest.min_app_api}`}` });
      return;
    }
    const M = vdoc.migrated_at_seq ?? appDoc.migrated_at_seq ?? 0;
    const dataReady = () => Object.keys(manifest.collections??{}).every(collection=>{
      const ns=resolveNs({id:appId,ver},collection);
      return !seqGreater(M,scopeCursors.get(ns)??'0');
    }) && (!Object.keys(manifest.collections??{}).length ? !seqGreater(M,checkpoint) : true);
    for(const collection of Object.keys(manifest.collections??{}))await subscribeCollection(resolveNs({id:appId,ver},collection));
    await waitUntil(() => dataReady() || superseded() || closed || !isLeader);
    if (superseded() || closed || !isLeader) return;
    await setActivation(appId, { target_ver: ver, state: 'data-ready' });
    const prevAct = activations.get(appId);
    const good = [...(prevAct?.good_vers ?? []).filter((v) => v !== ver), ver].slice(-10); // §16.19 Nr. 2: Rückfallziele
    await setActivation(appId, { target_ver: ver, activated_ver: ver, previous_ver: prevAct?.activated_ver ?? null, good_vers: good, state: 'activated', at_seq: checkpoint });
  }
  function compareVer(a, b) {
    const pa = String(a).split('.').map(Number), pb = String(b).split('.').map(Number);
    for (let i = 0; i < Math.max(pa.length, pb.length); i++) { const d = (pa[i] ?? 0) - (pb[i] ?? 0); if (d) return d; }
    return 0;
  }
  async function setActivation(appId, patch) {
    const value = await db.tx(async (t) => {
      await assertGen(t);
      const cur = (await t.get('meta', `activation:${appId}`)) ?? { key: `activation:${appId}`, app_id: appId, activated_ver: null, target_ver: null, state: null, error: null };
      const v = { ...cur, ...patch, updated_at: Date.now() };
      await t.put('meta', v);
      await bumpLocalSeq(t);
      return v;
    });
    await transport.appActivation({ app_id: appId, app_ver: value.target_ver, state: value.state, at_seq: checkpoint, ...(value.state === 'failed' && value.error ? { error: value.error } : {}) });
    simRecord.activationReports.push({ app_id: appId, app_ver: value.target_ver, state: value.state, at_seq: checkpoint, error: value.error ?? undefined });
    applyActivationLocal(value, true);
    post({ type: 'activation', value });
    return value;
  }
  function applyActivationLocal(value, fromSelf) {
    const prev = activations.get(value.app_id);
    if (prev && prev.updated_at > value.updated_at) return;
    if (prev && jsonEq(prev, value)) return;
    activations.set(value.app_id, value);
    notify('activation');
    const ev = Object.freeze({ app_id: value.app_id, ver: value.target_ver, state: value.state, activated_ver: value.activated_ver, error: value.error ?? null });
    for (const cb of [...activationListeners]) { try { cb(ev); } catch (e) { console.error('[ctox-kernel] activation listener', e); } }
    if (value.state === 'failed' && value.error?.startsWith('needs_update')) {
      zoneReport(value.app_id, { level: 'error', code: 'app.needs_update', text: `App ${value.app_id}@${value.target_ver} benötigt ein Update dieses Teilnehmers (${value.error.slice(13)})`, ref: value.target_ver }, 'kernel');
    } else if (value.state === 'failed' && value.error) {
      zoneReport(value.app_id, { level: 'error', code: 'app.activation_failed', text: `Aktivierung ${value.app_id}@${value.target_ver} fehlgeschlagen: ${value.error}`, ref: value.target_ver }, 'kernel');
    }
    if (value.activated_ver) scheduleRemount(value.app_id, value.activated_ver);
    if(isLeader && value.state==='activated' && value.previous_ver && value.previous_ver!==value.activated_ver) {
      later(()=>track(retireGenerations(value.app_id,value.activated_ver,value.previous_ver)),10*60*1000);
    }
    void fromSelf;
  }

  async function retireGenerations(appId,currentVer,previousVer) {
    const current=activations.get(appId);
    if(!isLeader || current?.state!=='activated' || current.activated_ver!==currentVer)return;
    const before=versionDoc(appId,previousVer)?.manifest?.generations??{};
    const after=versionDoc(appId,currentVer)?.manifest?.generations??{};
    const namespaces=Object.keys(after).filter(c=>(before[c]??1)!==after[c]).map(c=>nsName(c,before[c]??1));
    const retired = await db.tx(async t=>{
      await assertGen(t);
      const out=[];
      const entries=await t.all('outbox');
      for(const ns of namespaces){
        if(entries.some(e=>e.collection===ns))continue;
        for(const row of await t.all('docs'))if(row.collection===ns){
          const empty={collection:row.collection,id:row.id,data:null,base:emptyBase(),rev:null,seq:'0',deleted:true,pending:false,retired:true};await putDoc(t,empty);out.push(empty);
        }
      }
      return out;
    });
    for(const row of retired)mirrorPut(row);
    post({type:'docs',keys:retired.map(r=>[r.collection,r.id])});
  }

  // ---- Mount / Remount (§10.2a Nr. 1, §16.10) ----
  async function loadModule(appId, ver) {
    const key = `${appId}@${ver}`;
    if (moduleRegistry.has(key)) return moduleRegistry.get(key);
    if (moduleCache.has(key)) return moduleCache.get(key);
    const manifest = versionDoc(appId, ver)?.manifest;
    if (!manifest) throw new CtoxError('not_found', `Manifest ${key} fehlt`);
    const hash = manifest.files?.[manifest.entry];
    const art = hash ? await db.get('artifacts', hash) : null;
    if (!art) throw new CtoxError('not_found', `Einstiegsdatei ${manifest.entry} von ${key} nicht lokal`);
    let url, revoke = null;
    if (globalThis.navigator?.serviceWorker?.controller) {
      url = artifactUrl(appId,ver,manifest.entry);
    } else if (typeof window !== 'undefined' && typeof URL.createObjectURL === 'function' && hasBlob) {
      url = URL.createObjectURL(new Blob([art.content], { type: 'text/javascript' }));
      revoke = url;
    } else {
      url = `data:text/javascript;base64,${typeof art.content === 'string' ? utf8Base64(art.content) : bytesBase64(art.content)}`;
    }
    try {
      const mod = await import(/* @vite-ignore */ url);
      moduleCache.set(key, mod);
      return mod;
    } finally { if (revoke) URL.revokeObjectURL(revoke); }
  }
  function mountFnOf(mod) {
    const fn = mod?.mount ?? mod?.default?.mount ?? (typeof mod?.default === 'function' ? mod.default : null);
    if (typeof fn !== 'function') throw new CtoxError('invalid', 'App-Modul exportiert kein mount()');
    return fn;
  }
  async function mountVersion(root, appId, ver, ctx) {
    const mod = await loadModule(appId, ver);
    const api = makeAppApi(appId, ver);
    const handle = await mountFnOf(mod)(root, api, ctx);
    if (!handle || typeof handle.unmount !== 'function') throw new CtoxError('invalid', 'mount() muss {unmount} liefern');
    return { handle, api };
  }
  function withTimeout(p, ms, onLate) {
    return new Promise((resolve,reject)=>{
      let ended=false;
      const clear=()=>{clearTimeout(timer);deadlineCancels.delete(cancel);};
      const cancel=()=>{if(!ended){ended=true;clear();reject(new CtoxError('unavailable',`Operation did not finish within ${ms} ms or kernel closed`));}};
      // Deadline timers also run during terminal cleanup; lifecycle `later()`
      // intentionally suppresses callbacks after close and cannot be used here.
      const timer=setTimeout(cancel,ms);deadlineCancels.add(cancel);
      Promise.resolve(p).then(value=>{
        if(ended){try{onLate?.(value);}catch{}return;}
        ended=true;clear();resolve(value);
      },error=>{if(!ended){ended=true;clear();reject(error);}});
    });
  }
  function checkSnapshot(appId, snap) {
    if (snap === undefined) return undefined;
    try {
      const c = structuredClone(snap);
      const size = snapshotBytes(c);
      if (size > SNAPSHOT_MAX_BYTES) throw new CtoxError('quota', 'Snapshot > 1 MiB');
      return c;
    } catch (e) {
      zoneReport(appId, { level: 'warn', code: 'app.snapshot_invalid', text: `Snapshot verworfen: ${e?.message ?? e}` }, 'kernel');
      return undefined;
    }
  }
  function scheduleRemount(appId, ver) {
    const targets = [...mounts].filter((m) => m.appId === appId && m.ver !== ver && !m.closed);
    if (!targets.length) return;
    const prev = remountChains.get(appId) ?? Promise.resolve();
    const next = prev.then(async () => {
      for (const m of targets) if (mounts.has(m) && m.ver !== ver) await remountOne(m, ver);
    });
    remountChains.set(appId, next.catch(() => {}));
    track(next);
  }
  async function remountOne(m, newVer) {
    const oldVer = m.ver;
    let snap;
    try { snap = m.handle.snapshot ? await withTimeout(Promise.resolve().then(()=>m.handle.snapshot()),remountTimeoutMs) : undefined; } catch (e) {
      zoneReport(m.appId, { level: 'warn', code: 'app.snapshot_failed', text: `snapshot() warf: ${e?.message ?? e}` }, 'kernel');
    }
    if(closed)return;
    snap = checkSnapshot(m.appId, snap);
    // §16.10: Snapshot vor dem Umschalten in window_state (IDB)
    await db.tx((t) => t.put('window_state', { key: `${m.appId}:${m.id}`, app_id: m.appId, mount_id: m.id, ver: oldVer, next_ver: newVer, snapshot: snap ?? null, at: Date.now() }));
    try { await withTimeout(Promise.resolve().then(()=>m.handle.unmount()),remountTimeoutMs); } catch (e) { console.error('[ctox-kernel] unmount', e); }
    if(closed)return;
    try {
      const r = await withTimeout(mountVersion(m.root, m.appId, newVer, { restored: cloneValue(snap) }), remountTimeoutMs, (late) => { try { Promise.resolve(late.handle.unmount()).catch(()=>{}); } catch {} });
      Object.assign(m, { ver: newVer, handle: r.handle, api: r.api });
      zoneClear(m.appId, 'app.remount_failed');
    } catch (err) {
      if(closed)return;
      zoneReport(m.appId, { level: 'error', code: 'app.remount_failed', text: `Version ${newVer} startet nicht (${err?.message ?? err}); vorherige Version ${oldVer} läuft`, ref: newVer }, 'kernel');
      const r = await withTimeout(mountVersion(m.root, m.appId, oldVer, { restored: cloneValue(snap) }),remountTimeoutMs);
      Object.assign(m, { ver: oldVer, handle: r.handle, api: r.api });
      try { await reportFailed(m.appId, newVer, `mount_failed: ${String(err?.message ?? err)}`); } catch (e) { console.error('[ctox-kernel] reportFailed', e); }
    }
    notify('mounts');
  }

  // ---- Datei-Blobs ----
  function setUploadProgress(ref, sent, total, broadcast) {
    uploadProgress.set(ref, { sent, total });
    notify(`upload:${ref}`);
    if (broadcast) post({ type: 'upload', ref, sent, total });
  }

  async function fetchBlob(ref) {
    guardAlive();
    if(!isLeader || link.state!=='online')throw new CtoxError('unavailable','File not cached and leader offline');
    const blob=await transport.blob(ref);
    if(!blob)throw new CtoxError('not_found',`File ${ref} missing`);
    if(await sha256Hex(blob)!==ref)throw new CtoxError('invalid','File content hash mismatch');
    await db.tx(async t=>{await assertGen(t);await t.put('blobs',{ref,blob,size:blob.size,type:blob.type,pinned:false,last_used:Date.now()});});
    scheduleCacheBudget();
    return ref; // readers consume the shared IDB cache, not a Blob over BroadcastChannel
  }

  // ---- App-API (§10.2 + §10.2a + §16.10) ----
  function makeAppApi(appId, appVer) {
    if (typeof appId !== 'string' || !appId) throw new CtoxError('invalid', 'appId fehlt');
    const app = Object.freeze({ id: appId, ver: String(appVer ?? activations.get(appId)?.activated_ver ?? '0') });
    const win = windowStates.get(appId) ?? { title: appId, header: null, size: { width: 0, height: 0 }, mode: 'normal', closed: false };
    windowStates.set(appId, win);
    const winTag = `win:${appId}`;
    const shell = Object.freeze({
      setTitle(title) { win.title = String(title); notify(winTag); },
      registerHeader(spec) { win.header = spec == null ? null : cloneValue(spec); notify(winTag); },
      navigate(target, params = {}) {
        const known = target === 'shell' || !!visible(mirrorGet('business_apps', target));
        if (!known) {
          zoneReport(appId, { level: 'error', code: 'shell.navigate.unknown_target', text: `Navigationsziel unbekannt: ${target}`, ref: String(target) }, 'kernel');
          return false;
        }
        const ev = Object.freeze({ from: appId, to: target, params: cloneValue(params) });
        simRecord.navigations.push(ev);
        for (const cb of [...(routeListeners.get(target) ?? [])]) { try { cb(ev); } catch (e) { console.error(e); } }
        return true;
      },
      onRoute(cb) {
        if (!routeListeners.has(appId)) routeListeners.set(appId, new Set());
        routeListeners.get(appId).add(cb);
        return () => routeListeners.get(appId)?.delete(cb);
      },
      locale: makeLive(['locale'], () => locale),
      t: (key) => translations[locale]?.[key] ?? key,
      theme: makeLive(['theme'], () => ({ ...theme })),
      dialog(spec) { simRecord.dialogs.push({ app_id: appId, kind: 'dialog', spec: cloneValue(spec) }); return Promise.resolve(spec?.default ?? null); },
      menu(spec) { simRecord.dialogs.push({ app_id: appId, kind: 'menu', spec: cloneValue(spec) }); return Promise.resolve(spec?.default ?? null); },
      window: Object.freeze({
        size: makeLive([winTag], () => ({ ...win.size })),
        mode: makeLive([winTag], () => win.mode),
        close() { win.closed = true; notify(winTag); },
      }),
      openFile(ref) { simRecord.openedFiles.push({ app_id: appId, ref }); },
    });
    const files = Object.freeze({
      async get(ref) {
        guardAlive();
        const local = await db.get('blobs', ref);
        if (local) {
          await db.tx((t) => t.put('blobs', { ...local, last_used: Date.now() }));
          return local.blob;
        }
        if(isLeader)await fetchBlob(ref);else await callLeader('fetchBlob',[ref]);
        const fetched=await db.get('blobs',ref);
        if(!fetched)throw new CtoxError('unavailable','File unavailable after leader fetch');
        return fetched.blob;
      },
      put(blob, meta = {}) {
        let metaHandle = null, uploadOp = null, cancelledEarly = false, refValue = null, resolveResult;
        const result = new Promise((r) => { resolveResult = r; });
        const progressTag = () => `upload:${refValue}`;
        const progress = {
          get: () => (refValue ? uploadProgress.get(refValue) ?? { sent: 0, total: blob?.size ?? 0 } : { sent: 0, total: blob?.size ?? 0 }),
          subscribe(cb) {
            let off = null, stop = false;
            refP.then(() => { if (!stop) off = makeLive([progressTag()], () => progress.get()).subscribe(cb); }, () => {});
            cb(progress.get());
            return () => { stop = true; off?.(); };
          },
        };
        const refP = (async () => {
          guardAlive();
          if (!isBlob(blob)) throw new CtoxError('invalid', 'files.put braucht einen Blob', { path: 'blob' });
          refValue = await sha256Hex(blob);
          return refValue;
        })();
        refP.catch(() => {});
        const local = (async () => {
          const ref = await refP;
          if (!loaded) await ready;
          guardAlive();
          if (cancelledEarly) return;
          const id = meta.id ?? uuidv7();
          const op_id = uuidv7();uploadOp = op_id;
          setUploadProgress(ref, 0, blob.size, false);
          await db.tx(async (t) => {
            if(isLeader)await assertGen(t);
            const existing = await t.get('blobs', ref);
            await t.put('blobs', { ref, blob, size: blob.size, type: blob.type, pinned: true, last_used: Date.now(), ...(existing ? {} : {}) });
            await t.put('outbox', { op_id, collection: '__blobs', id: ref, op: 'upload', body: null, app_id: appId, app_ver: app.ver, created_at: Date.now(), origin_tab: tabId });
          });
          if(cancelledEarly){await db.tx(async t=>{await t.del('outbox',op_id);const b=await t.get('blobs',ref);if(b)await t.put('blobs',{...b,pinned:false});});return;}
          post({ type: 'outbox' });
          if (isLeader) schedulePump();
          metaHandle = localWrite(app, 'put', 'files', id, {
            id, name: String(meta.name ?? 'datei'), mime: String(meta.mime ?? (blob.type || 'application/octet-stream')),
            size: blob.size, ref, owner_app: appId,
          });
          await metaHandle.local;
        })();
        local.then(async () => {
          if (cancelledEarly || !metaHandle) { resolveResult({ status: 'rejected', reason: 'cancelled' }); return; }
          const r = await metaHandle.result;
          if (r.status !== 'rejected') await waitUntil(() => uploadsDone.has(refValue) || closed);
          resolveResult({ ...r, ref: refValue });
        }, (err) => resolveResult({ status: 'rejected', reason: err instanceof CtoxError ? err.code : 'local_error' }));
        local.catch(() => {});
        return {
          local, result, ref: refP,
          upload: Object.freeze({ progress }),
          cancel() {
            if (!metaHandle) { cancelledEarly = true; return true; }
            if(uploadOp && inflightOps.has(uploadOp))return false;
            const cancelled=metaHandle.cancel();
            if(cancelled && uploadOp)track(db.tx(async t=>{await t.del('outbox',uploadOp);const b=await t.get('blobs',refValue);if(b)await t.put('blobs',{...b,pinned:false});}));
            return cancelled;
          },
        };
      },
    });
    const actorApi = Object.freeze({
      get id() { return actor.id; },
      get display_name() { return actor.display_name; },
      get role() { return actor.role; },
      permissions: makeLive(['actor'], () => [...actor.permissions]),
    });
    return Object.freeze({
      version: APP_API_VERSION,
      app,
      query(collection, q = {}) {
        const cq = compileQuery(q);
        const ns = resolveNs(app, collection);
        lastUsed.set(ns,Date.now());
        if(evicted.has(ns)){if(isLeader)track(subscribeCollection(ns,'P1'));else post({type:'subscribe',collection:ns});}
        return makeLive([`c:${ns}`, 'actor'], () => {
          const m = mirror.get(ns);
          if (!m) return [];
          const docs = [];
          for (const r of m.values()) if (r.data != null && !r.deleted && ownRowOk(ns, r.id)) docs.push(withDefaults(app,collection,r.data));
          return runCompiledQuery(docs, cq);
        });
      },
      get(collection, id) {
        const ns = resolveNs(app, collection);
        lastUsed.set(ns,Date.now());
        if(evicted.has(ns)){if(isLeader)track(subscribeCollection(ns,'P1'));else post({type:'subscribe',collection:ns});}
        return makeLive([`c:${ns}`, 'actor'], () => (ownRowOk(ns, id) ? withDefaults(app,collection,visible(mirrorGet(ns,id))) : null));
      },
      put(collection, doc) { return localWrite(app, 'put', collection, doc?.id, doc); },
      patch(collection, id, patch) { return localWrite(app, 'patch', collection, id, patch); },
      remove(collection, id) { return localWrite(app, 'delete', collection, id, null); },
      command(type, payload = {}) {
        if (typeof type !== 'string' || !type) throw new CtoxError('invalid', 'Befehlstyp fehlt', { path: 'type' });
        const id = uuidv7();
        const write = localWrite(app, 'put', 'business_commands', id, {
          id, type, payload, status: 'queued', app_id: appId, actor_id: actor.id, created_at: new Date().toISOString(),
        });
        const status = makeLive(['c:business_commands', 'commands'], () => {
          const d = visible(mirrorGet('business_commands', id));
          if (!d) {
            if (cmdRejected.has(id)) return { status: 'failed', error: cmdRejected.get(id) };
            return { status: 'queued' };
          }
          const s = { status: d.status };
          if (d.progress !== undefined) s.progress = d.progress;
          if (d.result !== undefined) s.result = d.result;
          if (d.error !== undefined) s.error = d.error;
          return s;
        });
        write.result.then((r) => { if (r.status === 'rejected') { cmdRejected.set(id, r.reason); notify('commands'); } });
        return Object.freeze({ id, status, local: write.local, result: write.result, cancel: write.cancel });
      },
      setVisible(collections) {
        const resolved=[...(collections??[])].map(c=>resolveNs(app,c));
        if(isLeader)transport.priority?.({app_id:appId,collections:resolved});
        else post({type:'priority',app_id:appId,collections:resolved});
        simRecord.lastVisible = Object.freeze({ app_id: appId, collections:resolved, at: Date.now() });
      },
      status: Object.freeze({
        report(item) { zoneReport(appId, item, 'app'); },
        clear(code) { zoneClear(appId, code); },
      }),
      metrics: metricsLive,
      shell,
      actor: actorApi,
      files,
    });
  }

  // ---- Shell-Kernel-API (§10.2a Nr. 6–8) ----
  function hasPerm(p) { return actor.permissions.includes(p); }
  function requirePerm(p) { guardAlive(); if (!hasPerm(p)) throw new CtoxError('forbidden', `Recht ${p} fehlt`); }
  async function readArtifactContent(hash) {
    const a = await db.get('artifacts', hash);
    if (a) return a.content;
    const content = await transport.artifact(hash);
    if (content != null) return artifactContent(content);
    throw new CtoxError('not_found', `Artefakt ${hash} fehlt`);
  }
  // ---- Quelltext-API (§10.2a Nr. 7, §16.22 A1–A9, §16.26) ----
  // Entwurf je (App, Nutzer): die IDB gehört genau einem (Instanz, Nutzer) (§16.7), Zeile meta `draft:<app>`.
  // files: Pfad → Inhalt (Text oder Bytes), null = im Entwurf gelöscht. manifest.json ist keine Datei; Manifest-Felder
  // nur über draft.setManifest. Gleiche Regeln wie der Stub (kernel.source(appId) dort).
  const draftKey = (appId) => `draft:${appId}`;
  const activeVerOf = (appId) => visible(mirrorGet('business_apps', appId))?.active_ver ?? null;
  const asText = (c) => (typeof c === 'string' ? c : new TextDecoder().decode(c));
  function noManifestFile(path, field = 'path') {
    if (path === MANIFEST_FILE) throw new CtoxError('invalid', 'manifest.json ist im Entwurf keine Datei – Felder über draft.setManifest (§16.26)', { path: field });
  }
  function freshDraft(appId) {
    const base = activeVerOf(appId);
    const d = { key: draftKey(appId), app_id: appId, user_id: userId, base_app_ver: base, files: {}, manifest: {}, created_at: Date.now() };
    // §16.26: eine Basisdatei manifest.json wird beim Anlegen des Entwurfs verworfen
    if (base && versionDoc(appId, base)?.manifest?.files?.[MANIFEST_FILE]) d.files[MANIFEST_FILE] = null;
    return d;
  }
  async function draftOf(appId) {
    const d = await db.get('meta', draftKey(appId));
    if (!d) return null;
    // K-1-Zeile ({files, collections}) ohne Basis: Basis = aktive Version, Manifest leer.
    if (!Object.prototype.hasOwnProperty.call(d, 'base_app_ver')) return { ...freshDraft(appId), files: d.files ?? {} };
    return d;
  }
  // Wirksame Manifest-Verweise des Entwurfs: setManifest-Felder vor Basis-Manifest
  function effectiveRefs(appId, d) {
    const baseMan = d?.base_app_ver ? versionDoc(appId, d.base_app_ver)?.manifest : null;
    const m = d?.manifest ?? {};
    return {
      entry: m.entry ?? baseMan?.entry ?? 'index.js',
      handlers: { ...(m.handlers ?? baseMan?.handlers ?? {}) },
      styles: [...(m.styles ?? baseMan?.styles ?? [])],
    };
  }
  async function readVersionFile(appId, ver, path) {
    const hash = versionDoc(appId, ver)?.manifest?.files?.[path];
    if (!hash) throw new CtoxError('not_found', `${path} nicht in ${appId}@${ver}`);
    return readArtifactContent(hash);
  }
  // Lese-Ändere-Schreib-Schritt auf dem eigenen Entwurf in EINER Transaktion (Mehr-Tab: kein verlorenes Update).
  async function draftMutate(appId, mutate) {
    requirePerm('apps.source.edit');
    await db.tx(async (t) => {
      const stored = await t.get('meta', draftKey(appId));
      const d = !stored ? freshDraft(appId) : Object.prototype.hasOwnProperty.call(stored, 'base_app_ver') ? stored : { ...freshDraft(appId), files: stored.files ?? {} };
      mutate(d);
      d.updated_at = Date.now();
      await t.put('meta', d);
    });
  }
  async function baseSchemaOf(appId, base) {
    if (!base) return { collections: {}, migrations: {} };
    const man = versionDoc(appId, base)?.manifest;
    if (man?.files?.[SCHEMA_FILE]) return parseCollectionsFile(asText(await readArtifactContent(man.files[SCHEMA_FILE])));
    return manifestCollectionsToParsed(man?.collections);
  }
  function sourceApi(appId) {
    const viewBase = (d) => (d ? d.base_app_ver : activeVerOf(appId));
    async function readOverlay(path) {
      const d = await draftOf(appId);
      if (d && Object.prototype.hasOwnProperty.call(d.files, path)) {
        if (d.files[path] === null) throw new CtoxError('not_found', `${path} im Entwurf gelöscht`);
        return d.files[path];
      }
      const base = viewBase(d);
      if (!base) throw new CtoxError('not_found', `${path} nicht vorhanden (neue App ohne Basis)`);
      return readVersionFile(appId, base, path);
    }
    async function overlayPaths() {
      const d = await draftOf(appId);
      const base = viewBase(d);
      const paths = new Set(Object.keys(base ? versionDoc(appId, base)?.manifest?.files ?? {} : {}));
      for (const [p, t] of Object.entries(d?.files ?? {})) { if (t === null) paths.delete(p); else paths.add(p); }
      return paths;
    }
    return Object.freeze({
      async list(ver) {
        requirePerm('apps.source.view');
        if (ver !== undefined) return Object.keys(versionDoc(appId, ver)?.manifest?.files ?? {}).sort();
        return [...await overlayPaths()].sort();
      },
      async read(path, ver) {
        requirePerm('apps.source.view');
        if (ver !== undefined) return readVersionFile(appId, ver, path);
        return readOverlay(path);
      },
      draft: Object.freeze({
        // A4
        async info() {
          requirePerm('apps.source.view');
          const d = await draftOf(appId);
          return d ? { base_app_ver: d.base_app_ver, files: cloneValue(d.files), manifest: cloneValue(d.manifest ?? {}), updated_at: d.updated_at ?? d.created_at } : null;
        },
        // Text oder Bytes (Binärdateien wie PNG-Icons bleiben Bytes, Hash beim publish über die Bytes)
        async write(path, content) {
          requirePerm('apps.source.edit');
          if (typeof path !== 'string' || !path || content == null) throw new CtoxError('invalid', 'path und Inhalt nötig', { path: 'path' });
          noManifestFile(path);
          const value = await artifactContent(content);
          await draftMutate(appId, (d) => { d.files[path] = value; });
        },
        // A7
        async remove(path) {
          requirePerm('apps.source.edit');
          noManifestFile(path);
          if (!(await overlayPaths()).has(path)) throw new CtoxError('not_found', `${path} nicht vorhanden`, { path: 'path' });
          await draftMutate(appId, (d) => { d.files[path] = null; });
        },
        async rename(from, to) {
          requirePerm('apps.source.edit');
          if (typeof to !== 'string' || !to) throw new CtoxError('invalid', 'Zielpfad fehlt', { path: 'to' });
          noManifestFile(from, 'from');
          noManifestFile(to, 'to');
          const paths = await overlayPaths();
          if (!paths.has(from)) throw new CtoxError('not_found', `${from} nicht vorhanden`, { path: 'from' });
          if (paths.has(to)) throw new CtoxError('conflict', `${to} existiert bereits`, { path: 'to' });
          const content = await readOverlay(from);
          await draftMutate(appId, (d) => {
            d.files[to] = content;
            d.files[from] = null;
            // §16.26: Verweise im Manifest (Einstieg, Handler, Styles) wandern mit
            const refs = effectiveRefs(appId, d);
            if (refs.entry === from) d.manifest.entry = to;
            if (Object.values(refs.handlers).includes(from)) d.manifest.handlers = Object.fromEntries(Object.entries(refs.handlers).map(([type, p]) => [type, p === from ? to : p]));
            if (refs.styles.includes(from)) d.manifest.styles = refs.styles.map((x) => (x === from ? to : x));
          });
        },
        // A5: einzige Quelle der Manifest-Felder im Entwurf; null entfernt ein Feld (dann gilt wieder die Basis)
        async setManifest(fields) {
          requirePerm('apps.source.edit');
          if (!isPlainObject(fields)) throw new CtoxError('invalid', 'setManifest braucht ein Objekt', { path: '' });
          for (const k of Object.keys(fields)) if (MANIFEST_FIELDS_PROTECTED.has(k)) throw new CtoxError('invalid', `Manifest-Feld ${k} ist nicht änderbar`, { path: k });
          const clean = normalizeJson(fields, '');
          await draftMutate(appId, (d) => {
            const { handlers, ...rest } = clean;
            d.manifest = mergePatch(d.manifest ?? {}, rest);
            // handlers gegen die wirksame Karte (Basis + Entwurf) patchen, damit null auch Basis-Handler entfernt
            if (handlers !== undefined) d.manifest.handlers = handlers === null ? {} : mergePatch(effectiveRefs(appId, d).handlers, handlers);
          });
        },
        // A6/A9: vollständiger collections.schema.json-Inhalt; liefert den Diff gegen die Entwurfsbasis
        async setCollections(content) {
          requirePerm('apps.source.edit');
          const parsed = parseCollectionsFile(content);
          const obj = typeof content === 'string' ? JSON.parse(content) : content;
          const text = `${JSON.stringify(obj, null, 2)}\n`;
          await draftMutate(appId, (d) => { d.files[SCHEMA_FILE] = text; });
          const base = (await draftOf(appId))?.base_app_ver ?? null;
          return { base_app_ver: base, diff: classifyCollections(await baseSchemaOf(appId, base), parsed, { baseVer: base }) };
        },
        async discard() {
          requirePerm('apps.source.edit');
          await db.tx((t) => t.del('meta', draftKey(appId)));
        },
        async rebase(ver) {
          requirePerm('apps.source.edit');
          if (ver !== null && !versionDoc(appId, ver)) throw new CtoxError('not_found', `Version ${appId}@${ver} unbekannt`, { path: 'ver' });
          await draftMutate(appId, (d) => { d.base_app_ver = ver; });
        },
        // A2/A3: base_app_ver = null → neue App; ohne Angabe gilt die Basis des Entwurfs
        publish(o = {}) {
          let resolveResult;
          const result = new Promise((r) => { resolveResult = r; });
          const local = (async () => {
            guardAlive();
            if (!hasPerm('apps.source.edit')) { resolveResult({ status: 'rejected', reason: 'forbidden' }); return; }
            const d = await draftOf(appId);
            if (!d || (!Object.keys(d.files).length && !Object.keys(d.manifest ?? {}).length)) {
              resolveResult({ status: 'rejected', reason: 'invalid', detail: 'leerer Entwurf' });
              return;
            }
            const base = Object.prototype.hasOwnProperty.call(o ?? {}, 'base_app_ver') ? o.base_app_ver : d.base_app_ver;
            const r = await callLeader('publishDraft', [appId, base ?? null, d.files, d.manifest ?? {}]);
            if (r.status !== 'rejected') await db.tx((t) => t.del('meta', draftKey(appId))); // nur der eigene Entwurf
            resolveResult(r);
          })();
          local.catch((e) => resolveResult({ status: 'rejected', reason: e?.code ?? 'local_error', detail: String(e?.message ?? e) }));
          return { local: local.catch(() => {}), result, cancel: () => false };
        },
      }),
    });
  }
  const appsLive = makeLive(['c:business_apps', 'c:business_app_versions', 'activation', 'actor'], () => {
    const out = [];
    for (const r of mirror.get('business_apps')?.values() ?? []) {
      const d = visible(r);
      if (!d) continue;
      if (Array.isArray(d.roles) && !d.roles.includes(actor.role)) continue;
      const a = activations.get(d.id);
      // §16.19 Nr. 6: layout/styles der laufenden (sonst der veröffentlichten) Version
      const man = versionDoc(d.id, a?.activated_ver ?? d.active_ver)?.manifest ?? versionDoc(d.id, d.active_ver)?.manifest;
      out.push({
        id: d.id, title: d.title ?? d.id, active_ver: d.active_ver, activated_ver: a?.activated_ver ?? null, state: a?.state ?? 'received',
        target_ver: a?.target_ver ?? null, error: a?.error ?? null, layout: normalizeLayout(man?.layout), styles: [...(man?.styles ?? [])],
      });
    }
    return out.sort((x, y) => (x.id < y.id ? -1 : 1));
  });
  // window_state (§16.19 Nr. 3): je Tab (tabKey) und Nutzer (DB je Nutzer), überlebt Neuladen desselben Tabs
  const windowStateKey = (key) => {
    if (typeof key !== 'string' || !key) throw new CtoxError('invalid', 'key fehlt', { path: 'key' });
    return `tab:${tabKey}:${key}`;
  };
  async function windowStateDelete(key) { guardAlive(); const k = windowStateKey(key); await db.tx((t) => t.del('window_state', k)); }
  const windowStateApi = Object.freeze({
    async get(key) {
      guardAlive();
      const r = await db.get('window_state', windowStateKey(key));
      return r ? r.value : null;
    },
    async put(key, value) {
      guardAlive();
      const k = windowStateKey(key);
      let v;
      try { v = structuredClone(value); } catch { throw new CtoxError('invalid', 'Wert nicht structured-clone-fähig', { path: 'value' }); }
      await db.tx((t) => t.put('window_state', { key: k, tab_key: tabKey, user_id: userId, value: v, at: Date.now() }));
    },
    delete: windowStateDelete,
    del: windowStateDelete,
  });
  // §16.19 Nr. 2: Version für diesen Client failed, Zeiger zurück auf die letzte funktionierende, app_activation failed.
  function reportFailed(appId, ver, reason) {
    guardAlive();
    if (typeof appId !== 'string' || !appId || ver == null || ver === '') return Promise.reject(new CtoxError('invalid', 'appId und ver nötig', { path: 'appId' }));
    return callLeader('reportFailed', [appId, String(ver), String(reason ?? 'failed')]);
  }
  const shellApi = Object.freeze({
    apps: appsLive,
    activation: Object.freeze({
      get: (appId) => makeLive(['activation'], () => (activations.has(appId) ? { ...activations.get(appId) } : null)),
      onChange(cb) { activationListeners.add(cb); return () => activationListeners.delete(cb); },
      reportFailed,
    }),
    metrics: metricsLive,
    leader: leaderLive,
    status: statusZone,
    /** @deprecated seit §16.19 – Alias für `status`. */
    statusZone,
    windowState: windowStateApi,
    artifacts: Object.freeze({
      async read(hash) {
        guardAlive();
        const a = await db.get('artifacts', hash);
        if (!a) throw new CtoxError('not_found', `Artefakt ${hash} nicht lokal`);
        return new Blob([a.content], { type: artifactMime(a.path) }); // Bytes unverändert, Typ nach Endung (PNG → image/png)
      },
      async manifest(appId, ver) { return versionDoc(appId, ver)?.manifest ?? null; },
    }),
    source: sourceApi,
    identity: Object.freeze({
      get instance_id() { return instanceId; },
      get user_id() { return userId; },
      logout: (o) => logout(o),
      // Instanzwechsel: diesen Kernel schließen (Daten bleiben), neuen Kernel für (Instanz, Nutzer) liefern.
      async switchInstance(next = {}) {
        await logout({ deleteData: false });
        return createKernel({ ...opts, dbName: undefined, ...next });
      },
    }),
    window(appId) {
      const tag = `win:${appId}`;
      if (!windowStates.has(appId)) windowStates.set(appId, { title: appId, header: null, size: { width: 0, height: 0 }, mode: 'normal', closed: false });
      const win = windowStates.get(appId);
      return Object.freeze({
        state: makeLive([tag], () => cloneValue(win)),
        setSize(size) { win.size = { width: Number(size?.width ?? 0), height: Number(size?.height ?? 0) }; notify(tag); },
        setMode(mode) { win.mode = String(mode); notify(tag); },
        reopen() { win.closed = false; notify(tag); },
      });
    },
    async mount(root, appId) {
      guardAlive();
      await ready;
      const ver = activations.get(appId)?.activated_ver;
      if (!ver) throw new CtoxError('not_found', `App ${appId} ist nicht aktiviert`);
      const r = await mountVersion(root, appId, ver, { restored: undefined });
      const m = { id: uuidv7(), appId, ver, root, handle: r.handle, api: r.api, closed: false };
      mounts.add(m);
      notify('mounts');
      return Object.freeze({
        app_id: appId,
        get ver() { return m.ver; },
        get api() { return m.api; },
        snapshot: () => m.handle.snapshot?.(),
        async unmount() { m.closed = true; mounts.delete(m); notify('mounts'); await m.handle.unmount(); },
      });
    },
  });

  // ---- Sim-Implementierung (läuft im Leiter) ----
  const simImpl = {
    push: (collection, docs) => corePush(collection, docs),
    setLink(partial) {
      link = { ...link, ...partial };
      if(fakeCore)fakeCore.online = link.state === 'online';
      netChanged();
      if (link.state === 'online') { track(deliver().then(() => schedulePump())); }
      return { ...link };
    },
    setBacklog(n) { extraBacklog = Math.max(0, Number(n) || 0); netChanged(); },
    rejectNext(reason) { fakeCore?.reject.push(reason); },
    migrateNext(o) { fakeCore?.migrate.push(o ?? {}); },
    failCommand(type, error) { fakeCore?.failures.set(type, error); },
    async publishAppVersion(appId, ver, o) {
      await publishInternal(appId, ver, o);
      await waitUntil(() => isTerminalFor(appId, ver) || visible(mirrorGet('business_apps', appId))?.active_ver !== ver || closed);
      await (remountChains.get(appId) ?? Promise.resolve());
      return activations.has(appId) ? { ...activations.get(appId) } : null;
    },
    // §16.22 A2/A3/A6/A9 + §16.26 + Befund B4. Im Leiter, weil der Test-Core hier die Veröffentlichung simuliert.
    async publishDraft(appId, base, draftFiles, draftManifest) {
      const reject = (reason, detail) => ({ status: 'rejected', reason, ...(detail ? { detail } : {}) });
      const active = activeVerOf(appId);
      if ((active ?? null) !== (base ?? null)) {
        return reject('stale_base', base == null ? `App ${appId} existiert bereits (aktiv: ${active})` : `Basis ${base} ist nicht mehr aktiv (aktiv: ${active ?? '–'})`);
      }
      if (publishing.has(appId)) return reject('busy', `Veröffentlichung von ${appId} läuft`);
      const baseManifest = base ? versionDoc(appId, base)?.manifest : null;
      const files = {};
      for (const [p, h] of Object.entries(baseManifest?.files ?? {})) files[p] = await readArtifactContent(h);
      for (const [p, t] of Object.entries(draftFiles ?? {})) { if (t === null) delete files[p]; else files[p] = t; }
      delete files[MANIFEST_FILE]; // §16.26: keine Datei, nur setManifest-Felder zählen
      if (!Object.keys(files).length) return reject('invalid', 'leerer Entwurf');
      const mf = { ...(draftManifest ?? {}) };
      // §16.26: Verweise aus dem tatsächlichen Entwurfsstand prüfen
      const entry = mf.entry ?? baseManifest?.entry ?? 'index.js';
      const handlers = { ...(mf.handlers ?? baseManifest?.handlers ?? {}) };
      for (const [type, path] of Object.entries(handlers)) if (path === null) delete handlers[type];
      const styles = mf.styles ?? baseManifest?.styles ?? [];
      const dangling = [];
      if (files[entry] === undefined) dangling.push(`entry → ${entry}`);
      for (const [type, path] of Object.entries(handlers)) if (files[path] === undefined) dangling.push(`handlers.${type} → ${path}`);
      for (const path of styles) if (files[path] === undefined) dangling.push(`styles → ${path}`);
      if (dangling.length) return reject('invalid', `Verweis auf fehlende Datei: ${dangling.join(', ')}`);
      let parsedBase, parsedNext;
      try {
        parsedBase = await baseSchemaOf(appId, base);
        parsedNext = files[SCHEMA_FILE] !== undefined ? parseCollectionsFile(asText(files[SCHEMA_FILE])) : parsedBase;
      } catch (e) { return reject('invalid', e.message); }
      const diff = classifyCollections(parsedBase, parsedNext, { baseVer: base });
      const missing = diff.filter((x) => x.needsMigration && (!x.migration || (x.migration.migrate_fn && files[x.migration.migrate_fn] === undefined))).map((x) => x.name);
      if (missing.length) return reject('invalid', `Migration fehlt: ${missing.join(', ')}`);
      const migrate = {};
      for (const x of diff.filter((y) => y.kind === 'generation')) {
        if (Array.isArray(x.migration.steps)) { const steps = x.migration.steps; migrate[x.name] = (doc) => applyMigrationSteps(doc, steps); continue; }
        let fn;
        try { fn = (await importModuleText(asText(files[x.migration.migrate_fn]))).default; } catch (e) { return reject('invalid', `${x.migration.migrate_fn}: ${e.message}`); }
        if (typeof fn !== 'function') return reject('invalid', `${x.migration.migrate_fn}: kein default export`);
        migrate[x.name] = fn;
      }
      // Befund B4: ALLE nicht berechneten Felder des Basis-Manifests erben (triggers, command_rights,
      // foreign_collections, exports, row_filters, write_guard, …); im Entwurf gesetzte Felder gehen vor.
      const manifestExtra = {};
      for (const [k, v] of Object.entries(baseManifest ?? {})) if (!MANIFEST_FIELDS_COMPUTED.has(k)) manifestExtra[k] = cloneValue(v);
      for (const [k, v] of Object.entries(mf)) if (!MANIFEST_FIELDS_COMPUTED.has(k)) manifestExtra[k] = cloneValue(v);
      const ver = `s${BigInt(fakeCore?.head ?? '0') + 1n}`;
      try {
        await publishInternal(appId, ver, {
          files, entry, title: mf.title,
          collections: parsedToManifestCollections(parsedNext), incompatible: diff.filter((x) => x.kind === 'generation').map((x) => x.name),
          migrate, handlers, styles, capabilitiesRequired: mf.capabilities_required ?? baseManifest?.capabilities_required,
          minAppApi: mf.min_app_api ?? baseManifest?.min_app_api, layout: mf.layout,
          manifestExtra, locallyModified: true, baseAppVer: base ?? null,
        });
      } catch (e) {
        const msg = String(e?.message ?? e);
        return reject(msg.includes('stale_base') ? 'stale_base' : msg.includes('busy') ? 'busy' : (e?.code ?? 'unavailable'), msg);
      }
      return { status: 'accepted', rev: ver, diff: diff.map(({ name, kind }) => ({ name, kind })) };
    },
    async reportFailed(appId, ver, reason) {
      const cur = activations.get(appId) ?? {};
      const good = (cur.good_vers ?? []).filter((v) => v !== ver);
      const lastGood = cur.activated_ver && cur.activated_ver !== ver ? cur.activated_ver : good.at(-1) ?? null;
      const v = await setActivation(appId, { target_ver: ver, activated_ver: lastGood, good_vers: good, state: 'failed', error: `reported: ${reason}` });
      return { ...v };
    },
  };
  const forward = (name) => (...args) => callLeader(name, args);

  // ---- Sim-Fassade (Test-Hooks) ----
  const sim = {
    rejectNext: (reason = 'simulated_rejection') => forward('rejectNext')(reason),
    migrateNext: (o = {}) => forward('migrateNext')({ dropFields: o.dropFields ?? [] }),
    failCommand: (type, error = 'simulated failure') => forward('failCommand')(type, error),
    onCommand(type, fn) { fakeCore?.handlers.set(type, fn); },
    push: (collection, docs) => forward('push')(collection, docs),
    setLink: (partial) => forward('setLink')(partial),
    setBacklog: (n) => forward('setBacklog')(n),
    publishAppVersion(appId, ver, o = {}) {
      const { module, migrate, ...rest } = o;
      if (module) moduleRegistry.set(`${appId}@${ver}`, module);
      if (isLeader || !bc) return simImpl.publishAppVersion(appId, ver, { ...rest, migrate });
      return (async () => {
        await callLeader('publishAppVersion', [appId, ver, rest]); // migrate-Funktionen sind nicht übertragbar
        await waitUntil(() => isTerminalFor(appId, ver) || closed);
        await (remountChains.get(appId) ?? Promise.resolve());
        return activations.has(appId) ? { ...activations.get(appId) } : null;
      })();
    },
    registerModule(appId, ver, module) { moduleRegistry.set(`${appId}@${ver}`, module); },
    // Loader-Rolle (§16.15): Shell starten und live tauschen.
    async startShell(shellModule, o = {}) {
      if (typeof shellModule?.startShell !== 'function') throw new CtoxError('invalid', 'Shell-Modul exportiert kein startShell()');
      await ready;
      await shellModule.startShell(kernel, { restored: o.restored });
      currentShell = shellModule;
      return true;
    },
    async swapShell(nextShellModule) {
      if (typeof nextShellModule?.startShell !== 'function') throw new CtoxError('invalid', 'Shell-Modul exportiert kein startShell()');
      const prev = currentShell;
      let snap;
      if (prev?.stopShell) {
        try { snap = await withTimeout(Promise.resolve().then(()=>prev.stopShell()),remountTimeoutMs); } catch (e) {
          zoneReport('shell', { level: 'warn', code: 'shell.snapshot_failed', text: `stopShell() warf: ${e?.message ?? e}` }, 'kernel');
        }
      }
      snap = checkSnapshot('shell', snap);
      await windowStateApi.put('shell', snap ?? null); // §16.19 Nr. 3: Shell-Snapshot je Tab unter windowState 'shell'
      try {
        await withTimeout(Promise.resolve().then(() => nextShellModule.startShell(kernel, { restored: cloneValue(snap) })), remountTimeoutMs,
          () => { try { nextShellModule.stopShell?.(); } catch { /* egal */ } });
        currentShell = nextShellModule;
        zoneClear('shell', 'shell.swap_failed');
        return { swapped: true, restored: snap };
      } catch (err) {
        // §10.1: defekte Shell → vorherige Shell-Version starten und Fehler melden
        zoneReport('shell', { level: 'error', code: 'shell.swap_failed', text: `Neue Shell startet nicht (${err?.message ?? err}); vorherige Shell läuft`, ref: null }, 'kernel');
        if (prev) await withTimeout(Promise.resolve().then(()=>prev.startShell(kernel, { restored: cloneValue(snap) })),remountTimeoutMs);
        return { swapped: false, restored: snap, error: String(err?.message ?? err) };
      }
    },
    setPermissions(list) { actor = { ...actor, permissions: [...list] }; notify('actor'); post({ type: 'actor', actor }); },
    setActor(partial) { actor = { ...actor, ...partial }; notify('actor'); post({ type: 'actor', actor }); },
    setLocale(l) { locale = String(l); notify('locale'); },
    setTranslations(l, dict) { translations[l] = { ...(translations[l] ?? {}), ...dict }; notify('locale'); },
    setTheme(tokens) { theme = { ...tokens }; notify('theme'); },
    async evictBlobs() {
      let n = 0;
      await db.tx(async (t) => { for (const b of await t.all('blobs')) if (!b.pinned) { await t.del('blobs', b.ref); n++; } });
      return n;
    },
    async bumpLeaderGen() { await db.tx(async (t) => { const g = ((await t.get('meta', 'leader_gen'))?.value ?? 0) + 1; await t.put('meta', { key: 'leader_gen', value: g }); }); },
    dropBroadcasts(on = true) { dropBroadcasts = !!on; },
    reconcile: () => reconcile(),
    async inspect(collection, id) { return db.get('docs', [collection, id]); },
    outbox: async () => (await db.all('outbox')).sort(byOpId),
    archiveLocal: async () => db.all('archive_local'),
    windowStateRows: async () => db.all('window_state'),
    /** @deprecated seit §16.19 – Alias für `windowStateRows`. */
    windowState: async () => db.all('window_state'),
    coreHead: async () => Number(fakeCore?.head ?? '0'),
    get checkpoint() { return checkpoint; },
    get lastVisible() { return simRecord.lastVisible; },
    get activationReports() { return simRecord.activationReports; },
    get navigations() { return simRecord.navigations; },
    get dialogs() { return simRecord.dialogs; },
    get openedFiles() { return simRecord.openedFiles; },
    get staleDiscards() { return simRecord.staleDiscards; },
    get refetches() { return simRecord.refetches; },
    /** Die nächsten n Artefakt-Abrufe liefern verfälschte Bytes (§16.15a Nr. 3). */
    corruptNextFetch(n = 1) { corruptNextFetch += n; },
    async settle(timeoutMs = 10000) {
      const t0 = Date.now();
      for (;;) {
        await new Promise((r) => setTimeout(r, Math.max(5, latencyMs / 2)));
        const ob = db ? await db.all('outbox') : [];
        if (!ob.length && !undelivered.length && !inflight.size && !tasks.size && !waitList.length && !fakeCore?.pending.size) return true;
        if (Date.now() - t0 > timeoutMs) throw new Error(`settle: nach ${timeoutMs} ms nicht ruhig (outbox ${ob.length}, undelivered ${undelivered.length}, inflight ${inflight.size}, tasks ${tasks.size})`);
      }
    },
  };

  let currentShell = null;
  // ---- Schließen / Abmelden (§16.7) ----
  let ticker = null, reconcileTimer = null;
  const onVis = () => { track(reconcile()); };
  async function close() {
    if (closed) return;
    closed = true;
    for(const cancel of [...deadlineCancels])cancel();
    const cleanup=[];
    if(currentShell?.stopShell)cleanup.push(()=>currentShell.stopShell());
    for(const m of mounts)cleanup.push(()=>m.handle.unmount());
    mounts.clear();
    for (const t of timers) clearTimeout(t);
    timers.clear();
    clearInterval(ticker);
    clearInterval(reconcileTimer);
    if (typeof document !== 'undefined') document.removeEventListener('visibilitychange', onVis);
    if (typeof window !== 'undefined') window.removeEventListener('focus', onVis);
    lockAbort?.abort();
    const rel = releaseLock; releaseLock = null; rel?.();
    isLeader = false;
    for (const l of [...allLives]) l.dispose();
    allLives.clear();
    activationListeners.clear();
    routeListeners.clear();
    for (const w of waitList.splice(0)) w.resolve();
    for (const w of rpcWaiters.values()) w.rej(new CtoxError('unavailable', 'Kernel geschlossen'));
    rpcWaiters.clear();
    try { bc?.close(); } catch { /* egal */ }
    bc = null;
    try { transport.close(); } catch {}
    db?.close();
    // Release connection/locks/listeners before app code. A stuck app must not
    // keep an identity alive or prevent the next tab from becoming leader.
    await Promise.allSettled(cleanup.map(fn=>withTimeout(Promise.resolve().then(fn),remountTimeoutMs)));
  }
  async function logout({ deleteData = false } = {}) {
    loggedOut = true;
    await close();
    mirror.clear();
    zone.length = 0;
    if (deleteData && db) await deleteBackend(dbName, db.kind);
  }

  // ---- Start ----
  const kernel = {
    instanceId, userId, dbName, tabId, ready,
    get isLeader() { return isLeader; },
    get storageKind() { return db?.kind ?? null; },
    tabKey,
    appApi: (appId, appVer) => makeAppApi(appId, appVer),
    statusZone,
    shell: shellApi,
    // Shell-Kernel-Objekte direkt am Kernel (dieselben Objekte wie kernel.shell.*), wie im Stub
    apps: shellApi.apps,
    activation: shellApi.activation,
    metrics: shellApi.metrics,
    leader: shellApi.leader,
    status: shellApi.status,
    windowState: shellApi.windowState,
    artifacts: shellApi.artifacts,
    source: shellApi.source,
    identity: shellApi.identity,
    shellApi: () => shellApi,
    sim,
    receivePage,
    loadModule,
    async evict(collection) {
      guardAlive();
      const records = await evictCollection(db,collection,assertGen);
      for(const row of records)mirrorPut(row);
      evicted.add(collection);scopeCursors.delete(collection);subscriptions.delete(collection);
      post({type:'docs',keys:records.map(r=>[r.collection,r.id])});
      return records.length;
    },
    startShell: (...args) => sim.startShell(...args),
    swapShell: (...args) => sim.swapShell(...args),
    close,
    logout,
  };
  (async () => {
    db = await openBackend(dbName, opts.storage);
    db.onVersionChange = () => { void close(); };
    db.recoverQuota = async () => {
      if(!isLeader || closed)return false;
      const reclaimed=await reclaimCache(db,assertGen,lastUsed,SYSTEM_COLLECTIONS);
      applyReclaim(reclaimed);
      zoneReport('kernel',{level:reclaimed.reclaimed?'warn':'error',code:'storage.quota',text:reclaimed.reclaimed?'Local cache reclaimed after storage quota was reached':'Storage full; pending input is protected'},'kernel');
      return reclaimed.reclaimed;
    };
    await db.tx(async t=>{
      const identity=(await t.get('meta','identity'))?.value;
      if(identity && (identity.instanceId!==instanceId || identity.userId!==userId))throw new CtoxError('forbidden','Database belongs to another identity');
      await t.put('meta',{key:'identity',value:{instanceId,userId}});
    });
    if (closed) { db.close(); throw new CtoxError('unavailable','Kernel closed before ready'); }
    if(opts.legacyDatabase) {
      if(db.kind!=='indexeddb')throw new CtoxError('invalid','V3 takeover requires IndexedDB');
      await takeoverV3Database(db,opts.legacyDatabase,{instanceId,userId});
    }
    await loadFromStorage();
    if (hasLocks) {
      bc = new BroadcastChannel(channelName);
      bc.onmessage = (ev) => { onMessage(ev.data).catch((e) => { if (!closed) console.error('[ctox-kernel] message', e); }); };
    }
    await elect();
    guardAlive();
    if (!isLeader) post({ type: 'hello' });
    loaded = true;
    for (const r of preReady.splice(0)) r();
    ticker = setInterval(() => { if (outboxStat.count) notify('metrics'); }, 1000);
    ticker.unref?.();
    reconcileTimer = setInterval(() => { track(reconcile()); }, 2000);
    reconcileTimer.unref?.();
    if (typeof document !== 'undefined') document.addEventListener('visibilitychange', onVis);
    if (typeof window !== 'undefined') window.addEventListener('focus', onVis);
    if (typeof navigator !== 'undefined' && navigator.storage?.persist && typeof window !== 'undefined') navigator.storage.persist().catch(() => {});
    readyResolve();
  })().catch((err) => { console.error('[ctox-kernel] Start fehlgeschlagen', err); readyResolve(Promise.reject(err)); });
  return kernel;
}

export function createStubKernel(opts = {}) {
  const namespace = opts.dbName ?? `ctox-${opts.instanceId ?? 'stub'}-${opts.userId ?? opts.actor?.id ?? 'local'}`;
  return createKernel({instanceId:'stub',userId:'local',actor:{id:opts.userId??opts.actor?.id??'local',role:'admin',permissions:['apps.source.view','apps.source.edit'],...opts.actor},...opts,transport:new MemoryCoreTransport({namespace,latencyMs:opts.latencyMs})});
}
